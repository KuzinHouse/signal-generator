use crate::config::{GeneratorConfig, MqttConfig};
use crate::models::FlatEntry;
use crate::mqtt_client::MqttHandle;
use actix_web::{web, HttpResponse, Responder};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Состояние приложения
pub struct AppState {
    pub generators: Arc<Mutex<Vec<GeneratorConfig>>>,
    pub current_signals: Arc<Mutex<Vec<Vec<FlatEntry>>>>,
    pub mqtt_handle: Arc<MqttHandle>,
    pub tx_shutdown: tokio::sync::broadcast::Sender<String>,
    pub mqtt_config: Arc<Mutex<MqttConfig>>,
    pub api_token: Option<String>,
    #[allow(dead_code)]
    pub started_at: Instant,
}

/// Проверка Bearer-токена для мутирующих операций. Если токен не настроен — доступ открыт.
pub fn auth_ok(state: &AppState, req: &actix_web::HttpRequest) -> bool {
    let Some(token) = &state.api_token else {
        return true;
    };
    req.headers()
        .get(actix_web::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == token)
        .unwrap_or(false)
}

/// Сохранить конфиги в файл
async fn save_config(state: &AppState) {
    let gens = state.generators.lock().await;
    crate::persistence::save(&gens).await;
}

/// GET /api/generators
pub async fn list_generators(state: web::Data<AppState>) -> impl Responder {
    let gens = state.generators.lock().await;
    HttpResponse::Ok().json(gens.clone())
}

/// POST /api/generators
pub async fn create_generator(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<GeneratorConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let mut config = body.into_inner();
    if config.id.is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
    }
    config.topic = state.mqtt_config.lock().await.topic_for(&config.id);
    {
        let mut gens = state.generators.lock().await;
        let _ = state.tx_shutdown.send(format!("new:{}", config.id));
        gens.push(config.clone());
    } // lock released before save_config (avoids deadlock)
    save_config(&state).await;
    HttpResponse::Created().json(config)
}

/// PUT /api/generators/{id}
pub async fn update_generator(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
    body: web::Json<GeneratorConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let id = path.into_inner();
    let mut updated: Option<GeneratorConfig> = None;
    {
        let mut gens = state.generators.lock().await;
        if let Some(idx) = gens.iter().position(|g| g.id == id) {
            let mut config = body.into_inner();
            config.id = id.clone();
            config.topic = state.mqtt_config.lock().await.topic_for(&id);
            gens[idx] = config.clone();
            let _ = state.tx_shutdown.send(format!("update:{}", id));
            updated = Some(config);
        }
    } // lock released before save_config (avoids deadlock)
    if let Some(config) = updated {
        save_config(&state).await;
        HttpResponse::Ok().json(config)
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "Generator not found"}))
    }
}

/// DELETE /api/generators/{id}
pub async fn delete_generator(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let id = path.into_inner();
    let deleted = {
        let mut gens = state.generators.lock().await;
        if let Some(idx) = gens.iter().position(|g| g.id == id) {
            gens.remove(idx);
            let _ = state.tx_shutdown.send(format!("remove:{}", id));
            true
        } else {
            false
        }
    }; // lock released before save_config (avoids deadlock)
    if deleted {
        save_config(&state).await;
        HttpResponse::Ok().json(serde_json::json!({"status": "deleted"}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "Generator not found"}))
    }
}

/// PUT /api/generators/{id}/toggle
pub async fn toggle_generator(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let id = path.into_inner();
    let mut new_enabled: Option<bool> = None;
    {
        let mut gens = state.generators.lock().await;
        if let Some(g) = gens.iter_mut().find(|g| g.id == id) {
            g.enabled = !g.enabled;
            new_enabled = Some(g.enabled);
            let _ = state.tx_shutdown.send(format!(
                "{}:{}",
                if g.enabled { "resume" } else { "pause" },
                id
            ));
        }
    } // lock released before save_config (avoids deadlock)
    if let Some(enabled) = new_enabled {
        save_config(&state).await;
        HttpResponse::Ok().json(serde_json::json!({"enabled": enabled}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "Generator not found"}))
    }
}

/// GET /api/generators/{id}/current — возвращает полный JSON-LD массив
pub async fn get_current_signal(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let full_id = format!("sensor/{}", id);
    let signals = state.current_signals.lock().await;
    if let Some(payload) = signals
        .iter()
        .find(|p| p.iter().any(|e| e.id == full_id || e.id == id))
    {
        HttpResponse::Ok().json(payload)
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "No signal yet"}))
    }
}

/// GET /api/health — расширенная диагностика
pub async fn health(state: web::Data<AppState>) -> impl Responder {
    let gens = state.generators.lock().await;
    let signals = state.current_signals.lock().await;
    let mqtt = state.mqtt_handle.is_connected();
    let uptime = state.started_at.elapsed().as_secs();
    let gen_count = gens.len();
    let active_count = gens.iter().filter(|g| g.enabled).count();
    let signal_rate: f64 = gens
        .iter()
        .filter(|g| g.enabled)
        .map(|g| 1000.0 / g.interval_ms.max(1) as f64)
        .sum();

    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_sec": uptime,
        "generators": gen_count,
        "active": active_count,
        "signal_rate_hz": (signal_rate * 10.0).round() / 10.0,
        "mqtt_connected": mqtt,
        "signals_cached": signals.len(),
    }))
}

/// GET /api/broker
pub async fn broker_status(state: web::Data<AppState>) -> impl Responder {
    let bs = state.mqtt_handle.get_broker_status().await;
    HttpResponse::Ok().json(bs)
}

/// GET /api/broker/history
pub async fn broker_history(state: web::Data<AppState>) -> impl Responder {
    let h = state.mqtt_handle.get_broker_history().await;
    HttpResponse::Ok().json(h)
}

/// POST /api/mqtt/test — проверить доступность брокера (TCP-коннект с таймаутом)
pub async fn test_mqtt_connection(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<MqttConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let cfg = body.into_inner();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let started = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => HttpResponse::Ok().json(serde_json::json!({
            "ok": true,
            "host": cfg.host,
            "port": cfg.port,
            "tls": cfg.use_tls,
            "latency_ms": started.elapsed().as_millis(),
        })),
        Ok(Err(e)) => HttpResponse::Ok().json(serde_json::json!({
            "ok": false,
            "error": e.to_string(),
        })),
        Err(_) => HttpResponse::Ok().json(serde_json::json!({
            "ok": false,
            "error": "timeout after 3s",
        })),
    }
}

/// GET /api/mqtt/config — текущие настройки брокера (пароль маскируем)
pub async fn get_mqtt_config(state: web::Data<AppState>) -> impl Responder {
    let cfg = state.mqtt_config.lock().await;
    let mut out = cfg.clone();
    if !out.password.is_empty() {
        out.password = "••••••".to_string();
    }
    HttpResponse::Ok().json(out)
}

/// PUT /api/mqtt/config — сохранить настройки брокера (перезапуск MQTT)
pub async fn update_mqtt_config(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<MqttConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let mut cfg = body.into_inner();
    if cfg.host.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "host required"}));
    }
    if cfg.port == 0 {
        cfg.port = 1883;
    }
    if cfg.topic_prefix.trim().is_empty() {
        cfg.topic_prefix = "USEPI".to_string();
    }
    if cfg.diagnostics_topic.trim().is_empty() {
        cfg.diagnostics_topic = "USEPI/diagnostics".to_string();
    }
    cfg.topic_prefix = cfg.topic_prefix.trim().to_string();
    cfg.diagnostics_topic = cfg.diagnostics_topic.trim().to_string();

    // Обновляем конфиг в state
    {
        let mut cur = state.mqtt_config.lock().await;
        // Если пароль пришёл маской — сохраняем старый
        if cfg.password == "••••••" {
            cfg.password = cur.password.clone();
        }
        *cur = cfg.clone();
    } // lock released before save (avoids deadlock)
    crate::persistence::save_mqtt(&cfg).await;

    // Пересчитываем топики генераторов под новый префикс и перезапускаем их
    let snapshot;
    {
        let mut gens = state.generators.lock().await;
        for g in gens.iter_mut() {
            let new_topic = cfg.topic_for(&g.id);
            if g.topic != new_topic {
                g.topic = new_topic;
                let _ = state.tx_shutdown.send(format!("update:{}", g.id));
            }
        }
        snapshot = gens.clone();
    } // lock released
    crate::persistence::save(&snapshot).await;

    // Переподключение MQTT с новыми настройками
    let new_cfg = cfg.clone();
    let mqtt = state.mqtt_handle.clone();
    tokio::spawn(async move {
        // даём текущим публикациям завершиться, затем пересоздаём клиент
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        mqtt.reconnect(&new_cfg).await;
    });

    HttpResponse::Ok().json(cfg)
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/generators", web::get().to(list_generators))
            .route("/generators", web::post().to(create_generator))
            .route("/generators/{id}", web::put().to(update_generator))
            .route("/generators/{id}", web::delete().to(delete_generator))
            .route("/generators/{id}/toggle", web::put().to(toggle_generator))
            .route(
                "/generators/{id}/current",
                web::get().to(get_current_signal),
            )
            .route("/broker", web::get().to(broker_status))
            .route("/broker/history", web::get().to(broker_history))
            .route("/mqtt/config", web::get().to(get_mqtt_config))
            .route("/mqtt/config", web::put().to(update_mqtt_config))
            .route("/mqtt/test", web::post().to(test_mqtt_connection))
            .route("/health", web::get().to(health)),
    );
}
