use serde::{Deserialize, Serialize};

/// Один элемент плоского JSON-LD массива
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatEntry {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@type")]
    pub entry_type: String,
    pub name: String,
    #[serde(rename = "value")]
    pub entry_value: serde_json::Value,
}

impl FlatEntry {
    pub fn new(id: &str, name: &str, value: impl Into<serde_json::Value>) -> Self {
        Self {
            id: id.to_string(),
            entry_type: "value".to_string(),
            name: name.to_string(),
            entry_value: value.into(),
        }
    }
}

/// Весь сигнал — массив FlatEntry
pub type SignalPayload = Vec<FlatEntry>;

/// Построитель сигнала из данных генератора
#[allow(clippy::too_many_arguments)]
pub fn build_signal(
    id: &str,
    name: &str,
    value: f64,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
    warn_min: f64,
    warn_max: f64,
    alarm_min: f64,
    alarm_max: f64,
    status: &str,
) -> SignalPayload {
    let ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new(
            "response_code",
            "Код ответа",
            "Success. No Command-Specific Errors",
        ),
        FlatEntry::new(id, name, value),
        FlatEntry::new(&format!("{}_unit", id), "Единица измерения", unit),
        FlatEntry::new(&format!("{}_min", id), "Мин измерения", min),
        FlatEntry::new(&format!("{}_max", id), "Макс измерения", max),
        FlatEntry::new(&format!("{}_warn_min", id), "Порог предупр. мин", warn_min),
        FlatEntry::new(&format!("{}_warn_max", id), "Порог предупр. макс", warn_max),
        FlatEntry::new(&format!("{}_alarm_min", id), "Порог аварии мин", alarm_min),
        FlatEntry::new(&format!("{}_alarm_max", id), "Порог аварии макс", alarm_max),
        FlatEntry::new(&format!("{}_status", id), "Статус", status),
        FlatEntry::new(
            &format!("{}_quality", id),
            "Качество данных",
            quality as u64,
        ),
    ]
}

/// Построитель сигнала с Modbus полями
#[allow(clippy::too_many_arguments)]
pub fn build_modbus_signal(
    id: &str,
    name: &str,
    value: f64,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
    warning_min: f64,
    warning_max: f64,
    alarm_min: f64,
    alarm_max: f64,
    status: &str,
    modbus_addr: u16,
    modbus_fn: u8,
    modbus_type: u8,
    modbus_scale: f64,
    modbus_slave: u8,
) -> SignalPayload {
    let ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new(
            "response_code",
            "Код ответа",
            "Success. No Command-Specific Errors",
        ),
        FlatEntry::new(id, name, value),
        FlatEntry::new(&format!("{}_unit", id), "Единица измерения", unit),
        FlatEntry::new(&format!("{}_min", id), "Мин измерения", min),
        FlatEntry::new(&format!("{}_max", id), "Макс измерения", max),
        FlatEntry::new(&format!("{}_warn_min", id), "Порог предупр. мин", warning_min),
        FlatEntry::new(&format!("{}_warn_max", id), "Порог предупр. макс", warning_max),
        FlatEntry::new(&format!("{}_alarm_min", id), "Порог аварии мин", alarm_min),
        FlatEntry::new(&format!("{}_alarm_max", id), "Порог аварии макс", alarm_max),
        FlatEntry::new(&format!("{}_status", id), "Статус", status),
        FlatEntry::new(
            &format!("{}_quality", id),
            "Качество данных",
            quality as u64,
        ),
        FlatEntry::new(
            &format!("{}_modbus_addr", id),
            "Modbus адрес",
            modbus_addr as u64,
        ),
        FlatEntry::new(
            &format!("{}_modbus_fn", id),
            "Modbus функция",
            modbus_fn as u64,
        ),
        FlatEntry::new(
            &format!("{}_modbus_type", id),
            "Modbus тип данных",
            modbus_type as u64,
        ),
        FlatEntry::new(
            &format!("{}_modbus_scale", id),
            "Modbus масштаб",
            modbus_scale,
        ),
        FlatEntry::new(
            &format!("{}_modbus_slave", id),
            "Modbus slave ID",
            modbus_slave as u64,
        ),
    ]
}

/// Для обратной совместимости: last-сигнал (тот же набор)
#[allow(dead_code)]
pub fn current_signal(
    id: &str,
    name: &str,
    value: f64,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
) -> SignalPayload {
    build_signal(id, name, value, unit, min, max, quality, 0.0, 0.0, 0.0, 0.0, "ok")
}

/// Округлить число до не более 4 знаков после запятой.
/// Платформенные примеры (0.42, 12.0) — хвостовые нули не нужны,
/// serde_json отдаёт `12.0`, а не `12.000`.
pub fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

/// Идентификатор записи значения: ПОСЛЕДНИЙ сегмент топика.
///
/// Идентичность записи в BlueTraktor/УСЕПИ — это её `@id`, а не позиция.
/// Для топика `.../measurement/suctionPressure` запись имеет
/// `@id="suctionPressure"`. Хвостовой слэш игнорируется.
pub fn topic_leaf(topic: &str) -> String {
    topic
        .trim()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

/// Формат «массив записей» платформы BlueTraktor/УСЕПИ.
///
/// Порядок записей: `timestamp`, `<id>`, `<id>_unit`, `<id>_min`,
/// `<id>_max`, `quality`, где `<id>` = [`topic_leaf`] от топика.
/// `value` принимает любое JSON-значение (число/строка/bool) — для
/// дискретных и строковых сигналов формат тот же.
#[allow(clippy::too_many_arguments)]
pub fn build_records_signal(
    topic: &str,
    name: &str,
    value: serde_json::Value,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
) -> SignalPayload {
    let id = topic_leaf(topic);
    let ts = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();
    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new(&id, name, value),
        FlatEntry::new(&format!("{}_unit", id), "Единица измерения", unit),
        FlatEntry::new(&format!("{}_min", id), "Мин измерения", round4(min)),
        FlatEntry::new(&format!("{}_max", id), "Макс измерения", round4(max)),
        FlatEntry::new("quality", "Качество данных", quality as u64),
    ]
}

/// То же, что [`build_records_signal`], но для числового значения:
/// применяет [`round4`] (не более 4 знаков после запятой).
#[allow(clippy::too_many_arguments)]
pub fn build_records_signal_num(
    topic: &str,
    name: &str,
    value: f64,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
) -> SignalPayload {
    build_records_signal(
        topic,
        name,
        serde_json::json!(round4(value)),
        unit,
        min,
        max,
        quality,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_signal_structure() {
        let s = build_signal("temp-01", "Температура", 25.5, "°C", 0.0, 100.0, 95, 10.0, 90.0, 0.0, 110.0, "ok");
        assert_eq!(s.len(), 12);
        // @id обязателен у каждого элемента
        for e in &s {
            assert!(!e.id.is_empty(), "@id required");
            assert_eq!(e.entry_type, "value");
        }
        // Значение на месте
        let val = s.iter().find(|e| e.id == "temp-01").unwrap();
        assert_eq!(val.entry_value, serde_json::json!(25.5));
        // Качество
        let q = s.iter().find(|e| e.id == "temp-01_quality").unwrap();
        assert_eq!(q.entry_value, serde_json::json!(95));
        // Пороги тревог
        let wmin = s.iter().find(|e| e.id == "temp-01_warn_min").unwrap();
        assert_eq!(wmin.entry_value, serde_json::json!(10.0));
        let alarm = s.iter().find(|e| e.id == "temp-01_alarm_max").unwrap();
        assert_eq!(alarm.entry_value, serde_json::json!(110.0));
        let st = s.iter().find(|e| e.id == "temp-01_status").unwrap();
        assert_eq!(st.entry_value, serde_json::json!("ok"));
        // timestamp на месте
        assert!(s.iter().any(|e| e.id == "timestamp"));
    }

    #[test]
    fn test_build_modbus_signal_has_modbus_fields() {
        let s = build_modbus_signal(
            "valve-01",
            "Клапан",
            50.0,
            "%",
            0.0,
            100.0,
            90,
            10.0,
            90.0,
            0.0,
            110.0,
            "warning",
            28,
            3,
            0,
            1.0,
            1,
        );
        assert_eq!(s.len(), 17);
        let addr = s.iter().find(|e| e.id == "valve-01_modbus_addr").unwrap();
        assert_eq!(addr.entry_value, serde_json::json!(28));
        let slave = s.iter().find(|e| e.id == "valve-01_modbus_slave").unwrap();
        assert_eq!(slave.entry_value, serde_json::json!(1));
        let status = s.iter().find(|e| e.id == "valve-01_status").unwrap();
        assert_eq!(status.entry_value, serde_json::json!("warning"));
    }

    #[test]
    fn test_flat_entry_serialization_uses_at_id() {
        let e = FlatEntry::new("temp-01", "Температура", 25.5);
        let json = serde_json::to_string(&e).unwrap();
        assert!(
            json.contains("\"@id\":\"temp-01\""),
            "missing @id: {}",
            json
        );
        assert!(
            json.contains("\"@type\":\"value\""),
            "missing @type: {}",
            json
        );
    }

    // ===== Формат «массив записей» (BlueTraktor/УСЕПИ) =====

    #[test]
    fn test_round4_no_more_than_four_decimals() {
        assert_eq!(round4(0.423456), 0.4235);
        assert_eq!(round4(12.0), 12.0); // хвостовые нули не нужны
        assert_eq!(round4(-7.5), -7.5);
        assert_eq!(round4(1.0 / 3.0), 0.3333);
        assert_eq!(round4(0.0), 0.0);
    }

    #[test]
    fn test_topic_leaf_is_last_segment() {
        assert_eq!(topic_leaf("USEPI/temp-01"), "temp-01");
        assert_eq!(topic_leaf("USEPI/pressure/press-01"), "press-01");
        assert_eq!(topic_leaf("USEPI/temp-01/"), "temp-01"); // хвостовой слэш
        assert_eq!(topic_leaf(""), "");
    }

    #[test]
    fn test_records_id_equals_last_segment_of_nine_segment_topic() {
        // (в) топик из 9 сегментов → @id записи значения = последний сегмент
        let topic = "USEPI/plant-a/area-2/line-3/zone/measurement/sensor/pump-7/suctionPressure";
        assert_eq!(
            topic.split('/').count(),
            9,
            "тест предполагает 9 сегментов: {}",
            topic
        );
        let s = build_records_signal_num(topic, "Давление на всасе", 0.42, "bar", -7.5, 50.0, 98);
        assert_eq!(s[1].id, "suctionPressure");
        // записи-спутники строятся на том же @id
        assert_eq!(s[2].id, "suctionPressure_unit");
        assert_eq!(s[3].id, "suctionPressure_min");
        assert_eq!(s[4].id, "suctionPressure_max");
    }

    #[test]
    fn test_records_number_format() {
        // (а) формат records для number — структура и порядок как в примере
        let s =
            build_records_signal_num("USEPI/pressure/press-01", "Давление #1", 0.423456, "bar", -7.5, 50.0, 98);
        assert_eq!(s.len(), 6);
        assert_eq!(s[0].id, "timestamp");
        assert_eq!(s[1].id, "press-01");
        assert_eq!(s[2].id, "press-01_unit");
        assert_eq!(s[3].id, "press-01_min");
        assert_eq!(s[4].id, "press-01_max");
        assert_eq!(s[5].id, "quality");
        // у каждой записи @type = "value", name непустой
        for e in &s {
            assert_eq!(e.entry_type, "value");
            assert!(!e.name.is_empty());
        }
        // имена как в платформе
        assert_eq!(s[0].name, "Временная метка");
        assert_eq!(s[1].name, "Давление #1");
        assert_eq!(s[2].name, "Единица измерения");
        assert_eq!(s[3].name, "Мин измерения");
        assert_eq!(s[4].name, "Макс измерения");
        assert_eq!(s[5].name, "Качество данных");
        // значения
        assert!((s[1].entry_value.as_f64().unwrap() - 0.4235).abs() < 1e-9);
        assert_eq!(s[2].entry_value, serde_json::json!("bar"));
        assert_eq!(s[3].entry_value, serde_json::json!(-7.5));
        assert_eq!(s[4].entry_value, serde_json::json!(50.0));
        assert_eq!(s[5].entry_value, serde_json::json!(98));
    }

    #[test]
    fn test_records_string_and_bool_format() {
        // (а) формат records для string и bool — тот же массив записей
        let s = build_records_signal(
            "USEPI/valve/valve-01",
            "Положение клапана #1",
            serde_json::json!("open"),
            "",
            0.0,
            1.0,
            100,
        );
        assert_eq!(s.len(), 6);
        assert_eq!(s[1].id, "valve-01");
        assert_eq!(s[1].entry_value, serde_json::json!("open"));
        assert_eq!(s[2].entry_value, serde_json::json!(""));
        assert_eq!(s[5].entry_value, serde_json::json!(100));

        let b = build_records_signal(
            "USEPI/valve/valve-02",
            "Положение клапана #2",
            serde_json::json!(true),
            "",
            0.0,
            1.0,
            100,
        );
        assert_eq!(b[1].id, "valve-02");
        assert_eq!(b[1].entry_value, serde_json::json!(true));
    }

    #[test]
    fn test_records_timestamp_iso8601_utc_millis() {
        let s = build_records_signal_num("USEPI/t/t-01", "T", 1.0, "", 0.0, 1.0, 100);
        let ts = s[0].entry_value.as_str().unwrap();
        // ISO-8601 UTC с миллисекундами и Z: 2026-10-12T18:13:47.211Z
        assert!(ts.ends_with('Z'), "no Z suffix: {}", ts);
        assert_eq!(ts.len(), 24, "unexpected length: {}", ts);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[19..20], ".");
    }

    #[test]
    fn test_records_payload_example_output() {
        // Печатает пример реального payload (формат records), который
        // публикуется в MQTT (mqtt_client использует to_string_pretty).
        // Запуск: cargo test -- --nocapture records_payload_example_output
        let s = build_records_signal_num(
            "USEPI/pressure/press-01",
            "Давление #1",
            0.423456,
            "bar",
            -7.5,
            50.0,
            98,
        );
        println!("{}", serde_json::to_string_pretty(&s).unwrap());
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn test_records_serialization_uses_at_fields() {
        let s = build_records_signal_num("USEPI/press/press-01", "P", 0.42, "bar", -7.5, 50.0, 98);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.starts_with('['), "payload must be an array: {}", json);
        assert!(json.contains("\"@id\":\"press-01\""), "{}", json);
        assert!(json.contains("\"@id\":\"timestamp\""), "{}", json);
        assert!(json.contains("\"@id\":\"press-01_unit\""), "{}", json);
        assert!(json.contains("\"@id\":\"quality\""), "{}", json);
        assert!(json.contains("\"@type\":\"value\""), "{}", json);
    }
}
