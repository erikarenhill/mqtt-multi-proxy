# syntax=docker/dockerfile:1.4
# Multi-stage Dockerfile for MQTT Proxy
# Optimized for minimal image size and fast builds

# Web UI builder stage
FROM node:20-alpine AS web-builder

WORKDIR /app/web-ui

# Copy package files first for dependency caching
COPY web-ui/package*.json ./

# Install dependencies with npm cache mount
RUN --mount=type=cache,target=/root/.npm \
    npm ci

# Copy source files
COPY web-ui/ ./

# Build the web UI
RUN npm run build

# Rust builder stage
FROM rust:1.83-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy only Cargo.toml first for dependency caching
COPY Cargo.toml ./

# Create dummy files for dependency caching
RUN mkdir -p src benches && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > benches/latency.rs && \
    echo "fn main() {}" > benches/throughput.rs

# Build dependencies with cargo registry cache (cached layer)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    cargo build --release && \
    rm -rf src benches target/release/mqtt-proxy* target/release/deps/mqtt_proxy*

# Copy real source code
COPY src ./src
COPY benches ./benches

# Build the actual application with cache mounts
# Copy binary out of target dir since cache mounts don't persist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    touch src/main.rs src/lib.rs && \
    cargo build --release && \
    strip target/release/mqtt-proxy && \
    cp target/release/mqtt-proxy /mqtt-proxy

# Runtime stage
FROM alpine:3.19

# Install runtime dependencies and create user in single layer
RUN apk add --no-cache ca-certificates tzdata && \
    adduser -D -u 1000 appuser && \
    mkdir -p /app/config /app/data && \
    chown -R appuser:appuser /app

WORKDIR /app
USER appuser

# Copy binary from builder (use --link for layer independence)
COPY --link --from=builder /mqtt-proxy ./mqtt-proxy

# Copy web UI static files from web-builder
COPY --link --from=web-builder /app/web-ui/dist ./web-ui/dist

# Copy default config (changes more frequently, so copy last)
COPY --link --chown=appuser:appuser config/config.toml ./config/

# Expose ports
EXPOSE 1883 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:3000/health || exit 1

# Run as non-root
CMD ["./mqtt-proxy"]
