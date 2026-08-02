use actix_web::{web, HttpResponse, Responder};
use crate::config::GeneratorConfig;
use crate::models::FlatEntry;
use crate::mqtt_client::MqttHandle;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Состояние приложения
pub struct AppState {
    pub generators: Arc<Mutex<Vec<GeneratorConfig>>>,
    pub current_signals: Arc<Mutex<Vec<Vec<FlatEntry>>>>,
    pub mqtt_handle: Arc<MqttHandle>,
    pub tx_shutdown: tokio::sync::broadcast::Sender<String>,
    #[allow(dead_code)]
    pub started_at: Instant,
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
    body: web::Json<GeneratorConfig>,
) -> impl Responder {
    let mut config = body.into_inner();
    if config.id.is_empty() {
        config.id = uuid::Uuid::new_v4().to_string();
    }
    config.topic = format!("USEPI/{}", config.id);
    let mut gens = state.generators.lock().await;
    let _ = state.tx_shutdown.send(format!("new:{}", config.id));
    gens.push(config.clone());
    save_config(&state).await;
    HttpResponse::Created().json(config)
}

/// PUT /api/generators/{id}
pub async fn update_generator(
    state: web::Data<AppState>,
    path: web::Path<String>,
    body: web::Json<GeneratorConfig>,
) -> impl Responder {
    let id = path.into_inner();
    let mut gens = state.generators.lock().await;
    if let Some(idx) = gens.iter().position(|g| g.id == id) {
        let mut config = body.into_inner();
        config.id = id.clone();
        config.topic = format!("USEPI/{}", id);
        gens[idx] = config.clone();
        let _ = state.tx_shutdown.send(format!("update:{}", id));
        save_config(&state).await;
        HttpResponse::Ok().json(config)
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "Generator not found"}))
    }
}

/// DELETE /api/generators/{id}
pub async fn delete_generator(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let mut gens = state.generators.lock().await;
    if let Some(idx) = gens.iter().position(|g| g.id == id) {
        gens.remove(idx);
        let _ = state.tx_shutdown.send(format!("remove:{}", id));
        save_config(&state).await;
        HttpResponse::Ok().json(serde_json::json!({"status": "deleted"}))
    } else {
        HttpResponse::NotFound().json(serde_json::json!({"error": "Generator not found"}))
    }
}

/// PUT /api/generators/{id}/toggle
pub async fn toggle_generator(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let mut gens = state.generators.lock().await;
    if let Some(g) = gens.iter_mut().find(|g| g.id == id) {
        g.enabled = !g.enabled;
        let _ = state.tx_shutdown.send(format!(
            "{}:{}", if g.enabled { "resume" } else { "pause" }, id
        ));
        save_config(&state).await;
        HttpResponse::Ok().json(serde_json::json!({"enabled": g.enabled}))
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
    if let Some(payload) = signals.iter().find(|p| {
        p.iter().any(|e| e.id == full_id || e.id == id)
    }) {
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
    let signal_rate: f64 = gens.iter().filter(|g| g.enabled)
        .map(|g| 1000.0 / g.interval_ms.max(1) as f64).sum();

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

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .route("/generators", web::get().to(list_generators))
            .route("/generators", web::post().to(create_generator))
            .route("/generators/{id}", web::put().to(update_generator))
            .route("/generators/{id}", web::delete().to(delete_generator))
            .route("/generators/{id}/toggle", web::put().to(toggle_generator))
            .route("/generators/{id}/current", web::get().to(get_current_signal))
            .route("/broker", web::get().to(broker_status))
            .route("/broker/history", web::get().to(broker_history))
            .route("/health", web::get().to(health)),
    );
}
