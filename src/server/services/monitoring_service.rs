//! Monitoring service for managing board monitoring sessions

use anyhow::Result;
use log::{error, info, warn};
use regex::Regex;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

use crate::models::board::BoardStatus;
use crate::models::monitor::{
    KeepAliveRequest, KeepAliveResponse, LogMessage, MonitorRequest, MonitorResponse,
    StopMonitorRequest, StopMonitorResponse,
};
use crate::server::app::MonitoringSession;
use crate::server::app::ServerState;

/// Monitoring service for handling board monitoring operations
#[derive(Clone)]
pub struct MonitoringService {
    state: Arc<RwLock<ServerState>>,
}

impl MonitoringService {
    pub fn new(state: Arc<RwLock<ServerState>>) -> Self {
        Self { state }
    }

    /// Start monitoring a board
    pub async fn start_monitoring(&self, request: MonitorRequest) -> Result<MonitorResponse> {
        println!(
            "🔥 Starting monitoring session for board {}",
            request.board_id
        );

        // Get board port before borrowing mutably
        let board_port = {
            let state_lock = self.state.read().await;
            let board = state_lock
                .boards
                .get(&request.board_id)
                .ok_or_else(|| anyhow::anyhow!("Board not found: {}", request.board_id))?;
            board.port.clone()
        };

        // Update board status to monitoring
        {
            let mut state_lock = self.state.write().await;
            if let Some(board) = state_lock.boards.get_mut(&request.board_id) {
                board.status = BoardStatus::Monitoring;
                board.last_updated = chrono::Local::now();
            }
        }

        // Create new monitoring session
        let session_id = Uuid::new_v4().to_string();
        let baud_rate = request.baud_rate.unwrap_or(115200);
        let (sender, _receiver) = broadcast::channel(1000);

        // Create monitoring session
        let session = MonitoringSession {
            id: session_id.clone(),
            board_id: request.board_id.clone(),
            port: board_port.clone(),
            baud_rate,
            started_at: chrono::Local::now(),
            last_activity: chrono::Local::now(),
            sender: sender.clone(),
            task_handle: None,
        };

        // Spawn monitoring task
        let task_handle = {
            let session_id_clone = session_id.clone();
            let board_id_clone = request.board_id.clone();
            let port_clone = board_port.clone();
            let sender_clone = sender.clone();
            let filters = request.filters.clone();
            let timeout = request.timeout;
            let success_pattern = request.success_pattern.clone();
            let failure_pattern = request.failure_pattern.clone();
            let log_format = request.log_format.clone();
            let reset = request.reset;
            let non_interactive = request.non_interactive;

            tokio::spawn(async move {
                if let Err(e) = Self::monitor_serial_port(
                    session_id_clone,
                    board_id_clone,
                    port_clone,
                    baud_rate,
                    sender_clone,
                    filters,
                    timeout,
                    success_pattern,
                    failure_pattern,
                    log_format,
                    reset,
                    non_interactive,
                )
                .await
                {
                    eprintln!("❌ Serial monitoring task failed: {}", e);
                }
            })
        };

        // Store session with task handle
        {
            let state_lock = self.state.read().await;
            let mut sessions_lock = state_lock.monitoring_sessions.write().await;
            let mut session_with_handle = session;
            session_with_handle.task_handle = Some(task_handle);
            sessions_lock.insert(session_id.clone(), session_with_handle);
        }

        println!(
            "✅ Monitoring session {} started for board {}",
            session_id, request.board_id
        );

        Ok(MonitorResponse {
            success: true,
            message: "Monitoring session started successfully".to_string(),
            websocket_url: Some(format!("/ws/monitor/{}", session_id)),
            session_id: Some(session_id),
        })
    }

    /// Stop monitoring session
    pub async fn stop_monitoring(
        &self,
        request: StopMonitorRequest,
    ) -> Result<StopMonitorResponse> {
        let state_lock = self.state.read().await;
        let mut sessions_lock = state_lock.monitoring_sessions.write().await;

        if let Some(session) = sessions_lock.remove(&request.session_id) {
            drop(sessions_lock); // Release sessions lock early
            // Stop the monitoring task
            if let Some(task_handle) = session.task_handle {
                task_handle.abort();
            }

            // Update board status back to available (need write lock for boards)
            drop(state_lock); // Release read lock
            let mut state_write_lock = self.state.write().await;
            if let Some(board) = state_write_lock.boards.get_mut(&session.board_id) {
                board.status = BoardStatus::Available;
                board.last_updated = chrono::Local::now();
            }

            println!("🛑 Stopped monitoring session {}", request.session_id);

            Ok(StopMonitorResponse {
                success: true,
                message: "Monitoring session stopped successfully".to_string(),
            })
        } else {
            Ok(StopMonitorResponse {
                success: false,
                message: "Monitoring session not found".to_string(),
            })
        }
    }

    /// Keep monitoring session alive
    pub async fn keep_alive(&self, request: KeepAliveRequest) -> Result<KeepAliveResponse> {
        let state_lock = self.state.read().await;
        let mut sessions_lock = state_lock.monitoring_sessions.write().await;

        if let Some(session) = sessions_lock.get_mut(&request.session_id) {
            session.last_activity = chrono::Local::now();

            Ok(KeepAliveResponse {
                success: true,
                message: "Session keep-alive updated".to_string(),
            })
        } else {
            Ok(KeepAliveResponse {
                success: false,
                message: "Monitoring session not found".to_string(),
            })
        }
    }

    /// List active monitoring sessions
    pub async fn list_sessions(&self) -> Result<Vec<String>> {
        let state_lock = self.state.read().await;
        let sessions_lock = state_lock.monitoring_sessions.read().await;
        let sessions: Vec<String> = sessions_lock.keys().cloned().collect();
        Ok(sessions)
    }

    /// Get monitoring session by ID
    pub async fn get_session(&self, session_id: &str) -> Option<Arc<RwLock<MonitoringSession>>> {
        let state_lock = self.state.read().await;
        let sessions_lock = state_lock.monitoring_sessions.read().await;

        // Check if session exists and clone necessary data
        if let Some(session) = sessions_lock.get(session_id) {
            // Create a new session instance for sharing (without task handle)
            let shared_session = MonitoringSession {
                id: session.id.clone(),
                board_id: session.board_id.clone(),
                port: session.port.clone(),
                baud_rate: session.baud_rate,
                started_at: session.started_at,
                last_activity: session.last_activity,
                sender: session.sender.clone(),
                task_handle: None, // Don't share the task handle
            };
            Some(Arc::new(RwLock::new(shared_session)))
        } else {
            None
        }
    }

    /// Clean up inactive monitoring sessions
    pub async fn cleanup_inactive_sessions(&self) {
        let cutoff_time = chrono::Local::now() - chrono::Duration::minutes(2);
        let mut sessions_to_remove = Vec::new();

        // Identify sessions to remove
        {
            let state_lock = self.state.read().await;
            let sessions_lock = state_lock.monitoring_sessions.read().await;
            for (session_id, session) in sessions_lock.iter() {
                if session.last_activity < cutoff_time {
                    sessions_to_remove.push(session_id.clone());
                }
            }
        }

        // Remove inactive sessions
        if !sessions_to_remove.is_empty() {
            let mut removed_sessions = Vec::new();

            // Remove sessions from the sessions map
            {
                let state_lock = self.state.read().await;
                let mut sessions_lock = state_lock.monitoring_sessions.write().await;
                for session_id in &sessions_to_remove {
                    if let Some(session) = sessions_lock.remove(session_id) {
                        removed_sessions.push((session_id.clone(), session));
                    }
                }
            }

            // Clean up the removed sessions outside the lock
            for (session_id, session) in removed_sessions {
                // Stop the monitoring task
                if let Some(task_handle) = session.task_handle {
                    task_handle.abort();
                }

                // Update board status back to available
                {
                    let mut state_write_lock = self.state.write().await;
                    if let Some(board) = state_write_lock.boards.get_mut(&session.board_id) {
                        board.status = BoardStatus::Available;
                        board.last_updated = chrono::Local::now();
                    }
                }

                println!("🧹 Cleaned up inactive monitoring session {}", session_id);
            }
        }
    }

    /// Perform ESP32 board reset by toggling DTR and RTS signals
    /// This implements the standard ESP32 reset sequence used by esptool and similar tools
    async fn perform_esp32_reset(port: &str) -> Result<()> {
        use std::time::Duration;
        use tokio_serial::{SerialPort, SerialStream};

        // Open serial port with minimal configuration
        let builder = tokio_serial::new(port, 115200).timeout(Duration::from_secs(1));

        let mut serial = SerialStream::open(&builder)
            .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

        println!("🔄 Performing ESP32 reset on port {}", port);

        // ESP32 reset sequence:
        // 1. Set DTR=false, RTS=true (puts ESP32 in bootloader mode)
        serial
            .write_data_terminal_ready(false)
            .map_err(|e| anyhow::anyhow!("Failed to set DTR: {}", e))?;
        serial
            .write_request_to_send(true)
            .map_err(|e| anyhow::anyhow!("Failed to set RTS: {}", e))?;

        // Wait briefly
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 2. Set DTR=true, RTS=false (releases reset)
        serial
            .write_data_terminal_ready(true)
            .map_err(|e| anyhow::anyhow!("Failed to set DTR: {}", e))?;
        serial
            .write_request_to_send(false)
            .map_err(|e| anyhow::anyhow!("Failed to set RTS: {}", e))?;

        // Wait briefly
        tokio::time::sleep(Duration::from_millis(100)).await;

        // 3. Set both DTR=false, RTS=false (normal operation)
        serial
            .write_data_terminal_ready(false)
            .map_err(|e| anyhow::anyhow!("Failed to set DTR: {}", e))?;
        serial
            .write_request_to_send(false)
            .map_err(|e| anyhow::anyhow!("Failed to set RTS: {}", e))?;

        println!("✅ ESP32 reset sequence completed for port {}", port);
        Ok(())
    }

    /// Monitor serial port and broadcast log messages
    async fn monitor_serial_port(
        session_id: String,
        board_id: String,
        port: String,
        baud_rate: u32,
        sender: broadcast::Sender<String>,
        filters: Option<Vec<String>>,
        timeout: Option<u64>,
        success_pattern: Option<String>,
        failure_pattern: Option<String>,
        log_format: Option<String>,
        reset: Option<bool>,
        non_interactive: Option<bool>,
    ) -> Result<()> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio_serial::SerialStream;

        println!(
            "📺 Starting serial monitoring on port {} at {} baud",
            port, baud_rate
        );

        // Compile success and failure patterns if provided
        let success_regex = if let Some(ref pattern) = success_pattern {
            match Regex::new(pattern) {
                Ok(regex) => {
                    info!("Success pattern configured: {}", pattern);
                    Some(regex)
                }
                Err(e) => {
                    error!("Invalid success pattern '{}': {}", pattern, e);
                    None
                }
            }
        } else {
            None
        };

        let failure_regex = if let Some(ref pattern) = failure_pattern {
            match Regex::new(pattern) {
                Ok(regex) => {
                    error!("Failure pattern configured: {}", pattern);
                    Some(regex)
                }
                Err(e) => {
                    error!("Invalid failure pattern '{}': {}", pattern, e);
                    None
                }
            }
        } else {
            None
        };

        // Set timeout duration
        let timeout_duration = timeout.and_then(|t| {
            if t > 0 {
                Some(Duration::from_secs(t))
            } else {
                None
            }
        });

        // Start time for timeout tracking
        let start_time = Instant::now();

        // Perform ESP32 reset to trigger boot sequence if requested
        if reset.unwrap_or(false) {
            info!("Performing ESP32 reset...");
            if let Err(e) = Self::perform_esp32_reset(&port).await {
                warn!("Reset failed, continuing anyway: {}", e);
            } else {
                // Wait a moment for the reset to complete and boot sequence to start
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
        }

        // Compile regex filters if provided
        let compiled_filters: Vec<Regex> = if let Some(filter_patterns) = &filters {
            let mut compiled = Vec::new();
            for pattern in filter_patterns {
                match Regex::new(pattern) {
                    Ok(regex) => compiled.push(regex),
                    Err(e) => {
                        error!("Invalid regex pattern '{}': {}", pattern, e);
                    }
                }
            }
            if !compiled.is_empty() {
                info!("Applied {} log filters", compiled.len());
            }
            compiled
        } else {
            Vec::new()
        };

        let serial = SerialStream::open(&tokio_serial::new(&port, baud_rate))
            .map_err(|e| anyhow::anyhow!("Failed to open serial port {}: {}", port, e))?;

        let reader = BufReader::new(serial);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            // Apply filters if any are configured
            if !compiled_filters.is_empty() {
                let mut matches_filter = false;
                for regex in &compiled_filters {
                    if regex.is_match(&line) {
                        matches_filter = true;
                        break;
                    }
                }
                // Skip this line if it doesn't match any filter
                if !matches_filter {
                    continue;
                }
            }

            let log_message = LogMessage {
                session_id: session_id.clone(),
                board_id: board_id.clone(),
                content: line.clone(),
                timestamp: chrono::Local::now(),
                level: Self::detect_log_level(&line),
            };

            // Serialize the log message to JSON
            if let Ok(json_message) = serde_json::to_string(&log_message) {
                // Broadcast to WebSocket clients (ignore if no receivers)
                let _ = sender.send(json_message);
            }
        }

        println!("📺 Serial monitoring ended for session {}", session_id);
        Ok(())
    }

    /// Detect log level from log content
    fn detect_log_level(content: &str) -> Option<String> {
        let upper_content = content.to_uppercase();

        if upper_content.contains("ERROR") || upper_content.contains("E (") {
            Some("ERROR".to_string())
        } else if upper_content.contains("WARN") || upper_content.contains("W (") {
            Some("WARNING".to_string())
        } else if upper_content.contains("INFO") || upper_content.contains("I (") {
            Some("INFO".to_string())
        } else if upper_content.contains("DEBUG") || upper_content.contains("D (") {
            Some("DEBUG".to_string())
        } else {
            None
        }
    }

    /// Strip ANSI escape sequences from text
    fn strip_ansi_codes(text: &str) -> String {
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Start of ANSI escape sequence (ESC character)
                if let Some('[') = chars.next() {
                    // Skip until we find the end character (a-z, A-Z, or @)
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch.is_ascii_alphabetic() || next_ch == '@' {
                            chars.next(); // Consume the end character
                            break;
                        }
                        chars.next(); // Skip this character
                    }
                }
            } else if ch == '[' {
                // Handle malformed sequences that start with [ instead of ESC
                let mut seq_start = chars.clone();
                let mut is_ansi_sequence = false;

                // Check if this looks like an ANSI sequence (numbers and semicolons)
                while let Some(&next_ch) = seq_start.peek() {
                    if next_ch.is_ascii_digit() || next_ch == ';' {
                        seq_start.next(); // Skip number or semicolon
                    } else if next_ch.is_ascii_alphabetic() || next_ch == '@' {
                        is_ansi_sequence = true;
                        chars = seq_start; // Advance to after this character
                        chars.next(); // Skip the end character
                        break;
                    } else {
                        break; // Not an ANSI sequence
                    }
                }

                if !is_ansi_sequence {
                    result.push(ch); // Keep the [ character
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Fix ANSI codes that might be missing the ESC character
    fn fix_ansi_codes(text: &str) -> String {
        // If text already has proper ANSI codes, return as-is
        if text.contains('\x1b') {
            return text.to_string();
        }

        // Try to fix malformed codes like [0;32m...[0m by adding ESC characters
        let mut result = String::with_capacity(text.len() + text.matches('[').count() * 2);
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '[' {
                // Look ahead to see if this is an ANSI sequence
                let mut j = i + 1;
                let mut is_ansi_sequence = false;

                // Find the end of the potential ANSI sequence
                while j < chars.len() && !chars[j].is_whitespace() && chars[j] != '[' {
                    if chars[j].is_ascii_alphabetic() {
                        is_ansi_sequence = true;
                        j += 1; // Include the alphabetic end character
                        break;
                    }
                    j += 1;
                }

                if is_ansi_sequence {
                    // Add ESC character before the [
                    result.push('\x1b');
                }

                // Add the entire sequence (or just the [ if not ANSI)
                for k in i..j {
                    result.push(chars[k]);
                }

                i = j;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
    }
}
