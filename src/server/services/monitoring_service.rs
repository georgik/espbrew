//! Monitoring service for managing board monitoring sessions

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, broadcast};
use tokio_serial::SerialPortBuilderExt;
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

            tokio::spawn(async move {
                if let Err(e) = Self::monitor_serial_port(
                    session_id_clone,
                    board_id_clone,
                    port_clone,
                    baud_rate,
                    sender_clone,
                    filters,
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

    /// Monitor serial port and broadcast log messages
    async fn monitor_serial_port(
        session_id: String,
        board_id: String,
        port: String,
        baud_rate: u32,
        sender: broadcast::Sender<String>,
        filters: Option<Vec<String>>,
    ) -> Result<()> {
        use tokio::io::{AsyncBufReadExt, BufReader};
        use tokio_serial::SerialStream;

        println!(
            "📺 Starting serial monitoring on port {} at {} baud",
            port, baud_rate
        );

        // Compile regex filters if provided
        let compiled_filters: Vec<Regex> = if let Some(filter_patterns) = &filters {
            let mut compiled = Vec::new();
            for pattern in filter_patterns {
                match Regex::new(pattern) {
                    Ok(regex) => compiled.push(regex),
                    Err(e) => {
                        println!("⚠️  Invalid regex pattern '{}': {}", pattern, e);
                    }
                }
            }
            if !compiled.is_empty() {
                println!("🔍 Applied {} log filters", compiled.len());
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
}
