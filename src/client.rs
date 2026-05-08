/*
 * TermTalk - Terminal Chat Client
 * Version: 0.8
 * Copyright (c) 2025-2026 Peter Leukanič
 * Under MIT License
 *
 * Real-time terminal-based chat client with TUI interface
 */

use chrono::Local;
use crossterm::{
    event::{self, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::fs::OpenOptions;
use std::io::{self};
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tokio::time::{self, Duration};
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};

mod logging;
mod utils;

use logging::log_message;
use utils::{format_message, write_to_stream, FormattedMessage, MessagePart};

// Message buffer types
struct MessageBuffer {
    chat_messages: Vec<FormattedMessage>,
    user_list: Vec<String>,
    new_messages_count: usize,
}

impl MessageBuffer {
    fn new() -> Self {
        MessageBuffer {
            chat_messages: Vec::new(),
            user_list: Vec::new(),
            new_messages_count: 0,
        }
    }

    fn add_chat_message(&mut self, message: FormattedMessage) {
        self.chat_messages.push(message);
    }

    fn update_user_list(&mut self, user_list: String) {
        self.user_list.clear();
        if !user_list.is_empty() {
            for user in user_list.split(',') {
                self.user_list.push(format!("  • {}", user.trim()));
            }
        }
    }

    fn get_chat_messages(&self) -> &Vec<FormattedMessage> {
        &self.chat_messages
    }

    fn get_user_list(&self) -> &Vec<String> {
        &self.user_list
    }

    fn reset_new_messages_count(&mut self) {
        self.new_messages_count = 0;
    }

    fn increment_new_messages(&mut self) {
        self.new_messages_count += 1;
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("client.log")?;
    let log_file = Arc::new(Mutex::new(log_file));

    log_message(&log_file, "[DEBUG] Terminal initialized successfully").await;

    let (sender, mut receiver) = broadcast::channel::<(String, String)>(32);

    log_message(&log_file, "[DEBUG] Connecting to server...").await;
    let stream = match TcpStream::connect("127.0.0.1:8080").await {
        Ok(stream) => {
            log_message(&log_file, "[DEBUG] Connected to server").await;
            stream
        }
        Err(_) => {
            log_message(&log_file, "[DEBUG] Failed to connect to the server").await;
            return Ok(());
        }
    };
    let (read_stream, write_stream) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(read_stream);
    let write_stream = Arc::new(Mutex::new(write_stream));

    let mut token_message = String::new();
    if reader.read_line(&mut token_message).await.is_err() {
        log_message(&log_file, "[DEBUG] Failed to read token from server").await;
        return Ok(());
    }
    let client_token = token_message
        .trim()
        .split_whitespace()
        .last()
        .unwrap_or("unknown")
        .to_string();
    log_message(
        &log_file,
        &format!("[DEBUG] Client token: {}", client_token),
    )
    .await;

    // Username prompt
    let mut username = String::new();
    let mut error_message = String::new();
    loop {
        terminal.draw(|f| {
            let size = f.size();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(80), Constraint::Percentage(20)].as_ref())
                .split(size);

            let input_block = Paragraph::new(format!(
                "Enter your username: {}\n\nPress 'Esc' to quit.",
                username
            ))
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(input_block, chunks[0]);

            let error_spans = Spans::from(vec![Span::styled(
                error_message.clone(),
                Style::default().fg(Color::Red),
            )]);
            let error_block = Paragraph::new(Text::from(error_spans))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(error_block, chunks[1]);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => {
                    if username.trim().is_empty() {
                        error_message = "Error: Username cannot be empty!".to_string();
                    } else {
                        if write_to_stream(&write_stream, &format!("{}\n", username))
                            .await
                            .is_err()
                        {
                            log_message(&log_file, "[DEBUG] Failed to send username to server")
                                .await;
                            return Ok(());
                        }

                        let mut response = String::new();
                        if reader.read_line(&mut response).await.is_err() {
                            log_message(&log_file, "[DEBUG] Failed to read server response").await;
                            return Ok(());
                        }

                        log_message(
                            &log_file,
                            &format!("[DEBUG] Server response: {}", response.trim()),
                        )
                        .await;
                        let response = response
                            .trim()
                            .strip_prefix("Enter your username: ")
                            .unwrap_or(response.trim());

                        if response == "SUCCESS: Username accepted." {
                            break;
                        } else if response
                            == "ERROR: Username is already taken. Please choose a different one."
                        {
                            error_message = response.to_string();
                            username.clear();
                            continue;
                        } else {
                            error_message =
                                "Unexpected server response. Please try again.".to_string();
                            username.clear();
                            continue;
                        }
                    }
                }
                KeyCode::Backspace => {
                    username.pop();
                    error_message.clear();
                }
                KeyCode::Char(c) => {
                    username.push(c);
                    error_message.clear();
                }
                KeyCode::Esc => {
                    log_message(&log_file, "[DEBUG] Quitting application from login screen").await;
                    disable_raw_mode()?;
                    execute!(
                        terminal.backend_mut(),
                        LeaveAlternateScreen,
                        crossterm::event::DisableMouseCapture
                    )?;
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    log_message(&log_file, "[DEBUG] Transitioning to chat state").await;
    terminal.clear()?;

    let message_buffer = Arc::new(Mutex::new(MessageBuffer::new()));

    // Spawn message handler task
    let sender_clone = sender.clone();
    let username_clone = username.clone();
    let log_file_clone = Arc::clone(&log_file);
    let client_token_clone = client_token.clone();
    let write_stream_clone = Arc::clone(&write_stream);
    tokio::spawn(async move {
        log_message(
            &log_file_clone,
            &format!(
                "[DEBUG] Spawning client handler task for token {}",
                client_token_clone
            ),
        )
        .await;
        let mut input = String::new();
        loop {
            input.clear();
            if reader.read_line(&mut input).await.is_err() {
                log_message(&log_file_clone, "[DEBUG] Disconnected from server").await;
                break;
            }

            if input.trim() == "PING" {
                if write_to_stream(&write_stream_clone, "PONG\n")
                    .await
                    .is_err()
                {
                    log_message(&log_file_clone, "[DEBUG] Failed to send pong to server").await;
                    break;
                }
                continue;
            }

            log_message(
                &log_file_clone,
                &format!(
                    "[DEBUG] Received from server for token {}: {}",
                    client_token_clone,
                    input.trim()
                ),
            )
            .await;
            let _ = sender_clone.send((username_clone.clone(), input.trim().to_string()));
        }
        log_message(
            &log_file_clone,
            &format!(
                "[DEBUG] Client handler task for token {} exited",
                client_token_clone
            ),
        )
        .await;
    });

    // Main chat loop
    let mut input_text = String::new();
    let mut scroll_offset: u16 = 0;
    let mut show_user_list = false;
    let mut needs_redraw = true;
    let mut max_scroll: u16 = 0;

    loop {
        // Process messages from broadcast channel
        {
            let mut buffer = message_buffer.lock().await;
            while let Ok((sender_username, message)) = receiver.try_recv() {
                if message.starts_with("USERLIST:") {
                    let user_list = message.trim_start_matches("USERLIST:").trim();
                    buffer.update_user_list(user_list.to_string());
                    needs_redraw = true;
                    continue;
                }

                if message.contains("Enter your username:") {
                    continue;
                }

                let (display_username, content) = if message.starts_with("SERVER:") {
                    let content = message["SERVER:".len()..].trim();
                    ("SERVER", content)
                } else if let Some(colon_pos) = message.find(": ") {
                    let msg_username = &message[..colon_pos];
                    let msg_content = message[colon_pos + 2..].trim();
                    (msg_username, msg_content)
                } else {
                    (sender_username.as_str(), message.trim())
                };

                let is_server_message = display_username == "SERVER";
                let formatted_message =
                    format_message(display_username, content, is_server_message, &username);

                buffer.add_chat_message(formatted_message);

                if show_user_list {
                    buffer.increment_new_messages();
                }

                log_message(
                    &log_file,
                    &format!(
                        "[DEBUG] Buffered message from {}: {}",
                        display_username, content
                    ),
                )
                .await;
                needs_redraw = true;
            }
        }

        // Redraw UI
        if needs_redraw {
            terminal.clear()?;

            let buffer = message_buffer.lock().await;
            let chat_messages = buffer.get_chat_messages();
            let user_list = buffer.get_user_list();
            let new_count = buffer.new_messages_count;
            let username_clone = username.clone();
            let input_text_clone = input_text.clone();

            // Calculate scroll boundaries
            let total_lines = if show_user_list {
                user_list.len() + 3
            } else {
                chat_messages.len()
            };

            let terminal_height = terminal.size()?.height;
            let visible_area_height = ((terminal_height as f32) * 0.9) as usize;
            let visible_lines = if visible_area_height > 2 {
                visible_area_height - 2
            } else {
                1
            };

            max_scroll = if total_lines > visible_lines {
                (total_lines - visible_lines) as u16
            } else {
                0
            };

            if !show_user_list {
                scroll_offset = max_scroll;
            }

            if scroll_offset > max_scroll {
                scroll_offset = max_scroll;
            }

            // Clone the data we need for drawing
            let chat_messages_clone: Vec<FormattedMessage> =
                chat_messages.iter().cloned().collect();
            let user_list_clone = user_list.clone();

            drop(buffer);

            terminal.draw(|f| {
                let size = f.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(3), Constraint::Length(3)].as_ref())
                    .split(size);

                if show_user_list {
                    let user_list_display = if user_list_clone.is_empty() {
                        vec!["  No users online".to_string()]
                    } else {
                        user_list_clone.clone()
                    };

                    let mut display_lines = user_list_display;
                    display_lines.push(String::new());

                    if new_count > 0 {
                        display_lines.push(format!("  ⚠ {} new message(s) received", new_count));
                        display_lines.push(String::new());
                    }

                    display_lines.push("  Press 'r' to return to chat".to_string());
                    display_lines.push("  Use ↑↓ or scroll wheel to navigate".to_string());

                    let user_list_text = display_lines.join("\n");

                    let user_list_block = Paragraph::new(user_list_text)
                        .block(Block::default().borders(Borders::ALL).title(" User List "))
                        .scroll((scroll_offset, 0));
                    f.render_widget(user_list_block, chunks[0]);
                } else {
                    let mut all_spans: Vec<Spans> = Vec::new();

                    for msg in &chat_messages_clone {
                        let spans: Vec<Span> = msg
                            .parts
                            .iter()
                            .map(|part| match part {
                                MessagePart::Text(text) => Span::raw(text.clone()),
                                MessagePart::Colored(text, color_name) => {
                                    let color = match color_name.as_str() {
                                        "red" => Color::Red,
                                        "green" => Color::Green,
                                        "blue" => Color::Blue,
                                        "magenta" => Color::Magenta,
                                        "yellow" => Color::Yellow,
                                        "cyan" => Color::Cyan,
                                        _ => Color::White,
                                    };
                                    Span::styled(text.clone(), Style::default().fg(color))
                                }
                            })
                            .collect();

                        all_spans.push(Spans::from(spans));
                    }

                    let message_count = chat_messages_clone.len();
                    let message_text = Text::from(all_spans);

                    let message_block = Paragraph::new(message_text)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(format!(" Chat ({} messages) ", message_count)),
                        )
                        .scroll((scroll_offset, 0));
                    f.render_widget(message_block, chunks[0]);
                }

                let input_display = if input_text_clone.is_empty() {
                    format!("{}: ▌", username_clone)
                } else {
                    format!("{}: {}▌", username_clone, input_text_clone)
                };

                let input_block = Paragraph::new(input_display)
                    .block(Block::default().borders(Borders::ALL).title(" Message "));
                f.render_widget(input_block, chunks[1]);
            })?;

            needs_redraw = false;
        }

        // Handle events
        if event::poll(Duration::from_millis(10))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Enter => {
                        if !input_text.is_empty() {
                            if write_to_stream(&write_stream, &format!("{}\n", input_text.trim()))
                                .await
                                .is_err()
                            {
                                log_message(&log_file, "[DEBUG] Failed to send message to server")
                                    .await;
                                break;
                            }
                            input_text.clear();
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Backspace => {
                        input_text.pop();
                        needs_redraw = true;
                    }
                    KeyCode::Char(c) => {
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) {
                            match c {
                                'l' => {
                                    if write_to_stream(&write_stream, "GET_USERLIST\n")
                                        .await
                                        .is_err()
                                    {
                                        log_message(
                                            &log_file,
                                            "[DEBUG] Failed to request user list from server",
                                        )
                                        .await;
                                        break;
                                    }
                                    show_user_list = true;
                                    scroll_offset = 0;
                                    needs_redraw = true;
                                }
                                _ => {}
                            }
                        } else if show_user_list && c == 'r' {
                            show_user_list = false;
                            {
                                let mut buffer = message_buffer.lock().await;
                                buffer.reset_new_messages_count();
                            }
                            scroll_offset = max_scroll;
                            needs_redraw = true;
                        } else {
                            input_text.push(c);
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Up => {
                        if scroll_offset > 0 {
                            scroll_offset = scroll_offset.saturating_sub(1);
                            needs_redraw = true;
                        }
                    }
                    KeyCode::Down => {
                        if scroll_offset < max_scroll {
                            scroll_offset = scroll_offset.saturating_add(1);
                            needs_redraw = true;
                        }
                    }
                    KeyCode::PageUp => {
                        if scroll_offset >= 5 {
                            scroll_offset -= 5;
                        } else {
                            scroll_offset = 0;
                        }
                        needs_redraw = true;
                    }
                    KeyCode::PageDown => {
                        scroll_offset = std::cmp::min(scroll_offset.saturating_add(5), max_scroll);
                        needs_redraw = true;
                    }
                    KeyCode::Home => {
                        scroll_offset = 0;
                        needs_redraw = true;
                    }
                    KeyCode::End => {
                        scroll_offset = max_scroll;
                        needs_redraw = true;
                    }
                    KeyCode::Esc => break,
                    _ => {}
                },
                Event::Mouse(mouse_event) => match mouse_event.kind {
                    MouseEventKind::ScrollUp => {
                        if scroll_offset > 0 {
                            scroll_offset = scroll_offset.saturating_sub(1);
                            needs_redraw = true;
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if scroll_offset < max_scroll {
                            scroll_offset = scroll_offset.saturating_add(1);
                            needs_redraw = true;
                        }
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {
                    needs_redraw = true;
                }
                _ => {}
            }
        }
    }

    // Cleanup
    log_message(&log_file, "[DEBUG] Cleaning up terminal").await;
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    Ok(())
}
