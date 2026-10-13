use crate::config::{BrokerConfig, GeneratorConfig, MqttConfig};
use crate::models::FlatEntry;
use crate::mqtt_client::MqttManager;
use actix_web::{web, HttpResponse, Responder};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

/// Состояние приложения
pub struct AppState {
    pub generators: Arc<Mutex<Vec<GeneratorConfig>>>,
    pub current_signals: Arc<Mutex<Vec<Vec<FlatEntry>>>>,
    pub signal_history:
        Arc<Mutex<std::collections::HashMap<String, std::collections::VecDeque<(i64, f64)>>>>,
    /// Мультиброкерный MQTT-менеджер (параллельная публикация во все включённые брокеры)
    pub mqtt_handle: Arc<MqttManager>,
    pub tx_shutdown: tokio::sync::broadcast::Sender<String>,
    /// Список брокеров (персистентный)
    pub mqtt_config: Arc<Mutex<MqttConfig>>,
    pub api_token: Option<String>,
    /// Глобальный множитель частоты (скорость) для ВСЕХ генераторов. 1.0 = номинально.
    pub global_speed: Arc<Mutex<f64>>,
    #[allow(dead_code)]
    pub started_at: Instant,
}

/// Лимит точек истории на генератор
pub const HISTORY_CAP: usize = 300;

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

/// Топик генератора на ГЛАВНОМ (первом включённом) брокере — для отображения в UI
// Вспомогательная async-функция: топик на главном брокере
async fn topic_for_on_primary(state: &web::Data<AppState>, catalog: &str, id: &str) -> String {
    let cfg = state.mqtt_config.lock().await;
    cfg.primary_or_default().topic_for_catalog(catalog, id)
}

/// Сохранить MQTT-конфиг (копия) в файл
async fn save_mqtt_cfg(state: &AppState) {
    let cfg = state.mqtt_config.lock().await;
    crate::persistence::save_mqtt(&cfg).await;
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
    config.sanitize();
    config.topic = topic_for_on_primary(&state, &config.catalog, &config.id).await;
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
            config.sanitize();
            config.topic = topic_for_on_primary(&state, &config.catalog, &config.id).await;
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
    let mqtt = state.mqtt_handle.any_connected().await;
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

/// GET /api/broker — статус главного брокера (legacy)
pub async fn broker_status(state: web::Data<AppState>) -> impl Responder {
    match state.mqtt_handle.primary_status().await {
        Some(bs) => HttpResponse::Ok().json(bs),
        None => HttpResponse::Ok().json(serde_json::json!({"version":"—","clients_connected":0})),
    }
}

/// GET /api/broker/history — история главного брокера (legacy)
pub async fn broker_history(state: web::Data<AppState>) -> impl Responder {
    let primary = state.mqtt_config.lock().await.primary().map(|b| b.id);
    let id = primary.unwrap_or_else(|| "local".to_string());
    let h = state.mqtt_handle.broker_history(&id).await.unwrap_or_default();
    HttpResponse::Ok().json(h)
}

/// GET /api/brokers/status — живой статус ВСЕХ брокеров
pub async fn brokers_status(state: web::Data<AppState>) -> impl Responder {
    let statuses = state.mqtt_handle.statuses().await;
    HttpResponse::Ok().json(statuses)
}

/// GET /api/brokers/{id}/status — статус конкретного брокера
pub async fn broker_status_by_id(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    match state.mqtt_handle.broker_status(&id).await {
        Some(bs) => HttpResponse::Ok().json(bs),
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "broker not found"})),
    }
}

/// GET /api/brokers/{id}/history — история брокера
pub async fn broker_history_by_id(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let h = state.mqtt_handle.broker_history(&id).await.unwrap_or_default();
    HttpResponse::Ok().json(h)
}

/// GET /api/brokers — список брокеров (пароли маскируем)
pub async fn list_brokers(state: web::Data<AppState>) -> impl Responder {
    let cfg = state.mqtt_config.lock().await;
    let mut out = cfg.brokers.clone();
    for b in out.iter_mut() {
        if !b.password.is_empty() {
            b.password = "••••••".to_string();
        }
    }
    HttpResponse::Ok().json(serde_json::json!({"brokers": out}))
}

/// POST /api/brokers — добавить брокер (в список, сохранить, подключить)
pub async fn create_broker(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<BrokerConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let mut broker = body.into_inner();
    if broker.host.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "host required"}));
    }
    if broker.id.trim().is_empty() {
        broker.id = uuid::Uuid::new_v4().to_string();
    }
    if broker.port == 0 {
        broker.port = 1883;
    }
    broker.host = broker.host.trim().to_string();
    // Дедупликация по id
    {
        let mut cfg = state.mqtt_config.lock().await;
        cfg.brokers.retain(|b| b.id != broker.id);
        cfg.brokers.push(broker.clone());
    }
    save_mqtt_cfg(&state).await;
    // Подключаем брокер, если включён
    let mgr = state.mqtt_handle.clone();
    let list = state.mqtt_config.lock().await.brokers.clone();
    let broker_enabled = broker.enabled;
    if broker_enabled {
        tokio::spawn(async move {
            mgr.sync_brokers(&list).await;
        });
    }
    HttpResponse::Created().json(broker)
}

/// PUT /api/brokers/{id} — обновить брокер
pub async fn update_broker(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
    body: web::Json<BrokerConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let id = path.into_inner();
    let mut broker = body.into_inner();
    broker.id = id.clone();
    if broker.host.trim().is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "host required"}));
    }
    if broker.port == 0 {
        broker.port = 1883;
    }
    broker.host = broker.host.trim().to_string();
    let mut saved: Option<BrokerConfig> = None;
    {
        let mut cfg = state.mqtt_config.lock().await;
        if let Some(pos) = cfg.brokers.iter().position(|b| b.id == id) {
            // если пароль пришёл маской — сохраняем старый
            if broker.password == "••••••" {
                broker.password = cfg.brokers[pos].password.clone();
            }
            cfg.brokers[pos] = broker.clone();
            saved = Some(broker.clone());
        }
    }
    if saved.is_none() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "broker not found"}));
    }
    save_mqtt_cfg(&state).await;
    // Переподключаем весь набор
    let mgr = state.mqtt_handle.clone();
    let list = state.mqtt_config.lock().await.brokers.clone();
    tokio::spawn(async move {
        mgr.sync_brokers(&list).await;
    });
    HttpResponse::Ok().json(broker)
}

/// PUT /api/brokers/{id}/toggle — вкл/выкл брокер
pub async fn toggle_broker(
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
        let mut cfg = state.mqtt_config.lock().await;
        if let Some(b) = cfg.brokers.iter_mut().find(|b| b.id == id) {
            b.enabled = !b.enabled;
            new_enabled = Some(b.enabled);
        }
    }
    if new_enabled.is_none() {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "broker not found"}));
    }
    save_mqtt_cfg(&state).await;
    let mgr = state.mqtt_handle.clone();
    let list = state.mqtt_config.lock().await.brokers.clone();
    tokio::spawn(async move {
        mgr.sync_brokers(&list).await;
    });
    HttpResponse::Ok().json(serde_json::json!({"enabled": new_enabled.unwrap()}))
}

/// DELETE /api/brokers/{id} — удалить брокер
pub async fn delete_broker(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    path: web::Path<String>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let id = path.into_inner();
    let deleted = {
        let mut cfg = state.mqtt_config.lock().await;
        let before = cfg.brokers.len();
        cfg.brokers.retain(|b| b.id != id);
        cfg.brokers.len() != before
    };
    if !deleted {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "broker not found"}));
    }
    save_mqtt_cfg(&state).await;
    let mgr = state.mqtt_handle.clone();
    let list = state.mqtt_config.lock().await.brokers.clone();
    tokio::spawn(async move {
        mgr.sync_brokers(&list).await;
    });
    HttpResponse::Ok().json(serde_json::json!({"status": "deleted"}))
}

/// GET /api/brokers/{id}/config — конфиг брокера (пароль маскируем)
pub async fn get_broker_config(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    let id = path.into_inner();
    let cfg = state.mqtt_config.lock().await;
    match cfg.brokers.iter().find(|b| b.id == id) {
        Some(b) => {
            let mut out = b.clone();
            if !out.password.is_empty() {
                out.password = "••••••".to_string();
            }
            HttpResponse::Ok().json(out)
        }
        None => HttpResponse::NotFound().json(serde_json::json!({"error": "broker not found"})),
    }
}

/// POST /api/mqtt/test — проверить доступность брокера (TCP-коннект с таймаутом)
pub async fn test_mqtt_connection(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<BrokerConfig>,
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

/// GET /api/generators/{id}/history — история значений (для осциллографа)
pub async fn get_generator_history(
    state: web::Data<AppState>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> impl Responder {
    let id = path.into_inner();
    let limit: usize = query
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
        .min(HISTORY_CAP);
    let hist = state.signal_history.lock().await;
    let Some(buf) = hist.get(&id) else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "No history yet"}));
    };
    let points: Vec<serde_json::Value> = buf
        .iter()
        .skip(buf.len().saturating_sub(limit))
        .map(|(t, v)| serde_json::json!({"t": t, "v": v}))
        .collect();
    HttpResponse::Ok().json(serde_json::json!({"id": id, "points": points}))
}

/// GET /api/config/export — полный бэкап (генераторы + ВСЕ брокеры)
pub async fn export_config(state: web::Data<AppState>) -> impl Responder {
    let gens = state.generators.lock().await;
    let mqtt = state.mqtt_config.lock().await;
    let mut mqtt_out = mqtt.clone();
    for b in mqtt_out.brokers.iter_mut() {
        if !b.password.is_empty() {
            b.password = "••••••".to_string();
        }
    }
    HttpResponse::Ok().json(serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "generators": gens.clone(),
        "mqtt": mqtt_out,
    }))
}

/// POST /api/config/import — восстановить из бэкапа (заменяет генераторы и брокеры)
pub async fn import_config(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let gens: Vec<GeneratorConfig> = match body.get("generators").cloned() {
        Some(v) => match serde_json::from_value(v) {
            Ok(g) => g,
            Err(e) => {
                return HttpResponse::BadRequest()
                    .json(serde_json::json!({"error": format!("bad generators: {}", e)}))
            }
        },
        None => {
            return HttpResponse::BadRequest()
                .json(serde_json::json!({"error": "missing generators"}))
        }
    };
    if gens.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({"error": "empty generators"}));
    }

    // MQTT-настройки (брокеры) — если есть
    if let Some(mqtt_val) = body.get("mqtt").cloned() {
        // Может быть старый одиночный конфиг или новый список — поддерживаем оба
        if let Ok(mut mqtt_new) = serde_json::from_value::<MqttConfig>(mqtt_val.clone()) {
            // маскированные пароли переносим из текущего списка
            let cur = state.mqtt_config.lock().await;
            for b in mqtt_new.brokers.iter_mut() {
                if b.password == "••••••" {
                    if let Some(e) = cur.brokers.iter().find(|x| x.id == b.id) {
                        b.password = e.password.clone();
                    }
                }
            }
            drop(cur);
            {
                let mut cur = state.mqtt_config.lock().await;
                *cur = mqtt_new.clone();
            }
            save_mqtt_cfg(&state).await;
            let mgr = state.mqtt_handle.clone();
            let list = state.mqtt_config.lock().await.brokers.clone();
            tokio::spawn(async move {
                mgr.sync_brokers(&list).await;
            });
        } else if let Ok(old) = serde_json::from_value::<crate::config::BrokerConfig>(mqtt_val) {
            // одиночный старый формат → один брокер
            let cfg = MqttConfig {
                brokers: vec![old],
            };
            {
                let mut cur = state.mqtt_config.lock().await;
                *cur = cfg.clone();
            }
            save_mqtt_cfg(&state).await;
            let mgr = state.mqtt_handle.clone();
            let list = state.mqtt_config.lock().await.brokers.clone();
            tokio::spawn(async move {
                mgr.sync_brokers(&list).await;
            });
        }
    }

    // Заменяем генераторы и перезапускаем их
    let mut imported = gens;
    for g in imported.iter_mut() {
        g.sanitize();
        let topic = topic_for_on_primary(&state, &g.catalog, &g.id).await;
        if g.topic.is_empty() || g.topic != topic {
            g.topic = topic;
        }
    }
    {
        let mut cur = state.generators.lock().await;
        *cur = imported.clone();
        for g in imported.iter() {
            let _ = state.tx_shutdown.send(format!("update:{}", g.id));
        }
    }
    crate::persistence::save(&imported).await;

    HttpResponse::Ok().json(serde_json::json!({
        "status": "imported",
        "generators": imported.len(),
        "mqtt": true,
    }))
}

/// GET /api/mqtt/config — настройки брокеров (пароли маскируем)
pub async fn get_mqtt_config(state: web::Data<AppState>) -> impl Responder {
    let cfg = state.mqtt_config.lock().await;
    let mut out = cfg.clone();
    for b in out.brokers.iter_mut() {
        if !b.password.is_empty() {
            b.password = "••••••".to_string();
        }
    }
    HttpResponse::Ok().json(out)
}

/// PUT /api/mqtt/config — сохранить список брокеров (переподключить MQTT)
pub async fn update_mqtt_config(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<MqttConfig>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let mut cfg = body.into_inner();
    if cfg.brokers.is_empty() {
        return HttpResponse::BadRequest()
            .json(serde_json::json!({"error": "at least one broker required"}));
    }
    // Нормализация и перенос маскированных паролей
    {
        let cur = state.mqtt_config.lock().await;
        for b in cfg.brokers.iter_mut() {
            if b.host.trim().is_empty() {
                b.host = "127.0.0.1".to_string();
            }
            if b.port == 0 {
                b.port = 1883;
            }
            if b.topic_prefix.trim().is_empty() {
                b.topic_prefix = "USEPI".to_string();
            }
            if b.diagnostics_topic.trim().is_empty() {
                b.diagnostics_topic = "USEPI/diagnostics".to_string();
            }
            if b.id.trim().is_empty() {
                b.id = uuid::Uuid::new_v4().to_string();
            }
            if b.password == "••••••" {
                if let Some(e) = cur.brokers.iter().find(|x| x.id == b.id) {
                    b.password = e.password.clone();
                }
            }
            b.host = b.host.trim().to_string();
            b.topic_prefix = b.topic_prefix.trim().to_string();
            b.diagnostics_topic = b.diagnostics_topic.trim().to_string();
        }
    }

    {
        let mut cur = state.mqtt_config.lock().await;
        *cur = cfg.clone();
    } // lock released before save (avoids deadlock)
    crate::persistence::save_mqtt(&cfg).await;

    // Пересчитываем топики генераторов под главного брокера и перезапускаем их
    let snapshot;
    {
        let mut gens = state.generators.lock().await;
        let primary = cfg.primary_or_default();
        for g in gens.iter_mut() {
            let new_topic = primary.topic_for_catalog(&g.catalog, &g.id);
            if g.topic != new_topic {
                g.topic = new_topic;
                let _ = state.tx_shutdown.send(format!("update:{}", g.id));
            }
        }
        snapshot = gens.clone();
    } // lock released
    crate::persistence::save(&snapshot).await;

    // Переподключение MQTT-менеджера под новый список
    let mgr = state.mqtt_handle.clone();
    let list = cfg.brokers.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        mgr.sync_brokers(&list).await;
    });

    HttpResponse::Ok().json(cfg)
}

/// Множитель скорости (0.1..10) — ограничение, чтобы не зайти в 0/бесконечность
const SPEED_MIN: f64 = 0.1;
const SPEED_MAX: f64 = 10.0;

/// GET /api/global/speed — текущий глобальный множитель частоты
pub async fn get_global_speed(state: web::Data<AppState>) -> impl Responder {
    let speed = *state.global_speed.lock().await;
    HttpResponse::Ok().json(serde_json::json!({
        "speed": (speed * 100.0).round() / 100.0,
        "min": SPEED_MIN,
        "max": SPEED_MAX,
    }))
}

/// PUT /api/global/speed — установить глобальный множитель частоты (для всех генераторов).
/// Меняет интервал публикации БЕЗ перезапуска генераторов (читается в run_single_generator).
pub async fn set_global_speed(
    state: web::Data<AppState>,
    req: actix_web::HttpRequest,
    body: web::Json<serde_json::Value>,
) -> impl Responder {
    if !auth_ok(&state, &req) {
        return HttpResponse::Unauthorized().json(serde_json::json!({"error": "unauthorized"}));
    }
    let speed: f64 = body
        .get("speed")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0)
        .clamp(SPEED_MIN, SPEED_MAX);
    // спасаем от NaN/бесконечности
    let speed = if speed.is_finite() { speed } else { 1.0 };
    {
        let mut cur = state.global_speed.lock().await;
        *cur = speed;
    }
    HttpResponse::Ok().json(serde_json::json!({
        "speed": (speed * 100.0).round() / 100.0,
        "min": SPEED_MIN,
        "max": SPEED_MAX,
    }))
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
            .route(
                "/generators/{id}/history",
                web::get().to(get_generator_history),
            )
            .route("/broker", web::get().to(broker_status))
            .route("/broker/history", web::get().to(broker_history))
            .route("/brokers", web::get().to(list_brokers))
            .route("/brokers", web::post().to(create_broker))
            .route("/brokers/status", web::get().to(brokers_status))
            .route("/brokers/{id}", web::put().to(update_broker))
            .route("/brokers/{id}", web::delete().to(delete_broker))
            .route("/brokers/{id}/toggle", web::put().to(toggle_broker))
            .route("/brokers/{id}/config", web::get().to(get_broker_config))
            .route("/brokers/{id}/status", web::get().to(broker_status_by_id))
            .route("/brokers/{id}/history", web::get().to(broker_history_by_id))
            .route("/mqtt/config", web::get().to(get_mqtt_config))
            .route("/mqtt/config", web::put().to(update_mqtt_config))
            .route("/mqtt/test", web::post().to(test_mqtt_connection))
            .route("/config/export", web::get().to(export_config))
            .route("/config/import", web::post().to(import_config))
            .route("/global/speed", web::get().to(get_global_speed))
            .route("/global/speed", web::put().to(set_global_speed))
            .route("/health", web::get().to(health)),
    );
}
