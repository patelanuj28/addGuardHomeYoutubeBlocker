use rumqttc::{AsyncClient, MqttOptions, QoS, Event, Incoming, Transport};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{info, error};
use uuid::Uuid;

#[derive(Debug)]
pub enum MqttCommand {
    EnableYouTube,
    DisableYouTube,
}

pub async fn start_mqtt_listener(
    tx: mpsc::Sender<MqttCommand>,
    mqtt_host: &str,
    mqtt_port: u16,
    mqtt_username: &str,
    mqtt_password: &str,
    topic: &str,
) {
    let client_id = format!("adguard_controller_{}", Uuid::new_v4());
    let mut mqttoptions = MqttOptions::new(client_id, mqtt_host, mqtt_port);

    mqttoptions.set_keep_alive(Duration::from_secs(10));
    mqttoptions.set_credentials(mqtt_username, mqtt_password);

    // 🌟 Use Rustls-based TLS transport
    mqttoptions.set_transport(Transport::tls_with_default_config());

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 10);

    client.subscribe(topic, QoS::AtMostOnce).await.unwrap();
    info!("Subscribed to MQTT topic: {}", topic);

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::Publish(p))) => {
                let payload = String::from_utf8_lossy(&p.payload);
                info!("Received MQTT message: {}", payload);

                match payload.trim() {
                    "enable" => {
                        if let Err(e) = tx.send(MqttCommand::EnableYouTube).await {
                            error!("Failed to send enable command: {}", e);
                        }
                    }
                    "disable" => {
                        if let Err(e) = tx.send(MqttCommand::DisableYouTube).await {
                            error!("Failed to send disable command: {}", e);
                        }
                    }
                    other => {
                        error!("Unknown MQTT command received: {}", other);
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                error!("MQTT error: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
