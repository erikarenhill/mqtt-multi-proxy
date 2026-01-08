use anyhow::Result;
use mqtt_proxy::{config::Config, proxy::MqttProxy};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mqtt_proxy=info,rumqttc=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    tracing::info!("Starting MQTT Proxy");

    // Load configuration
    let config = Config::from_env()?;
    tracing::info!("Configuration loaded: {:?}", config);

    // Create and start proxy
    let proxy = MqttProxy::new(config).await?;
    proxy.run().await?;

    Ok(())
}
