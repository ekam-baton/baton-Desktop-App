use clap::{Parser, Subcommand};
use futures::{SinkExt, StreamExt};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "baton-tester")]
#[command(about = "Load testing agent for baton-backend", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "ws://localhost:8080")]
    url: String,

    #[arg(short, long, default_value = "baton-dev-secret-change-in-production")]
    secret: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Simulates a stampede of clients connecting at once
    Storm {
        #[arg(short, long, default_value_t = 1000)]
        clients: usize,
    },
    /// Connects clients and holds the connection open to test memory footprint
    Idle {
        #[arg(short, long, default_value_t = 5000)]
        clients: usize,
    },
    /// Connects clients, waits, then one client sends a message to all others
    Burst {
        #[arg(short, long, default_value_t = 1000)]
        clients: usize,
    },
    /// Sends messages to offline clients, then connects them to measure drain speed
    OfflineDrain {
        #[arg(short, long, default_value_t = 1000)]
        clients: usize,
        #[arg(short, long, default_value_t = 5)]
        messages_per_client: usize,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct A2AMessage {
    sender_id: String,
    receiver_id: String,
    payload: serde_json::Value,
}

fn mint_token(client_id: &str, secret: &str) -> String {
    let exp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() + 86400; // 24 hours
        
    let claims = Claims {
        sub: client_id.to_string(),
        exp: exp as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

use tokio_tungstenite::tungstenite::client::IntoClientRequest;

async fn connect_client(client_id: &str, url: &str, secret: &str) -> Option<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
    let token = mint_token(client_id, secret);
    
    let req_url = format!("{}/ws/{}", url, client_id);
    let mut request = match req_url.into_client_request() {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse URL: {}", e);
            return None;
        }
    };
    
    let auth_header = format!("Bearer {}", token);
    request.headers_mut().insert(
        "Authorization",
        auth_header.parse().unwrap(),
    );

    match connect_async(request).await {
        Ok((ws_stream, _)) => Some(ws_stream),
        Err(e) => {
            error!("Failed to connect {}: {}", client_id, e);
            None
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Storm { clients } => {
            info!("Starting Storm scenario with {} clients", clients);
            let start = Instant::now();
            let mut tasks = vec![];
            
            for i in 0..clients {
                let url = cli.url.clone();
                let secret = cli.secret.clone();
                let cid = format!("storm_user_{}", i);
                
                tasks.push(tokio::spawn(async move {
                    if let Some(mut ws) = connect_client(&cid, &url, &secret).await {
                        // Just hold for a few seconds to let everyone connect
                        sleep(Duration::from_secs(10)).await;
                        let _ = ws.close(None).await;
                    } else {
                        warn!("Failed to connect {}", cid);
                    }
                }));
            }
            
            futures::future::join_all(tasks).await;
            info!("Storm scenario finished in {:?}", start.elapsed());
        }
        Commands::Idle { clients } => {
            info!("Starting Idle scenario with {} clients", clients);
            let mut tasks = vec![];
            
            for i in 0..clients {
                let url = cli.url.clone();
                let secret = cli.secret.clone();
                let cid = format!("idle_user_{}", i);
                
                tasks.push(tokio::spawn(async move {
                    if let Some(mut ws) = connect_client(&cid, &url, &secret).await {
                        // Hold open forever (or until interrupted)
                        while let Some(Ok(_)) = ws.next().await {}
                    }
                }));
                
                // Slight stagger to avoid completely destroying local ephemeral ports instantly
                if i % 100 == 0 {
                    sleep(Duration::from_millis(50)).await;
                }
            }
            
            info!("All {} idle clients spawned. Holding...", clients);
            futures::future::join_all(tasks).await;
        }
        Commands::Burst { clients } => {
            info!("Starting Burst scenario with {} clients", clients);
            let mut tasks = vec![];
            let (tx, _rx) = tokio::sync::broadcast::channel(clients);
            
            for i in 0..clients {
                let url = cli.url.clone();
                let secret = cli.secret.clone();
                let cid = format!("burst_user_{}", i);
                let mut b_rx = tx.subscribe();
                
                tasks.push(tokio::spawn(async move {
                    if let Some(mut ws) = connect_client(&cid, &url, &secret).await {
                        // Wait for the go signal
                        let _ = b_rx.recv().await;
                        
                        if i == 0 {
                            // User 0 sends to everyone else
                            let start = Instant::now();
                            for j in 1..clients {
                                let msg = A2AMessage {
                                    sender_id: cid.clone(),
                                    receiver_id: format!("burst_user_{}", j),
                                    payload: serde_json::json!({"test": "burst"}),
                                };
                                let _ = ws.send(Message::Text(serde_json::to_string(&msg).unwrap())).await;
                            }
                            info!("User 0 finished dispatching in {:?}", start.elapsed());
                        } else {
                            // Others receive
                            if let Some(Ok(Message::Text(_))) = ws.next().await {
                                // Received
                            }
                        }
                    }
                }));
                
                if i % 100 == 0 {
                    sleep(Duration::from_millis(50)).await;
                }
            }
            
            sleep(Duration::from_secs(2)).await;
            info!("Broadcasting GO signal");
            let _ = tx.send(());
            futures::future::join_all(tasks).await;
            info!("Burst complete");
        }
        Commands::OfflineDrain { clients, messages_per_client } => {
            info!("Starting OfflineDrain scenario: {} clients, {} msgs each", clients, messages_per_client);
            
            // 1. Send messages while they are offline (User 0 is the sender)
            let url = cli.url.clone();
            let secret = cli.secret.clone();
            let sender_cid = "drain_sender".to_string();
            
            if let Some(mut ws) = connect_client(&sender_cid, &url, &secret).await {
                info!("Populating offline queue...");
                let start = Instant::now();
                for i in 0..clients {
                    let target_cid = format!("drain_user_{}", i);
                    for _ in 0..messages_per_client {
                        let msg = A2AMessage {
                            sender_id: sender_cid.clone(),
                            receiver_id: target_cid.clone(),
                            payload: serde_json::json!({"data": "offline queue data"}),
                        };
                        let _ = ws.send(Message::Text(serde_json::to_string(&msg).unwrap())).await;
                    }
                }
                let _ = ws.close(None).await;
                info!("Queue populated in {:?}", start.elapsed());
            } else {
                error!("Sender failed to connect");
                return;
            }
            
            sleep(Duration::from_secs(2)).await; // give PG time to flush
            
            // 2. Connect clients and measure time to drain
            info!("Connecting clients to drain queue...");
            let start = Instant::now();
            let mut tasks = vec![];
            
            for i in 0..clients {
                let url = cli.url.clone();
                let secret = cli.secret.clone();
                let cid = format!("drain_user_{}", i);
                let expected = messages_per_client;
                
                tasks.push(tokio::spawn(async move {
                    if let Some(mut ws) = connect_client(&cid, &url, &secret).await {
                        let mut received = 0;
                        while let Some(Ok(Message::Text(_))) = ws.next().await {
                            received += 1;
                            if received == expected {
                                break;
                            }
                        }
                    }
                }));
            }
            
            futures::future::join_all(tasks).await;
            info!("All offline queues drained in {:?}", start.elapsed());
        }
    }
}
