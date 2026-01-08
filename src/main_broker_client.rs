use crate::config::MainBrokerConfig;
use crate::connection_manager::ConnectionManager;
use anyhow::Result;
use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Create a hash from topic and payload for deduplication
fn message_hash(topic: &str, payload: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    topic.hash(&mut hasher);
    payload.hash(&mut hasher);
    hasher.finish()
}

/// Cache entry for deduplication
struct MessageCacheEntry {
    hash: u64,
    timestamp: Instant,
}

pub struct MainBrokerClient {
    config: MainBrokerConfig,
    #[allow(dead_code)] // Client is recreated in run() for proper eventloop handling
    client: AsyncClient,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
    messages_received: Option<Arc<AtomicU64>>,
    messages_forwarded: Option<Arc<AtomicU64>>,
    total_latency_ns: Option<Arc<AtomicU64>>,
}

impl MainBrokerClient {
    pub async fn new(
        config: MainBrokerConfig,
        connection_manager: Arc<RwLock<ConnectionManager>>,
        message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
        messages_received: Option<Arc<AtomicU64>>,
        messages_forwarded: Option<Arc<AtomicU64>>,
        total_latency_ns: Option<Arc<AtomicU64>>,
    ) -> Result<Self> {
        let mut mqtt_options = MqttOptions::new(&config.client_id, &config.address, config.port);
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(60));

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            mqtt_options.set_credentials(username, password);
        }

        let (client, _eventloop) = AsyncClient::new(mqtt_options, 10000);

        Ok(Self {
            config,
            client,
            connection_manager,
            message_tx,
            messages_received,
            messages_forwarded,
            total_latency_ns,
        })
    }

    pub async fn run(self) -> Result<()> {
        info!(
            "Starting main broker client, connecting to {}:{}",
            self.config.address, self.config.port
        );

        let mut mqtt_options = MqttOptions::new(
            &self.config.client_id,
            &self.config.address,
            self.config.port,
        );
        mqtt_options.set_keep_alive(std::time::Duration::from_secs(60));

        if let (Some(username), Some(password)) = (&self.config.username, &self.config.password) {
            mqtt_options.set_credentials(username, password);
        }

        let (client, mut eventloop) = AsyncClient::new(mqtt_options, 10000);

        // Subscribe to all topics from all downstream brokers
        let subscribed_topics = self.subscribe_to_all_topics(&client).await;
        info!("Subscribed to {} unique topics", subscribed_topics.len());

        // Message deduplication cache - prevents forwarding echoed messages
        // Key: hash, Value: timestamp of when we last forwarded this message
        let mut message_cache: Vec<MessageCacheEntry> = Vec::new();
        const DEDUP_WINDOW_MS: u64 = 1000; // Ignore duplicates within 1 second

        // Process incoming messages
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                    info!(
                        "Connected to main broker at {}:{}",
                        self.config.address, self.config.port
                    );

                    // Re-subscribe after reconnection
                    let subscribed = self.subscribe_to_all_topics(&client).await;
                    info!(
                        "Re-subscribed to {} topics after reconnection",
                        subscribed.len()
                    );
                }
                Ok(Event::Incoming(Incoming::Publish(publish))) => {
                    let start = Instant::now();

                    let topic = publish.topic.clone();
                    let payload = bytes::Bytes::from(publish.payload.to_vec());
                    let qos = publish.qos;
                    let retain = publish.retain;

                    // Compute message hash for deduplication
                    let hash = message_hash(&topic, &payload);

                    // Clean old entries from cache
                    let now = Instant::now();
                    message_cache.retain(|e| {
                        now.duration_since(e.timestamp) < Duration::from_millis(DEDUP_WINDOW_MS)
                    });

                    // Check if this is a duplicate (echoed message)
                    let is_duplicate = message_cache.iter().any(|e| e.hash == hash);
                    if is_duplicate {
                        debug!("🔄 Skipping duplicate message: topic='{}' (already forwarded recently)", topic);
                        continue;
                    }

                    // Add to cache
                    message_cache.push(MessageCacheEntry {
                        hash,
                        timestamp: now,
                    });

                    debug!(
                        "📥 Received from main broker: topic='{}', {} bytes",
                        topic,
                        payload.len()
                    );

                    // Increment received counter
                    if let Some(counter) = &self.messages_received {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }

                    // Broadcast to Web UI
                    if let Some(tx) = &self.message_tx {
                        let mqtt_msg = crate::web_server::MqttMessage {
                            timestamp: chrono::Utc::now(),
                            client_id: "main-broker".to_string(),
                            topic: topic.clone(),
                            payload: payload.to_vec(),
                            qos: match qos {
                                QoS::AtMostOnce => 0,
                                QoS::AtLeastOnce => 1,
                                QoS::ExactlyOnce => 2,
                            },
                            retain,
                        };
                        let _ = tx.send(mqtt_msg);
                    }

                    // Forward to matching downstream brokers
                    let manager = self.connection_manager.read().await;
                    if let Err(e) = manager
                        .forward_message(&topic, payload, qos, retain, &self.messages_forwarded)
                        .await
                    {
                        error!("Failed to forward message: {}", e);
                    }

                    // Record latency
                    let elapsed = start.elapsed();
                    if let Some(latency_counter) = &self.total_latency_ns {
                        latency_counter.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
                    }
                }
                Ok(_) => {
                    // Other events
                }
                Err(e) => {
                    error!("Main broker connection error: {}", e);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    }

    async fn subscribe_to_all_topics(&self, client: &AsyncClient) -> HashSet<String> {
        // Always subscribe to all topics (#) so the WebUI can monitor everything
        // Message filtering for downstream brokers happens in forward_message()
        let mut all_topics = HashSet::new();
        all_topics.insert("#".to_string());

        match client.subscribe("#", QoS::AtMostOnce).await {
            Ok(_) => info!("Subscribed to all topics (#) for monitoring"),
            Err(e) => error!("Failed to subscribe to #: {}", e),
        }

        all_topics
    }
}
