use crate::api::AppState;
use actix::prelude::*;
use actix_web_actors::ws;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// WebSocket-клиент: получает снапшоты состояния (генераторы + сигналы + брокер)
///
/// Оптимизация латентности:
/// - каждые 500 мс шлём только ИЗМЕНИВШИЕСЯ сигналы (по timestamp) — кадр ~1-2KB вместо 25KB
/// - полный снапшот раз в 5 секунд (генераторы могут меняться через API) и при подключении
pub struct WsSession {
    state: actix_web::web::Data<AppState>,
    last_timestamps: Arc<Mutex<HashMap<String, String>>>,
}

impl WsSession {
    pub fn new(state: actix_web::web::Data<AppState>) -> Self {
        Self {
            state,
            last_timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Полный снапшот: генераторы + все сигналы + брокер + история
    async fn full_snapshot(state: &actix_web::web::Data<AppState>) -> serde_json::Value {
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
            "type": "snapshot",
            "generators": gens.clone(),
            "signals": signals_map,
            "broker": serde_json::to_value(broker).unwrap_or(serde_json::Value::Null),
            "history": serde_json::to_value(history).unwrap_or(serde_json::Value::Null),
        })
    }

    /// Дифф-снапшот: только сигналы, чей timestamp изменился с прошлого раза.
    /// Обновляет last_timestamps. Возвращает None, если изменений нет.
    async fn diff_snapshot(
        state: &actix_web::web::Data<AppState>,
        last_timestamps: &Mutex<HashMap<String, String>>,
    ) -> Option<serde_json::Value> {
        let sigs = state.current_signals.lock().await;
        let mut last = last_timestamps.lock().await;
        let mut changed = serde_json::Map::new();
        let mut changed_any = false;

        for payload in sigs.iter() {
            let Some(first) = payload.first() else {
                continue;
            };
            let id = first.id.trim_start_matches("sensor/").to_string();
            let ts = payload
                .iter()
                .find(|e| e.id == "timestamp")
                .and_then(|e| e.entry_value.as_str())
                .unwrap_or("")
                .to_string();
            if last.get(&id) != Some(&ts) {
                last.insert(id.clone(), ts);
                changed.insert(
                    id,
                    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
                );
                changed_any = true;
            }
        }

        if changed_any {
            Some(serde_json::json!({ "type": "update", "signals": changed }))
        } else {
            None
        }
    }
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address();

        // Полный снапшот при подключении
        let state = self.state.clone();
        tokio::spawn(async move {
            let snap = WsSession::full_snapshot(&state).await;
            addr.do_send(SendSnapshot(snap));
        });

        // Heartbeat каждые 30с
        ctx.run_interval(Duration::from_secs(30), |_act, ctx| {
            ctx.ping(b"ping");
        });

        // Дифф-обновления каждые 500мс; полный снапшот каждые 5с
        let state = self.state.clone();
        let timestamps = self.last_timestamps.clone();
        let addr2 = ctx.address();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(500));
            let mut full_counter = 0u32;
            loop {
                tick.tick().await;
                full_counter += 1;
                if full_counter >= 10 {
                    full_counter = 0;
                    let snap = WsSession::full_snapshot(&state).await;
                    addr2.do_send(SendSnapshot(snap));
                } else {
                    if let Some(diff) = WsSession::diff_snapshot(&state, &timestamps).await {
                        addr2.do_send(SendSnapshot(diff));
                    }
                }
            }
        });
    }
}

/// Сообщение: отправить снапшот/дифф клиенту
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
