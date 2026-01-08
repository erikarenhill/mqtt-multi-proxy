use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub main_broker: MainBrokerConfig,
    pub web_ui: WebUiConfig,
    pub storage: StorageConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainBrokerConfig {
    /// Address of the main MQTT broker to connect to
    pub address: String,
    pub port: u16,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub listen_address: String,
    pub max_packet_size: usize,
    #[serde(rename = "connection_timeout_secs")]
    pub connection_timeout_secs: u64,
    /// Optional authentication for incoming client connections
    #[serde(default)]
    pub require_auth: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// TLS settings for incoming connections
    #[serde(default)]
    pub use_tls: bool,
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    #[serde(default)]
    pub tls_key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebUiConfig {
    pub port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Path to broker storage file
    pub broker_store_path: String,
}

fn default_true() -> bool {
    true
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let config_path = std::env::var("MQTT_PROXY_CONFIG")
            .unwrap_or_else(|_| "./config/proxy.toml".to_string());

        Self::from_file(&config_path)
    }

    pub fn from_file(path: &str) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;

        let config: Config =
            toml::from_str(&contents).with_context(|| "Failed to parse TOML configuration")?;

        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            main_broker: MainBrokerConfig {
                address: std::env::var("MAIN_BROKER_ADDRESS")
                    .unwrap_or_else(|_| "localhost".to_string()),
                port: 1883,
                client_id: "mqtt-proxy".to_string(),
                username: None,
                password: None,
            },
            web_ui: WebUiConfig {
                port: 3000,
                enabled: true,
            },
            storage: StorageConfig {
                broker_store_path: "./data/brokers.json".to_string(),
            },
        }
    }
}
