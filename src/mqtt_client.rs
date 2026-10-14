use crate::config::BrokerConfig;
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
}

/// Один подключённый брокер: клиент + его статус + история.
/// Обёрнут в Arc, чтобы сервисы могли его клонировать (требование JoinSet).
pub struct BrokerConnection {
    pub config: BrokerConfig,
    client: Arc<std::sync::RwLock<AsyncClient>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
    broker_status: Arc<Mutex<BrokerStatus>>,
    broker_history: Arc<Mutex<BrokerHistory>>,
}

impl BrokerConnection {
    /// Создать клиент к брокеру и запустить eventloop (подписка $SYS, статус, история)
    async fn connect(cfg: BrokerConfig, client_id: &str) -> Arc<Self> {
        let mut mqttopts = MqttOptions::new(client_id, &cfg.host, cfg.port);
        mqttopts.set_keep_alive(Duration::from_secs(120));
        mqttopts.set_clean_session(true);
        if !cfg.username.is_empty() {
            mqttopts.set_credentials(&cfg.username, &cfg.password);
        }
        if cfg.use_tls {
            mqttopts.set_transport(tls_transport());
            info!("MQTT TLS enabled for {}:{}", cfg.host, cfg.port);
        }
        // Last Will: если процесс умрёт, брокер опубликует offline-сообщение
        mqttopts.set_last_will(
            rumqttc::LastWill::new(
                &cfg.diagnostics_topic,
                "{\"@id\":\"signal-generator\",\"@type\":\"status\",\"name\":\"Signal Generator\",\"value\":\"offline\"}",
                QoS::AtMostOnce,
                false,
            ),
        );

        let (client, mut eventloop) = AsyncClient::new(mqttopts, 5000);
        // Подписка на $SYS/# до оборачивания в Arc/RwLock.
        if let Err(e) = client.subscribe("$SYS/#", QoS::AtMostOnce).await {
            warn!("Failed to subscribe $SYS/# on {}: {:?}", cfg.host, e);
        }
        let client = Arc::new(std::sync::RwLock::new(client));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let connected_clone = connected.clone();
        let broker_status = Arc::new(Mutex::new(BrokerStatus::new()));
        let broker_history = Arc::new(Mutex::new(BrokerHistory::new(120)));
        let bs_clone = broker_status.clone();
        let bh_clone = broker_history.clone();

        // Подписка на $SYS/# (отдельный блок ниже удалён — подписка сделана выше)

        // Фоновая задача: обработка событий MQTT
        let host = cfg.host.clone();
        let port = cfg.port;
        tokio::spawn(async move {
            let mut last_record = Instant::now();
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::ConnAck(_))) => {
                        connected_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                        info!("MQTT connected to broker {}:{}", host, port);
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
                        warn!("MQTT error on {}:{}: {:?}", host, port, e);
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

        Arc::new(Self {
            config: cfg,
            client,
            connected,
            broker_status,
            broker_history,
        })
    }

    async fn publish(&self, topic: &str, signal: &[crate::models::FlatEntry]) -> Result<(), String> {
        let payload = serde_json::to_string(signal).map_err(|e| e.to_string())?;
        // Клонируем дешёвый Arc<AsyncClient> и сразу отпускаем блокировку: на высоких
        // частотах (750 сигналов × 4 Гц) удержание мьютекса на каждое сообщение
        // становится узким местом.
        let client = {
            let guard = self.client.read().unwrap_or_else(|e| e.into_inner());
            guard.clone()
        };
        // QoS 0, без retain: при 750 сигналах и высокой частоте постоянные обновления
        // retained-хранилища нагружают брокер (EMQX) и вызывают потери в потоке.
        // При интервале от 250 мс подписчик всё равно получает значение почти сразу.
        client
            .try_publish(topic, QoS::AtMostOnce, false, payload)
            .map_err(|e| format!("MQTT publish skipped (queue full): {}", e))
    }

    async fn reconnect(&self, cfg: &BrokerConfig) {
        let cfg = cfg.clone();
        let mut mqttopts = MqttOptions::new("signal-generator", &cfg.host, cfg.port);
        mqttopts.set_keep_alive(Duration::from_secs(120));
        mqttopts.set_clean_session(true);
        if !cfg.username.is_empty() {
            mqttopts.set_credentials(&cfg.username, &cfg.password);
        }
        if cfg.use_tls {
            mqttopts.set_transport(tls_transport());
            info!("MQTT TLS enabled for {}:{}", cfg.host, cfg.port);
        }

        let (client, mut eventloop) = AsyncClient::new(mqttopts, 5000);
        if let Err(e) = client.subscribe("$SYS/#", QoS::AtMostOnce).await {
            warn!("Failed to resubscribe $SYS/#: {:?}", e);
        }
        // Заменяем клиент
        {
            let mut guard = self.client.write().unwrap_or_else(|e| e.into_inner());
            *guard = client;
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

/// Мультиброкерный MQTT-менеджер: держит по клиенту на каждый включённый брокер
/// и ПУБЛИКУЕТ ВСЕ СИГНАЛЫ ПАРАЛЛЕЛЬНО во все включённые брокеры.
pub struct MqttManager {
    connections: Arc<Mutex<Vec<Arc<BrokerConnection>>>>,
}

impl MqttManager {
    /// Подключиться ко ВСЕМ включённым брокерам из списка
    pub async fn connect(cfg: &crate::config::MqttConfig) -> Self {
        let mut conns = Vec::new();
        for broker in cfg.brokers.iter().filter(|b| b.enabled) {
            conns.push(BrokerConnection::connect(broker.clone(), "signal-generator").await);
        }
        info!("MqttManager: {} broker(s) connected", conns.len());
        Self {
            connections: Arc::new(Mutex::new(conns)),
        }
    }

    /// Опубликовать сигнал параллельно во все включённые брокеры.
    /// Если у генератора задан явный `topic`, публикуем в него (одинаково во все брокеры),
    /// иначе топик строится по префиксу брокера через topic_for_catalog.
    /// Возвращает число успешных публикаций.
    pub async fn publish_catalog(
        &self,
        catalog: &str,
        id: &str,
        signal: &[crate::models::FlatEntry],
    ) -> usize {
        self.publish_catalog_explicit(catalog, id, None, signal).await
    }

    /// То же, но с явным переопределением топика (`Some(непустой)` = публикуем как задано).
    pub async fn publish_catalog_explicit(
        &self,
        catalog: &str,
        id: &str,
        explicit_topic: Option<&str>,
        signal: &[crate::models::FlatEntry],
    ) -> usize {
        let conns = self.connections.lock().await;
        if conns.is_empty() {
            return 0;
        }
        let explicit = explicit_topic.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        // Публикуем напрямую, без спавна задачи и таймаута на каждое сообщение:
        // при 750 сигналах × 4 Гц (3000 сообщений/с) спавн + JoinSet на сообщение
        // съедают больше, чем сама отправка (try_publish не блокирует).
        let mut published = 0usize;
        for conn in conns.iter() {
            let topic = match &explicit {
                Some(t) => t.clone(),
                None => conn.config.topic_for_catalog(catalog, id),
            };
            if conn.publish(&topic, signal).await.is_ok() {
                published += 1;
            }
        }
        published
    }

    /// Опубликовать диагностику во все включённые брокеры (каждый на своём топике)
    pub async fn publish_diagnostics(&self, signal: &[crate::models::FlatEntry]) -> usize {
        let conns = self.connections.lock().await;
        if conns.is_empty() {
            return 0;
        }
        let mut set = tokio::task::JoinSet::new();
        for conn in conns.iter() {
            let topic = conn.config.diagnostics_topic.clone();
            let signal_c = signal.to_vec();
            let conn = conn.clone();
            set.spawn(async move {
                tokio::time::timeout(Duration::from_secs(5), conn.publish(&topic, &signal_c)).await
            });
        }
        let mut published = 0usize;
        while let Some(res) = set.join_next().await {
            if let Ok(Ok(Ok(_))) = res {
                published += 1;
            }
        }
        published
    }

    /// Есть ли хоть один подключённый брокер
    pub async fn any_connected(&self) -> bool {
        let conns = self.connections.lock().await;
        conns
            .iter()
            .any(|c| c.connected.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Статус брокера с ЖИВЫМИ $SYS-метриками для legacy /api/broker.
    /// Предпочитает брокера, у которого метрики реально собраны (version ≠ «—»),
    /// иначе фолбэк на первого подключённого.
    pub async fn primary_status(&self) -> Option<BrokerStatus> {
        let conns = self.connections.lock().await;
        // 1) первый с живыми метриками
        let mut first_live = None;
        // 2) первый подключённый (фолбэк)
        let mut first_any = None;
        for c in conns.iter() {
            let status = { c.broker_status.lock().await.clone() };
            if first_any.is_none() {
                first_any = Some(status.clone());
            }
            // «живой» = собраны $SYS-метрики (не версия-заглушка «—»)
            if status.version != "—" && !status.version.is_empty() {
                first_live = Some(status);
                break;
            }
        }
        first_live.or(first_any)
    }

    /// Статус брокера по id
    pub async fn broker_status(&self, id: &str) -> Option<BrokerStatus> {
        let conn = self
            .connections
            .lock()
            .await
            .iter()
            .find(|c| c.config.id == id)
            .cloned()?;
        let status = conn.broker_status.lock().await.clone();
        Some(status)
    }

    pub async fn broker_history(&self, id: &str) -> Option<Vec<BrokerSnapshot>> {
        let conn = self
            .connections
            .lock()
            .await
            .iter()
            .find(|c| c.config.id == id)
            .cloned()?;
        let history = conn.broker_history.lock().await.snapshot();
        Some(history)
    }

    /// Список брокеров с живым статусом (для /api/brokers/status)
    pub async fn statuses(&self) -> Vec<serde_json::Value> {
        let conns = self.connections.lock().await;
        let mut out = Vec::new();
        for c in conns.iter() {
            let st = c.broker_status.lock().await;
            let msg_rate = st.messages_sent; // база для истории
            out.push(serde_json::json!({
                "id": c.config.id,
                "name": c.config.name,
                "host": c.config.host,
                "port": c.config.port,
                "topics": c.config.topic_prefix,
                "connected": c.connected.load(std::sync::atomic::Ordering::SeqCst),
                "version": st.version,
                "clients": st.clients_connected,
                "subs": st.subscriptions,
                "topics_count": st.topics,
                "messages_sent": msg_rate,
                "last_update": st.last_update,
            }));
        }
        out
    }

    /// Добавить брокер (создать соединение)
    pub async fn add_broker(&self, cfg: &BrokerConfig) {
        let conn = BrokerConnection::connect(cfg.clone(), "signal-generator").await;
        self.connections.lock().await.push(conn);
    }

    /// Удалить брокер по id
    #[allow(dead_code)]
    pub async fn remove_broker(&self, id: &str) -> bool {
        let mut conns = self.connections.lock().await;
        let before = conns.len();
        conns.retain(|c| c.config.id != id);
        conns.len() != before
    }

    /// Переподключить брокер по id с новым конфигом (или создать, если нет)
    #[allow(dead_code)]
    pub async fn upsert_broker(&self, cfg: &BrokerConfig) {
        let conns = self.connections.lock().await;
        if let Some(conn) = conns.iter().find(|c| c.config.id == cfg.id) {
            conn.reconnect(cfg).await;
        } else {
            drop(conns);
            self.add_broker(cfg).await;
        }
    }

    /// Заменить весь набор брокеров (после правок списка).
    /// Включённые — пересоздаём/переподключаем; отключённые/удалённые — убираем.
    pub async fn sync_brokers(&self, brokers: &[BrokerConfig]) {
        let enabled: Vec<&BrokerConfig> = brokers.iter().filter(|b| b.enabled).collect();
        let desired_ids: Vec<String> = enabled.iter().map(|b| b.id.clone()).collect();

        // Убираем те, которых больше нет среди включённых
        {
            let mut conns = self.connections.lock().await;
            conns.retain(|c| desired_ids.contains(&c.config.id));
        }

        // Для существующих — проверяем изменение конфига
        for b in &enabled {
            let exists = {
                let conns = self.connections.lock().await;
                conns.iter().any(|c| c.config.id == b.id)
            };
            if exists {
                let conns = self.connections.lock().await;
                let conn = conns.iter().find(|c| c.config.id == b.id).unwrap();
                if !conn.config_eq(b) {
                    conn.reconnect(b).await;
                }
                drop(conns);
            } else {
                self.add_broker(b).await;
            }
        }
    }
}

impl BrokerConnection {
    fn config_eq(&self, other: &BrokerConfig) -> bool {
        self.config.host == other.host
            && self.config.port == other.port
            && self.config.username == other.username
            && self.config.password == other.password
            && self.config.use_tls == other.use_tls
            && self.config.topic_prefix == other.topic_prefix
            && self.config.diagnostics_topic == other.diagnostics_topic
    }
}
