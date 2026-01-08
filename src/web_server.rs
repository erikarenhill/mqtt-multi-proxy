use crate::broker_storage::{BrokerConfig, BrokerStorage};
use crate::connection_manager::ConnectionManager;
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
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
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
