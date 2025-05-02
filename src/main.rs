use axum::{
    routing::{get},
    http::StatusCode,
    Json, Router, extract::State,
    response::{IntoResponse},
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, net::SocketAddr};
use tokio::net::TcpListener;
use tracing::{info, error};
use tracing_subscriber;
use anyhow::anyhow;
use reqwest::{Client, header};
use serde_json::json;
use std::time::Duration;
mod mqtt; // <-- new
use mqtt::{start_mqtt_listener, MqttCommand};
use tokio::sync::mpsc;

/// Configuration for AdGuard Home API connection
#[derive(Clone, Debug, Deserialize)]
pub struct AdGuardConfig {
    /// The base URL of your AdGuard Home instance (e.g., "http://192.168.1.10:3000")
    pub base_url: String,
    /// Username for authentication
    pub username: String,
    /// Password for authentication
    pub password: String,
    /// Timeout in seconds for API requests
    pub timeout_seconds: u64,
}

#[derive(Clone)]
struct AppState {
    client: AdGuardClient,
}

/// Client for AdGuard Home API operations
#[derive(Clone)]
pub struct AdGuardClient {
    config: AdGuardConfig,
    client: Client,
}

#[derive(Serialize, Deserialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

impl AdGuardClient {
    /// Create a new AdGuard Home API client
    pub fn new(config: AdGuardConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to build HTTP client");

        Self { config, client }
    }

    /// Login to AdGuard Home and get authentication token
    pub async fn login(&self) -> Result<String, anyhow::Error> {
        let login_url = format!("{}/control/login", self.config.base_url);
        
        info!("Sending login request to: {}", login_url);
        
        let auth_payload = json!({
            "name": self.config.username,
            "password": self.config.password
        });
    
        // Send the request
        let response = self.client
            .post(&login_url)
            .json(&auth_payload)
            .send()
            .await?;
    
        // Extract cookies from the response headers before consuming the response
        let headers = response.headers().clone();
        if let Some(cookies) = headers.get(header::SET_COOKIE) {
            info!("Login successful, received cookies");
            let cookies_str = cookies.to_str().unwrap_or_default().to_string();
            return Ok(cookies_str);
        }
    
        // Get the response text for debugging if no cookies were found
        let response_text = response.text().await?;
        error!("Login response body with no cookies: {}", response_text);
    
        // Return a custom error when no cookies are found
        Err(anyhow!("No cookies found in response"))
    }

    /// Disable YouTube blocking
    async fn disable_youtube(&self, token: &str) -> Result<(), reqwest::Error> {
        let disable_url = format!("{}/control/blocked_services/update", self.config.base_url);
        info!("Disabling YouTube blocking at: {}", disable_url);
        
        let disable_payload = json!({
            "ids": ["youtube"],
            "schedule": {
                "time_zone": "Local"
            }
        });

        let response = self.client
            .put(&disable_url)
            .header(header::COOKIE, token)
            .json(&disable_payload)
            .send()
            .await?;

        info!("Disable YouTube response status: {}", response.status());
        let response_text = response.text().await?;
        info!("Response body: {}", response_text);

        Ok(())
    }
    
    /// Enable YouTube blocking
    async fn enable_youtube(&self, token: &str) -> Result<(), reqwest::Error> {
        let enable_url = format!("{}/control/blocked_services/update", self.config.base_url);
        info!("Enabling YouTube blocking at: {}", enable_url);
        
        let enable_payload = json!({
            "ids": [],
            "schedule": {
                "time_zone": "Local"
            }
        });

        let response = self.client
            .put(&enable_url)
            .header(header::COOKIE, token)
            .json(&enable_payload)
            .send()
            .await?;

        info!("Enable YouTube response status: {}", response.status());
        let response_text = response.text().await?;
        info!("Response body: {}", response_text);

        Ok(())
    }
}

async fn enable_youtube_handler(
    State(state): State<Arc<AppState>>
) -> impl IntoResponse {
    match state.client.login().await {
        Ok(token) => {
            match state.client.enable_youtube(&token).await {
                Ok(_) => {
                    info!("YouTube blocking enabled successfully");
                    (
                        StatusCode::OK, 
                        Json(ApiResponse {
                            success: true,
                            message: "YouTube blocking enabled successfully".to_string(),
                        })
                    )
                },
                Err(e) => {
                    error!("Failed to enable YouTube blocking: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            message: format!("Failed to enable YouTube blocking: {}", e),
                        })
                    )
                }
            }
        },
        Err(e) => {
            error!("Failed to login: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse {
                    success: false,
                    message: format!("Failed to login to AdGuard Home: {}", e),
                })
            )
        }
    }
}

async fn disable_youtube_handler(
    State(state): State<Arc<AppState>>
) -> impl IntoResponse {
    match state.client.login().await {
        Ok(token) => {
            match state.client.disable_youtube(&token).await {
                Ok(_) => {
                    info!("YouTube blocking disabled successfully");
                    (
                        StatusCode::OK, 
                        Json(ApiResponse {
                            success: true,
                            message: "YouTube blocking disabled successfully".to_string(),
                        })
                    )
                },
                Err(e) => {
                    error!("Failed to disable YouTube blocking: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiResponse {
                            success: false,
                            message: format!("Failed to disable YouTube blocking: {}", e),
                        })
                    )
                }
            }
        },
        Err(e) => {
            error!("Failed to login: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse {
                    success: false,
                    message: format!("Failed to login to AdGuard Home: {}", e),
                })
            )
        }
    }
}

async fn status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            message: "AdGuard YouTube API is running".to_string(),
        })
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load AdGuard configuration
    let config = AdGuardConfig {
        base_url: std::env::var("ADGUARD_URL").unwrap_or_else(|_| "default ip".to_string()),
        username: std::env::var("ADGUARD_USERNAME").unwrap_or_else(|_| "default username".to_string()),
        password: std::env::var("ADGUARD_PASSWORD").unwrap_or_else(|_| "default password".to_string()),
        timeout_seconds: std::env::var("ADGUARD_TIMEOUT")
            .unwrap_or_else(|_| "30".to_string())
            .parse()
            .unwrap_or(30),
    };

    // Load MQTT configuration
    let mqtt_host = std::env::var("MQTT_HOST").unwrap_or_else(|_| "broker.emqx.io".to_string());
    let mqtt_port = std::env::var("MQTT_PORT").unwrap_or_else(|_| "1883".to_string()).parse::<u16>().unwrap_or(1883);
    let mqtt_username = std::env::var("MQTT_USERNAME").unwrap_or_else(|_| "".to_string());
    let mqtt_password = std::env::var("MQTT_PASSWORD").unwrap_or_else(|_| "".to_string());
    let mqtt_topic = std::env::var("MQTT_TOPIC").unwrap_or_else(|_| "/home/adGuardhomeyoutube/youtube".to_string());

    info!("Starting AdGuard YouTube API and MQTT listener");

    let client = AdGuardClient::new(config);
    let state = Arc::new(AppState { client: client.clone() });

    // Set up channel to receive MQTT commands
    let (tx, mut rx) = mpsc::channel(10);

    // Start MQTT listener
    tokio::spawn(async move {
        start_mqtt_listener(tx, &mqtt_host, mqtt_port, &mqtt_username, &mqtt_password, &mqtt_topic).await;
    });

    // Handle MQTT commands in background
    let command_state = state.clone();
    tokio::spawn(async move {
        while let Some(command) = rx.recv().await {
            match command {
                MqttCommand::EnableYouTube => {
                    if let Ok(token) = command_state.client.login().await {
                        if let Err(e) = command_state.client.enable_youtube(&token).await {
                            error!("Failed to enable YouTube: {}", e);
                        }
                    }
                }
                MqttCommand::DisableYouTube => {
                    if let Ok(token) = command_state.client.login().await {
                        if let Err(e) = command_state.client.disable_youtube(&token).await {
                            error!("Failed to disable YouTube: {}", e);
                        }
                    }
                }
            }
        }
    });

    // Build Axum router
    let app = Router::new()
        .route("/", get(status_handler))
        .route("/youtube/enable", get(enable_youtube_handler))
        .route("/youtube/disable", get(disable_youtube_handler))
        .with_state(state);

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string()).parse::<u16>().unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!("Listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}