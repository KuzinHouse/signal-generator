use crate::api::AppState;
use actix::prelude::*;
use actix_web_actors::ws;
use std::time::Duration;

/// WebSocket-клиент: получает снапшоты состояния (генераторы + сигналы + брокер)
pub struct WsSession {
    state: actix_web::web::Data<AppState>,
}

impl WsSession {
    pub fn new(state: actix_web::web::Data<AppState>) -> Self {
        Self { state }
    }

    /// Собрать JSON-снапшот для клиента
    async fn snapshot(state: &actix_web::web::Data<AppState>) -> serde_json::Value {
        let gens = state.generators.lock().await;
        let sigs = state.current_signals.lock().await;
        let broker = state.mqtt_handle.get_broker_status().await;
        let history = state.mqtt_handle.get_broker_history().await;

        let mut signals_map = serde_json::Map::new();
        for payload in sigs.iter() {
            if let Some(first) = payload.first() {
                let id = first.id.trim_start_matches("sensor/").to_string();
                signals_map.insert(
                    id,
                    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
                );
            }
        }

        serde_json::json!({
            "generators": gens.clone(),
            "signals": signals_map,
            "broker": serde_json::to_value(broker).unwrap_or(serde_json::Value::Null),
            "history": serde_json::to_value(history).unwrap_or(serde_json::Value::Null),
        })
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address();
        // Первый снапшот сразу
        let state = self.state.clone();
        tokio::spawn(async move {
            let snap = WsSession::snapshot(&state).await;
            addr.do_send(SendSnapshot(snap));
        });

        // Heartbeat каждые 30с
        ctx.run_interval(Duration::from_secs(30), |_act, ctx| {
            ctx.ping(b"ping");
        });

        // Обновления раз в секунду
        let state = self.state.clone();
        let addr2 = ctx.address();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tick.tick().await;
                let snap = WsSession::snapshot(&state).await;
                addr2.do_send(SendSnapshot(snap));
            }
        });
    }
}

/// Сообщение: отправить снапшот клиенту
struct SendSnapshot(serde_json::Value);

impl Message for SendSnapshot {
    type Result = ();
}

impl Handler<SendSnapshot> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: SendSnapshot, ctx: &mut Self::Context) {
        ctx.text(msg.0.to_string());
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Pong(_)) => {}
            Ok(ws::Message::Text(_)) => {} // клиент может слать что угодно — игнорируем
            Ok(ws::Message::Binary(_)) => {}
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            _ => ctx.stop(),
        }
    }
}

/// HTTP-хендлер: апгрейд соединения до WebSocket
pub async fn ws_index(
    req: actix_web::HttpRequest,
    stream: actix_web::web::Payload,
    state: actix_web::web::Data<AppState>,
) -> Result<actix_web::HttpResponse, actix_web::Error> {
    ws::start(WsSession::new(state), &req, stream)
}
