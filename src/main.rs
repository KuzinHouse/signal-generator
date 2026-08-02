mod api;
mod config;
mod diagnostics;
mod generator;
mod models;
mod mqtt_client;
mod persistence;
mod ui;
mod ws;

use actix_files::Files;
use actix_web::{middleware, web, App, HttpServer};
use api::AppState;
use config::{GeneratorConfig, WaveType};
use generator::GeneratorState;
use log::info;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::sync::Mutex;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // MQTT конфиг: из файла или дефолт
    let mqtt_config = persistence::load_mqtt().await.unwrap_or_else(|| {
        log::info!("No saved MQTT config, using defaults");
        config::MqttConfig::default()
    });
    let mqtt_host = mqtt_config.host.clone();
    let mqtt_port = mqtt_config.port;

    // Демо-генераторы с реалистичными настройками
    let mut default_generators = vec![
        GeneratorConfig {
            // Температура печи: синус с небольшим дрейфом и шумом, редкие выбросы
            id: "temp-01".into(),
            name: "Температура печи".into(),
            enabled: true,
            topic: "USEPI/temp-01".into(),
            wave_type: WaveType::Sine,
            interval_ms: 1000,
            amplitude: 10.0,
            offset: 250.0,
            frequency: 0.05,
            unit: "°C".into(),
            quality: 98,
            min: 100.0,
            max: 400.0,
            drift: 0.05,
            spike_prob: 0.005,
            spike_amp: 8.0,
            noise_amp: 0.02,
            deadband: 0.5,
            hysteresis: 0.0,
            stuck_prob: 0.0005,
            stuck_duration_ms: 3000,
            jitter: 5.0,
            trend: 0.0,
            drop_prob: 0.005,
            degradation_rate: 0.0,
            modbus_addr: 0,
            modbus_fn: 0,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 0,
        },
        GeneratorConfig {
            // Давление: шум с джиттером, случайные выбросы
            id: "pressure-01".into(),
            name: "Давление в магистрали".into(),
            enabled: true,
            topic: "USEPI/pressure-01".into(),
            wave_type: WaveType::Noise,
            interval_ms: 500,
            amplitude: 3.0,
            offset: 5.0,
            frequency: 0.0,
            unit: "bar".into(),
            quality: 92,
            min: 0.0,
            max: 12.0,
            drift: 0.02,
            spike_prob: 0.01,
            spike_amp: 5.0,
            noise_amp: 0.1,
            deadband: 0.05,
            hysteresis: 0.02,
            stuck_prob: 0.001,
            stuck_duration_ms: 2000,
            jitter: 15.0,
            trend: 0.0,
            drop_prob: 0.01,
            degradation_rate: 0.0,
            modbus_addr: 0,
            modbus_fn: 0,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 0,
        },
        GeneratorConfig {
            // Уровень: пила с гистерезисом, дрейф вниз
            id: "level-01".into(),
            name: "Уровень в резервуаре".into(),
            enabled: true,
            topic: "USEPI/level-01".into(),
            wave_type: WaveType::Sawtooth,
            interval_ms: 2000,
            amplitude: 45.0,
            offset: 50.0,
            frequency: 0.02,
            unit: "%".into(),
            quality: 100,
            min: 0.0,
            max: 100.0,
            drift: -0.05,
            spike_prob: 0.002,
            spike_amp: 3.0,
            noise_amp: 0.01,
            deadband: 0.5,
            hysteresis: 1.0,
            stuck_prob: 0.0,
            stuck_duration_ms: 0,
            jitter: 20.0,
            trend: 0.0,
            drop_prob: 0.002,
            degradation_rate: 0.0,
            modbus_addr: 0,
            modbus_fn: 0,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 0,
        },
        GeneratorConfig {
            // Вибрация: случайный сигнал с частыми выбросами, деградация
            id: "vibration-01".into(),
            name: "Вибрация насоса".into(),
            enabled: true,
            topic: "USEPI/vibration-01".into(),
            wave_type: WaveType::Random,
            interval_ms: 100,
            amplitude: 2.0,
            offset: 3.0,
            frequency: 0.0,
            unit: "mm/s".into(),
            quality: 85,
            min: 0.0,
            max: 15.0,
            drift: 0.0,
            spike_prob: 0.02,
            spike_amp: 6.0,
            noise_amp: 0.15,
            deadband: 0.1,
            hysteresis: 0.0,
            stuck_prob: 0.002,
            stuck_duration_ms: 1000,
            jitter: 25.0,
            trend: 0.05,
            drop_prob: 0.02,
            degradation_rate: 0.5,
            modbus_addr: 0,
            modbus_fn: 0,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 0,
        },
        GeneratorConfig {
            // Клапан: константа, мёртвая зона, редкие переключения
            id: "status-01".into(),
            name: "Состояние клапана".into(),
            enabled: true,
            topic: "USEPI/status-01".into(),
            wave_type: WaveType::Constant,
            interval_ms: 5000,
            amplitude: 1.0,
            offset: 1.0,
            frequency: 0.0,
            unit: "".into(),
            quality: 100,
            min: 0.0,
            max: 1.0,
            drift: 0.0,
            spike_prob: 0.0,
            spike_amp: 0.0,
            noise_amp: 0.0,
            deadband: 0.0,
            hysteresis: 0.0,
            stuck_prob: 0.0,
            stuck_duration_ms: 0,
            jitter: 0.0,
            trend: 0.0,
            drop_prob: 0.0,
            degradation_rate: 0.0,
            modbus_addr: 0,
            modbus_fn: 0,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 0,
        },
        // Modbus-тег: давление как Holding Register
        GeneratorConfig {
            id: "modbus-pressure".into(),
            name: "MB Давление (HR4001)".into(),
            enabled: true,
            topic: "USEPI/modbus-pressure".into(),
            wave_type: WaveType::Noise,
            interval_ms: 1000,
            amplitude: 50.0,
            offset: 500.0,
            frequency: 0.0,
            unit: "raw".into(),
            quality: 95,
            min: 0.0,
            max: 1000.0,
            drift: 0.5,
            spike_prob: 0.005,
            spike_amp: 10.0,
            noise_amp: 0.05,
            deadband: 1.0,
            hysteresis: 0.0,
            stuck_prob: 0.001,
            stuck_duration_ms: 2000,
            jitter: 5.0,
            trend: 0.0,
            drop_prob: 0.0,
            degradation_rate: 0.0,
            modbus_addr: 4001,
            modbus_fn: 3,
            modbus_type: 0,
            modbus_scale: 0.1,
            modbus_slave: 1,
        },
    ];
    // Добавляем 40 дополнительных генераторов из шаблонов
    let scenarios = [
        (
            "temp",
            "Температура",
            "°C",
            WaveType::Sine,
            1000_u64,
            10.0_f64,
            -20.0_f64,
            200.0_f64,
            0.1_f64,
        ),
        (
            "press",
            "Давление",
            "bar",
            WaveType::Noise,
            500,
            5.0,
            0.0,
            50.0,
            0.0,
        ),
        (
            "level",
            "Уровень",
            "%",
            WaveType::Sawtooth,
            2000,
            50.0,
            0.0,
            100.0,
            0.02,
        ),
        (
            "flow",
            "Расход",
            "m³/h",
            WaveType::Sine,
            1000,
            30.0,
            0.0,
            200.0,
            0.05,
        ),
        (
            "vibro",
            "Вибрация",
            "mm/s",
            WaveType::Random,
            100,
            8.0,
            0.0,
            30.0,
            0.0,
        ),
        (
            "speed",
            "Скорость",
            "rpm",
            WaveType::Sine,
            500,
            500.0,
            0.0,
            3000.0,
            0.01,
        ),
        (
            "current",
            "Ток",
            "A",
            WaveType::Sine,
            200,
            50.0,
            0.0,
            400.0,
            0.02,
        ),
        (
            "voltage",
            "Напряжение",
            "V",
            WaveType::Noise,
            300,
            20.0,
            0.0,
            250.0,
            0.0,
        ),
        (
            "power",
            "Мощность",
            "kW",
            WaveType::Sine,
            1000,
            100.0,
            0.0,
            500.0,
            0.03,
        ),
        (
            "freq",
            "Частота сети",
            "Hz",
            WaveType::Constant,
            1000,
            0.5,
            50.0,
            60.0,
            0.0,
        ),
        ("ph", "pH", "", WaveType::Noise, 2000, 2.0, 7.0, 14.0, 0.0),
        (
            "conduct",
            "Проводимость",
            "µS/cm",
            WaveType::Sine,
            1500,
            500.0,
            0.0,
            2000.0,
            0.01,
        ),
        (
            "turbid",
            "Мутность",
            "NTU",
            WaveType::Random,
            2000,
            50.0,
            0.0,
            200.0,
            0.0,
        ),
        (
            "oxy",
            "Кислород",
            "mg/l",
            WaveType::Sine,
            1000,
            4.0,
            8.0,
            15.0,
            0.05,
        ),
        (
            "co2",
            "CO₂",
            "ppm",
            WaveType::Sawtooth,
            3000,
            200.0,
            400.0,
            1000.0,
            0.01,
        ),
        (
            "humidity",
            "Влажность",
            "%",
            WaveType::Sine,
            2000,
            30.0,
            50.0,
            100.0,
            0.02,
        ),
        (
            "pressure2",
            "Давл. ресивера",
            "bar",
            WaveType::Noise,
            600,
            2.0,
            8.0,
            12.0,
            0.0,
        ),
        (
            "temp2",
            "Темп. теплоносителя",
            "°C",
            WaveType::Sine,
            1500,
            15.0,
            60.0,
            120.0,
            0.03,
        ),
        (
            "valve",
            "Положение клапана",
            "%",
            WaveType::Square,
            3000,
            50.0,
            0.0,
            100.0,
            0.01,
        ),
        (
            "weight",
            "Вес",
            "kg",
            WaveType::Noise,
            1000,
            500.0,
            0.0,
            5000.0,
            0.0,
        ),
    ];
    let mut next_id = 10;
    for scenario in scenarios.iter() {
        let (prefix, name, unit, ref wtype, interval, amp, off, mx, freq) = *scenario;
        for inst in 1..=2 {
            // 2 экземпляра каждого типа
            let id = format!("{}-{:02}", prefix, inst);
            let n = format!("{} #{}", name, inst);
            let base_quality = (70 + (inst * 15)).min(100);
            default_generators.push(GeneratorConfig {
                id,
                name: n,
                enabled: true,
                topic: format!("USEPI/{}-{:02}", prefix, inst),
                wave_type: wtype.clone(),
                interval_ms: interval,
                amplitude: amp,
                offset: off,
                frequency: freq,
                unit: unit.to_string(),
                quality: base_quality as u8,
                min: off - amp * 1.5_f64,
                max: mx.max(off + amp * 1.5_f64),
                drift: 0.0,
                spike_prob: 0.005,
                spike_amp: 3.0,
                noise_amp: 0.05,
                deadband: amp * 0.01,
                hysteresis: amp * 0.005,
                stuck_prob: 0.001,
                stuck_duration_ms: 2000,
                jitter: 10.0,
                trend: 0.0,
                drop_prob: 0.003,
                degradation_rate: 0.0,
                modbus_addr: next_id,
                modbus_fn: 3,
                modbus_type: 0,
                modbus_scale: 1.0,
                modbus_slave: 1,
            });
            next_id += 1;
        }
    }

    // Загружаем сохранённые конфиги или используем демо
    let initial_generators = persistence::load().await.unwrap_or_else(|| {
        log::info!("No saved config, using defaults");
        default_generators.clone()
    });
    if initial_generators.len() != default_generators.len() {
        log::info!("Loaded {} generators from file", initial_generators.len());
    }

    let mqtt = mqtt_client::MqttHandle::connect(&mqtt_config, "signal-generator").await;
    let mqtt_arc = Arc::new(mqtt);
    let mqtt_config_arc = Arc::new(Mutex::new(mqtt_config));

    let generators = Arc::new(Mutex::new(initial_generators));
    let current_signals: Arc<Mutex<Vec<Vec<models::FlatEntry>>>> = Arc::new(Mutex::new(Vec::new()));
    let signal_history: Arc<
        Mutex<std::collections::HashMap<String, std::collections::VecDeque<(i64, f64)>>>,
    > = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let (tx_shutdown, rx_shutdown) = tokio::sync::broadcast::channel::<String>(100);
    let started_at = Instant::now();

    let gen_generators = generators.clone();
    let gen_signals = current_signals.clone();
    let gen_history = signal_history.clone();
    let gen_mqtt = mqtt_arc.clone();
    let mut gen_rx = rx_shutdown;

    tokio::spawn(async move {
        run_generators(
            gen_generators,
            gen_signals,
            gen_history,
            gen_mqtt,
            &mut gen_rx,
        )
        .await;
    });

    // Публикация диагностики в MQTT каждые 10 секунд
    let diag_generators = generators.clone();
    let diag_mqtt = mqtt_arc.clone();
    let diag_mqtt_cfg = mqtt_config_arc.clone();
    let diag_started = started_at;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            let gens = diag_generators.lock().await;
            let total = gens.len();
            let rate: f64 = gens
                .iter()
                .filter(|g| g.enabled)
                .map(|g| 1000.0 / g.interval_ms.max(1) as f64)
                .sum();
            let diag = diagnostics::build_diagnostics(
                env!("CARGO_PKG_VERSION"),
                diag_started.elapsed().as_secs(),
                &gens,
                diag_mqtt.is_connected(),
                rate,
                total,
            );
            drop(gens);
            let topic = diag_mqtt_cfg.lock().await.diagnostics_topic.clone();
            let _ = diag_mqtt.publish(&topic, &diag).await;
        }
    });

    // API-токен: если задан SIGNAL_API_TOKEN, мутации требуют Bearer-авторизацию
    let api_token: Option<String> = std::env::var("SIGNAL_API_TOKEN")
        .ok()
        .filter(|t| !t.is_empty());
    if api_token.is_some() {
        info!("API auth enabled (SIGNAL_API_TOKEN) — mutations require Bearer token");
    } else {
        info!("API auth disabled — set SIGNAL_API_TOKEN to protect mutations");
    }

    let state = web::Data::new(AppState {
        generators: generators.clone(),
        current_signals: current_signals.clone(),
        signal_history: signal_history.clone(),
        tx_shutdown: tx_shutdown.clone(),
        mqtt_handle: mqtt_arc.clone(),
        mqtt_config: mqtt_config_arc.clone(),
        api_token,
        started_at: Instant::now(),
    });

    info!("Starting server on http://0.0.0.0:8080");
    info!("MQTT broker: {}:{}", mqtt_host, mqtt_port);

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .wrap(middleware::Logger::default())
            .configure(api::configure_routes)
            .route("/ws", web::get().to(ws::ws_index))
            .route("/", web::get().to(index_with_panels))
            .service(Files::new("/", "static").index_file("index.html"))
    })
    .bind("0.0.0.0:8080")?
    .workers(8)
    .keep_alive(std::time::Duration::from_secs(5))
    .client_request_timeout(std::time::Duration::from_secs(10))
    .run()
    .await
}

/// GET / — index.html с серверным рендерингом панелей (видно без JS)
async fn index_with_panels(state: web::Data<api::AppState>) -> actix_web::HttpResponse {
    let gens = state.generators.lock().await;
    let sigs = state.current_signals.lock().await;
    let panels_html = ui::render_panels_html(&gens, &sigs);
    drop(sigs);
    drop(gens);
    let html = match tokio::fs::read_to_string("static/index.html").await {
        Ok(h) => h,
        Err(_) => {
            return actix_web::HttpResponse::InternalServerError().body("index.html not found")
        }
    };
    let html = html.replace(
        "<!-- PANELS_PLACEHOLDER -->",
        &format!(
            "<div class=\"panels-grid\" id=\"panelsGrid\">{}</div>",
            panels_html
        ),
    );
    actix_web::HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html)
}

async fn run_generators(
    generators: Arc<Mutex<Vec<GeneratorConfig>>>,
    current_signals: Arc<Mutex<Vec<Vec<models::FlatEntry>>>>,
    signal_history: Arc<
        Mutex<std::collections::HashMap<String, std::collections::VecDeque<(i64, f64)>>>,
    >,
    mqtt: Arc<mqtt_client::MqttHandle>,
    rx: &mut tokio::sync::broadcast::Receiver<String>,
) {
    let mut handles: Vec<(String, tokio::task::JoinHandle<()>)> = Vec::new();

    loop {
        // Обработка команд из канала (update/remove/new)
        loop {
            match rx.try_recv() {
                Ok(msg) => {
                    if let Some(id) = msg
                        .strip_prefix("update:")
                        .or_else(|| msg.strip_prefix("remove:"))
                    {
                        // Убиваем старый handle генератора — он пересоздастся в следующем цикле
                        if let Some(pos) = handles.iter().position(|(hid, _)| hid == id) {
                            let (_, handle) = handles.remove(pos);
                            handle.abort();
                            info!("Restarting generator: {}", id);
                        }
                    }
                    // "new:" и "resume:"/ "pause:" — просто allow next cycle
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {}
            }
        }
        let gens = generators.lock().await;
        let active_ids: Vec<String> = gens
            .iter()
            .filter(|g| g.enabled)
            .map(|g| g.id.clone())
            .collect();

        for gen in gens.iter().filter(|g| g.enabled) {
            let exists = handles.iter().any(|(hid, handle)| {
                if hid == &gen.id {
                    if handle.is_finished() {
                        info!("Generator {} task died, restarting...", hid);
                        false // remove dead handle
                    } else {
                        true
                    }
                } else {
                    false
                }
            });
            if !exists {
                let gen_id = gen.id.clone();
                let gen_cfg = gen.clone();
                let signals = current_signals.clone();
                let history = signal_history.clone();
                let mqtt = mqtt.clone();
                let handle = tokio::spawn(async move {
                    run_single_generator(gen_cfg, signals, history, mqtt).await;
                });
                handles.push((gen_id, handle));
            }
        }

        handles.retain(|(id, _)| {
            if !active_ids.contains(id) {
                info!("Stopping generator: {}", id);
                false
            } else {
                true
            }
        });

        drop(gens);
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn run_single_generator(
    config: GeneratorConfig,
    current_signals: Arc<Mutex<Vec<Vec<models::FlatEntry>>>>,
    signal_history: Arc<
        Mutex<std::collections::HashMap<String, std::collections::VecDeque<(i64, f64)>>>,
    >,
    mqtt: Arc<mqtt_client::MqttHandle>,
) {
    let topic = config.topic.clone();
    let id = config.id.clone();
    let interval = Duration::from_millis(config.interval_ms);
    let state = GeneratorState::new(config);

    info!("Generator started: {} -> {} | wave={} drift={} spike={} noise={} deadband={} hysteresis={} stuck={} jitter={} trend={} drop={} degrade={}",
        id, topic,
        state.config.wave_type,
        state.config.drift, state.config.spike_prob, state.config.noise_amp,
        state.config.deadband, state.config.hysteresis,
        state.config.stuck_prob, state.config.jitter,
        state.config.trend, state.config.drop_prob, state.config.degradation_rate,
    );

    let state = tokio::sync::Mutex::new(state);

    // Первая публикация немедленно
    let mut first = true;
    loop {
        // Джиттер: варьируем интервал (только если включён)
        let jitter_factor = {
            let s = state.lock().await;
            if s.config.jitter > 0.0 {
                let j = s.config.jitter / 100.0;
                1.0 + (rand::random::<f64>() * 2.0 - 1.0) * j
            } else {
                1.0
            }
        };
        let actual_interval = interval.mul_f64(jitter_factor.max(0.1));

        // Стабильный тик: публикация в фиксированные моменты без накопления задержки
        if !first {
            tokio::time::sleep(actual_interval).await;
        }
        first = false;

        let value = {
            let mut s = state.lock().await;
            let dt = actual_interval.as_secs_f64();
            s.next_value(Some(dt))
        };

        if let Some(val) = value {
            let (quality, cfg) = {
                let s = state.lock().await;
                (s.quality(), s.config.clone())
            };

            let signal = if cfg.modbus_addr > 0 {
                models::build_modbus_signal(
                    &id,
                    &cfg.name,
                    val,
                    &cfg.unit,
                    cfg.min,
                    cfg.max,
                    quality,
                    cfg.modbus_addr,
                    cfg.modbus_fn,
                    cfg.modbus_type,
                    cfg.modbus_scale,
                    cfg.modbus_slave,
                )
            } else {
                models::build_signal(&id, &cfg.name, val, &cfg.unit, cfg.min, cfg.max, quality)
            };

            // Сохраняем текущее значение (весь массив)
            {
                let mut sigs = current_signals.lock().await;
                if let Some(existing) = sigs.iter_mut().find(|p| {
                    p.iter()
                        .any(|e| e.id == format!("sensor/{}", id) || e.id == id)
                }) {
                    *existing = signal.clone();
                } else {
                    sigs.push(signal.clone());
                }
            }

            // Пишем точку в историю (для осциллографа)
            {
                let mut hist = signal_history.lock().await;
                let buf = hist.entry(id.clone()).or_default();
                buf.push_back((chrono::Utc::now().timestamp_millis(), val));
                if buf.len() > api::HISTORY_CAP {
                    buf.pop_front();
                }
            }

            // Публикуем в MQTT с таймаутом (5 сек), чтобы генератор не зависал
            let publish = mqtt.publish(&topic, &signal);
            match tokio::time::timeout(Duration::from_secs(5), publish).await {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => log::warn!("MQTT publish failed for {}: {}", id, e),
                Err(_) => log::warn!("MQTT publish timeout for {} (broker slow)", id),
            }
        } else {
            log::debug!("Generator {}: packet dropped (simulated)", id);
        }
    }
}
