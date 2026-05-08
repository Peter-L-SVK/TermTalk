/*
 * TermTalk - Utility Functions Module
 * Version: 0.8
 * Copyright (c) 2025-2026 Peter Leukanič
 * Under MIT License
 *
 * Shared utility functions for stream I/O and message formatting
 */

use std::marker::Unpin;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

// Helper function to write to a stream
pub async fn write_to_stream(
    write_stream: &Arc<Mutex<impl AsyncWriteExt + Unpin>>,
    message: &str,
) -> std::io::Result<()> {
    let mut stream = write_stream.lock().await;
    stream.write_all(message.as_bytes()).await
}

// Message part types for TUI rendering
#[derive(Clone)]
pub enum MessagePart {
    Text(String),
    Colored(String, String), // (text, color_name)
}

#[derive(Clone)]
pub struct FormattedMessage {
    pub parts: Vec<MessagePart>,
    pub plain_text: String,
}

pub fn format_message(
    sender_username: &str,
    message: &str,
    is_server_message: bool,
    my_username: &str,
) -> FormattedMessage {
    let timestamp = chrono::Local::now().format("[%d.%m.%Y %H:%M]").to_string();
    let mut parts = Vec::new();
    let mut plain_parts = Vec::new();

    // Add timestamp
    parts.push(MessagePart::Text(format!("{} ", timestamp)));
    plain_parts.push(format!("{} ", timestamp));

    if is_server_message {
        // Server message
        parts.push(MessagePart::Colored(
            "SERVER".to_string(),
            "magenta".to_string(),
        ));
        parts.push(MessagePart::Text(format!(": {}", message.trim())));
        plain_parts.push(format!("SERVER: {}", message.trim()));
    } else {
        // User message
        let username_color = if sender_username == my_username {
            "green"
        } else {
            "blue"
        };

        parts.push(MessagePart::Colored(
            sender_username.to_string(),
            username_color.to_string(),
        ));
        parts.push(MessagePart::Text(": ".to_string()));
        plain_parts.push(format!("{}: ", sender_username));

        // Handle mentions
        let cleaned_message = message.trim();
        if cleaned_message.contains(&format!("@{}", my_username))
            || cleaned_message.contains("@all")
        {
            let words: Vec<&str> = cleaned_message.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                if i > 0 {
                    parts.push(MessagePart::Text(" ".to_string()));
                    plain_parts.push(" ".to_string());
                }

                if *word == &format!("@{}", my_username) || *word == "@all" {
                    parts.push(MessagePart::Colored(word.to_string(), "red".to_string()));
                } else {
                    parts.push(MessagePart::Text(word.to_string()));
                }
                plain_parts.push(word.to_string());
            }
        } else {
            parts.push(MessagePart::Text(cleaned_message.to_string()));
            plain_parts.push(cleaned_message.to_string());
        }
    }

    FormattedMessage {
        parts,
        plain_text: plain_parts.concat(),
    }
}
