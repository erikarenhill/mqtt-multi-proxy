use crate::broker_storage::BrokerStorage;
use crate::config::{Config, MainBrokerConfig};
use crate::connection_manager::ConnectionManager;
use crate::main_broker_client::MainBrokerClient;
use crate::settings_storage::SettingsStorage;
use crate::web_server::WebServer;
use anyhow::Result;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{error, info};

pub struct MqttProxy {
    config: Config,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    #[allow(dead_code)] // Storage is managed by WebServer, kept for potential direct access
    broker_storage: Arc<BrokerStorage>,
    settings_storage: Arc<SettingsStorage>,
    web_server: Option<WebServer>,
    main_broker_restart_rx: mpsc::Receiver<()>,
    message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
    messages_received: Option<Arc<AtomicU64>>,
    messages_forwarded: Option<Arc<AtomicU64>>,
    total_latency_ns: Option<Arc<AtomicU64>>,
}

impl MqttProxy {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing MQTT Proxy Forwarder");

        // Initialize broker storage
        let broker_storage = Arc::new(BrokerStorage::new(&config.storage.broker_store_path)?);

        // Initialize settings storage
        let settings_storage = Arc::new(SettingsStorage::new(&config.storage.settings_store_path)?);

        // Initialize with default test brokers if empty
        broker_storage.init_defaults().await?;

        // Load broker configurations (with decrypted passwords for connections)
        let broker_configs = broker_storage.list_with_passwords().await;
        info!(
            "Loaded {} downstream broker configurations",
            broker_configs.len()
        );

        // Resolve main broker config: settings.json > config.toml/env > defaults
        let main_broker_config =
            Self::resolve_main_broker_config(&settings_storage, &config.main_broker).await;

        // Initialize connection manager (connects to downstream brokers)
        let connection_manager = Arc::new(RwLock::new(
            ConnectionManager::new(
                broker_configs,
                Arc::new(crate::client_registry::ClientRegistry::new()),
                main_broker_config.address.clone(),
                main_broker_config.port,
            )
            .await?,
        ));

        // Create restart channel for main broker client
        let (restart_tx, restart_rx) = mpsc::channel(1);

        // Initialize web server if enabled
        let (web_server, message_tx, messages_received, messages_forwarded, total_latency_ns) =
            if config.web_ui.enabled {
                let (web_server, msg_tx, recv_counter, fwd_counter, latency_counter) =
                    WebServer::new(
                        config.web_ui.port,
                        Arc::clone(&connection_manager),
                        Arc::clone(&broker_storage),
                        Arc::clone(&settings_storage),
                        restart_tx,
                    );
                (
                    Some(web_server),
                    Some(msg_tx),
                    Some(recv_counter),
                    Some(fwd_counter),
                    Some(latency_counter),
                )
            } else {
                (None, None, None, None, None)
            };

        Ok(Self {
            config,
            connection_manager,
            broker_storage,
            settings_storage,
            web_server,
            main_broker_restart_rx: restart_rx,
            message_tx,
            messages_received,
            messages_forwarded,
            total_latency_ns,
        })
    }

    /// Resolve main broker config with priority: settings.json > config.toml/env > defaults
    async fn resolve_main_broker_config(
        settings_storage: &SettingsStorage,
        fallback: &MainBrokerConfig,
    ) -> MainBrokerConfig {
        if let Some(saved) = settings_storage.get_main_broker().await {
            info!(
                "Using main broker settings from storage: {}:{}",
                saved.address, saved.port
            );
            MainBrokerConfig {
                address: saved.address,
                port: saved.port,
                client_id: saved.client_id,
                username: saved.username,
                password: saved.password,
            }
        } else {
            info!(
                "Using main broker settings from config/defaults: {}:{}",
                fallback.address, fallback.port
            );
            fallback.clone()
        }
    }

    pub async fn run(mut self) -> Result<()> {
        info!("Starting MQTT Proxy Forwarder");

        // Resolve initial main broker config
        let initial_config =
            Self::resolve_main_broker_config(&self.settings_storage, &self.config.main_broker)
                .await;
        info!(
            "Main broker: {}:{}",
            initial_config.address, initial_config.port
        );

        // Start web server
        if let Some(web_server) = self.web_server {
            info!("Starting Web UI on port {}", self.config.web_ui.port);
            tokio::spawn(async move {
                if let Err(e) = web_server.run().await {
                    error!("Web server error: {}", e);
                }
            });
        }

        // Main broker client restart loop
        let mut current_config = initial_config;

        loop {
            // Create shutdown channel for current main broker client
            let (shutdown_tx, shutdown_rx) = watch::channel(false);

            let main_client = MainBrokerClient::new(
                current_config.clone(),
                Arc::clone(&self.connection_manager),
                self.message_tx.clone(),
                self.messages_received.clone(),
                self.messages_forwarded.clone(),
                self.total_latency_ns.clone(),
            )
            .await?;

            info!("Connecting to main broker and subscribing to topics...");

            tokio::select! {
                result = main_client.run(shutdown_rx) => {
                    error!("Main broker client stopped: {:?}", result);
                    result?;
                    break;
                }
                _ = self.main_broker_restart_rx.recv() => {
                    info!("Main broker restart requested, reconnecting with new settings...");
                    // Signal shutdown to the current client
                    let _ = shutdown_tx.send(true);

                    // Resolve new config from settings storage
                    current_config = Self::resolve_main_broker_config(
                        &self.settings_storage,
                        &self.config.main_broker,
                    )
                    .await;
                    info!(
                        "Restarting main broker client with new config: {}:{}",
                        current_config.address, current_config.port
                    );

                    // Small delay to let the old client shut down cleanly
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    continue;
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("Shutting down MQTT Proxy");
                    break;
                }
            }
        }

        Ok(())
    }
}
