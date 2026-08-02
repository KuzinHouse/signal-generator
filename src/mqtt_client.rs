use log::{info, warn};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Построить TLS-транспорт для rumqttc (доверяет системным корням ОС)
fn tls_transport() -> Transport {
    let mut roots = rustls::RootCertStore::empty();
    if let Ok(certs) = rustls_native_certs::load_native_certs() {
        for cert in certs {
            let _ = roots.add(cert);
        }
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Transport::Tls(TlsConfiguration::Rustls(Arc::new(config)))
}

/// MQTT клиент для публикации сигналов + $SYS мониторинг
pub struct MqttHandle {
    client: Arc<Mutex<AsyncClient>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    broker_status: Arc<Mutex<BrokerStatus>>,
    broker_history: Arc<Mutex<BrokerHistory>>,
}

/// Расширенный статус брокера
#[derive(Debug, Clone, Serialize)]
pub struct BrokerStatus {
    pub version: String,
    pub uptime: u64,
    pub clients_connected: u32,
    pub clients_max: u32,
    pub clients_expired: u32,
    pub clients_disconnected: u32,
    pub clients_total: u32,
    pub subscriptions: u32,
    pub topics: u32,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub heap_used: u64,
    pub heap_max: u64,
    pub retained_count: u32,
    pub store_count: u32,
    pub load_connections: f64,
    pub load_sessions: f64,
    pub load_sockets: f64,
    pub last_update: String,
}

impl BrokerStatus {
    fn new() -> Self {
        Self {
            version: "—".into(),
            uptime: 0,
            clients_connected: 0,
            clients_max: 0,
            clients_expired: 0,
            clients_disconnected: 0,
            clients_total: 0,
            subscriptions: 0,
            topics: 0,
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            heap_used: 0,
            heap_max: 0,
            retained_count: 0,
            store_count: 0,
            load_connections: 0.0,
            load_sessions: 0.0,
            load_sockets: 0.0,
            last_update: chrono::Utc::now().format("%H:%M:%S").to_string(),
        }
    }

    fn parse_sys(&mut self, topic: &str, payload: &str) {
        self.last_update = chrono::Utc::now().format("%H:%M:%S").to_string();
        match topic {
            t if t.ends_with("/version") => self.version = payload.into(),
            t if t.ends_with("/uptime") => self.uptime = payload.parse().unwrap_or(self.uptime),
            t if t.ends_with("/clients/connected") => {
                self.clients_connected = payload.parse().unwrap_or(self.clients_connected)
            }
            t if t.ends_with("/clients/maximum") => {
                self.clients_max = payload.parse().unwrap_or(self.clients_max)
            }
            t if t.ends_with("/clients/expired") => {
                self.clients_expired = payload.parse().unwrap_or(self.clients_expired)
            }
            t if t.ends_with("/clients/disconnected") => {
                self.clients_disconnected = payload.parse().unwrap_or(self.clients_disconnected)
            }
            t if t.ends_with("/clients/total") => {
                self.clients_total = payload.parse().unwrap_or(self.clients_total)
            }
            t if t.ends_with("/subscriptions/count") => {
                self.subscriptions = payload.parse().unwrap_or(self.subscriptions)
            }
            t if t.ends_with("/topics/count") => {
                self.topics = payload.parse().unwrap_or(self.topics)
            }
            t if t.ends_with("/messages/sent") => {
                self.messages_sent = payload.parse().unwrap_or(self.messages_sent)
            }
            t if t.ends_with("/messages/received") => {
                self.messages_received = payload.parse().unwrap_or(self.messages_received)
            }
            t if t.ends_with("/bytes/sent") => {
                self.bytes_sent = payload.parse().unwrap_or(self.bytes_sent)
            }
            t if t.ends_with("/bytes/received") => {
                self.bytes_received = payload.parse().unwrap_or(self.bytes_received)
            }
            t if t.ends_with("/heap/current") => {
                self.heap_used = payload.parse().unwrap_or(self.heap_used)
            }
            t if t.ends_with("/heap/maximum") => {
                self.heap_max = payload.parse().unwrap_or(self.heap_max)
            }
            t if t.ends_with("/retained messages/count") => {
                self.retained_count = payload.parse().unwrap_or(self.retained_count)
            }
            t if t.ends_with("/store/messages/count") => {
                self.store_count = payload.parse().unwrap_or(self.store_count)
            }
            t if t.ends_with("/load/connections") => {
                self.load_connections = payload.parse().unwrap_or(self.load_connections)
            }
            t if t.ends_with("/load/sessions") => {
                self.load_sessions = payload.parse().unwrap_or(self.load_sessions)
            }
            t if t.ends_with("/load/sockets") => {
                self.load_sockets = payload.parse().unwrap_or(self.load_sockets)
            }
            _ => {}
        }
    }
}

/// Снимок метрик для истории
#[derive(Debug, Clone, Serialize)]
pub struct BrokerSnapshot {
    pub time: String,
    pub clients: u32,
    pub msg_rate: f64,  // сообщений/с (сумма sent+received дельта)
    pub byte_rate: f64, // байт/с
    pub heap_pct: f64,  // % использования heap
}

/// Кольцевой буфер истории метрик брокера
pub struct BrokerHistory {
    buffer: Vec<BrokerSnapshot>,
    capacity: usize,
    last_msg_total: u64,
    last_byte_total: u64,
    last_record_time: Instant,
}

impl BrokerHistory {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            capacity,
            last_msg_total: 0,
            last_byte_total: 0,
            last_record_time: Instant::now(),
        }
    }

    pub fn record(&mut self, status: &BrokerStatus) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_record_time)
            .as_secs_f64()
            .max(0.1);

        let msg_total = status.messages_sent + status.messages_received;
        let byte_total = status.bytes_sent + status.bytes_received;

        let msg_rate = (msg_total - self.last_msg_total) as f64 / dt;
        let byte_rate = (byte_total - self.last_byte_total) as f64 / dt;
        let heap_pct = if status.heap_max > 0 {
            (status.heap_used as f64 / status.heap_max as f64) * 100.0
        } else {
            0.0
        };

        self.last_msg_total = msg_total;
        self.last_byte_total = byte_total;
        self.last_record_time = now;

        let snap = BrokerSnapshot {
            time: status.last_update.clone(),
            clients: status.clients_connected,
            msg_rate: (msg_rate * 10.0).round() / 10.0,
            byte_rate: (byte_rate * 10.0).round() / 10.0,
            heap_pct: (heap_pct * 10.0).round() / 10.0,
        };

        if self.buffer.len() >= self.capacity {
            self.buffer.remove(0);
        }
        self.buffer.push(snap);
    }

    pub fn snapshot(&self) -> Vec<BrokerSnapshot> {
        self.buffer.clone()
    }

    /// Текущие скорости (для быстрого доступа)
    #[allow(dead_code)]
    pub fn current_rates(&self) -> (f64, f64) {
        self.buffer
            .last()
            .map(|s| (s.msg_rate, s.byte_rate))
            .unwrap_or((0.0, 0.0))
    }
}

impl MqttHandle {
    pub async fn connect(cfg: &crate::config::MqttConfig, client_id: &str) -> Self {
        let mut mqttopts = MqttOptions::new(client_id, &cfg.host, cfg.port);
        mqttopts.set_keep_alive(Duration::from_secs(30));
        mqttopts.set_clean_session(true);
        if !cfg.username.is_empty() {
            mqttopts.set_credentials(&cfg.username, &cfg.password);
        }
        if cfg.use_tls {
            mqttopts.set_transport(tls_transport());
            info!("MQTT TLS enabled for {}:{}", cfg.host, cfg.port);
        }

        let (client, mut eventloop) = AsyncClient::new(mqttopts, 100);
        let client = Arc::new(Mutex::new(client));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let connected_clone = connected.clone();
        let broker_status = Arc::new(Mutex::new(BrokerStatus::new()));
        let broker_history = Arc::new(Mutex::new(BrokerHistory::new(120)));
        let bs_clone = broker_status.clone();
        let bh_clone = broker_history.clone();

        // Подписка на $SYS/#
        {
            let c = client.lock().await;
            if let Err(e) = c.subscribe("$SYS/#", QoS::AtMostOnce).await {
                warn!("Failed to subscribe to $SYS/#: {:?}", e);
            }
        }

        // Фоновая задача: обработка событий MQTT
        tokio::spawn(async move {
            let mut last_record = Instant::now();
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        connected_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                        info!("MQTT connected to broker");
                    }
                    Ok(Event::Incoming(Packet::Publish(pub_))) => {
                        let topic = pub_.topic.clone();
                        if topic.starts_with("$SYS/") {
                            let payload = String::from_utf8_lossy(&pub_.payload).to_string();
                            let mut bs = bs_clone.lock().await;
                            bs.parse_sys(&topic, &payload);
                        }
                    }
                    Ok(Event::Incoming(_)) => {}
                    Ok(Event::Outgoing(_)) => {}
                    Err(e) => {
                        connected_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                        warn!("MQTT error: {:?}", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }

                // Каждые 5 секунд записываем снимок в историю
                if last_record.elapsed() >= Duration::from_secs(5) {
                    let bs = bs_clone.lock().await;
                    let mut bh = bh_clone.lock().await;
                    bh.record(&bs);
                    last_record = Instant::now();
                }
            }
        });

        Self {
            client,
            connected,
            broker_status,
            broker_history,
        }
    }

    pub async fn publish(
        &self,
        topic: &str,
        signal: &[crate::models::FlatEntry],
    ) -> Result<(), String> {
        let payload = serde_json::to_string_pretty(signal).map_err(|e| e.to_string())?;
        let client = self.client.lock().await;
        client
            .publish(topic, QoS::AtLeastOnce, true, payload)
            .await
            .map_err(|e| format!("MQTT publish error: {}", e))
    }

    pub async fn get_broker_status(&self) -> BrokerStatus {
        self.broker_status.lock().await.clone()
    }

    pub async fn get_broker_history(&self) -> Vec<BrokerSnapshot> {
        self.broker_history.lock().await.snapshot()
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Пересоздать MQTT клиент с новыми настройками (для hot-reload конфига)
    pub async fn reconnect(&self, cfg: &crate::config::MqttConfig) {
        let cfg = cfg.clone();
        let mut mqttopts = MqttOptions::new("signal-generator", &cfg.host, cfg.port);
        mqttopts.set_keep_alive(Duration::from_secs(30));
        mqttopts.set_clean_session(true);
        if !cfg.username.is_empty() {
            mqttopts.set_credentials(&cfg.username, &cfg.password);
        }
        if cfg.use_tls {
            mqttopts.set_transport(tls_transport());
            info!("MQTT TLS enabled for {}:{}", cfg.host, cfg.port);
        }

        let (client, mut eventloop) = AsyncClient::new(mqttopts, 100);
        // Заменяем клиент
        {
            let mut c = self.client.lock().await;
            *c = client;
            if let Err(e) = c.subscribe("$SYS/#", QoS::AtMostOnce).await {
                warn!("Failed to resubscribe $SYS/#: {:?}", e);
            }
        }
        self.connected
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let connected_clone = self.connected.clone();
        let bs_clone = self.broker_status.clone();
        let bh_clone = self.broker_history.clone();
        // сброс истории при смене брокера
        {
            let mut bh = bh_clone.lock().await;
            *bh = BrokerHistory::new(120);
        }

        tokio::spawn(async move {
            let mut last_record = Instant::now();
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        connected_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                        info!("MQTT reconnected to broker {}:{}", cfg.host, cfg.port);
                    }
                    Ok(Event::Incoming(Packet::Publish(pub_))) => {
                        let topic = pub_.topic.clone();
                        if topic.starts_with("$SYS/") {
                            let payload = String::from_utf8_lossy(&pub_.payload).to_string();
                            let mut bs = bs_clone.lock().await;
                            bs.parse_sys(&topic, &payload);
                        }
                    }
                    Ok(Event::Incoming(_)) => {}
                    Ok(Event::Outgoing(_)) => {}
                    Err(e) => {
                        connected_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                        warn!("MQTT reconnect error: {:?}", e);
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
                if last_record.elapsed() >= Duration::from_secs(5) {
                    let bs = bs_clone.lock().await;
                    let mut bh = bh_clone.lock().await;
                    bh.record(&bs);
                    last_record = Instant::now();
                }
            }
        });
    }
}
