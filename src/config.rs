use serde::{Deserialize, Serialize};

/// Тип волны генератора
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum WaveType {
    Sine,
    Sawtooth,
    Square,
    Noise,
    Random,
    Constant,
}

fn default_topic() -> String {
    String::new()
}

/// Конфигурация корреляции: ведомый сигнал = master_value × factor + offset.
/// Собственная волна/шум ведомого накладываются поверх коррелированной базы.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CorrelationConfig {
    /// id генератора-мастера (источник значения)
    #[serde(rename = "masterId")]
    pub master_id: String,
    /// Коэффициент связи: slave = master × factor + offset
    pub factor: f64,
    /// Смещение связи
    pub offset: f64,
}

/// Конфигурация MQTT брокера — сохраняется в config/mqtt.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MqttConfig {
    /// Адрес брокера (host или IP)
    pub host: String,
    /// Порт брокера
    pub port: u16,
    /// Имя пользователя (пусто = без auth)
    #[serde(default)]
    pub username: String,
    /// Пароль (пусто = без auth)
    #[serde(default)]
    pub password: String,
    /// Префикс топиков публикации: {prefix}/{id} (пусто = без префикса)
    #[serde(default = "default_topic")]
    pub topic_prefix: String,
    /// Топик диагностики
    #[serde(default = "default_diag_topic")]
    pub diagnostics_topic: String,
    /// Использовать TLS (порт 8883 обычно)
    #[serde(default)]
    pub use_tls: bool,
}

fn default_diag_topic() -> String {
    "USEPI/diagnostics".to_string()
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            host: "79.174.94.236".to_string(),
            port: 1883,
            username: String::new(),
            password: String::new(),
            topic_prefix: "USEPI".to_string(),
            diagnostics_topic: "USEPI/diagnostics".to_string(),
            use_tls: false,
        }
    }
}

impl MqttConfig {
    /// Полный топик для генератора: {prefix}/{id} или /{id}, если префикс пуст
    pub fn topic_for(&self, id: &str) -> String {
        if self.topic_prefix.is_empty() {
            format!("/{}", id)
        } else {
            format!("{}/{}", self.topic_prefix.trim_end_matches('/'), id)
        }
    }

    /// Полный топик с учётом каталога (субтопика): {prefix}/{catalog}/{id}.
    /// Если каталог пуст — поведение как у topic_for().
    pub fn topic_for_catalog(&self, catalog: &str, id: &str) -> String {
        let cat = catalog.trim().trim_matches('/');
        if cat.is_empty() {
            self.topic_for(id)
        } else if self.topic_prefix.is_empty() {
            format!("/{}/{}", cat, id)
        } else {
            format!("{}/{}/{}", self.topic_prefix.trim_end_matches('/'), cat, id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_for_default() {
        let cfg = MqttConfig::default();
        assert_eq!(cfg.topic_for("temp-01"), "USEPI/temp-01");
    }

    #[test]
    fn test_topic_for_custom_prefix() {
        let cfg = MqttConfig {
            topic_prefix: "PLANT-A".into(),
            ..MqttConfig::default()
        };
        assert_eq!(cfg.topic_for("valve-02"), "PLANT-A/valve-02");
    }

    #[test]
    fn test_topic_for_trailing_slash() {
        let cfg = MqttConfig {
            topic_prefix: "PLANT-A/".into(),
            ..MqttConfig::default()
        };
        assert_eq!(cfg.topic_for("valve-02"), "PLANT-A/valve-02");
    }

    #[test]
    fn test_topic_for_empty_prefix() {
        let cfg = MqttConfig {
            topic_prefix: String::new(),
            ..MqttConfig::default()
        };
        assert_eq!(cfg.topic_for("temp-01"), "/temp-01");
    }

    #[test]
    fn test_topic_for_catalog() {
        let cfg = MqttConfig::default();
        assert_eq!(
            cfg.topic_for_catalog("agronomy", "soil-moist-01"),
            "USEPI/agronomy/soil-moist-01"
        );
    }

    #[test]
    fn test_topic_for_catalog_empty() {
        let cfg = MqttConfig::default();
        assert_eq!(cfg.topic_for_catalog("", "temp-01"), "USEPI/temp-01");
        assert_eq!(
            cfg.topic_for_catalog("  ", "temp-01"),
            "USEPI/temp-01",
            "whitespace catalog must be ignored"
        );
    }

    #[test]
    fn test_topic_for_catalog_slashes_stripped() {
        let cfg = MqttConfig::default();
        assert_eq!(
            cfg.topic_for_catalog("/livestock/", "cow-01"),
            "USEPI/livestock/cow-01"
        );
    }

    #[test]
    fn test_topic_for_catalog_no_prefix() {
        let cfg = MqttConfig {
            topic_prefix: String::new(),
            ..MqttConfig::default()
        };
        assert_eq!(
            cfg.topic_for_catalog("agronomy", "soil-01"),
            "/agronomy/soil-01"
        );
    }

    #[test]
    fn test_default_config() {
        let cfg = MqttConfig::default();
        assert_eq!(cfg.host, "79.174.94.236");
        assert_eq!(cfg.port, 1883);
        assert_eq!(cfg.diagnostics_topic, "USEPI/diagnostics");
        assert!(cfg.username.is_empty());
    }

    #[test]
    fn test_correlation_default_none() {
        // старый конфиг без поля correlation десериализуется в None
        let c: GeneratorConfig = serde_json::from_str(
            r#"{"id":"x","name":"x","enabled":true,"topic":"t","catalog":"",
                "waveType":"Sine","intervalMs":1000,"amplitude":1,"offset":0,
                "frequency":0,"unit":"","quality":100,"min":0,"max":10,
                "drift":0.0,"spikeProb":0.0,"spikeAmp":0.0,"noiseAmp":0.0,
                "deadband":0.0,"hysteresis":0.0,"stuckProb":0.0,"stuckDurationMs":0,
                "jitter":0.0,"trend":0.0,"dropProb":0.0,"degradationRate":0.0}"#,
        )
        .unwrap();
        assert!(c.correlation.is_none());
    }

    #[test]
    fn test_correlation_roundtrip() {
        let mut c = GeneratorConfig::default_sine("s", "slave");
        c.correlation = Some(CorrelationConfig {
            master_id: "m-01".into(),
            factor: 2.0,
            offset: 5.0,
        });
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"masterId\":\"m-01\""), "{}", json);
        let back: GeneratorConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.correlation, c.correlation);
    }
}

impl std::fmt::Display for WaveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveType::Sine => write!(f, "Sine"),
            WaveType::Sawtooth => write!(f, "Sawtooth"),
            WaveType::Square => write!(f, "Square"),
            WaveType::Noise => write!(f, "Noise"),
            WaveType::Random => write!(f, "Random"),
            WaveType::Constant => write!(f, "Constant"),
        }
    }
}

/// Конфигурация генератора сигналов — расширенная
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorConfig {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    #[serde(default = "default_topic")]
    pub topic: String,
    /// Каталог (субтопик): {prefix}/{catalog}/{id}. Пусто = без каталога.
    #[serde(default = "default_topic")]
    pub catalog: String,
    /// Корреляция с мастер-сигналом. None = независимый генератор.
    #[serde(default)]
    pub correlation: Option<CorrelationConfig>,
    #[serde(rename = "waveType")]
    pub wave_type: WaveType,
    #[serde(rename = "intervalMs")]
    pub interval_ms: u64,
    pub amplitude: f64,
    pub offset: f64,
    pub frequency: f64,
    pub unit: String,
    pub quality: u8,
    pub min: f64,
    pub max: f64,

    // === Реалистичные эффекты ===
    /// Дрейф (ед/сек) — медленное смещение сигнала
    pub drift: f64,

    /// Вероятность выброса 0..1
    #[serde(rename = "spikeProb")]
    pub spike_prob: f64,

    /// Амплитуда выброса (множитель от amplitude)
    #[serde(rename = "spikeAmp")]
    pub spike_amp: f64,

    /// Амплитуда шума (доля от amplitude)
    #[serde(rename = "noiseAmp")]
    pub noise_amp: f64,

    /// Мёртвая зона — мин. изменение для публикации нового значения
    pub deadband: f64,

    /// Гистерезис (единиц)
    pub hysteresis: f64,

    /// Вероятность залипания 0..1
    #[serde(rename = "stuckProb")]
    pub stuck_prob: f64,

    /// Длительность залипания в мс
    #[serde(rename = "stuckDurationMs")]
    pub stuck_duration_ms: u64,

    /// Джиттер интервала (% от interval_ms)
    pub jitter: f64,

    /// Тренд (ед/сек²) — ускорение изменения offset
    pub trend: f64,

    /// Вероятность пропуска пакета 0..1
    #[serde(rename = "dropProb")]
    pub drop_prob: f64,

    /// Скорость деградации качества (ед/сек)
    #[serde(rename = "degradationRate")]
    pub degradation_rate: f64,

    // === Modbus параметры ===
    /// Modbus адрес регистра
    #[serde(rename = "modbusAddr", default)]
    pub modbus_addr: u16,

    /// Modbus функция: 1=Coil, 2=DiscreteInput, 3=HoldingRegister, 4=InputRegister
    #[serde(rename = "modbusFn", default)]
    pub modbus_fn: u8,

    /// Modbus тип данных: 0=u16, 1=i16, 2=u32, 3=i32, 4=float, 5=bool
    #[serde(rename = "modbusType", default)]
    pub modbus_type: u8,

    /// Modbus масштаб (множитель для raw→engineering)
    #[serde(rename = "modbusScale", default)]
    pub modbus_scale: f64,

    /// Modbus устройство ID (slave ID)
    #[serde(rename = "modbusSlave", default)]
    pub modbus_slave: u8,
}

impl GeneratorConfig {
    #[allow(dead_code)]
    pub fn default_sine(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            enabled: true,
            topic: format!("USEPI/{}", id),
            catalog: String::new(),
            correlation: None,
            wave_type: WaveType::Sine,
            interval_ms: 1000,
            amplitude: 10.0,
            offset: 0.0,
            frequency: 0.1,
            unit: "°C".to_string(),
            quality: 100,
            min: -50.0,
            max: 150.0,
            drift: 0.0,
            spike_prob: 0.01,
            spike_amp: 3.0,
            noise_amp: 0.05,
            deadband: 0.0,
            hysteresis: 0.0,
            stuck_prob: 0.001,
            stuck_duration_ms: 5000,
            jitter: 10.0,
            trend: 0.0,
            drop_prob: 0.0,
            degradation_rate: 0.0,
            modbus_addr: 0,
            modbus_fn: 3,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 1,
        }
    }
}
