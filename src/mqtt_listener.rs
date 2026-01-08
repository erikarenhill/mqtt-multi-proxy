use anyhow::{Context, Result};
use bytes::{Buf, Bytes, BytesMut};
use mqttrs::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::client_registry::{ClientRegistry, ClientMessage};
use crate::connection_manager::ConnectionManager;

/// Messages that can be sent to a client
enum ClientWrite {
    /// MQTT message from bidirectional broker
    Message(ClientMessage),
    /// Raw MQTT packet bytes (for protocol responses)
    RawPacket(Vec<u8>),
}

pub struct MqttListenerServer {
    listen_address: String,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    client_registry: Arc<ClientRegistry>,
    message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
    messages_received: Option<Arc<AtomicU64>>,
    messages_forwarded: Option<Arc<AtomicU64>>,
    total_latency_ns: Option<Arc<AtomicU64>>,
}

// Parse MQTT packet length from variable header
fn parse_packet_length(buffer: &[u8]) -> Option<usize> {
    if buffer.is_empty() {
        return None;
    }

    let mut multiplier = 1;
    let mut value = 0usize;
    let mut offset = 1; // Skip fixed header byte

    loop {
        if offset >= buffer.len() {
            return None; // Need more data
        }

        let byte = buffer[offset];
        value += (byte as usize & 127) * multiplier;

        if byte & 128 == 0 {
            // Last byte of length
            // Total packet size = 1 (fixed header) + offset (length bytes) + value (remaining length)
            return Some(1 + offset + value);
        }

        multiplier *= 128;
        offset += 1;

        if offset > 4 {
            // Invalid - length can't be more than 4 bytes
            return None;
        }
    }
}

impl MqttListenerServer {
    pub fn new(
        listen_address: String,
        connection_manager: Arc<RwLock<ConnectionManager>>,
        client_registry: Arc<ClientRegistry>,
        message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
        messages_received: Option<Arc<AtomicU64>>,
        messages_forwarded: Option<Arc<AtomicU64>>,
        total_latency_ns: Option<Arc<AtomicU64>>,
    ) -> Self {
        Self {
            listen_address,
            connection_manager,
            client_registry,
            message_tx,
            messages_received,
            messages_forwarded,
            total_latency_ns,
        }
    }

    pub async fn run(self) -> Result<()> {
        let listener = TcpListener::bind(&self.listen_address)
            .await
            .context(format!("Failed to bind to {}", self.listen_address))?;

        info!("MQTT Listener started on {}", self.listen_address);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    info!("New client connection from {}", addr);
                    let connection_manager = Arc::clone(&self.connection_manager);
                    let client_registry = Arc::clone(&self.client_registry);
                    let message_tx = self.message_tx.clone();
                    let messages_received = self.messages_received.clone();
                    let messages_forwarded = self.messages_forwarded.clone();
                    let total_latency_ns = self.total_latency_ns.clone();

                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, connection_manager, client_registry, message_tx, messages_received, messages_forwarded, total_latency_ns).await {
                            error!("Client connection error from {}: {}", addr, e);
                        }
                    });
                }
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    connection_manager: Arc<RwLock<ConnectionManager>>,
    client_registry: Arc<ClientRegistry>,
    message_tx: Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
    messages_received: Option<Arc<AtomicU64>>,
    messages_forwarded: Option<Arc<AtomicU64>>,
    total_latency_ns: Option<Arc<AtomicU64>>,
) -> Result<()> {
    let peer_addr = stream.peer_addr()?;
    let mut buffer = BytesMut::with_capacity(4096);
    let mut client_id = String::from("unknown");
    let mut client_registered = false;

    // Create channel for sending to this client (both messages and protocol responses)
    let (to_client_tx, mut to_client_rx) = mpsc::channel::<ClientWrite>(100);

    // Create a separate channel for bidirectional MQTT messages
    let (mqtt_msg_tx, mut mqtt_msg_rx) = mpsc::channel::<ClientMessage>(100);

    // Clone the sender for use in the main loop (sender is Clone)
    let to_client_tx_clone = to_client_tx.clone();

    // Split the stream for concurrent read/write
    let (mut read_half, mut write_half) = stream.into_split();

    // Spawn task to send to client - handles both protocol responses and MQTT messages
    let _client_writer = tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(write) = to_client_rx.recv() => {
                    match write {
                        ClientWrite::RawPacket(bytes) => {
                            if write_half.write_all(&bytes).await.is_err() {
                                break; // Connection closed
                            }
                        }
                        ClientWrite::Message(msg) => {
                            // Convert QoS to mqttrs QosPid
                            let qospid = match msg.qos {
                                rumqttc::QoS::AtMostOnce => QosPid::AtMostOnce,
                                rumqttc::QoS::AtLeastOnce => QosPid::AtLeastOnce(Pid::try_from(1).unwrap()),
                                rumqttc::QoS::ExactlyOnce => QosPid::ExactlyOnce(Pid::try_from(1).unwrap()),
                            };

                            let publish = Packet::Publish(Publish {
                                dup: false,
                                qospid,
                                retain: msg.retain,
                                topic_name: &msg.topic,
                                payload: &msg.payload,
                            });

                            // Encode and send packet
                            let mut buf = vec![0u8; 4096];
                            if let Ok(bytes_written) = encode_slice(&publish, &mut buf) {
                                if write_half.write_all(&buf[..bytes_written]).await.is_err() {
                                    break; // Connection closed
                                }
                                debug!("Sent PUBLISH to client: topic='{}'", msg.topic);
                            }
                        }
                    }
                }
                Some(msg) = mqtt_msg_rx.recv() => {
                    // Forward MQTT message from bidirectional broker
                    if to_client_tx.send(ClientWrite::Message(msg)).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    loop {
        // Read data from the stream
        let n = read_half.read_buf(&mut buffer).await?;

        if n == 0 {
            info!("Client {} disconnected", client_id);
            if client_registered {
                client_registry.unregister_client(&client_id).await;
            }
            break;
        }

        // Try to decode MQTT packets from buffer
        loop {
            // First, check if we can determine the packet length
            let packet_len = match parse_packet_length(&buffer[..]) {
                Some(len) => len,
                None => {
                    // Need more data to determine packet length
                    break;
                }
            };

            // Make sure we have the complete packet
            if buffer.len() < packet_len {
                // Need more data
                break;
            }

            // Clone the packet data for decoding
            let packet_data = buffer[..packet_len].to_vec();

            match decode_slice(&packet_data) {
                Ok(Some(packet)) => {
                    // Handle the packet
                    match handle_packet(
                        &to_client_tx_clone,
                        &packet,
                        &connection_manager,
                        &client_registry,
                        &mut client_id,
                        &mut client_registered,
                        &mqtt_msg_tx,
                        &message_tx,
                        &messages_received,
                        &messages_forwarded,
                        &total_latency_ns
                    ).await {
                        Ok(should_continue) => {
                            if !should_continue {
                                info!("Client {} requested disconnect", client_id);
                                if client_registered {
                                    client_registry.unregister_client(&client_id).await;
                                }
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            error!("Error handling packet from {}: {}", client_id, e);
                            if client_registered {
                                client_registry.unregister_client(&client_id).await;
                            }
                            return Err(e);
                        }
                    }

                    // Remove processed bytes from buffer
                    buffer.advance(packet_len);
                }
                Ok(None) => {
                    // This shouldn't happen since we have the complete packet
                    error!("Failed to decode complete packet");
                    buffer.advance(1);
                }
                Err(e) => {
                    error!("Failed to decode MQTT packet from {}: {:?}", peer_addr, e);
                    // Try to recover by advancing past this packet
                    buffer.advance(packet_len.min(buffer.len()));
                }
            }
        }
    }

    Ok(())
}

async fn handle_packet<'a>(
    to_client_tx: &mpsc::Sender<ClientWrite>,
    packet: &Packet<'a>,
    connection_manager: &Arc<RwLock<ConnectionManager>>,
    client_registry: &Arc<ClientRegistry>,
    client_id: &mut String,
    client_registered: &mut bool,
    mqtt_msg_tx: &mpsc::Sender<ClientMessage>,
    message_tx: &Option<tokio::sync::broadcast::Sender<crate::web_server::MqttMessage>>,
    messages_received: &Option<Arc<AtomicU64>>,
    messages_forwarded: &Option<Arc<AtomicU64>>,
    total_latency_ns: &Option<Arc<AtomicU64>>,
) -> Result<bool> {
    match packet {
        Packet::Connect(connect) => {
            *client_id = connect.client_id.to_string();
            info!("CONNECT from client '{}' (protocol: {:?}, clean_session: {})",
                client_id, connect.protocol, connect.clean_session);

            // Register client with registry (use mqtt_msg_tx for bidirectional messages)
            client_registry.register_client(client_id.clone(), mqtt_msg_tx.clone()).await;
            *client_registered = true;
            info!("✅ Client '{}' registered for bidirectional message forwarding", client_id);

            // Send CONNACK - manually constructed for reliability
            // CONNACK: Fixed header (0x20) + Remaining length (0x02) + Session present (0x00) + Return code (0x00 = accepted)
            let connack_bytes = vec![0x20u8, 0x02, 0x00, 0x00];
            to_client_tx.send(ClientWrite::RawPacket(connack_bytes)).await
                .context("Failed to send CONNACK")?;
            debug!("Sent CONNACK to client '{}'", client_id);
            Ok(true)
        }

        Packet::Publish(publish) => {
            // Start timing for latency measurement
            let start = Instant::now();

            let topic = &publish.topic_name;
            let payload = Bytes::copy_from_slice(&publish.payload);

            // Extract QoS and packet ID from QosPid enum
            let (qos, pkid) = match &publish.qospid {
                QosPid::AtMostOnce => (rumqttc::QoS::AtMostOnce, None),
                QosPid::AtLeastOnce(pid) => (rumqttc::QoS::AtLeastOnce, Some(*pid)),
                QosPid::ExactlyOnce(pid) => (rumqttc::QoS::ExactlyOnce, Some(*pid)),
            };

            // Increment received message counter
            if let Some(counter) = messages_received {
                counter.fetch_add(1, Ordering::Relaxed);
            }

            info!(
                "📨 PUBLISH from '{}': topic='{}', payload_size={} bytes, qos={:?}, retain={}",
                client_id,
                topic,
                payload.len(),
                qos,
                publish.retain
            );

            // Debug: Log payload content (first 100 bytes)
            if payload.len() > 0 {
                let preview = if payload.len() <= 100 {
                    String::from_utf8_lossy(&payload).to_string()
                } else {
                    format!("{}... (truncated)", String::from_utf8_lossy(&payload[..100]))
                };
                debug!("📄 Payload preview: {}", preview);
            }

            // Broadcast to WebSocket clients
            if let Some(tx) = message_tx {
                let qos_u8 = match qos {
                    rumqttc::QoS::AtMostOnce => 0,
                    rumqttc::QoS::AtLeastOnce => 1,
                    rumqttc::QoS::ExactlyOnce => 2,
                };

                let mqtt_msg = crate::web_server::MqttMessage {
                    timestamp: chrono::Utc::now(),
                    client_id: client_id.clone(),
                    topic: topic.to_string(),
                    payload: payload.to_vec(),
                    qos: qos_u8,
                    retain: publish.retain,
                };

                // Send to WebSocket subscribers (ignore if no subscribers)
                let _ = tx.send(mqtt_msg);
            }

            // Forward to all downstream brokers
            let manager = connection_manager.read().await;
            match manager.forward_message(topic, payload, qos, publish.retain, messages_forwarded).await {
                Ok(_) => {
                    info!("✅ Message forwarded to all brokers: topic='{}'", topic);
                }
                Err(e) => {
                    warn!("⚠️  Failed to forward message: {}", e);
                }
            }

            // Record latency
            let elapsed = start.elapsed();
            if let Some(latency_counter) = total_latency_ns {
                latency_counter.fetch_add(elapsed.as_nanos() as u64, Ordering::Relaxed);
            }

            // Send PUBACK if QoS 1
            if let Some(pid) = pkid {
                if matches!(qos, rumqttc::QoS::AtLeastOnce) {
                    // Get the packet ID as u16
                    let pid_bytes = format!("{:?}", pid); // Format: "Pid(123)"
                    if let Some(num_str) = pid_bytes.strip_prefix("Pid(").and_then(|s| s.strip_suffix(")")) {
                        if let Ok(pid_u16) = num_str.parse::<u16>() {
                            // PUBACK: Fixed header (0x40) + Remaining length (0x02) + Packet ID (2 bytes, big-endian)
                            let puback_bytes = vec![0x40u8, 0x02, (pid_u16 >> 8) as u8, (pid_u16 & 0xFF) as u8];
                            if to_client_tx.send(ClientWrite::RawPacket(puback_bytes)).await.is_ok() {
                                debug!("Sent PUBACK to client '{}' for packet {}", client_id, pid_u16);
                            }
                        }
                    }
                }
            }

            Ok(true)
        }

        Packet::Pingreq => {
            debug!("PINGREQ from client '{}'", client_id);
            // PINGRESP: Fixed header (0xD0) + Remaining length (0x00)
            let pingresp_bytes = vec![0xD0u8, 0x00];
            to_client_tx.send(ClientWrite::RawPacket(pingresp_bytes)).await
                .context("Failed to send PINGRESP")?;
            debug!("Sent PINGRESP to client '{}'", client_id);
            Ok(true)
        }

        Packet::Subscribe(subscribe) => {
            let topics: Vec<String> = subscribe.topics.iter().map(|t| t.topic_path.to_string()).collect();
            info!("SUBSCRIBE from client '{}': topics={:?}", client_id, topics);

            // Add subscriptions to client registry
            let subscribed_topics = client_registry.add_subscriptions(client_id, topics.clone()).await;

            // Subscribe to these topics on all bidirectional brokers
            if !subscribed_topics.is_empty() {
                let manager = connection_manager.read().await;
                manager.subscribe_to_topics(&subscribed_topics).await;
            }

            // Send SUBACK
            let suback = Packet::Suback(Suback {
                pid: subscribe.pid,
                return_codes: subscribe.topics.iter().map(|_| SubscribeReturnCodes::Success(QoS::AtMostOnce)).collect(),
            });

            send_packet(to_client_tx, &suback).await?;
            debug!("Sent SUBACK to client '{}'", client_id);
            Ok(true)
        }

        Packet::Unsubscribe(unsubscribe) => {
            let topics: Vec<String> = unsubscribe.topics.iter().map(|t| t.to_string()).collect();
            info!("UNSUBSCRIBE from client '{}': topics={:?}", client_id, topics);

            // Remove subscriptions from client registry
            client_registry.remove_subscriptions(client_id, &topics).await;

            // Unsubscribe from brokers (only if no other clients are subscribed)
            // Note: For simplicity, we'll keep broker subscriptions active
            // A more advanced implementation would track subscription counts

            let unsuback = Packet::Unsuback(unsubscribe.pid);
            send_packet(to_client_tx, &unsuback).await?;
            Ok(true)
        }

        Packet::Disconnect => {
            info!("DISCONNECT from client '{}'", client_id);
            Ok(false)
        }

        other => {
            debug!("Received packet from '{}': {:?}", client_id, other);
            Ok(true)
        }
    }
}

async fn send_packet<'a>(to_client_tx: &mpsc::Sender<ClientWrite>, packet: &Packet<'a>) -> Result<()> {
    // Use a fixed-size buffer for encoding
    let mut buf = vec![0u8; 4096];

    let bytes_written = encode_slice(packet, &mut buf)
        .map_err(|e| anyhow::anyhow!("Failed to encode packet: {:?}", e))?;

    debug!("Encoded packet: {} bytes", bytes_written);
    to_client_tx.send(ClientWrite::RawPacket(buf[..bytes_written].to_vec())).await
        .context("Failed to send packet")?;
    Ok(())
}
