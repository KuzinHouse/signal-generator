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
    /// Треугольная волна (ходы штоков, плавное нарастание-спад)
    Triangle,
    /// Импульсная с настраиваемым duty_cycle (дозирование, ШИМ-клапан)
    Pulse,
    /// Линейный нарастающий/спадающий (загрузка/разгрузка резервуара)
    Ramp,
    /// Случайное блуждание (уровни, концентрации, pH) с walk_sigma
    RandomWalk,
    /// Биения двух синусов с secondary_frequency (вибрация машин, насосы)
    DoubleSine,
}

fn default_topic() -> String {
    String::new()
}

fn default_topic_prefix() -> String {
    "USEPI".to_string()
}

fn default_diag_topic() -> String {
    "USEPI/diagnostics".to_string()
}

fn default_port() -> u16 {
    1883
}

/// Формат публикуемого MQTT-payload.
///
/// - `flat`    — плоский JSON-LD массив (текущее поведение, по умолчанию);
/// - `records` — массив записей платформы BlueTraktor/УСЕПИ
///   (`@id`, `@type`, `name`, `value`), где `@id` записи значения равен
///   последнему сегменту топика.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PayloadFormat {
    /// Плоский JSON-LD (по умолчанию)
    Flat,
    /// Массив записей BlueTraktor/УСЕПИ
    Records,
}

impl Default for PayloadFormat {
    fn default() -> Self {
        PayloadFormat::Flat
    }
}

/// Конфигурация ОДНОГО MQTT брокера (мультиброкерная публикация).
///
/// Сохраняется в config/brokers.json. Каждый включённый брокер получает
/// КОПИЮ каждого сигнала на своём префиксе топиков, независимо от остальных.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    /// Уникальный id брокера (например "local", "remote")
    pub id: String,
    /// Человекочитаемое имя для UI
    #[serde(default)]
    pub name: String,
    /// Включён ли брокер (true = публиковать в него)
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Адрес брокера (host или IP)
    pub host: String,
    /// Порт брокера
    #[serde(default = "default_port")]
    pub port: u16,
    /// Имя пользователя (пусто = без auth)
    #[serde(default)]
    pub username: String,
    /// Пароль (пусто = без auth)
    #[serde(default)]
    pub password: String,
    /// Префикс топиков публикации: {prefix}/{id} (пусто = без префикса)
    #[serde(default = "default_topic_prefix")]
    pub topic_prefix: String,
    /// Топик диагностики
    #[serde(default = "default_diag_topic")]
    pub diagnostics_topic: String,
    /// Использовать TLS (порт 8883 обычно)
    #[serde(default)]
    pub use_tls: bool,
}

fn default_true() -> bool {
    true
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            id: "local".into(),
            name: "Локальный EMQX".into(),
            enabled: true,
            host: "127.0.0.1".into(),
            port: 1883,
            username: String::new(),
            password: String::new(),
            topic_prefix: default_topic_prefix(),
            diagnostics_topic: default_diag_topic(),
            use_tls: false,
        }
    }
}

impl BrokerConfig {
    /// Полный топик для генератора: {prefix}/{id} или /{id}, если префикс пуст
    pub fn topic_for(&self, id: &str) -> String {
        if self.topic_prefix.is_empty() {
            format!("/{}", id)
        } else {
            format!("{}/{}", self.topic_prefix.trim_end_matches('/'), id)
        }
    }

    /// Полный топик с учётом каталога (субтопика): {prefix}/{catalog}/{id}.
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

/// Конфигурация MQTT — СПИСОК брокеров (мультиброкерная публикация)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MqttConfig {
    /// Все брокеры. Публикация идёт параллельно во включённые.
    #[serde(default)]
    pub brokers: Vec<BrokerConfig>,
}

impl MqttConfig {
    /// Включённые брокеры
    pub fn enabled_brokers(&self) -> Vec<BrokerConfig> {
        self.brokers.iter().filter(|b| b.enabled).cloned().collect()
    }

    /// Первый включённый брокер — главный (для legacy-совместимости / панелей).
    pub fn primary(&self) -> Option<BrokerConfig> {
        self.enabled_brokers().into_iter().next()
    }

    /// Главный брокер или дефолт (если ни один не включён)
    pub fn primary_or_default(&self) -> BrokerConfig {
        self.primary().unwrap_or_else(BrokerConfig::default)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn broker() -> BrokerConfig {
        BrokerConfig {
            id: "local".into(),
            name: "Локальный".into(),
            enabled: true,
            host: "127.0.0.1".into(),
            port: 1883,
            username: String::new(),
            password: String::new(),
            topic_prefix: "USEPI".into(),
            diagnostics_topic: "USEPI/diagnostics".into(),
            use_tls: false,
        }
    }

    #[test]
    fn test_topic_for_default() {
        let cfg = broker();
        assert_eq!(cfg.topic_for("temp-01"), "USEPI/temp-01");
    }

    #[test]
    fn test_topic_for_custom_prefix() {
        let mut cfg = broker();
        cfg.topic_prefix = "PLANT-A".into();
        assert_eq!(cfg.topic_for("valve-02"), "PLANT-A/valve-02");
    }

    #[test]
    fn test_topic_for_trailing_slash() {
        let mut cfg = broker();
        cfg.topic_prefix = "PLANT-A/".into();
        assert_eq!(cfg.topic_for("valve-02"), "PLANT-A/valve-02");
    }

    #[test]
    fn test_topic_for_empty_prefix() {
        let mut cfg = broker();
        cfg.topic_prefix = String::new();
        assert_eq!(cfg.topic_for("temp-01"), "/temp-01");
    }

    #[test]
    fn test_topic_for_catalog() {
        let cfg = broker();
        assert_eq!(
            cfg.topic_for_catalog("agronomy", "soil-moist-01"),
            "USEPI/agronomy/soil-moist-01"
        );
    }

    #[test]
    fn test_topic_for_catalog_empty() {
        let cfg = broker();
        assert_eq!(cfg.topic_for_catalog("", "temp-01"), "USEPI/temp-01");
        assert_eq!(
            cfg.topic_for_catalog("  ", "temp-01"),
            "USEPI/temp-01",
            "whitespace catalog must be ignored"
        );
    }

    #[test]
    fn test_topic_for_catalog_slashes_stripped() {
        let cfg = broker();
        assert_eq!(
            cfg.topic_for_catalog("/livestock/", "cow-01"),
            "USEPI/livestock/cow-01"
        );
    }

    #[test]
    fn test_topic_for_catalog_no_prefix() {
        let mut cfg = broker();
        cfg.topic_prefix = String::new();
        assert_eq!(
            cfg.topic_for_catalog("agronomy", "soil-01"),
            "/agronomy/soil-01"
        );
    }

    #[test]
    fn test_default_config() {
        let cfg = BrokerConfig::default();
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 1883);
        assert_eq!(cfg.diagnostics_topic, "USEPI/diagnostics");
        assert!(cfg.username.is_empty());
        assert!(cfg.enabled);
    }

    #[test]
    fn test_mqtt_config_enabled_brokers() {
        let mut local = BrokerConfig::default();
        local.id = "local".into();
        let mut remote = BrokerConfig::default();
        remote.id = "remote".into();
        remote.host = "79.174.94.236".into();
        remote.enabled = false;
        let cfg = MqttConfig {
            brokers: vec![local, remote],
        };
        let enabled = cfg.enabled_brokers();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "local");
        assert_eq!(cfg.primary().unwrap().id, "local");
    }

    #[test]
    fn test_mqtt_config_empty_primary_fallback() {
        let cfg = MqttConfig::default(); // пустой список
        assert!(cfg.primary().is_none());
        assert_eq!(cfg.primary_or_default().host, "127.0.0.1");
    }

    #[test]
    fn test_mqtt_config_roundtrip() {
        let cfg = MqttConfig {
            brokers: vec![
                BrokerConfig::default(),
                BrokerConfig {
                    id: "remote".into(),
                    name: "Внешний".into(),
                    enabled: true,
                    host: "79.174.94.236".into(),
                    port: 1883,
                    username: String::new(),
                    password: String::new(),
                    topic_prefix: "USEPI".into(),
                    diagnostics_topic: "USEPI/diagnostics".into(),
                    use_tls: false,
                },
            ],
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("remote"));
        let back: MqttConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.brokers.len(), 2);
        assert!(back.brokers[1].enabled);
        assert_eq!(back.enabled_brokers().len(), 2);
    }

    #[test]
    fn test_broker_config_deserialize_partial() {
        // поля с дефолтами должны подхватываться
        let c: BrokerConfig = serde_json::from_str(
            r#"{"id":"x","host":"1.2.3.4"}"#,
        )
        .unwrap();
        assert_eq!(c.port, 1883);
        assert_eq!(c.topic_prefix, "USEPI");
        assert!(c.enabled);
        assert!(!c.use_tls);
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

    #[test]
    fn test_payload_format_default_is_flat() {
        // (б) дефолт payloadFormat = flat
        assert_eq!(PayloadFormat::default(), PayloadFormat::Flat);
        let c = GeneratorConfig::default_sine("x", "x");
        assert_eq!(c.payload_format, PayloadFormat::Flat);
    }

    #[test]
    fn test_payload_format_serde_field_names() {
        // поле сериализуется как "payloadFormat" со строчными значениями
        let c = GeneratorConfig::default_sine("x", "x");
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"payloadFormat\":\"flat\""), "{}", json);

        let mut c2 = GeneratorConfig::default_sine("x", "x");
        c2.payload_format = PayloadFormat::Records;
        let json2 = serde_json::to_string(&c2).unwrap();
        assert!(json2.contains("\"payloadFormat\":\"records\""), "{}", json2);
    }

    #[test]
    fn test_payload_format_records_deserialize() {
        let c: GeneratorConfig = serde_json::from_str(
            r#"{"id":"x","name":"x","enabled":true,"topic":"t","catalog":"",
                "payloadFormat":"records",
                "waveType":"Sine","intervalMs":1000,"amplitude":1,"offset":0,
                "frequency":0,"unit":"","quality":100,"min":0,"max":10,
                "drift":0.0,"spikeProb":0.0,"spikeAmp":0.0,"noiseAmp":0.0,
                "deadband":0.0,"hysteresis":0.0,"stuckProb":0.0,"stuckDurationMs":0,
                "jitter":0.0,"trend":0.0,"dropProb":0.0,"degradationRate":0.0}"#,
        )
        .unwrap();
        assert_eq!(c.payload_format, PayloadFormat::Records);
    }

    #[test]
    fn test_payload_format_absent_is_flat_backward_compat() {
        // (г) обратная совместимость: json без payloadFormat десериализуется как flat
        let c: GeneratorConfig = serde_json::from_str(
            r#"{"id":"x","name":"x","enabled":true,"topic":"t","catalog":"",
                "waveType":"Sine","intervalMs":1000,"amplitude":1,"offset":0,
                "frequency":0,"unit":"","quality":100,"min":0,"max":10,
                "drift":0.0,"spikeProb":0.0,"spikeAmp":0.0,"noiseAmp":0.0,
                "deadband":0.0,"hysteresis":0.0,"stuckProb":0.0,"stuckDurationMs":0,
                "jitter":0.0,"trend":0.0,"dropProb":0.0,"degradationRate":0.0}"#,
        )
        .unwrap();
        assert_eq!(c.payload_format, PayloadFormat::Flat);
    }

    #[test]
    fn test_sanitize_normalizes_bad_input() {
        let mut c = GeneratorConfig::default_sine("x", "x");
        // невалидные значения, которые ломали генерацию
        c.interval_ms = 0;
        c.frequency = 1000.0; // 1000 Гц — вне физического диапазона
        c.secondary_frequency = -5.0;
        c.amplitude = -20.0; // отрицательная
        c.phase_deg = 720.0;
        c.duty_cycle = 0.0;
        c.walk_sigma = 2.0;
        c.step_period = 0.0;
        c.drop_prob = 3.0; // > 1
        c.min = 100.0;
        c.max = 0.0; // min > max (перевёрнуто)
        c.sanitize();
        assert_eq!(c.interval_ms, 100, "interval должно быть clamped к 100");
        assert_eq!(c.frequency, 50.0, "frequency должно быть clamped к 50");
        assert_eq!(c.secondary_frequency, 0.0);
        assert!(c.amplitude > 0.0, "amplitude неотрицательна");
        assert!(c.phase_deg <= 360.0);
        assert!(c.duty_cycle >= 0.01 && c.duty_cycle <= 0.99);
        assert!(c.walk_sigma <= 1.0);
        assert!(c.step_period >= 0.1);
        assert!(c.drop_prob <= 1.0);
        assert!(c.min <= c.max, "min должно быть <= max после sanitize");
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
            WaveType::Triangle => write!(f, "Triangle"),
            WaveType::Pulse => write!(f, "Pulse"),
            WaveType::Ramp => write!(f, "Ramp"),
            WaveType::RandomWalk => write!(f, "RandomWalk"),
            WaveType::DoubleSine => write!(f, "DoubleSine"),
        }
    }
}

fn default_phase() -> f64 {
    0.0
}
fn default_duty() -> f64 {
    0.5
}
fn default_walk() -> f64 {
    0.1
}
fn default_step() -> f64 {
    10.0
}
fn default_secfreq() -> f64 {
    0.0
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

    /// Формат публикуемого payload: "flat" (по умолчанию) | "records".
    /// Дефолт `flat` — старые config/generators.json без поля не ломаются.
    #[serde(rename = "payloadFormat", default)]
    pub payload_format: PayloadFormat,

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

    // === Волны: параметры новых типов ===
    /// Фазовый сдвиг (градусы) для Sine/Triangle/Pulse/DoubleSine
    #[serde(rename = "phaseDeg", default = "default_phase")]
    pub phase_deg: f64,

    /// Коэффициент заполнения 0..1 для Pulse (доля «высокого» уровня)
    #[serde(rename = "dutyCycle", default = "default_duty")]
    pub duty_cycle: f64,

    /// Шаг случайного блуждания (доля от amplitude) для RandomWalk
    #[serde(rename = "walkSigma", default = "default_walk")]
    pub walk_sigma: f64,

    /// Период нарастания/спада (сек) для Ramp
    #[serde(rename = "stepPeriod", default = "default_step")]
    pub step_period: f64,

    /// Частота второй гармоники для DoubleSine (биения)
    #[serde(rename = "secondaryFrequency", default = "default_secfreq")]
    pub secondary_frequency: f64,

    // === Пределы измерения и пороги тревог ===
    /// Порог предупреждения МИН (None = не задан)
    #[serde(rename = "warnMin", default)]
    pub warn_min: Option<f64>,
    /// Порог предупреждения МАКС (None = не задан)
    #[serde(rename = "warnMax", default)]
    pub warn_max: Option<f64>,
    /// Порог аварии МИН (None = не задан)
    #[serde(rename = "alarmMin", default)]
    pub alarm_min: Option<f64>,
    /// Порог аварии МАКС (None = не задан)
    #[serde(rename = "alarmMax", default)]
    pub alarm_max: Option<f64>,

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
            payload_format: PayloadFormat::Flat,
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
            phase_deg: 0.0,
            duty_cycle: 0.5,
            walk_sigma: 0.1,
            step_period: 10.0,
            secondary_frequency: 0.0,
            warn_min: None,
            warn_max: None,
            alarm_min: None,
            alarm_max: None,
            modbus_addr: 0,
            modbus_fn: 3,
            modbus_type: 0,
            modbus_scale: 1.0,
            modbus_slave: 1,
        }
    }

    /// Нормализовать поля в безопасные диапазоны (валидация ввода).
    /// Предотвращает «поломку» генерации при недопустимых значениях
    /// (напр. frequency=1000 Гц, intervalMs=0, dutyCycle вне 0..1).
    pub fn sanitize(&mut self) {
        use std::f64::consts::PI;
        // интервал: минимум 100мс, максимум 1 час (защита от 0 и отрицательных)
        self.interval_ms = self.interval_ms.clamp(100, 3_600_000);
        // частота основной волны: 0..50 Гц в физическом диапазоне
        self.frequency = self.frequency.clamp(0.0, 50.0);
        // вторая частота (биения): 0..50 Гц
        self.secondary_frequency = self.secondary_frequency.clamp(0.0, 50.0);
        // амплитуда неотрицательна, минимум 0.001 чтобы не делить на 0
        self.amplitude = self.amplitude.abs().max(0.001);
        // фаза 0..360
        self.phase_deg = self.phase_deg.clamp(0.0, 360.0);
        // duty cycle строго 0.01..0.99 (иначе Pulse вырождается)
        self.duty_cycle = self.duty_cycle.clamp(0.01, 0.99);
        // walk_sigma 0..1
        self.walk_sigma = self.walk_sigma.clamp(0.0, 1.0);
        // период хода ramp: минимум 0.1с
        self.step_period = self.step_period.abs().max(0.1);
        // вероятности 0..1
        self.spike_prob = self.spike_prob.clamp(0.0, 1.0);
        self.noise_amp = self.noise_amp.clamp(0.0, 1.0);
        self.stuck_prob = self.stuck_prob.clamp(0.0, 1.0);
        self.drop_prob = self.drop_prob.clamp(0.0, 1.0);
        // джиттер 0..100 (%)
        self.jitter = self.jitter.clamp(0.0, 100.0);
        // качество 0..100
        self.quality = self.quality.min(100);
        // пределы измерения: min <= max
        if self.min > self.max {
            std::mem::swap(&mut self.min, &mut self.max);
        }
        // дрейф/тренд/деградация не имеют «потолка», но не должны быть NaN
        for v in [
            &mut self.drift,
            &mut self.trend,
            &mut self.degradation_rate,
            &mut self.deadband,
            &mut self.hysteresis,
            &mut self.spike_amp,
        ] {
            if !v.is_finite() {
                *v = 0.0;
            }
            *v = v.abs();
        }
        // пороги тревог: если заданы — нормализуем по модулю; min ≤ max
        if let Some(v) = &mut self.warn_min {
            if !v.is_finite() { *v = 0.0; }
        }
        if let Some(v) = &mut self.warn_max {
            if !v.is_finite() { *v = 0.0; }
        }
        if let Some(v) = &mut self.alarm_min {
            if !v.is_finite() { *v = 0.0; }
        }
        if let Some(v) = &mut self.alarm_max {
            if !v.is_finite() { *v = 0.0; }
        }
        if let (Some(lo), Some(hi)) = (&mut self.warn_min, &mut self.warn_max) {
            if lo > hi { std::mem::swap(lo, hi); }
        }
        if let (Some(lo), Some(hi)) = (&mut self.alarm_min, &mut self.alarm_max) {
            if lo > hi { std::mem::swap(lo, hi); }
        }
        // modbus адрес не может быть неотрицательным (u16 уже), но масштаб >0
        self.modbus_scale = self.modbus_scale.abs().max(0.0001);
        let _ = PI;
    }
}
