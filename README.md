# MQTT Proxy - 1:N Device Multiplexer

A high-performance, containerized MQTT proxy written in Rust that accepts a single device connection and multiplexes it to multiple MQTT brokers simultaneously.

## Features

- **1:N Architecture**: One device connects to the proxy, which forwards to N brokers
- **High Performance**: Async Rust + Tokio runtime for low-latency message forwarding
- **Real-time Web UI**: Monitor traffic, connections, and performance metrics live
- **Docker Native**: Containerized with optimized multi-stage builds
- **Production Ready**: TLS support, authentication, metrics, and health checks
- **Bi-directional**: Hashes payloads/messages to avoid circular loops, let brokers subscribe to other brokers
## Architecture

```
┌────────────────┐
│  IoT Device    │
│  (MQTT Client) │
└────────┬───────┘
         │ MQTT (1883)
         ▼
┌─────────────────────────┐
│    MQTT Proxy           │
│  ┌──────────────────┐   │
│  │  Message Router  │   │
│  └──────────────────┘   │
│  ┌──────────────────┐   │
│  │  Web Server      │◄──┼── HTTP :3000
│  └──────────────────┘   │
└───┬────────┬────────┬───┘
    │        │        │
    ▼        ▼        ▼
 ┌─────┐ ┌─────┐ ┌─────┐
 │ MQTT│ │ MQTT│ │ MQTT│
 │Brok1│ │Brok2│ │BrokN│
 └─────┘ └─────┘ └─────┘
```

## Quick Start

### Prerequisites

- Docker & Docker Compose (required)
- Rust 1.75+ (optional, for local development)
- Node.js 20+ (optional, for Web UI development)

### Run with Docker Compose

```bash
# Clone the repository
git clone https://github.com/erikarenhill/mqtt-multi-proxy.git
cd mqtt-proxy


# Start everything
docker-compose up --build

# Access the Web UI to manage brokers
open http://localhost:3000  # Web UI
```

**Note**: Broker configurations are managed via the Web UI

### Local Development

```bash
# Backend (Rust)
cargo build --release
cargo run

# Frontend (Web UI)
cd web-ui
npm install
npm run dev

# Run tests
cargo test

# Run benchmarks
cargo bench
```

### Broker Configuration (Web UI)

**Brokers are managed through the Web UI**

1. Open http://localhost:3000
2. Click "+ Add Broker"
3. Fill in the form:
   - Name, IP/hostname, port
   - Optional: username/password
   - Optional: Enable TLS + skip certificate verification
   - Select bi-directional if you want to subscribe to the other broker

Broker configurations are stored persistently in `./data/brokers.json` (Docker volume).

### Environment Variables

- `LOG_LEVEL` - Logging verbosity: `error`, `warn`, `info`, `debug`, `trace`
- `RUST_LOG` - Fine-grained logging: `mqtt_proxy=debug,rumqttc=warn`
- `MQTT_PROXY_SECRET` - Secret key for encrypting broker passwords in config storage. **Change this in production!**

## Web UI

The dashboard provides:

- **Real-time Monitoring**: Live message traffic visualization
- **Broker Status**: Connected/disconnected state for each broker
- **Performance Metrics**: Latency, throughput, active connections
- **Connection Management**: Add/remove/pause broker connections
- **Health Checks**: System status at a glance

Access at: `http://localhost:3000` 

## Performance

### Target Metrics

- **Latency**: < 5ms message forwarding (p99)
- **Throughput**: 10,000+ messages/second per connection
- **Memory**: < 50MB base + 10MB per broker connection
- **CPU**: Minimal via async I/O

### Benchmarking

```bash
# Run all benchmarks
cargo bench

# Specific benchmarks
cargo bench --bench latency
cargo bench --bench throughput

# Profile with flamegraph
cargo install flamegraph
cargo flamegraph --bin mqtt-proxy
```

## Specialized Claude Agents

This project includes three specialized Claude Code subagents:

### 1. Docker Expert (`docker-expert`)
- Container optimization and debugging
- Multi-stage build improvements
- Image size reduction
- Security hardening

### 2. Rust Performance Expert (`rust-perf-expert`)
- High-traffic, low-latency optimization
- Async/await best practices
- Memory profiling and allocation reduction
- CPU profiling and hot path analysis

### 3. MQTT Protocol Expert (`mqtt-protocol-expert`)
- MQTT 3.1.1 and 5.0 compliance
- QoS handling and optimization
- Connection management strategies
- Protocol debugging

These agents are automatically available when working in this project with Claude Code.

## Development Workflow

```bash
# Run pre-commit checks
cargo test && cargo clippy && cargo fmt --check
cd web-ui && npm run typecheck && npm run lint

# Build optimized container
docker build --target release -t mqtt-proxy:latest .

# Check image size
docker images mqtt-proxy

# Test with multiple brokers
docker-compose up

# Monitor logs
docker logs -f mqtt-proxy
```

## Testing

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration

# Test with MQTT client
mosquitto_pub -h localhost -p 1883 -t test/topic -m "hello world"

# Subscribe to downstream broker
mosquitto_sub -h localhost -p 1884 -t '#' -v
```

## Security

- **TLS/SSL**: Supported for encrypted MQTT connections
- **Authentication**: Username/password per broker
- **Non-root Container**: Runs as unprivileged user (UID 1000)
- **Network Isolation**: Docker networks for service isolation

## Troubleshooting

### Connection Issues

```bash
# Check proxy logs
docker logs mqtt-proxy

# Test broker connectivity
telnet broker.example.com 1883

# Debug with MQTT client
mosquitto_sub -h localhost -p 1883 -t '$SYS/#' -v
```

### Performance Issues

```bash
# Profile the application
cargo flamegraph --bin mqtt-proxy

# Check resource usage
docker stats mqtt-proxy

# Monitor metrics
curl http://localhost:3000/metrics
```

### Build Issues

```bash
# Clean build
cargo clean && cargo build --release

# Update dependencies
cargo update

# Check for common issues
cargo clippy
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes
4. Run tests: `cargo test && cd web-ui && npm test`
5. Run linters: `cargo clippy && cargo fmt`
6. Commit: `git commit -am 'Add feature'`
7. Push: `git push origin feature/my-feature`
8. Open a Pull Request

## License

MIT

## Acknowledgments

- Built with [rumqttc](https://github.com/bytebeamio/rumqtt) - Pure Rust MQTT client
- [Tokio](https://tokio.rs/) - Async runtime
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [React](https://react.dev/) - UI framework

## Resources

- [MQTT Protocol Spec](https://mqtt.org/mqtt-specification/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
