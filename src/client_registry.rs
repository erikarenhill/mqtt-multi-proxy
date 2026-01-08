use bytes::Bytes;
use mqttrs::*;
use rumqttc::QoS;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

/// Message to be sent to a client
#[derive(Debug, Clone)]
pub struct ClientMessage {
    pub topic: String,
    pub payload: Bytes,
    pub qos: QoS,
    pub retain: bool,
}

/// Client connection information
struct ClientInfo {
    client_id: String,
    tx: mpsc::Sender<ClientMessage>,
    subscriptions: HashSet<String>,
}

/// Registry for managing client connections and their subscriptions
pub struct ClientRegistry {
    clients: Arc<RwLock<HashMap<String, ClientInfo>>>,
}

impl ClientRegistry {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new client connection
    pub async fn register_client(
        &self,
        client_id: String,
        tx: mpsc::Sender<ClientMessage>,
    ) {
        let mut clients = self.clients.write().await;
        clients.insert(
            client_id.clone(),
            ClientInfo {
                client_id,
                tx,
                subscriptions: HashSet::new(),
            },
        );
        info!("Client registered in registry");
    }

    /// Unregister a client when they disconnect
    pub async fn unregister_client(&self, client_id: &str) {
        let mut clients = self.clients.write().await;
        clients.remove(client_id);
        info!("Client '{}' unregistered from registry", client_id);
    }

    /// Add subscriptions for a client
    pub async fn add_subscriptions(&self, client_id: &str, topics: Vec<String>) -> Vec<String> {
        let mut clients = self.clients.write().await;

        if let Some(client) = clients.get_mut(client_id) {
            for topic in &topics {
                client.subscriptions.insert(topic.clone());
                info!("Client '{}' subscribed to '{}'", client_id, topic);
            }
            topics
        } else {
            warn!("Attempted to add subscriptions for unknown client '{}'", client_id);
            Vec::new()
        }
    }

    /// Remove subscriptions for a client
    pub async fn remove_subscriptions(&self, client_id: &str, topics: &[String]) {
        let mut clients = self.clients.write().await;

        if let Some(client) = clients.get_mut(client_id) {
            for topic in topics {
                client.subscriptions.remove(topic);
                info!("Client '{}' unsubscribed from '{}'", client_id, topic);
            }
        }
    }

    /// Get all unique topics that any client is subscribed to
    pub async fn get_all_subscribed_topics(&self) -> Vec<String> {
        let clients = self.clients.read().await;
        let mut topics: HashSet<String> = HashSet::new();

        for client in clients.values() {
            topics.extend(client.subscriptions.iter().cloned());
        }

        topics.into_iter().collect()
    }

    /// Forward a message to all clients subscribed to the topic
    pub async fn forward_to_subscribers(&self, topic: &str, message: ClientMessage) {
        let clients = self.clients.read().await;
        let mut sent_count = 0;

        for client in clients.values() {
            // Check if client is subscribed to this exact topic
            // TODO: Implement wildcard matching (+, #) for full MQTT compliance
            if client.subscriptions.contains(topic) {
                match client.tx.send(message.clone()).await {
                    Ok(_) => {
                        debug!("Forwarded message on '{}' to client '{}'", topic, client.client_id);
                        sent_count += 1;
                    }
                    Err(e) => {
                        warn!("Failed to send message to client '{}': {}", client.client_id, e);
                    }
                }
            }
        }

        if sent_count > 0 {
            info!("📤 Message on '{}' forwarded to {} subscribed client(s)", topic, sent_count);
        }
    }

    /// Check if topic matches a subscription pattern
    /// Supports MQTT wildcards: + (single level), # (multi level)
    fn topic_matches(subscription: &str, topic: &str) -> bool {
        // Quick exact match
        if subscription == topic {
            return true;
        }

        // Check for wildcards
        if !subscription.contains('+') && !subscription.contains('#') {
            return false;
        }

        let sub_parts: Vec<&str> = subscription.split('/').collect();
        let topic_parts: Vec<&str> = topic.split('/').collect();

        let mut sub_idx = 0;
        let mut topic_idx = 0;

        while sub_idx < sub_parts.len() && topic_idx < topic_parts.len() {
            let sub_part = sub_parts[sub_idx];
            let topic_part = topic_parts[topic_idx];

            if sub_part == "#" {
                // Multi-level wildcard - matches everything remaining
                return sub_idx == sub_parts.len() - 1; // # must be last
            } else if sub_part == "+" {
                // Single-level wildcard - matches this level
                sub_idx += 1;
                topic_idx += 1;
            } else if sub_part == topic_part {
                // Exact match
                sub_idx += 1;
                topic_idx += 1;
            } else {
                // No match
                return false;
            }
        }

        // Both must be fully consumed for a match
        sub_idx == sub_parts.len() && topic_idx == topic_parts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_matching() {
        // Exact matches
        assert!(ClientRegistry::topic_matches("home/temp", "home/temp"));
        assert!(!ClientRegistry::topic_matches("home/temp", "home/humidity"));

        // Single-level wildcard (+)
        assert!(ClientRegistry::topic_matches("home/+", "home/temp"));
        assert!(ClientRegistry::topic_matches("home/+", "home/humidity"));
        assert!(!ClientRegistry::topic_matches("home/+", "home/living/temp"));

        // Multi-level wildcard (#)
        assert!(ClientRegistry::topic_matches("home/#", "home/temp"));
        assert!(ClientRegistry::topic_matches("home/#", "home/living/temp"));
        assert!(ClientRegistry::topic_matches("home/#", "home/living/room/temp"));
        assert!(!ClientRegistry::topic_matches("home/#", "office/temp"));

        // Combined wildcards
        assert!(ClientRegistry::topic_matches("home/+/temp", "home/living/temp"));
        assert!(!ClientRegistry::topic_matches("home/+/temp", "home/living/room/temp"));
    }
}
