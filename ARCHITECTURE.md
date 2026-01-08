# MQTT Proxy Architecture

## Overview

The MQTT Proxy is designed as a 1:N multiplexer that accepts a single device connection and forwards all MQTT messages to multiple downstream brokers simultaneously.

## Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         MQTT Proxy                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────────┐    ┌──────────────────┐                  │
│  │   MQTT Server    │    │   Web Server     │                  │
│  │   (Port 1883)    │    │   (Port 3000)    │                  │
│  └────────┬─────────┘    └────────┬─────────┘                  │
│           │                       │                              │
│           │                       │                              │
│  ┌────────▼────────────────────────▼──────────┐                │
│  │         Connection Manager                  │                │
│  │  - Manages downstream broker connections    │                │
│  │  - Handles message forwarding                │                │
│  │  - Monitors connection status                │                │
│  └────────┬────────────────────────────────────┘                │
│           │                                                       │
│  ┌────────▼────────────────────────────────────┐                │
│  │        Broker Storage                        │                │
│  │  - Persistent JSON storage                   │                │
│  │  - CRUD operations for broker configs        │                │
│  └──────────────────────────────────────────────┘                │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
         │              │              │
         ▼              ▼              ▼
    ┌────────┐    ┌────────┐    ┌────────┐
    │ Broker │    │ Broker │    │ Broker │
    │   1    │    │   2    │    │   N    │
    └────────┘    └────────┘    └────────┘
```

## Data Flow

### 1. Broker Configuration

**Storage**: Brokers are stored in a persistent JSON file (`./data/brokers.json`)

**Management**: Via Web UI/API:
- `POST /api/brokers` - Add new broker
- `GET /api/brokers` - List all brokers
- `GET /api/brokers/:id` - Get single broker
- `PUT /api/brokers/:id` - Update broker
- `DELETE /api/brokers/:id` - Delete broker
- `POST /api/brokers/:id/toggle` - Enable/disable broker

**Broker Configuration Structure**:
```json
{
  "id": "uuid-generated-id",
  "name": "production",
  "address": "mqtt.example.com",
  "port": 8883,
  "client_id_prefix": "proxy-device",
  "username": "optional",
  "password": "optional",
  "enabled": true,
  "use_tls": true,
  "insecure_skip_verify": false,
  "ca_cert_path": "/path/to/ca.crt"
}
```

### 2. Message Flow

1. **Device Connects**: IoT device connects to proxy on port 1883
2. **Authentication** (optional): Proxy validates credentials from `proxy.toml`
3. **Message Received**: Device publishes MQTT message
4. **Forwarding**: Connection Manager forwards to all enabled brokers
5. **Zero-Copy**: Uses `bytes::Bytes` for efficient message cloning
6. **Async Execution**: All broker forwards happen concurrently

### 3. Connection Management

**Initialization**:
1. Load `proxy.toml` configuration (proxy settings only)
2. Initialize `BrokerStorage` from JSON file
3. Load all broker configurations from storage
4. Create connections to all enabled brokers
5. Spawn event loops for each connection

**Runtime**:
- Automatic reconnection with exponential backoff
- Health monitoring via ping/pong
- Connection status exposed via API

**Dynamic Updates**:
- Adding broker: Immediately establish connection
- Updating broker: Disconnect old, connect new
- Deleting broker: Gracefully disconnect
- Toggle enabled: Connect/disconnect on demand

## File Structure

### Configuration Files

**`config/proxy.toml`**: Static proxy configuration
- Incoming connection settings (IP, port, auth)
- TLS settings for incoming connections
- Web UI port
- Storage path

**`data/brokers.json`**: Dynamic broker storage (Docker volume)
- All broker configurations
- Managed via Web UI
- Persisted across restarts

### Source Files

**`src/main.rs`**: Application entry point
**`src/lib.rs`**: Public API exports
**`src/config.rs`**: TOML configuration parsing
**`src/broker_storage.rs`**: Persistent broker configuration storage
**`src/connection_manager.rs`**: MQTT broker connection handling
**`src/web_server.rs`**: REST API for broker management
**`src/proxy.rs`**: Main proxy orchestration
**`src/metrics.rs`**: Performance metrics

## Docker Deployment

### Volumes

```yaml
volumes:
  - ./config/proxy.toml:/app/config/proxy.toml:ro  # Read-only config
  - mqtt-proxy-data:/app/data                       # Persistent storage
```

**mqtt-proxy-data** volume contains:
- `brokers.json` - Broker configurations managed via Web UI

### Networks

All services communicate via `mqtt-network`:
- `mqtt-proxy` - Main proxy application
- `test-broker-1` - Test MQTT broker
- `test-broker-2` - Test MQTT broker
- `web-ui` - Optional standalone UI

## Security Considerations

### Incoming Connections

- Optional authentication (`require_auth`)
- TLS/SSL support for encrypted connections
- Certificate-based client authentication

### Broker Connections

- Per-broker username/password
- TLS/SSL support
- Optional certificate verification skip (for self-signed)
- CA certificate path for verification

### Web API

- Currently no authentication (TODO)
- Should be secured in production:
  - API key authentication
  - JWT tokens
  - Rate limiting
  - CORS configuration

## Performance Characteristics

### Latency

**Target**: < 5ms (p99)

**Optimizations**:
- Zero-copy message forwarding with `bytes::Bytes`
- Async I/O throughout
- Minimal allocations in hot path
- Connection pooling

### Throughput

**Target**: 10,000+ messages/second

**Bottlenecks**:
- Network bandwidth to downstream brokers
- Serialization overhead
- Lock contention in connection manager

### Memory

**Target**: < 50MB base + 10MB per broker

**Management**:
- Bounded channels for backpressure
- Connection pooling
- Efficient buffer reuse

## Monitoring

### Metrics

Prometheus-compatible metrics endpoint: `/metrics`

- `mqtt_messages_received_total`
- `mqtt_messages_forwarded_total`
- `mqtt_message_latency_seconds`
- `mqtt_active_connections`
- `mqtt_broker_connection_status`

### Web Dashboard

Real-time monitoring at `http://localhost:3000`:
- Broker connection status
- Message throughput
- Latency histograms
- Active connections

## Future Enhancements

### Phase 1 (Core Functionality)
- [ ] Complete MQTT server implementation
- [ ] TLS/SSL support for broker connections
- [ ] Proper error handling and reconnection
- [ ] Message buffering and replay

### Phase 2 (Features)
- [ ] Topic filtering and routing rules
- [ ] QoS level handling (0, 1, 2)
- [ ] Last Will and Testament support
- [ ] Retained message handling
- [ ] Session persistence

### Phase 3 (Production)
- [ ] API authentication
- [ ] Rate limiting
- [ ] Circuit breaker pattern
- [ ] Load balancing across brokers
- [ ] High availability (clustering)
- [ ] Metrics aggregation
- [ ] Alerting integration

## Development Workflow

1. **Code Changes**: Edit Rust source files
2. **Testing**: `cargo test`
3. **Local Run**: `cargo run`
4. **Docker Build**: `docker-compose up --build`
5. **Benchmarking**: `cargo bench`
6. **Profiling**: `cargo flamegraph`

## Deployment

### Development

```bash
docker-compose up
```

### Production

1. Configure `proxy.toml` with production settings
2. Enable TLS for incoming connections
3. Set up strong authentication
4. Configure resource limits
5. Set up monitoring and alerting
6. Use secrets management for credentials
7. Deploy behind reverse proxy (nginx)
8. Enable HTTPS for Web UI

### Scaling

**Horizontal**: Deploy multiple instances with load balancer
**Vertical**: Increase CPU/memory limits in docker-compose

## Troubleshooting

### Broker Won't Connect

1. Check broker accessibility: `telnet <host> <port>`
2. Verify credentials in Web UI
3. Check TLS settings match broker configuration
4. View logs: `docker logs mqtt-proxy`

### High Latency

1. Check network latency to brokers
2. Review broker load
3. Profile with `cargo flamegraph`
4. Check for lock contention

### Memory Growth

1. Monitor with `docker stats`
2. Check for unbounded channels
3. Profile with `heaptrack`
4. Review connection count
