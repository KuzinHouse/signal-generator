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
pub fn build_signal(
    id: &str,
    name: &str,
    value: f64,
    unit: &str,
    min: f64,
    max: f64,
    quality: u8,
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
        FlatEntry::new(&format!("{}_min", id), "Минимум", min),
        FlatEntry::new(&format!("{}_max", id), "Максимум", max),
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
        FlatEntry::new(&format!("{}_min", id), "Минимум", min),
        FlatEntry::new(&format!("{}_max", id), "Максимум", max),
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
    build_signal(id, name, value, unit, min, max, quality)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_signal_structure() {
        let s = build_signal("temp-01", "Температура", 25.5, "°C", 0.0, 100.0, 95);
        assert_eq!(s.len(), 7);
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
            28,
            3,
            0,
            1.0,
            1,
        );
        assert_eq!(s.len(), 12);
        let addr = s.iter().find(|e| e.id == "valve-01_modbus_addr").unwrap();
        assert_eq!(addr.entry_value, serde_json::json!(28));
        let slave = s.iter().find(|e| e.id == "valve-01_modbus_slave").unwrap();
        assert_eq!(slave.entry_value, serde_json::json!(1));
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
}
