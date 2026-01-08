# Multi-stage Dockerfile for MQTT Proxy
# Optimized for minimal image size and fast builds

# Web UI builder stage
FROM node:20-alpine AS web-builder

WORKDIR /app/web-ui

# Copy package files
COPY web-ui/package*.json ./

# Install dependencies
RUN npm ci

# Copy source files
COPY web-ui/ ./

# Build the web UI
RUN npm run build

# Rust builder stage
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests and benchmark placeholders
COPY Cargo.toml ./

# Create dummy files for dependency caching
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > benches/latency.rs && \
    echo "fn main() {}" > benches/throughput.rs

# Build dependencies (cached layer)
RUN cargo build --release && \
    rm -rf src benches target/release/mqtt-proxy* target/release/deps/mqtt_proxy*

# Copy real source code
COPY src ./src
COPY benches ./benches

# Build the actual application
RUN touch src/main.rs src/lib.rs && \
    cargo build --release && \
    strip target/release/mqtt-proxy

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies
RUN apk add --no-cache ca-certificates tzdata && \
    adduser -D -u 1000 appuser

# Create necessary directories
RUN mkdir -p /app/config && \
    chown -R appuser:appuser /app

USER appuser
WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/mqtt-proxy .

# Copy web UI static files from web-builder
COPY --from=web-builder /app/web-ui/dist ./web-ui/dist

# Create data directory
RUN mkdir -p data

# Expose ports
EXPOSE 1883 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:3000/health || exit 1

# Run as non-root
CMD ["./mqtt-proxy"]
