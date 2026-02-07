use crate::crypto::{decrypt_password, encrypt_password};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainBrokerSettings {
    pub address: String,
    pub port: u16,
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

impl MainBrokerSettings {
    /// Returns a copy with the password encrypted (for storage)
    fn with_encrypted_password(&self) -> Self {
        let mut settings = self.clone();
        if let Some(ref password) = settings.password {
            settings.password = Some(encrypt_password(password));
        }
        settings
    }

    /// Returns a copy with the password decrypted (for internal use)
    fn with_decrypted_password(&self) -> Self {
        let mut settings = self.clone();
        if let Some(ref password) = settings.password {
            match decrypt_password(password) {
                Some(decrypted) => settings.password = Some(decrypted),
                None => {
                    warn!("Failed to decrypt main broker password, using as-is");
                }
            }
        }
        settings
    }

    /// Returns a copy with password hidden (for API responses)
    pub fn with_hidden_password(&self) -> Self {
        let mut settings = self.clone();
        if settings.password.is_some() {
            settings.password = Some("********".to_string());
        }
        settings
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SettingsStore {
    #[serde(default)]
    main_broker: Option<MainBrokerSettings>,
}

pub struct SettingsStorage {
    store_path: PathBuf,
    store: Arc<RwLock<SettingsStore>>,
}

impl SettingsStorage {
    pub fn new<P: AsRef<Path>>(store_path: P) -> Result<Self> {
        let store_path = store_path.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        if let Some(parent) = store_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {:?}", parent))?;
        }

        // Load existing store or create new one
        let store = if store_path.exists() {
            let contents = std::fs::read_to_string(&store_path)
                .with_context(|| format!("Failed to read settings file: {:?}", store_path))?;

            serde_json::from_str(&contents).unwrap_or_else(|e| {
                error!("Failed to parse settings store, starting fresh: {}", e);
                SettingsStore::default()
            })
        } else {
            info!("No existing settings store found, using defaults");
            SettingsStore::default()
        };

        Ok(Self {
            store_path,
            store: Arc::new(RwLock::new(store)),
        })
    }

    /// Returns main broker settings with decrypted password (for internal use)
    pub async fn get_main_broker(&self) -> Option<MainBrokerSettings> {
        let store = self.store.read().await;
        store
            .main_broker
            .as_ref()
            .map(|s| s.with_decrypted_password())
    }

    /// Returns main broker settings with hidden password (for API responses)
    pub async fn get_main_broker_for_api(&self) -> Option<MainBrokerSettings> {
        let store = self.store.read().await;
        store.main_broker.as_ref().map(|s| s.with_hidden_password())
    }

    /// Save main broker settings (encrypts password before storing)
    pub async fn set_main_broker(&self, settings: MainBrokerSettings) -> Result<()> {
        let mut store = self.store.write().await;

        // Handle password: if placeholder, keep existing
        let settings_to_store = match &settings.password {
            Some(p) if p == "********" => {
                // Keep existing password
                let mut s = settings.with_encrypted_password();
                if let Some(existing) = &store.main_broker {
                    s.password = existing.password.clone();
                }
                s
            }
            _ => settings.with_encrypted_password(),
        };

        store.main_broker = Some(settings_to_store);
        drop(store);

        self.save().await?;
        info!("Main broker settings saved");
        Ok(())
    }

    async fn save(&self) -> Result<()> {
        let store = self.store.read().await;
        let json =
            serde_json::to_string_pretty(&*store).context("Failed to serialize settings store")?;

        // Write to temp file first, then rename (atomic operation)
        let temp_path = self.store_path.with_extension("tmp");
        std::fs::write(&temp_path, json)
            .with_context(|| format!("Failed to write temp file: {:?}", temp_path))?;

        std::fs::rename(&temp_path, &self.store_path)
            .with_context(|| format!("Failed to save settings store: {:?}", self.store_path))?;

        Ok(())
    }
}
