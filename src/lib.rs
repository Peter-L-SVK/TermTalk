/*
 * TermTalk - Shared Library Module
 * Version: 0.8
 * Copyright (c) 2025-2026 Peter Leukanič
 * Under MIT License
 *
 * Core client handling and message routing functionality
 */

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};
use utils::write_to_stream;

mod utils;

async fn send_user_list(
    write_stream: &Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    token_username_map: &Arc<Mutex<HashMap<usize, String>>>,
) -> std::io::Result<()> {
    let map = token_username_map.lock().await;
    let user_list = map.values().cloned().collect::<Vec<String>>().join(", ");
    write_to_stream(write_stream, &format!("USERLIST: {}\n", user_list)).await
}

pub async fn handle_client(
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    write_stream: Arc<Mutex<tokio::net::tcp::OwnedWriteHalf>>,
    sender: broadcast::Sender<(String, String)>,
    mut receiver: broadcast::Receiver<(String, String)>,
    my_username: String,
    client_token: usize,
    token_username_map: Arc<Mutex<HashMap<usize, String>>>,
) {
    println!(
        "DEBUG: Handling client {} with username: {}",
        client_token, my_username
    );

    let mut reader = reader;
    let write_stream_clone = Arc::clone(&write_stream);

    // Handle incoming messages from client
    let message_handler = tokio::spawn(async move {
        println!(
            "DEBUG: Spawning task for client {} messages and pings",
            client_token
        );

        let mut buf = String::new();
        loop {
            buf.clear();

            match timeout(Duration::from_secs(15), reader.read_line(&mut buf)).await {
                Ok(Ok(n)) if n > 0 => {
                    let trimmed = buf.trim();

                    if trimmed == "PONG" {
                        println!("DEBUG: Received PONG from client {}", client_token);
                        continue;
                    }

                    if trimmed == "GET_USERLIST" {
                        if send_user_list(&write_stream_clone, &token_username_map)
                            .await
                            .is_err()
                        {
                            println!("DEBUG: Failed to send user list to client {}", client_token);
                        }
                        continue;
                    }

                    // Broadcast raw message content
                    println!(
                        "DEBUG: Broadcasting message from client {}: {}",
                        client_token, trimmed
                    );
                    let _ = sender.send((my_username.clone(), trimmed.to_string()));
                }
                Ok(Ok(_)) | Ok(Err(_)) => {
                    println!("DEBUG: Client {} disconnected", client_token);
                    break;
                }
                Err(_) => {
                    println!("DEBUG: Timeout from client {}, sending PING", client_token);
                    if write_stream_clone
                        .lock()
                        .await
                        .write_all(b"PING\n")
                        .await
                        .is_err()
                    {
                        println!("DEBUG: Failed to send PING to client {}", client_token);
                        break;
                    }
                }
            }
        }

        // Broadcast disconnect
        let disconnect_message = format!("{} has left the chat!", my_username);
        println!(
            "DEBUG: Broadcasting disconnect message: {}",
            disconnect_message
        );
        let _ = sender.send(("SERVER".to_string(), disconnect_message));

        // Clean up user from map
        let mut map = token_username_map.lock().await;
        map.remove(&client_token);
        println!(
            "DEBUG: Removed token-username mapping: {} -> {}",
            client_token, my_username
        );

        // Send updated user list
        let user_list = map.values().cloned().collect::<Vec<String>>().join(", ");
        let _ = sender.send(("SERVER".to_string(), format!("USERLIST: {}", user_list)));
    });

    // Handle forwarding broadcast messages to client
    let write_stream_clone = Arc::clone(&write_stream);
    let broadcast_handler = tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok((sender_username, message)) => {
                    let formatted = if sender_username == "SERVER" {
                        format!("SERVER: {}\n", message)
                    } else {
                        format!("{}: {}\n", sender_username, message)
                    };

                    if write_stream_clone
                        .lock()
                        .await
                        .write_all(formatted.as_bytes())
                        .await
                        .is_err()
                    {
                        println!(
                            "DEBUG: Failed to forward message to client {}",
                            client_token
                        );
                        break;
                    }
                }
                Err(_) => {
                    println!(
                        "DEBUG: Broadcast channel closed for client {}",
                        client_token
                    );
                    break;
                }
            }
        }
    });

    let (message_handler_result, broadcast_handler_result) =
        tokio::join!(message_handler, broadcast_handler);
    if let Err(e) = message_handler_result {
        println!("DEBUG: Message handler task failed: {:?}", e);
    }

    if let Err(e) = broadcast_handler_result {
        println!("DEBUG: Broadcast handler task failed: {:?}", e);
    }

    println!("DEBUG: Exiting handle_client for client {}", client_token);
}
