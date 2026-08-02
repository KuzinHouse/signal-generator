use crate::config::GeneratorConfig;
use crate::models::FlatEntry;

/// Построить JSON-LD массив диагностики системы (компактный, < 10KB)
pub fn build_diagnostics(
    version: &str,
    uptime_sec: u64,
    generators: &[GeneratorConfig],
    mqtt_connected: bool,
    signal_rate_hz: f64,
    signals_cached: usize,
) -> Vec<FlatEntry> {
    let ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    let total = generators.len();
    let active = generators.iter().filter(|g| g.enabled).count();
    let modbus_count = generators.iter().filter(|g| g.modbus_addr > 0).count();
    let avg_interval: u64 = if total > 0 {
        generators.iter().map(|g| g.interval_ms).sum::<u64>() / total as u64
    } else {
        0
    };

    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new("response_code", "Код ответа", "Success. Diagnostics"),
        FlatEntry::new("system_version", "Версия системы", version),
        FlatEntry::new("system_uptime_sec", "Аптайм (сек)", uptime_sec),
        FlatEntry::new(
            "system_mqtt",
            "MQTT подключение",
            if mqtt_connected {
                "connected"
            } else {
                "disconnected"
            },
        ),
        FlatEntry::new("system_generators_total", "Генераторов всего", total),
        FlatEntry::new("system_generators_active", "Генераторов активно", active),
        FlatEntry::new(
            "system_generators_modbus",
            "Генераторов Modbus",
            modbus_count,
        ),
        FlatEntry::new(
            "system_avg_interval_ms",
            "Средний интервал (мс)",
            avg_interval,
        ),
        FlatEntry::new(
            "system_signal_rate_hz",
            "Частота сигналов (Гц)",
            (signal_rate_hz * 10.0).round() / 10.0,
        ),
        FlatEntry::new("system_signals_cached", "Сигналов в кэше", signals_cached),
    ]
}
