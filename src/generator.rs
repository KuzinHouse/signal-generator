use crate::config::{GeneratorConfig, WaveType};
use rand::Rng;
use std::time::Instant;

/// Статус сигнала по порогам измерения (alarm/warning/ok/stuck)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalStatus {
    Ok,
    Warning,
    Alarm,
}

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

    // runtime: последнее значение мастера (для корреляции)
    master_value: Option<f64>,

    // runtime: аккумулятор случайного блуждания (для RandomWalk)
    walk_acc: f64,

    // runtime: накопленное время по dt (для частоты/фазы/ramp — детерминированная симуляция)
    time_acc: f64,
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
            master_value: None,
            walk_acc: 0.0,
            time_acc: 0.0,
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
        self.master_value = None;
        self.walk_acc = 0.0;
        self.time_acc = 0.0;
    }

    /// Установить последнее значение мастера (для коррелированных генераторов)
    pub fn set_master(&mut self, v: Option<f64>) {
        self.master_value = v;
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
        // Время для волны считаем как НАКОПЛЕННОЕ by dt (детерминированная симуляция),
        // а не по реальному started_at.elapsed() — иначе низкочастотные волны/ramp «стоят».
        self.time_acc += dt;
        let wave = wave_part(
            cfg,
            self.time_acc,
            &mut rng,
            &mut self.noise_extra,
            &mut self.walk_acc,
        );
        let base_value = match (&cfg.correlation, self.master_value) {
            // Корреляция: база = master × factor + offset + собственная волна/дрейф
            (Some(corr), Some(mv)) => mv * corr.factor + corr.offset + self.drift_acc + wave,
            // Без мастера — обычное поведение (offset или середина диапазона для Random)
            _ => {
                let center = match cfg.wave_type {
                    WaveType::Random => (cfg.max + cfg.min) * 0.5,
                    _ => cfg.offset,
                };
                center + self.drift_acc + wave
            }
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

    /// Статус сигнала по порогам измерения/тревог.
    /// Возвращает "ok" | "warning" | "alarm" (приоритет alarm > warning > ok).
    /// Порог считается ЗАДАННЫМ, только если Some (None = не задан).
    pub fn status(&self, current: f64) -> SignalStatus {
        let cfg = &self.config;
        let below_alarm = cfg.alarm_min.map(|v| current < v).unwrap_or(false);
        let above_alarm = cfg.alarm_max.map(|v| current > v).unwrap_or(false);
        let below_warn = cfg.warn_min.map(|v| current < v).unwrap_or(false);
        let above_warn = cfg.warn_max.map(|v| current > v).unwrap_or(false);
        if (below_alarm || above_alarm) || self.is_stuck() {
            SignalStatus::Alarm
        } else if below_warn || above_warn {
            SignalStatus::Warning
        } else {
            SignalStatus::Ok
        }
    }

    /// Флаг: залип ли генератор
    #[allow(dead_code)]
    pub fn is_stuck(&self) -> bool {
        self.stuck_until.is_some()
    }
}

/// Чистая волновая часть сигнала (без offset/центрирования и дрейфа).
/// Для Random возвращает spread × U(-1,1) — база центрируется вызывающим кодом.
fn wave_part(
    cfg: &GeneratorConfig,
    elapsed: f64,
    rng: &mut impl Rng,
    noise_extra: &mut Option<f64>,
    walk_acc: &mut f64,
) -> f64 {
    use std::f64::consts::PI;
    let phase = cfg.phase_deg.to_radians();
    match cfg.wave_type {
        WaveType::Sine => {
            cfg.amplitude * (2.0 * PI * cfg.frequency * elapsed + phase).sin()
        }
        WaveType::Sawtooth => {
            let p = (elapsed * cfg.frequency + phase / (2.0 * PI)) % 1.0;
            cfg.amplitude * (2.0 * p - 1.0)
        }
        WaveType::Square => {
            let p = (elapsed * cfg.frequency + phase / (2.0 * PI)) % 1.0;
            if p < 0.5 {
                cfg.amplitude
            } else {
                -cfg.amplitude
            }
        }
        WaveType::Triangle => {
            let p = (elapsed * cfg.frequency + phase / (2.0 * PI)) % 1.0;
            // 0..0.5 от -1 к +1, 0.5..1 от +1 к -1
            if p < 0.5 {
                cfg.amplitude * (4.0 * p - 1.0)
            } else {
                cfg.amplitude * (3.0 - 4.0 * p)
            }
        }
        WaveType::Pulse => {
            let p = (elapsed * cfg.frequency + phase / (2.0 * PI)) % 1.0;
            let duty = cfg.duty_cycle.clamp(0.01, 0.99);
            if p < duty {
                cfg.amplitude
            } else {
                -cfg.amplitude
            }
        }
        WaveType::Ramp => {
            // линейный ход: 0..step_period нарастает от -amp к +amp, потом спад
            let period = cfg.step_period.max(0.01);
            let p = (elapsed + phase / (2.0 * PI)) % (2.0 * period);
            if p < period {
                cfg.amplitude * (2.0 * (p / period) - 1.0)
            } else {
                cfg.amplitude * (3.0 - 2.0 * (p / period))
            }
        }
        WaveType::RandomWalk => {
            *walk_acc += cfg.walk_sigma * cfg.amplitude.max(1.0) * (2.0 * rng.gen::<f64>() - 1.0);
            *walk_acc
        }
        WaveType::DoubleSine => {
            let f2 = if cfg.secondary_frequency > 0.0 {
                cfg.secondary_frequency
            } else {
                cfg.frequency * 1.5
            };
            cfg.amplitude
                * (2.0 * PI * cfg.frequency * elapsed + phase).sin()
                + cfg.amplitude * 0.3 * (2.0 * PI * f2 * elapsed).sin()
        }
        WaveType::Noise => {
            // Box-Muller с запоминанием второго значения
            let n = if let Some(extra) = noise_extra.take() {
                extra
            } else {
                let u1: f64 = rng.gen();
                let u2: f64 = rng.gen();
                let n1 = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos();
                let n2 = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).sin();
                *noise_extra = Some(n2);
                n1
            };
            cfg.amplitude * 0.5 * n
        }
        WaveType::Random => {
            let spread = (cfg.max - cfg.min) * 0.5;
            spread * (2.0 * rng.gen::<f64>() - 1.0)
        }
        WaveType::Constant => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CorrelationConfig;

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
        cfg.spike_prob = 0.0; // отключаем спайки/дроп для детерминизма
        cfg.drop_prob = 0.0;
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
        cfg.noise_amp = 0.0; // отключаем шум/спайки/дроп для детерминизма
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
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
        cfg.drop_prob = 0.0; // иначе дроп может вернуть None и unwrap упадёт
        let mut state = GeneratorState::new(cfg);
        let v = state.next_value(Some(1.0)).unwrap();
        assert!(v.abs() > 5.0 || v.abs() < 5.0, "Spike magnitude: {}", v);
    }

    #[test]
    fn test_correlation_with_master() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Constant;
        cfg.amplitude = 0.0; // чистая база, без собственной волны
        cfg.offset = 0.0;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        cfg.correlation = Some(CorrelationConfig {
            master_id: "master-01".into(),
            factor: 2.0,
            offset: 5.0,
        });
        cfg.max = 1000.0; // не клиппить коррелированную базу 205
        let mut state = GeneratorState::new(cfg.clone());
        state.set_master(Some(100.0));
        let v = state.next_value(Some(0.01)).unwrap();
        assert!((v - 205.0).abs() < 0.001, "Corr base: {}", v);
        // без мастера — fallback на собственный offset
        let mut state2 = GeneratorState::new(cfg);
        let v2 = state2.next_value(Some(0.01)).unwrap();
        assert!(v2.abs() < 0.001, "No-master fallback: {}", v2);
    }

    #[test]
    fn test_correlation_sine_follows_master() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Sine;
        cfg.amplitude = 1.0;
        cfg.frequency = 0.001;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        cfg.correlation = Some(CorrelationConfig {
            master_id: "par-01".into(),
            factor: 0.012,
            offset: 19.0,
        });
        let mut state = GeneratorState::new(cfg);
        state.set_master(Some(400.0));
        let v1 = state.next_value(Some(0.01)).unwrap();
        state.set_master(Some(500.0)); // мастер вырос
        let v2 = state.next_value(Some(0.01)).unwrap();
        assert!(v2 > v1, "Slave should follow master: {} -> {}", v1, v2);
    }

    #[test]
    fn test_triangle_wave_stays_in_range() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Triangle;
        cfg.amplitude = 10.0;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        cfg.drift = 0.0;
        let mut state = GeneratorState::new(cfg);
        for _ in 0..200 {
            let v = state.next_value(Some(0.02)).unwrap();
            assert!(v.abs() <= 10.001, "Triangle out of range: {}", v);
        }
    }

    #[test]
    fn test_pulse_duty_cycle() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Pulse;
        cfg.amplitude = 10.0;
        cfg.frequency = 1.0;
        cfg.duty_cycle = 0.5;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        let mut state = GeneratorState::new(cfg);
        let mut saw_high = false;
        let mut saw_low = false;
        for _ in 0..200 {
            let v = state.next_value(Some(0.01)).unwrap();
            if v > 5.0 {
                saw_high = true;
            }
            if v < -5.0 {
                saw_low = true;
            }
        }
        assert!(saw_high && saw_low, "Pulse should toggle high/low");
    }

    #[test]
    fn test_double_sine_in_range() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::DoubleSine;
        cfg.amplitude = 5.0;
        cfg.secondary_frequency = 0.3;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        cfg.drift = 0.0;
        let mut state = GeneratorState::new(cfg);
        for _ in 0..200 {
            // амплитуда + 0.3*амплитуда = до 1.3*amp = 6.5 + клиппинг min/max
            let v = state.next_value(Some(0.02)).unwrap();
            assert!(v.abs() <= 10.001, "DoubleSine out of range: {}", v);
        }
    }

    #[test]
    fn test_ramp_changes_over_time() {
        let mut cfg = base_cfg();
        cfg.wave_type = WaveType::Ramp;
        cfg.amplitude = 10.0;
        cfg.step_period = 10.0;
        cfg.noise_amp = 0.0;
        cfg.spike_prob = 0.0;
        cfg.drop_prob = 0.0;
        cfg.drift = 0.0;
        let mut state = GeneratorState::new(cfg);
        let v1 = state.next_value(Some(0.5)).unwrap();
        let v2 = state.next_value(Some(1.0)).unwrap();
        assert!((v2 - v1).abs() > 0.1, "Ramp should move: {} -> {}", v1, v2);
    }

    #[test]
    fn test_status_alarm_warning_ok() {
        let mut cfg = base_cfg();
        cfg.warn_min = Some(10.0);
        cfg.warn_max = Some(90.0);
        cfg.alarm_min = Some(0.0);
        cfg.alarm_max = Some(100.0);
        let state = GeneratorState::new(cfg);
        // внутри нормы
        assert_eq!(state.status(50.0), SignalStatus::Ok);
        // warning: ниже warn_min, но выше alarm_min
        assert_eq!(state.status(11.0), SignalStatus::Ok);
        assert_eq!(state.status(5.0), SignalStatus::Warning);
        // alarm: выше alarm_max / ниже alarm_min
        assert_eq!(state.status(105.0), SignalStatus::Alarm);
        assert_eq!(state.status(-1.0), SignalStatus::Alarm);
    }

    #[test]
    fn test_status_none_thresholds_always_ok() {
        let cfg = base_cfg(); // пороги None
        let state = GeneratorState::new(cfg);
        assert_eq!(state.status(-999.0), SignalStatus::Ok);
        assert_eq!(state.status(999.0), SignalStatus::Ok);
    }
}
