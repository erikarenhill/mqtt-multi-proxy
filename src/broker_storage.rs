use crate::crypto::{decrypt_password, encrypt_password, warn_if_encryption_not_configured};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrokerConfig {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub client_id_prefix: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub insecure_skip_verify: bool,
    #[serde(default)]
    pub ca_cert_path: Option<String>,
    #[serde(default)]
    pub bidirectional: bool,
    /// Topics to filter which messages get forwarded to this broker
    #[serde(default)]
    pub topics: Vec<String>,
    /// Topics to subscribe to on bidirectional brokers (if empty, uses topics list)
    #[serde(default)]
    pub subscription_topics: Vec<String>,
}

fn default_true() -> bool {
    true
}

impl BrokerConfig {
    /// Returns a copy with the password encrypted (for storage)
    fn with_encrypted_password(&self) -> Self {
        let mut config = self.clone();
        if let Some(ref password) = config.password {
            config.password = Some(encrypt_password(password));
        }
        config
    }

    /// Returns a copy with the password decrypted (for internal use)
    fn with_decrypted_password(&self) -> Self {
        let mut config = self.clone();
        if let Some(ref password) = config.password {
            match decrypt_password(password) {
                Some(decrypted) => config.password = Some(decrypted),
                None => {
                    warn!("Failed to decrypt password for broker '{}', using as-is", self.name);
                }
            }
        }
        config
    }

    /// Returns a copy with password hidden (for API responses)
    pub fn with_hidden_password(&self) -> Self {
        let mut config = self.clone();
        if config.password.is_some() {
            config.password = Some("********".to_string());
        }
        config
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct BrokerStore {
    brokers: Vec<BrokerConfig>,
}

pub struct BrokerStorage {
    store_path: PathBuf,
    store: Arc<RwLock<BrokerStore>>,
}

impl BrokerStorage {
    pub fn new<P: AsRef<Path>>(store_path: P) -> Result<Self> {
        let store_path = store_path.as_ref().to_path_buf();

        // Check if encryption is configured
        warn_if_encryption_not_configured();

        // Create directory if it doesn't exist
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Load existing store or create new one
        let store = if store_path.exists() {
            let contents = std::fs::read_to_string(&store_path)
                .with_context(|| format!("Failed to read store file: {:?}", store_path))?;

            serde_json::from_str(&contents).unwrap_or_else(|e| {
                error!("Failed to parse broker store, starting fresh: {}", e);
                BrokerStore::default()
            })
        } else {
            info!("No existing broker store found, creating new one");
            BrokerStore::default()
        };

        Ok(Self {
            store_path,
            store: Arc::new(RwLock::new(store)),
        })
    }

    /// Returns all brokers with passwords hidden (for API responses)
    pub async fn list(&self) -> Vec<BrokerConfig> {
        let store = self.store.read().await;
        store
            .brokers
            .iter()
            .map(|b| b.with_hidden_password())
            .collect()
    }

    /// Returns all brokers with decrypted passwords (for internal use)
    pub async fn list_with_passwords(&self) -> Vec<BrokerConfig> {
        let store = self.store.read().await;
        store
            .brokers
            .iter()
            .map(|b| b.with_decrypted_password())
            .collect()
    }

    /// Returns a broker with password hidden (for API responses)
    pub async fn get(&self, id: &str) -> Option<BrokerConfig> {
        let store = self.store.read().await;
        store
            .brokers
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.with_hidden_password())
    }

    /// Returns a broker with decrypted password (for internal use)
    pub async fn get_with_password(&self, id: &str) -> Option<BrokerConfig> {
        let store = self.store.read().await;
        store
            .brokers
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.with_decrypted_password())
    }

    pub async fn add(&self, broker: BrokerConfig) -> Result<()> {
        let mut store = self.store.write().await;

        // Check for duplicate ID or name
        if store.brokers.iter().any(|b| b.id == broker.id) {
            anyhow::bail!("Broker with ID '{}' already exists", broker.id);
        }
        if store.brokers.iter().any(|b| b.name == broker.name) {
            anyhow::bail!("Broker with name '{}' already exists", broker.name);
        }

        // Encrypt password before storing
        store.brokers.push(broker.with_encrypted_password());
        drop(store); // Release lock before saving

        self.save().await?;
        info!("Broker added successfully");
        Ok(())
    }

    pub async fn update(&self, id: &str, updated: BrokerConfig) -> Result<()> {
        let mut store = self.store.write().await;

        let index = store
            .brokers
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| anyhow::anyhow!("Broker with ID '{}' not found", id))?;

        // Check for name conflicts (excluding the current broker)
        if store
            .brokers
            .iter()
            .enumerate()
            .any(|(i, b)| i != index && b.name == updated.name)
        {
            anyhow::bail!("Broker with name '{}' already exists", updated.name);
        }

        // Handle password: if not provided or is the hidden placeholder, keep existing
        let mut config_to_store = updated.clone();
        match &updated.password {
            None => {
                // Keep existing password
                config_to_store.password = store.brokers[index].password.clone();
            }
            Some(p) if p == "********" => {
                // Hidden placeholder, keep existing password
                config_to_store.password = store.brokers[index].password.clone();
            }
            Some(_) => {
                // New password provided, encrypt it
                config_to_store = config_to_store.with_encrypted_password();
            }
        }

        store.brokers[index] = config_to_store;
        drop(store);

        self.save().await?;
        info!("Broker '{}' updated successfully", id);
        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        let mut store = self.store.write().await;

        let index = store
            .brokers
            .iter()
            .position(|b| b.id == id)
            .ok_or_else(|| anyhow::anyhow!("Broker with ID '{}' not found", id))?;

        let broker = store.brokers.remove(index);
        drop(store);

        self.save().await?;
        info!("Broker '{}' deleted successfully", broker.name);
        Ok(())
    }

    pub async fn toggle_enabled(&self, id: &str, enabled: bool) -> Result<()> {
        let mut store = self.store.write().await;

        let broker = store
            .brokers
            .iter_mut()
            .find(|b| b.id == id)
            .ok_or_else(|| anyhow::anyhow!("Broker with ID '{}' not found", id))?;

        broker.enabled = enabled;
        drop(store);

        self.save().await?;
        info!(
            "Broker '{}' {} successfully",
            id,
            if enabled { "enabled" } else { "disabled" }
        );
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let store = self.store.read().await;
        let json =
            serde_json::to_string_pretty(&*store).context("Failed to serialize broker store")?;

        // Write to temp file first, then rename (atomic operation)
        let temp_path = self.store_path.with_extension("tmp");
        std::fs::write(&temp_path, json)
            .with_context(|| format!("Failed to write temp file: {:?}", temp_path))?;

        std::fs::rename(&temp_path, &self.store_path)
            .with_context(|| format!("Failed to save broker store: {:?}", self.store_path))?;

        Ok(())
    }

    /// Initialize storage (creates empty file if needed)
    pub async fn init_defaults(&self) -> Result<()> {
        let store = self.store.read().await;
        if !store.brokers.is_empty() {
            info!(
                "Loaded {} existing broker(s) from storage",
                store.brokers.len()
            );
        } else {
            info!("No brokers configured. Add brokers via Web UI at http://localhost:3000");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_broker_storage() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("brokers.json");

        let storage = BrokerStorage::new(&store_path).unwrap();

        // Add a broker
        let broker = BrokerConfig {
            id: "test-1".to_string(),
            name: "Test Broker".to_string(),
            address: "localhost".to_string(),
            port: 1883,
            client_id_prefix: "test".to_string(),
            username: None,
            password: None,
            enabled: true,
            use_tls: false,
            insecure_skip_verify: false,
            ca_cert_path: None,
            bidirectional: false,
            topics: vec![],
            subscription_topics: vec![],
        };

        storage.add(broker.clone()).await.unwrap();

        // List brokers
        let brokers = storage.list().await;
        assert_eq!(brokers.len(), 1);
        assert_eq!(brokers[0].name, "Test Broker");

        // Get specific broker
        let retrieved = storage.get("test-1").await.unwrap();
        assert_eq!(retrieved.name, "Test Broker");

        // Update broker
        let mut updated = retrieved.clone();
        updated.port = 8883;
        storage.update("test-1", updated).await.unwrap();

        let retrieved = storage.get("test-1").await.unwrap();
        assert_eq!(retrieved.port, 8883);

        // Delete broker
        storage.delete("test-1").await.unwrap();
        let brokers = storage.list().await;
        assert_eq!(brokers.len(), 0);
    }

    #[tokio::test]
    async fn test_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let store_path = temp_dir.path().join("brokers.json");

        // Create storage and add broker
        {
            let storage = BrokerStorage::new(&store_path).unwrap();
            let broker = BrokerConfig {
                id: "test-1".to_string(),
                name: "Persistent Broker".to_string(),
                address: "localhost".to_string(),
                port: 1883,
                client_id_prefix: "test".to_string(),
                username: None,
                password: None,
                enabled: true,
                use_tls: false,
                insecure_skip_verify: false,
                ca_cert_path: None,
                bidirectional: false,
                topics: vec![],
                subscription_topics: vec![],
            };
            storage.add(broker).await.unwrap();
        }

        // Load storage again and verify persistence
        {
            let storage = BrokerStorage::new(&store_path).unwrap();
            let brokers = storage.list().await;
            assert_eq!(brokers.len(), 1);
            assert_eq!(brokers[0].name, "Persistent Broker");
        }
    }
}
