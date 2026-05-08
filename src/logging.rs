/*
 * TermTalk - Logging Module
 * Version: 0.8
 * Copyright (c) 2025-2026 Peter Leukanič
 * Under MIT License
 *
 * Thread-safe file-based logging utilities
 */

use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn log_message(log_file: &Arc<Mutex<File>>, message: &str) {
    let mut file = log_file.lock().await;
    if writeln!(&mut *file, "{}", message).is_err() {
        eprintln!("Failed to write to log file: {}", message);
    }
}
