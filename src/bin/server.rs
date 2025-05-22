use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde_json::json;
use std::error::Error;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::{channel, Sender};
use tokio_websockets::{Message, ServerBuilder, WebSocketStream};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize)]
struct MessageData {
    from: String,
    message: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct WebSocketMessage {
    message_type: String,
    data: Option<String>,
    data_array: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
struct ServerState {
    users: Arc<Mutex<HashMap<SocketAddr, String>>>,
}

async fn handle_connection(
    addr: SocketAddr,
    mut ws_stream: WebSocketStream<TcpStream>,
    bcast_tx: Sender<String>,
    state: Arc<Mutex<HashMap<SocketAddr, String>>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut bcast_rx = bcast_tx.subscribe();

    loop {
        tokio::select! {
            incoming = ws_stream.next() => {
                match incoming {
                    Some(Ok(msg)) => {
                        if let Some(text) = msg.as_text() {
                            println!("From client {addr:?} {text:?}");

                            // Deserialize message
                            let parsed: WebSocketMessage = match serde_json::from_str(text) {
                                Ok(msg) => msg,
                                Err(_) => continue,
                            };

                            println!("{}",parsed.message_type.as_str());

                            match parsed.message_type.as_str() {
                                "register" => {
                                    if let Some(username) = parsed.data {
                                        state.lock().unwrap().insert(addr, username);
                                        // broadcast daftar user
                                        let users: Vec<String> = state.lock().unwrap().values().cloned().collect();
                                        let msg_out = WebSocketMessage {
                                            message_type: "Users".to_string(),
                                            data: None,
                                            data_array: Some(users),
                                        };
                                        bcast_tx.send(serde_json::to_string(&msg_out).unwrap())?;
                                    }
                                }
                                "message" => {
                                    if let Some(content) = parsed.data {
                                        let from = state.lock().unwrap().get(&addr).cloned().unwrap_or("unknown".into());
                                        let msg_payload = json!({
                                            "from": from,
                                            "message": content
                                        });

                                        let msg_out = WebSocketMessage {
                                            message_type: "Message".to_string(),
                                            data: Some(serde_json::to_string(&MessageData {
                                                from,
                                                message: content,
                                            }).unwrap()),
                                            data_array: None,
                                        };

                                        bcast_tx.send(serde_json::to_string(&msg_out).unwrap())?;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Err(err)) => return Err(err.into()),
                    None => {
                        state.lock().unwrap().remove(&addr);
                        return Ok(());
                    }
                }
            }
            msg = bcast_rx.recv() => {
                ws_stream.send(Message::text(msg?)).await?;
            }
        }
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (bcast_tx, _) = channel(16);
    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("listening on port 8080");

    let state = Arc::new(Mutex::new(HashMap::new()));

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {addr:?}");

        let bcast_tx = bcast_tx.clone();
        let state = state.clone();

        tokio::spawn(async move {
            let ws_stream = ServerBuilder::new().accept(socket).await?;
            handle_connection(addr, ws_stream, bcast_tx, state).await
        });
    }
}
