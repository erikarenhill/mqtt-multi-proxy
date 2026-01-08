use prometheus::{
    register_histogram, register_int_counter, register_int_gauge, Histogram, IntCounter, IntGauge,
};
use std::sync::Arc;

pub struct Metrics {
    pub messages_received: IntCounter,
    pub messages_forwarded: IntCounter,
    pub message_latency: Histogram,
    pub active_connections: IntGauge,
    pub broker_connections: IntGauge,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            messages_received: register_int_counter!(
                "mqtt_messages_received_total",
                "Total number of messages received from devices"
            )
            .unwrap(),
            messages_forwarded: register_int_counter!(
                "mqtt_messages_forwarded_total",
                "Total number of messages forwarded to brokers"
            )
            .unwrap(),
            message_latency: register_histogram!(
                "mqtt_message_latency_seconds",
                "Message forwarding latency in seconds"
            )
            .unwrap(),
            active_connections: register_int_gauge!(
                "mqtt_active_connections",
                "Number of active device connections"
            )
            .unwrap(),
            broker_connections: register_int_gauge!(
                "mqtt_broker_connections",
                "Number of active broker connections"
            )
            .unwrap(),
        })
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new().as_ref().clone()
    }
}

impl Clone for Metrics {
    fn clone(&self) -> Self {
        Self {
            messages_received: self.messages_received.clone(),
            messages_forwarded: self.messages_forwarded.clone(),
            message_latency: self.message_latency.clone(),
            active_connections: self.active_connections.clone(),
            broker_connections: self.broker_connections.clone(),
        }
    }
}
