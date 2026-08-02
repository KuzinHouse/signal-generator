use crate::config::{GeneratorConfig, MqttConfig};
use std::path::PathBuf;
use tokio::fs;

const CONFIG_FILE: &str = "config/generators.json";
const MQTT_FILE: &str = "config/mqtt.json";

/// Загрузить конфиги из файла. Если файла нет — вернуть None.
pub async fn load() -> Option<Vec<GeneratorConfig>> {
    let path = PathBuf::from(CONFIG_FILE);
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(&path).await {
        Ok(data) => match serde_json::from_str::<Vec<GeneratorConfig>>(&data) {
            Ok(configs) => {
                log::info!(
                    "Loaded {} generator configs from {}",
                    configs.len(),
                    CONFIG_FILE
                );
                Some(configs)
            }
            Err(e) => {
                log::error!("Failed to parse {}: {}", CONFIG_FILE, e);
                None
            }
        },
        Err(e) => {
            log::error!("Failed to read {}: {}", CONFIG_FILE, e);
            None
        }
    }
}

/// Сохранить конфиги в файл (атомарно через rename)
pub async fn save(generators: &[GeneratorConfig]) {
    let path = PathBuf::from(CONFIG_FILE);
    // Создаём директорию если нужно
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    // Сериализуем
    let data = match serde_json::to_string_pretty(generators) {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to serialize configs: {}", e);
            return;
        }
    };
    // Пишем во временный файл, затем переименовываем (атомарно)
    let tmp = path.with_extension("json.tmp");
    match fs::write(&tmp, &data).await {
        Ok(_) => {
            if let Err(e) = fs::rename(&tmp, &path).await {
                log::error!("Failed to rename config file: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to write config file: {}", e);
        }
    }
}

/// Загрузить MQTT конфиг. Если файла нет — вернуть None.
pub async fn load_mqtt() -> Option<MqttConfig> {
    let path = PathBuf::from(MQTT_FILE);
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(&path).await {
        Ok(data) => match serde_json::from_str::<MqttConfig>(&data) {
            Ok(cfg) => {
                log::info!("Loaded MQTT config from {}", MQTT_FILE);
                Some(cfg)
            }
            Err(e) => {
                log::error!("Failed to parse {}: {}", MQTT_FILE, e);
                None
            }
        },
        Err(e) => {
            log::error!("Failed to read {}: {}", MQTT_FILE, e);
            None
        }
    }
}

/// Сохранить MQTT конфиг (атомарно через rename)
pub async fn save_mqtt(cfg: &MqttConfig) {
    let path = PathBuf::from(MQTT_FILE);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let data = match serde_json::to_string_pretty(cfg) {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to serialize MQTT config: {}", e);
            return;
        }
    };
    let tmp = path.with_extension("json.tmp");
    match fs::write(&tmp, &data).await {
        Ok(_) => {
            if let Err(e) = fs::rename(&tmp, &path).await {
                log::error!("Failed to rename MQTT config file: {}", e);
            }
        }
        Err(e) => {
            log::error!("Failed to write MQTT config file: {}", e);
        }
    }
}
