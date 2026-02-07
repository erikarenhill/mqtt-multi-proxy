use crate::broker_storage::{BrokerConfig, BrokerStorage};
use crate::connection_manager::ConnectionManager;
use crate::settings_storage::{MainBrokerSettings, SettingsStorage};
use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Utc};
use rumqttc::{Event, Incoming, MqttOptions};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tower_http::services::ServeDir;
use tracing::{debug, error, info};

// Message structure for real-time updates
#[derive(Clone, Debug, Serialize)]
pub struct MqttMessage {
    pub timestamp: DateTime<Utc>,
    pub client_id: String,
    pub topic: String,
    pub payload: Vec<u8>,
    pub qos: u8,
    pub retain: bool,
}

pub struct WebServer {
    port: u16,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    broker_storage: Arc<BrokerStorage>,
    settings_storage: Arc<SettingsStorage>,
    main_broker_restart_tx: mpsc::Sender<()>,
    message_tx: broadcast::Sender<MqttMessage>,
    messages_received: Arc<AtomicU64>,
    messages_forwarded: Arc<AtomicU64>,
    total_latency_ns: Arc<AtomicU64>,
}

impl WebServer {
    pub fn new(
        port: u16,
        connection_manager: Arc<RwLock<ConnectionManager>>,
        broker_storage: Arc<BrokerStorage>,
        settings_storage: Arc<SettingsStorage>,
        main_broker_restart_tx: mpsc::Sender<()>,
    ) -> (
        Self,
        broadcast::Sender<MqttMessage>,
        Arc<AtomicU64>,
        Arc<AtomicU64>,
        Arc<AtomicU64>,
    ) {
        let (message_tx, _) = broadcast::channel(1000); // Buffer 1000 messages
        let tx_clone = message_tx.clone();
        let messages_received = Arc::new(AtomicU64::new(0));
        let messages_forwarded = Arc::new(AtomicU64::new(0));
        let total_latency_ns = Arc::new(AtomicU64::new(0));
        let received_clone = Arc::clone(&messages_received);
        let forwarded_clone = Arc::clone(&messages_forwarded);
        let latency_clone = Arc::clone(&total_latency_ns);

        (
            Self {
                port,
                connection_manager,
                broker_storage,
                settings_storage,
                main_broker_restart_tx,
                message_tx,
                messages_received,
                messages_forwarded,
                total_latency_ns,
            },
            tx_clone,
            received_clone,
            forwarded_clone,
            latency_clone,
        )
    }

    pub async fn run(self) -> anyhow::Result<()> {
        let app_state = AppState {
            connection_manager: self.connection_manager,
            broker_storage: self.broker_storage,
            settings_storage: self.settings_storage,
            main_broker_restart_tx: self.main_broker_restart_tx,
            message_tx: self.message_tx.clone(),
            messages_received: self.messages_received,
            messages_forwarded: self.messages_forwarded,
            total_latency_ns: self.total_latency_ns,
        };

        let app = Router::new()
            .route("/health", get(health_check))
            .route("/api/brokers", get(list_brokers).post(add_broker))
            .route(
                "/api/brokers/:id",
                get(get_broker).put(update_broker).delete(delete_broker),
            )
            .route("/api/brokers/:id/toggle", post(toggle_broker))
            .route("/api/status", get(get_status))
            .route(
                "/api/settings/main-broker",
                get(get_main_broker_settings).put(update_main_broker_settings),
            )
            .route(
                "/api/settings/main-broker/test",
                post(test_main_broker_connection),
            )
            .route("/ws/messages", get(websocket_handler))
            .nest_service("/", ServeDir::new("web-ui/dist"))
            .with_state(app_state);

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port)).await?;
        info!("Web UI listening on http://0.0.0.0:{}", self.port);

        axum::serve(listener, app).await?;
        Ok(())
    }
}

#[derive(Clone)]
struct AppState {
    connection_manager: Arc<RwLock<ConnectionManager>>,
    broker_storage: Arc<BrokerStorage>,
    settings_storage: Arc<SettingsStorage>,
    main_broker_restart_tx: mpsc::Sender<()>,
    message_tx: broadcast::Sender<MqttMessage>,
    messages_received: Arc<AtomicU64>,
    messages_forwarded: Arc<AtomicU64>,
    total_latency_ns: Arc<AtomicU64>,
}

// Health check endpoint
async fn health_check() -> &'static str {
    "OK"
}

// List all brokers
async fn list_brokers(
    State(state): State<AppState>,
) -> Result<Json<ListBrokersResponse>, AppError> {
    let brokers = state.broker_storage.list().await;
    Ok(Json(ListBrokersResponse { brokers }))
}

// Get single broker
async fn get_broker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BrokerConfig>, AppError> {
    let broker = state
        .broker_storage
        .get(&id)
        .await
        .ok_or(AppError::NotFound)?;
    Ok(Json(broker))
}

// Add new broker
async fn add_broker(
    State(state): State<AppState>,
    Json(payload): Json<AddBrokerRequest>,
) -> Result<Json<BrokerConfig>, AppError> {
    // Generate unique ID
    let id = uuid::Uuid::new_v4().to_string();

    let broker = BrokerConfig {
        id: id.clone(),
        name: payload.name,
        address: payload.address,
        port: payload.port,
        client_id_prefix: payload.client_id_prefix,
        username: if payload.username.is_empty() {
            None
        } else {
            Some(payload.username)
        },
        password: if payload.password.is_empty() {
            None
        } else {
            Some(payload.password)
        },
        enabled: payload.enabled.unwrap_or(true),
        use_tls: payload.use_tls.unwrap_or(false),
        insecure_skip_verify: payload.insecure_skip_verify.unwrap_or(false),
        ca_cert_path: payload.ca_cert_path,
        bidirectional: payload.bidirectional.unwrap_or(false),
        topics: payload.topics.unwrap_or_default(),
        subscription_topics: payload.subscription_topics.unwrap_or_default(),
    };

    state.broker_storage.add(broker.clone()).await?;

    // Notify connection manager to establish connection (uses plaintext password)
    let mut manager = state.connection_manager.write().await;
    manager.add_broker(broker.clone()).await?;

    info!("Broker '{}' added via API", broker.name);
    // Return config with hidden password
    Ok(Json(broker.with_hidden_password()))
}

// Update existing broker
async fn update_broker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<UpdateBrokerRequest>,
) -> Result<Json<BrokerConfig>, AppError> {
    // Get existing broker to preserve credentials if not provided
    let existing = state
        .broker_storage
        .get(&id)
        .await
        .ok_or(AppError::NotFound)?;

    let updated = BrokerConfig {
        id: id.clone(),
        name: payload.name,
        address: payload.address,
        port: payload.port,
        client_id_prefix: payload.client_id_prefix,
        // If username not provided or empty, keep existing; otherwise use new value
        username: match payload.username {
            Some(u) if !u.is_empty() => Some(u),
            Some(_) => None,           // Empty string means remove username
            None => existing.username, // Not provided, keep existing
        },
        // If password not provided or empty, keep existing; otherwise use new value
        password: match payload.password {
            Some(p) if !p.is_empty() => Some(p),
            Some(_) => None,           // Empty string means remove password
            None => existing.password, // Not provided, keep existing
        },
        bidirectional: payload.bidirectional,
        enabled: payload.enabled,
        use_tls: payload.use_tls,
        insecure_skip_verify: payload.insecure_skip_verify,
        ca_cert_path: payload.ca_cert_path,
        topics: payload.topics,
        subscription_topics: payload.subscription_topics,
    };

    state.broker_storage.update(&id, updated.clone()).await?;

    // Update connection manager (need decrypted password for connections)
    let broker_with_password = state
        .broker_storage
        .get_with_password(&id)
        .await
        .ok_or(AppError::NotFound)?;
    let mut manager = state.connection_manager.write().await;
    manager.update_broker(broker_with_password).await?;

    info!("Broker '{}' updated via API", updated.name);
    // Return config with hidden password
    Ok(Json(updated.with_hidden_password()))
}

// Delete broker
async fn delete_broker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    state.broker_storage.delete(&id).await?;

    // Remove from connection manager
    let mut manager = state.connection_manager.write().await;
    manager.remove_broker(&id).await?;

    info!("Broker '{}' deleted via API", id);
    Ok(StatusCode::NO_CONTENT)
}

// Toggle broker enabled/disabled
async fn toggle_broker(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<ToggleBrokerRequest>,
) -> Result<StatusCode, AppError> {
    state
        .broker_storage
        .toggle_enabled(&id, payload.enabled)
        .await?;

    // Update connection manager (need decrypted password for connections)
    let mut manager = state.connection_manager.write().await;
    if payload.enabled {
        let broker = state
            .broker_storage
            .get_with_password(&id)
            .await
            .ok_or(AppError::NotFound)?;
        manager.enable_broker(broker).await?;
    } else {
        manager.disable_broker(&id).await?;
    }

    Ok(StatusCode::OK)
}

// Get overall system status
async fn get_status(State(state): State<AppState>) -> Result<Json<SystemStatus>, AppError> {
    let manager = state.connection_manager.read().await;
    let broker_statuses = manager.get_broker_status();

    let messages_received = state.messages_received.load(Ordering::Relaxed);
    let total_latency_ns = state.total_latency_ns.load(Ordering::Relaxed);

    // Calculate average latency in milliseconds
    let avg_latency_ms = if messages_received > 0 {
        (total_latency_ns as f64 / messages_received as f64) / 1_000_000.0 // Convert ns to ms
    } else {
        0.0
    };

    Ok(Json(SystemStatus {
        brokers: broker_statuses,
        total_messages_received: messages_received,
        total_messages_forwarded: state.messages_forwarded.load(Ordering::Relaxed),
        avg_latency_ms,
    }))
}

// Request/Response types
#[derive(Debug, Serialize)]
struct ListBrokersResponse {
    brokers: Vec<BrokerConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddBrokerRequest {
    name: String,
    address: String,
    port: u16,
    client_id_prefix: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    use_tls: Option<bool>,
    #[serde(default)]
    insecure_skip_verify: Option<bool>,
    #[serde(default)]
    ca_cert_path: Option<String>,
    #[serde(default)]
    bidirectional: Option<bool>,
    #[serde(default)]
    topics: Option<Vec<String>>,
    #[serde(default)]
    subscription_topics: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateBrokerRequest {
    name: String,
    address: String,
    port: u16,
    client_id_prefix: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    enabled: bool,
    use_tls: bool,
    insecure_skip_verify: bool,
    #[serde(default)]
    ca_cert_path: Option<String>,
    #[serde(default)]
    bidirectional: bool,
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    subscription_topics: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ToggleBrokerRequest {
    enabled: bool,
}

#[derive(Debug, Serialize)]
struct SystemStatus {
    brokers: Vec<BrokerStatus>,
    total_messages_received: u64,
    total_messages_forwarded: u64,
    avg_latency_ms: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BrokerStatus {
    pub id: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    pub connected: bool,
    pub enabled: bool,
    pub bidirectional: bool,
    pub topics: Vec<String>,
    pub subscription_topics: Vec<String>,
}

// Error handling
enum AppError {
    Internal(anyhow::Error),
    NotFound,
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            AppError::Internal(err) => {
                error!("Internal error: {}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Internal error: {}", err),
                )
            }
            AppError::NotFound => (StatusCode::NOT_FOUND, "Broker not found".to_string()),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

// Main broker settings endpoints
async fn get_main_broker_settings(
    State(state): State<AppState>,
) -> Result<Json<MainBrokerSettingsResponse>, AppError> {
    let settings = state.settings_storage.get_main_broker_for_api().await;
    Ok(Json(MainBrokerSettingsResponse { settings }))
}

async fn update_main_broker_settings(
    State(state): State<AppState>,
    Json(payload): Json<UpdateMainBrokerRequest>,
) -> Result<Json<MainBrokerSettingsResponse>, AppError> {
    let settings = MainBrokerSettings {
        address: payload.address.clone(),
        port: payload.port,
        client_id: payload.client_id,
        username: if payload.username.as_deref() == Some("") {
            None
        } else {
            payload.username
        },
        password: if payload.password.as_deref() == Some("") {
            None
        } else {
            payload.password
        },
    };

    state.settings_storage.set_main_broker(settings).await?;

    // Update connection manager with new main broker address for reverse connections
    {
        let mut manager = state.connection_manager.write().await;
        manager.update_main_broker_config(payload.address, payload.port);
    }

    // Signal the proxy to restart the main broker client
    let _ = state.main_broker_restart_tx.send(()).await;

    let saved = state.settings_storage.get_main_broker_for_api().await;
    Ok(Json(MainBrokerSettingsResponse { settings: saved }))
}

async fn test_main_broker_connection(
    Json(payload): Json<TestConnectionRequest>,
) -> Result<Json<TestConnectionResponse>, AppError> {
    let client_id = format!("{}-test-{}", payload.client_id, uuid::Uuid::new_v4());
    let mut mqtt_options = MqttOptions::new(&client_id, &payload.address, payload.port);
    mqtt_options.set_keep_alive(std::time::Duration::from_secs(5));

    if let Some(ref username) = payload.username {
        if !username.is_empty() {
            let password = payload.password.as_deref().unwrap_or("");
            mqtt_options.set_credentials(username, password);
        }
    }

    let (_client, mut eventloop) = rumqttc::AsyncClient::new(mqtt_options, 10);

    let start = std::time::Instant::now();

    // Try to connect with a 5 second timeout
    match tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Incoming::ConnAck(connack))) => {
                    return Ok(connack);
                }
                Ok(_) => continue,
                Err(e) => return Err(e),
            }
        }
    })
    .await
    {
        Ok(Ok(_connack)) => {
            let latency_ms = start.elapsed().as_millis() as u64;
            // Disconnect cleanly
            let _ = _client.disconnect().await;
            Ok(Json(TestConnectionResponse {
                success: true,
                message: format!(
                    "Connected to {}:{} successfully",
                    payload.address, payload.port
                ),
                latency_ms: Some(latency_ms),
            }))
        }
        Ok(Err(e)) => Ok(Json(TestConnectionResponse {
            success: false,
            message: format!("Connection failed: {}", e),
            latency_ms: None,
        })),
        Err(_) => Ok(Json(TestConnectionResponse {
            success: false,
            message: format!(
                "Connection timed out after 5s ({}:{})",
                payload.address, payload.port
            ),
            latency_ms: None,
        })),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MainBrokerSettingsResponse {
    settings: Option<MainBrokerSettings>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateMainBrokerRequest {
    address: String,
    port: u16,
    client_id: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TestConnectionRequest {
    address: String,
    port: u16,
    client_id: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TestConnectionResponse {
    success: bool,
    message: String,
    latency_ms: Option<u64>,
}

// WebSocket handler for real-time MQTT messages
async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    info!("New WebSocket client connected");
    let mut rx = state.message_tx.subscribe();

    while let Ok(msg) = rx.recv().await {
        let json = serde_json::to_string(&msg).unwrap_or_default();
        if socket.send(Message::Text(json)).await.is_err() {
            debug!("WebSocket client disconnected");
            break;
        }
    }
}
