# Multi-stage Dockerfile for MQTT Proxy
# Uses cargo-chef for dependency caching and xx for native cross-compilation

# Web UI builder stage (platform-independent)
FROM node:20-alpine AS web-builder

WORKDIR /app/web-ui

# Copy package files first for dependency caching
COPY web-ui/package*.json ./

# Install dependencies
RUN npm ci

# Copy source files
COPY web-ui/ ./

# Copy Cargo.toml so vite can read the app version
COPY Cargo.toml /app/Cargo.toml

# Build the web UI
RUN npm run build

# Chef stage - install cargo-chef and cross-compilation tools on the BUILD platform
# This avoids QEMU emulation for the entire Rust compilation
FROM --platform=$BUILDPLATFORM rust:1.83-alpine AS chef

COPY --from=tonistiigi/xx:1.6.1 / /

RUN apk add --no-cache clang lld musl-dev && \
    cargo install cargo-chef

WORKDIR /app

# Planner stage - analyze dependencies (runs on build platform)
FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches

RUN cargo chef prepare --recipe-path recipe.json

# Builder stage - cross-compile for the target platform
FROM chef AS builder

ARG TARGETPLATFORM

# Install target sysroot and add Rust target
RUN xx-apk add --no-cache musl-dev gcc && \
    rustup target add $(xx-info rust-target) || true

# Cook dependencies (cached layer - only invalidated when Cargo.toml/Cargo.lock change)
COPY --from=planner /app/recipe.json recipe.json
RUN xx-cargo chef cook --release --recipe-path recipe.json

# Copy real source code
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY benches ./benches

# Build the actual application
RUN xx-cargo build --release && \
    cp target/$(xx-info rust-target)/release/mqtt-proxy /app/mqtt-proxy && \
    xx-verify /app/mqtt-proxy

# Runtime stage - minimal image
FROM alpine:3.19

# Install runtime dependencies and create user in single layer
RUN apk add --no-cache ca-certificates tzdata && \
    adduser -D -u 1000 appuser && \
    mkdir -p /app/config /app/data && \
    chown -R appuser:appuser /app

WORKDIR /app
USER appuser

# Copy binary from builder
COPY --from=builder /app/mqtt-proxy ./mqtt-proxy

# Copy web UI static files from web-builder
COPY --from=web-builder /app/web-ui/dist ./web-ui/dist

# Copy default config
COPY --chown=appuser:appuser config/config.toml ./config/

# Expose ports
EXPOSE 1883 3000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD wget --quiet --tries=1 --spider http://localhost:3000/health || exit 1

# Run as non-root
CMD ["./mqtt-proxy"]
