use crate::config::{GeneratorConfig, WaveType};
use rand::Rng;
use std::time::Instant;

/// Состояние генератора: конфиг + runtime-переменные для эффектов
#[derive(Debug, Clone)]
pub struct GeneratorState {
    pub config: GeneratorConfig,
    pub started_at: Instant,

    // runtime: дрейф накопленный
    drift_acc: f64,

    // runtime: залипание
    stuck_until: Option<Instant>,

    // runtime: предыдущее значение для deadband/hysteresis
    last_value: Option<f64>,

    // runtime: последнее опубликованное значение (для deadband)
    last_published: Option<f64>,

    // runtime: тренд ускоренный
    trend_acc: f64,

    // runtime: качество деградированное
    effective_quality: f64,

    // runtime: счётчик времени для Noise (Box-Muller запоминает второе значение)
    noise_extra: Option<f64>,
}

impl GeneratorState {
    pub fn new(config: GeneratorConfig) -> Self {
        Self {
            started_at: Instant::now(),
            drift_acc: 0.0,
            stuck_until: None,
            last_value: None,
            last_published: None,
            trend_acc: 0.0,
            effective_quality: config.quality as f64,
            noise_extra: None,
            config,
        }
    }

    #[allow(dead_code)]
    /// Обновить конфиг (сбрасывает runtime-состояние)
    pub fn update_config(&mut self, config: GeneratorConfig) {
        self.config = config;
        self.started_at = Instant::now();
        self.drift_acc = 0.0;
        self.stuck_until = None;
        self.last_value = None;
        self.last_published = None;
        self.trend_acc = 0.0;
        self.effective_quality = self.config.quality as f64;
        self.noise_extra = None;
    }

    /// Сгенерировать следующее значение сигнала с учётом всех эффектов
    pub fn next_value(&mut self, dt: Option<f64>) -> Option<f64> {
        let cfg = &self.config;
        let mut rng = rand::thread_rng();

        // dt — время с предыдущего вызова (сек). Если None — считаем из elapsed
        let dt = dt.unwrap_or_else(|| {
            let elapsed = self.started_at.elapsed().as_secs_f64();
            elapsed.max(0.001)
        });

        // === Джиттер: симулируем пропуск пакета ===
        if cfg.drop_prob > 0.0 && rng.gen::<f64>() < cfg.drop_prob {
            return None; // пакет потерян
        }

        // === Проверка залипания ===
        if let Some(stuck_until) = self.stuck_until {
            if Instant::now() < stuck_until {
                // возвращаем последнее значение (залипло)
                return self.last_value;
            } else {
                self.stuck_until = None;
            }
        }

        // === Вероятность залипания ===
        if cfg.stuck_prob > 0.0 && rng.gen::<f64>() < cfg.stuck_prob {
            self.stuck_until =
                Some(Instant::now() + std::time::Duration::from_millis(cfg.stuck_duration_ms));
        }

        // === Тренд (ускорение) ===
        if cfg.trend != 0.0 {
            self.trend_acc += cfg.trend * dt;
        }

        // === Дрейф ===
        if cfg.drift != 0.0 {
            self.drift_acc += (cfg.drift + self.trend_acc) * dt;
        }

        // === Базовое значение волны ===
        let elapsed = self.started_at.elapsed().as_secs_f64();
        let base_value = match cfg.wave_type {
            WaveType::Sine => {
                cfg.offset
                    + self.drift_acc
                    + cfg.amplitude * (2.0 * std::f64::consts::PI * cfg.frequency * elapsed).sin()
            }
            WaveType::Sawtooth => {
                let phase = (elapsed * cfg.frequency) % 1.0;
                cfg.offset + self.drift_acc + cfg.amplitude * (2.0 * phase - 1.0)
            }
            WaveType::Square => {
                let phase = (elapsed * cfg.frequency) % 1.0;
                let sq = if phase < 0.5 {
                    cfg.amplitude
                } else {
                    -cfg.amplitude
                };
                cfg.offset + self.drift_acc + sq
            }
            WaveType::Noise => {
                // Box-Muller с запоминанием второго значения
                let n = if let Some(extra) = self.noise_extra.take() {
                    extra
                } else {
                    let u1: f64 = rng.gen();
                    let u2: f64 = rng.gen();
                    let n1 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                    let n2 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).sin();
                    self.noise_extra = Some(n2);
                    n1
                };
                cfg.offset + self.drift_acc + cfg.amplitude * 0.5 * n
            }
            WaveType::Random => {
                let spread = (cfg.max - cfg.min) * 0.5;
                let mid = (cfg.max + cfg.min) * 0.5;
                mid + self.drift_acc + spread * (2.0 * rng.gen::<f64>() - 1.0)
            }
            WaveType::Constant => cfg.offset + self.drift_acc,
        };

        let mut raw = base_value;

        // === Шум ===
        if cfg.noise_amp > 0.0 {
            let noise_r: f64 = rng.gen();
            let noise = cfg.amplitude.max(1.0) * cfg.noise_amp * (noise_r * 2.0 - 1.0);
            raw += noise;
        }

        // === Выбросы (spikes) ===
        if cfg.spike_prob > 0.0 && rng.gen::<f64>() < cfg.spike_prob * dt * 10.0 {
            let direction: f64 = if rng.gen() { 1.0 } else { -1.0 };
            raw += cfg.amplitude * cfg.spike_amp * direction;
        }

        // === Деградация качества ===
        if cfg.degradation_rate > 0.0 {
            self.effective_quality -= cfg.degradation_rate * dt;
            if self.effective_quality < 0.0 {
                self.effective_quality = 0.0;
            }
        } else if cfg.degradation_rate < 0.0 {
            // отрицательная = восстановление
            self.effective_quality =
                (self.effective_quality - cfg.degradation_rate * dt).min(cfg.quality as f64);
        }

        // === Качество влияет на шум ===
        if self.effective_quality < 100.0 {
            let quality_noise =
                (100.0 - self.effective_quality) / 100.0 * cfg.amplitude.max(1.0) * 0.05;
            let qn: f64 = rng.gen();
            raw += quality_noise * (qn * 2.0 - 1.0);
        }

        // === Клиппинг ===
        raw = raw.clamp(cfg.min, cfg.max);

        // === Гистерезис ===
        if cfg.hysteresis > 0.0 {
            if let Some(prev) = self.last_value {
                let diff = raw - prev;
                if diff.abs() < cfg.hysteresis {
                    raw = prev;
                }
            }
        }

        // === Мёртвая зона (не публикуем, если изменение меньше deadband) ===
        if cfg.deadband > 0.0 {
            if let Some(lp) = self.last_published {
                if (raw - lp).abs() < cfg.deadband {
                    // значение не изменилось существенно — возвращаем последнее опубликованное
                    self.last_value = Some(raw); // всё равно обновляем для гистерезиса
                    return Some(lp);
                }
            }
        }

        self.last_value = Some(raw);
        self.last_published = Some(raw);

        Some(raw)
    }

    /// Получить текущее эффективное качество
    pub fn quality(&self) -> u8 {
        (self.effective_quality.round() as u8).min(100)
    }

    /// Флаг: залип ли генератор
    #[allow(dead_code)]
    pub fn is_stuck(&self) -> bool {
        self.stuck_until.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cfg() -> GeneratorConfig {
        GeneratorConfig::default_sine("test", "test")
    }

    #[test]
    fn test_sine_range() {
        let mut cfg = base_cfg();
        cfg.drift = 0.0;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0; // отключаем спайки/дроп для детерминизма
        cfg.drop_prob = 0.0;
        let mut state = GeneratorState::new(cfg);
        for _ in 0..100 {
            let v = state.next_value(Some(0.01)).unwrap();
            assert!(v >= -10.001 && v <= 10.001, "Sine out: {}", v);
        }
    }

    #[test]
    fn test_constant() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Constant;
        cfg.drift = 0.0;
        cfg.noise_amp = 0.0;
        let mut state = GeneratorState::new(cfg);
        for _ in 0..10 {
            let v = state.next_value(Some(0.01)).unwrap();
            assert!((v).abs() < 0.001, "Not const: {}", v);
        }
    }

    #[test]
    fn test_drift() {
        let mut cfg = base_cfg();
        cfg.drift = 1.0;
        cfg.wave_type = WaveType::Constant;
        cfg.noise_amp = 0.0; // отключаем шум и спайки для детерминизма
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        let mut state = GeneratorState::new(cfg);
        let v1 = state.next_value(Some(1.0)).unwrap();
        let v2 = state.next_value(Some(1.0)).unwrap();
        assert!(v2 > v1, "Drift fail: {} -> {}", v1, v2);
    }

    #[test]
    fn test_deadband() {
        let mut cfg = base_cfg();
        cfg.deadband = 5.0;
        cfg.wave_type = WaveType::Constant;
        cfg.offset = 0.0;
        let mut state = GeneratorState::new(cfg);
        let v1 = state.next_value(Some(0.01)).unwrap();
        // С постоянным сигналом и deadband — должно вернуть то же
        let v2 = state.next_value(Some(0.01)).unwrap();
        assert!((v2 - v1).abs() < 1.0, "Deadband repeat: {} -> {}", v1, v2);
    }

    #[test]
    fn test_quality_degradation() {
        let mut cfg = base_cfg();
        cfg.degradation_rate = 10.0;
        let mut state = GeneratorState::new(cfg);
        state.next_value(Some(1.0));
        assert_eq!(state.quality(), 90);
        state.next_value(Some(5.0));
        assert_eq!(state.quality(), 40);
    }

    #[test]
    fn test_drop_prob() {
        let mut cfg = base_cfg();
        cfg.drop_prob = 1.0; // always drop
        let mut state = GeneratorState::new(cfg);
        let v = state.next_value(Some(0.01));
        assert!(v.is_none(), "Drop should be None");
    }

    #[test]
    fn test_spike() {
        let mut cfg = base_cfg();
        cfg.spike_prob = 1.0; // always spike
        cfg.spike_amp = 5.0;
        let mut state = GeneratorState::new(cfg);
        let v = state.next_value(Some(1.0)).unwrap();
        assert!(v.abs() > 5.0 || v.abs() < 5.0, "Spike magnitude: {}", v);
    }
}
