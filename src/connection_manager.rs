use crate::broker_storage::BrokerConfig;
use crate::client_registry::{ClientRegistry, ClientMessage};
use anyhow::Result;
use bytes::Bytes;
use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, QoS};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

/// Cache entry for tracking recently published messages from bidirectional brokers
#[derive(Clone)]
struct MessageCacheEntry {
    hash: u64,
    timestamp: Instant,
}

/// Shared cache for deduplication - tracks messages published by each broker
pub type MessageCache = Arc<Mutex<HashMap<String, Vec<MessageCacheEntry>>>>;

/// Create a hash from topic and payload for deduplication
fn message_hash(topic: &str, payload: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    topic.hash(&mut hasher);
    payload.hash(&mut hasher);
    hasher.finish()
}

pub struct ConnectionManager {
    brokers: HashMap<String, BrokerConnection>,
    client_registry: Arc<ClientRegistry>,
    main_broker_address: String,
    main_broker_port: u16,
    /// Cache of recently published messages per broker (for loop prevention)
    message_cache: MessageCache,
}

struct BrokerConnection {
    config: BrokerConfig,
    client: AsyncClient,
    connected: Arc<AtomicBool>,
    main_broker_client: Option<AsyncClient>,
}

impl ConnectionManager {
    pub async fn new(
        broker_configs: Vec<BrokerConfig>,
        client_registry: Arc<ClientRegistry>,
        main_broker_address: String,
        main_broker_port: u16,
    ) -> Result<Self> {
        let mut brokers = HashMap::new();
        let message_cache: MessageCache = Arc::new(Mutex::new(HashMap::new()));

        for config in broker_configs {
            if config.enabled {
                match Self::create_broker_connection(
                    config.clone(),
                    Arc::clone(&client_registry),
                    &main_broker_address,
                    main_broker_port,
                    Arc::clone(&message_cache),
                ).await {
                    Ok(connection) => {
                        info!("Connected to broker: {}", config.name);
                        brokers.insert(config.id.clone(), connection);
                    }
                    Err(e) => {
                        error!("Failed to connect to broker {}: {}", config.name, e);
                    }
                }
            }
        }

        Ok(Self {
            brokers,
            client_registry,
            main_broker_address,
            main_broker_port,
            message_cache,
        })
    }

    async fn create_broker_connection(
        config: BrokerConfig,
        client_registry: Arc<ClientRegistry>,
        main_broker_address: &str,
        main_broker_port: u16,
        message_cache: MessageCache,
    ) -> Result<BrokerConnection> {
        let client_id = format!("{}-{}", config.client_id_prefix, uuid::Uuid::new_v4());

        let mut mqtt_options = MqttOptions::new(&client_id, &config.address, config.port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(60));

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            mqtt_options.set_credentials(username, password);
        }

        // TODO: Add TLS support
        // if config.use_tls {
        //     let transport = if config.insecure_skip_verify {
        //         // Skip certificate verification
        //     } else {
        //         // Proper certificate verification
        //     };
        //     mqtt_options.set_transport(transport);
        // }

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10000);

        // Clone broker name early for use in spawned tasks
        let broker_name = config.name.clone();

        // Create main broker client for bidirectional communication
        let main_broker_client = if config.bidirectional {
            let main_client_id = format!("{}-reverse-{}", config.client_id_prefix, uuid::Uuid::new_v4());
            let mut main_mqtt_options = MqttOptions::new(&main_client_id, main_broker_address, main_broker_port);
            main_mqtt_options.set_keep_alive(std::time::Duration::from_secs(60));
            let (main_client, mut main_eventloop) = AsyncClient::new(main_mqtt_options, 10000);

            // Clone data for the reverse connection handler
            let reverse_broker_name = format!("{} (reverse)", broker_name);

            // Spawn eventloop handler for reverse connection to main broker
            // This eventloop is needed to drive outgoing publishes to mosquitto
            // (when bidirectional broker sends messages that need to go to main broker)
            // NOTE: We do NOT subscribe to topics here - forward_message already handles
            // forwarding from mosquitto to downstream brokers. This connection is only
            // for the reverse direction (downstream broker -> mosquitto).
            tokio::spawn(async move {
                info!("Starting reverse connection eventloop for '{}'", reverse_broker_name);
                loop {
                    match main_eventloop.poll().await {
                        Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                            info!("Reverse connection to main broker established for '{}'", reverse_broker_name);
                            // No subscriptions needed - this connection is only for publishing
                        }
                        Ok(_) => {
                            // Other events - connection is active, outgoing publishes are being sent
                        }
                        Err(e) => {
                            warn!("Reverse connection error for '{}': {}", reverse_broker_name, e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            });

            Some(main_client)
        } else {
            None
        };

        // Create shared connection status
        let connected = Arc::new(AtomicBool::new(false));
        let connected_clone = Arc::clone(&connected);
        let broker_name_clone = broker_name.clone();
        let broker_id_clone = config.id.clone();
        let bidirectional = config.bidirectional;
        let main_client_clone = main_broker_client.clone();
        let subscribe_topics = config.topics.clone();
        let client_clone = client.clone();
        let message_cache_clone = Arc::clone(&message_cache);

        // Spawn connection handler
        tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                        connected_clone.store(true, Ordering::Relaxed);
                        info!("Broker '{}' connected (bidirectional: {})", broker_name_clone, bidirectional);

                        // Subscribe to topics on bidirectional brokers to receive their messages
                        if bidirectional {
                            let topics_to_sub = if subscribe_topics.is_empty() {
                                vec!["#".to_string()] // Subscribe to all topics if none specified
                            } else {
                                subscribe_topics.iter().map(|t| {
                                    if t.ends_with('#') || t.ends_with('+') {
                                        t.clone()
                                    } else {
                                        format!("{}/#", t)
                                    }
                                }).collect()
                            };

                            for topic in &topics_to_sub {
                                match client_clone.subscribe(topic, QoS::AtMostOnce).await {
                                    Ok(_) => info!("Subscribed to '{}' on bidirectional broker '{}'", topic, broker_name_clone),
                                    Err(e) => warn!("Failed to subscribe to '{}' on '{}': {}", topic, broker_name_clone, e),
                                }
                            }
                        }
                    }
                    Ok(Event::Incoming(Incoming::Publish(publish))) => {
                        // Forward incoming messages from bidirectional brokers back to main broker
                        if bidirectional {
                            if let Some(main_client) = &main_client_clone {
                                let topic = publish.topic.clone();
                                let payload = Bytes::from(publish.payload.to_vec());
                                let qos = publish.qos;
                                let retain = publish.retain;

                                // Check if this message was recently forwarded TO this broker (echo detection)
                                let hash = message_hash(&topic, &payload);
                                let is_echo = {
                                    let mut cache = message_cache_clone.lock().await;
                                    let entries = cache.entry(broker_id_clone.clone()).or_insert_with(Vec::new);
                                    let now = Instant::now();
                                    // Clean old entries
                                    entries.retain(|e| now.duration_since(e.timestamp) < Duration::from_millis(500));
                                    // Check if this hash exists (meaning we forwarded it recently)
                                    if entries.iter().any(|e| e.hash == hash) {
                                        // Remove the entry so subsequent identical messages can get through
                                        entries.retain(|e| e.hash != hash);
                                        true
                                    } else {
                                        false
                                    }
                                };

                                if is_echo {
                                    debug!("🔄 Skipping echo from '{}': topic='{}' (already on Mosquitto)",
                                        broker_name_clone, topic);
                                } else {
                                    debug!("📤 Publishing to main broker from '{}': topic='{}', {} bytes",
                                        broker_name_clone, topic, payload.len());

                                    // Publish to main broker with timeout to prevent blocking
                                    match tokio::time::timeout(
                                        Duration::from_secs(5),
                                        main_client.publish(topic, qos, retain, payload)
                                    ).await {
                                        Ok(Ok(_)) => {}
                                        Ok(Err(e)) => {
                                            warn!("Failed to publish to main broker from '{}': {}", broker_name_clone, e);
                                        }
                                        Err(_) => {
                                            warn!("Publish to main broker timed out from '{}'", broker_name_clone);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(_) => {
                        // Other events - connection is active
                    }
                    Err(e) => {
                        connected_clone.store(false, Ordering::Relaxed);
                        warn!("MQTT connection error for '{}': {}", broker_name_clone, e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });

        Ok(BrokerConnection {
            config,
            client,
            connected,
            main_broker_client,
        })
    }

    pub async fn add_broker(&mut self, config: BrokerConfig) -> Result<()> {
        if !config.enabled {
            info!("Broker '{}' added but disabled", config.name);
            return Ok(());
        }

        match Self::create_broker_connection(
            config.clone(),
            Arc::clone(&self.client_registry),
            &self.main_broker_address,
            self.main_broker_port,
            Arc::clone(&self.message_cache),
        ).await {
            Ok(connection) => {
                info!("Broker '{}' connected", config.name);
                self.brokers.insert(config.id.clone(), connection);
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to broker '{}': {}", config.name, e);
                Err(e)
            }
        }
    }

    pub async fn update_broker(&mut self, config: BrokerConfig) -> Result<()> {
        // Remove old connection
        self.brokers.remove(&config.id);

        // Add new connection
        if config.enabled {
            self.add_broker(config).await?;
        }

        Ok(())
    }

    pub async fn remove_broker(&mut self, id: &str) -> Result<()> {
        if let Some(broker) = self.brokers.remove(id) {
            info!("Broker '{}' removed", broker.config.name);
        }
        Ok(())
    }

    pub async fn enable_broker(&mut self, config: BrokerConfig) -> Result<()> {
        let id = config.id.clone();
        let name = config.name.clone();

        // Remove if already exists
        self.brokers.remove(&id);

        // Create new connection
        match Self::create_broker_connection(
            config,
            Arc::clone(&self.client_registry),
            &self.main_broker_address,
            self.main_broker_port,
            Arc::clone(&self.message_cache),
        ).await {
            Ok(connection) => {
                info!("Broker '{}' enabled and connected", name);
                self.brokers.insert(id, connection);
                Ok(())
            }
            Err(e) => {
                error!("Failed to enable broker '{}': {}", name, e);
                Err(e)
            }
        }
    }

    pub async fn disable_broker(&mut self, id: &str) -> Result<()> {
        if let Some(broker) = self.brokers.remove(id) {
            info!("Broker '{}' disabled and disconnected", broker.config.name);
        }
        Ok(())
    }

    /// Check if a topic matches a pattern (supports MQTT wildcards + and #)
    fn topic_matches_pattern(pattern: &str, topic: &str) -> bool {
        // Empty pattern matches all topics
        if pattern.is_empty() || pattern == "#" {
            return true;
        }

        let pattern_parts: Vec<&str> = pattern.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        let mut p_idx = 0;
        let mut t_idx = 0;

        while p_idx < pattern_parts.len() && t_idx < topic_parts.len() {
            let p = pattern_parts[p_idx];
            let t = topic_parts[t_idx];

            if p == "#" {
                // Multi-level wildcard - matches everything remaining
                return p_idx == pattern_parts.len() - 1; // # must be last
            } else if p == "+" {
                // Single-level wildcard - matches this level
                p_idx += 1;
                t_idx += 1;
            } else if p == t {
                // Exact match
                p_idx += 1;
                t_idx += 1;
            } else {
                // No match
                return false;
            }
        }

        // Both must be fully consumed for a match (unless pattern ends with #)
        p_idx == pattern_parts.len() && t_idx == topic_parts.len()
    }

    pub async fn forward_message(
        &self,
        topic: &str,
        payload: bytes::Bytes,
        qos: QoS,
        retain: bool,
        messages_forwarded: &Option<Arc<AtomicU64>>,
    ) -> Result<()> {
        let broker_count = self.brokers.len();
        let connected_count = self.brokers.values().filter(|b| b.connected.load(Ordering::Relaxed)).count();

        // Calculate message hash for loop prevention
        let msg_hash = message_hash(topic, &payload);

        // Filter brokers by topic patterns (include bidirectional brokers - loop prevention is handled elsewhere)
        let matching_brokers: Vec<_> = self.brokers.iter()
            .filter(|(_id, broker)| {
                if !broker.connected.load(Ordering::Relaxed) {
                    return false;
                }
                // If broker has no topics configured, forward all messages
                if broker.config.topics.is_empty() {
                    return true;
                }
                // Check if topic matches any of the broker's patterns
                broker.config.topics.iter().any(|pattern| Self::topic_matches_pattern(pattern, topic))
            })
            .collect();

        info!(
            "🔄 Forwarding message to {}/{} brokers (topic: '{}', {} bytes, qos: {:?})",
            matching_brokers.len(), broker_count, topic, payload.len(), qos
        );

        // Forward to all matching connected brokers
        let mut success_count = 0;
        let mut fail_count = 0;

        for (id, broker) in matching_brokers {
            if broker.connected.load(Ordering::Relaxed) {
                // Use timeout to prevent blocking forever if broker's eventloop is stuck
                let publish_result = tokio::time::timeout(
                    Duration::from_secs(5),
                    broker.client.publish(topic, qos, retain, payload.clone())
                ).await;

                match publish_result {
                    Ok(Ok(_)) => {
                        info!("  ✓ Forwarded to '{}' ({}:{})", broker.config.name, broker.config.address, broker.config.port);
                        success_count += 1;
                        // Increment forwarded counter
                        if let Some(counter) = messages_forwarded {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }

                        // For bidirectional brokers, record the hash so we can detect echoes
                        if broker.config.bidirectional {
                            let mut cache = self.message_cache.lock().await;
                            let entries = cache.entry(id.clone()).or_insert_with(Vec::new);
                            // Clean old entries first
                            let now = Instant::now();
                            entries.retain(|e| now.duration_since(e.timestamp) < Duration::from_millis(500));
                            // Add this message hash
                            entries.push(MessageCacheEntry {
                                hash: msg_hash,
                                timestamp: now,
                            });
                            debug!("  📝 Recorded hash for echo detection (broker: '{}')", broker.config.name);
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("  ✗ Failed to forward to '{}': {}", broker.config.name, e);
                        fail_count += 1;
                    }
                    Err(_) => {
                        // Timeout - broker eventloop may be stuck
                        warn!("  ⏱ Publish timeout for '{}' - eventloop may be stuck", broker.config.name);
                        broker.connected.store(false, Ordering::Relaxed);
                        fail_count += 1;
                    }
                }
            } else {
                warn!("  ⊘ Skipped '{}' (not connected)", broker.config.name);
            }
        }

        if success_count > 0 {
            info!("✅ Successfully forwarded to {}/{} connected brokers", success_count, connected_count);
        } else if connected_count == 0 {
            warn!("⚠️  No brokers connected - message not forwarded!");
        } else {
            warn!("⚠️  All forward attempts failed ({} errors)", fail_count);
        }

        Ok(())
    }

    pub fn get_broker_status(&self) -> Vec<crate::web_server::BrokerStatus> {
        self.brokers
            .iter()
            .map(|(id, broker)| crate::web_server::BrokerStatus {
                id: id.clone(),
                name: broker.config.name.clone(),
                address: broker.config.address.clone(),
                port: broker.config.port,
                connected: broker.connected.load(Ordering::Relaxed),
                enabled: broker.config.enabled,
                bidirectional: broker.config.bidirectional,
                topics: broker.config.topics.clone(),
            })
            .collect()
    }

    pub fn get_all_brokers(&self) -> Vec<BrokerConfig> {
        self.brokers
            .values()
            .map(|broker| broker.config.clone())
            .collect()
    }

    /// Subscribe to topics on all bidirectional brokers
    pub async fn subscribe_to_topics(&self, topics: &[String]) {
        for (_id, broker) in &self.brokers {
            if broker.config.bidirectional && broker.connected.load(Ordering::Relaxed) {
                for topic in topics {
                    match broker.client.subscribe(topic, QoS::AtMostOnce).await {
                        Ok(_) => {
                            info!("📝 Subscribed to '{}' on broker '{}'", topic, broker.config.name);
                        }
                        Err(e) => {
                            warn!("Failed to subscribe to '{}' on broker '{}': {}", topic, broker.config.name, e);
                        }
                    }
                }
            }
        }
    }

    /// Unsubscribe from topics on all bidirectional brokers
    pub async fn unsubscribe_from_topics(&self, topics: &[String]) {
        for (_id, broker) in &self.brokers {
            if broker.config.bidirectional && broker.connected.load(Ordering::Relaxed) {
                for topic in topics {
                    match broker.client.unsubscribe(topic).await {
                        Ok(_) => {
                            debug!("Unsubscribed from '{}' on broker '{}'", topic, broker.config.name);
                        }
                        Err(e) => {
                            warn!("Failed to unsubscribe from '{}' on broker '{}': {}", topic, broker.config.name, e);
                        }
                    }
                }
            }
        }
    }
}
