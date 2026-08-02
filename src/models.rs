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
pub fn build_signal(id: &str, name: &str, value: f64, unit: &str, min: f64, max: f64, quality: u8) -> SignalPayload {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new("response_code", "Код ответа", "Success. No Command-Specific Errors"),
        FlatEntry::new(id, name, value),
        FlatEntry::new(&format!("{}_unit", id), "Единица измерения", unit),
        FlatEntry::new(&format!("{}_min", id), "Минимум", min),
        FlatEntry::new(&format!("{}_max", id), "Максимум", max),
        FlatEntry::new(&format!("{}_quality", id), "Качество данных", quality as u64),
    ]
}

/// Построитель сигнала с Modbus полями
#[allow(clippy::too_many_arguments)]
pub fn build_modbus_signal(
    id: &str, name: &str, value: f64, unit: &str,
    min: f64, max: f64, quality: u8,
    modbus_addr: u16, modbus_fn: u8, modbus_type: u8, modbus_scale: f64, modbus_slave: u8,
) -> SignalPayload {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    vec![
        FlatEntry::new("timestamp", "Временная метка", ts),
        FlatEntry::new("response_code", "Код ответа", "Success. No Command-Specific Errors"),
        FlatEntry::new(id, name, value),
        FlatEntry::new(&format!("{}_unit", id), "Единица измерения", unit),
        FlatEntry::new(&format!("{}_min", id), "Минимум", min),
        FlatEntry::new(&format!("{}_max", id), "Максимум", max),
        FlatEntry::new(&format!("{}_quality", id), "Качество данных", quality as u64),
        FlatEntry::new(&format!("{}_modbus_addr", id), "Modbus адрес", modbus_addr as u64),
        FlatEntry::new(&format!("{}_modbus_fn", id), "Modbus функция", modbus_fn as u64),
        FlatEntry::new(&format!("{}_modbus_type", id), "Modbus тип данных", modbus_type as u64),
        FlatEntry::new(&format!("{}_modbus_scale", id), "Modbus масштаб", modbus_scale),
        FlatEntry::new(&format!("{}_modbus_slave", id), "Modbus slave ID", modbus_slave as u64),
    ]
}

/// Для обратной совместимости: last-сигнал (тот же набор)
#[allow(dead_code)]
pub fn current_signal(id: &str, name: &str, value: f64, unit: &str, min: f64, max: f64, quality: u8) -> SignalPayload {
    build_signal(id, name, value, unit, min, max, quality)
}
