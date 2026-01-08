use crate::broker_storage::BrokerStorage;
use crate::config::Config;
use crate::connection_manager::ConnectionManager;
use crate::main_broker_client::MainBrokerClient;
use crate::web_server::WebServer;
use anyhow::Result;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

pub struct MqttProxy {
    config: Config,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    #[allow(dead_code)] // Storage is managed by WebServer, kept for potential direct access
    broker_storage: Arc<BrokerStorage>,
    web_server: Option<WebServer>,
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

        // Initialize with default test brokers if empty
        broker_storage.init_defaults().await?;

        // Load broker configurations (with decrypted passwords for connections)
        let broker_configs = broker_storage.list_with_passwords().await;
        info!(
            "Loaded {} downstream broker configurations",
            broker_configs.len()
        );

        // Initialize connection manager (connects to downstream brokers)
        let connection_manager = Arc::new(RwLock::new(
            ConnectionManager::new(
                broker_configs,
                Arc::new(crate::client_registry::ClientRegistry::new()),
                config.main_broker.address.clone(),
                config.main_broker.port,
            )
            .await?,
        ));

        // Initialize web server if enabled
        let (web_server, message_tx, messages_received, messages_forwarded, total_latency_ns) =
            if config.web_ui.enabled {
                let (web_server, msg_tx, recv_counter, fwd_counter, latency_counter) =
                    WebServer::new(
                        config.web_ui.port,
                        Arc::clone(&connection_manager),
                        Arc::clone(&broker_storage),
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
            web_server,
            message_tx,
            messages_received,
            messages_forwarded,
            total_latency_ns,
        })
    }

    pub async fn run(self) -> Result<()> {
        info!("Starting MQTT Proxy Forwarder");
        info!(
            "Main broker: {}:{}",
            self.config.main_broker.address, self.config.main_broker.port
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

        // Create main broker client (connects to Mosquitto)
        let main_client = MainBrokerClient::new(
            self.config.main_broker.clone(),
            Arc::clone(&self.connection_manager),
            self.message_tx,
            self.messages_received,
            self.messages_forwarded,
            self.total_latency_ns,
        )
        .await?;

        info!("Connecting to main broker and subscribing to topics...");

        tokio::select! {
            result = main_client.run() => {
                error!("Main broker client stopped: {:?}", result);
                result?;
            }
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down MQTT Proxy");
            }
        }

        Ok(())
    }
}
