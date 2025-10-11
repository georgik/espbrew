//! Logging utilities and initialization for ESPBrew

use anyhow::Result;
use env_logger::{Builder, Target};
use log::LevelFilter;
use std::io::Write;
use tokio::sync::mpsc;

use crate::models::AppEvent;

/// Initialize logging for ESPBrew CLI
pub fn init_cli_logging(verbose: u8, quiet: bool, tui_mode: bool) -> Result<()> {
    let level = match (quiet, verbose) {
        (true, _) => LevelFilter::Error,
        (false, 0) => LevelFilter::Info,
        (false, 1) => LevelFilter::Debug,
        (false, _) => LevelFilter::Trace,
    };

    if tui_mode {
        // File logging only for TUI mode to avoid terminal interference
        init_file_logger(level)?;
    } else {
        // Stderr logging for CLI mode
        Builder::from_default_env()
            .target(Target::Stderr)
            .filter_level(level)
            .format_timestamp_secs()
            .format_module_path(false)
            .init();
    }

    // Initialize panic logging
    #[cfg(debug_assertions)]
    log_panics::init();

    log::debug!("ESPBrew logging initialized with level: {:?}", level);
    Ok(())
}

/// Initialize logging for ESPBrew server
pub fn init_server_logging(
    structured: bool,
    log_file: Option<&str>,
    level: Option<LevelFilter>,
) -> Result<()> {
    let level = level.unwrap_or(LevelFilter::Info);

    if structured {
        init_json_logger(level, log_file)?;
    } else {
        init_human_readable_server_logger(level)?;
    }

    // Always initialize panic logging for server
    log_panics::init();

    log::info!("ESPBrew server logging initialized with level: {:?}", level);
    Ok(())
}

/// Initialize file-based logging for TUI mode
fn init_file_logger(level: LevelFilter) -> Result<()> {
    use std::fs::OpenOptions;

    // Create logs directory if it doesn't exist
    let log_dir = dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("espbrew")
        .join("logs");

    std::fs::create_dir_all(&log_dir)?;

    let log_file = log_dir.join("espbrew.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)?;

    Builder::from_default_env()
        .target(Target::Pipe(Box::new(file)))
        .filter_level(level)
        .format_timestamp_secs()
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}: {}",
                buf.timestamp(),
                record.level(),
                record.module_path().unwrap_or("unknown"),
                record.args()
            )
        })
        .init();

    Ok(())
}

/// Initialize JSON structured logging for server
fn init_json_logger(level: LevelFilter, log_file: Option<&str>) -> Result<()> {
    use chrono::Utc;
    use std::fs::OpenOptions;

    let target: Box<dyn Write + Send> = if let Some(file_path) = log_file {
        Box::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)?,
        )
    } else {
        Box::new(std::io::stdout())
    };

    Builder::from_default_env()
        .target(Target::Pipe(target))
        .filter_level(level)
        .format(|buf, record| {
            let json = serde_json::json!({
                "timestamp": Utc::now().to_rfc3339(),
                "level": record.level().to_string(),
                "module": record.module_path().unwrap_or("unknown"),
                "message": record.args().to_string(),
                "target": record.target(),
            });
            writeln!(buf, "{}", json)
        })
        .init();

    Ok(())
}

/// Initialize human-readable logging for server
fn init_human_readable_server_logger(level: LevelFilter) -> Result<()> {
    Builder::from_default_env()
        .target(Target::Stdout)
        .filter_level(level)
        .format_timestamp_secs()
        .format_module_path(false)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {}: {}",
                buf.timestamp(),
                record.level(),
                record.module_path().unwrap_or("unknown"),
                record.args()
            )
        })
        .init();

    Ok(())
}

/// TUI-safe logging helper that sends messages via AppEvent instead of direct output
pub struct TuiLogger {
    tx: mpsc::UnboundedSender<AppEvent>,
}

impl TuiLogger {
    pub fn new(tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        Self { tx }
    }

    /// Send error message to TUI via AppEvent
    pub fn error(&self, message: String) {
        log::error!("{}", message);
        if let Err(e) = self.tx.send(AppEvent::Error(message)) {
            // Fallback to direct logging if event channel is broken
            log::error!("Failed to send error event to TUI: {}", e);
        }
    }

    /// Send warning message to TUI via AppEvent
    pub fn warning(&self, message: String) {
        log::warn!("{}", message);
        if let Err(e) = self.tx.send(AppEvent::Warning(message)) {
            log::warn!("Failed to send warning event to TUI: {}", e);
        }
    }

    /// Send info message to TUI via AppEvent
    pub fn info(&self, message: String) {
        log::info!("{}", message);
        if let Err(e) = self.tx.send(AppEvent::Info(message)) {
            log::info!("Failed to send info event to TUI: {}", e);
        }
    }

    /// Debug logging (file only, not sent to TUI)
    pub fn debug(&self, message: &str) {
        log::debug!("{}", message);
    }

    /// Trace logging (file only, not sent to TUI)  
    pub fn trace(&self, message: &str) {
        log::trace!("{}", message);
    }
}

/// Create a TUI-safe logger instance
pub fn create_tui_logger(tx: mpsc::UnboundedSender<AppEvent>) -> TuiLogger {
    TuiLogger::new(tx)
}

/// Macro for easy TUI logging with format strings
#[macro_export]
macro_rules! tui_log {
    ($logger:expr, error, $($arg:tt)*) => {
        $logger.error(format!($($arg)*))
    };
    ($logger:expr, warning, $($arg:tt)*) => {
        $logger.warning(format!($($arg)*))
    };
    ($logger:expr, info, $($arg:tt)*) => {
        $logger.info(format!($($arg)*))
    };
    ($logger:expr, debug, $($arg:tt)*) => {
        $logger.debug(&format!($($arg)*))
    };
    ($logger:expr, trace, $($arg:tt)*) => {
        $logger.trace(&format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_tui_logger_sends_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let logger = TuiLogger::new(tx);

        logger.error("test error".to_string());
        logger.warning("test warning".to_string());
        logger.info("test info".to_string());

        // Check that events were sent
        match rx.recv().await {
            Some(AppEvent::Error(msg)) => assert_eq!(msg, "test error"),
            other => panic!("Expected Error event, got: {:?}", other),
        }

        match rx.recv().await {
            Some(AppEvent::Warning(msg)) => assert_eq!(msg, "test warning"),
            other => panic!("Expected Warning event, got: {:?}", other),
        }

        match rx.recv().await {
            Some(AppEvent::Info(msg)) => assert_eq!(msg, "test info"),
            other => panic!("Expected Info event, got: {:?}", other),
        }
    }

    #[test]
    fn test_log_level_selection() {
        // Test quiet mode
        let level = match (true, 0) {
            (true, _) => LevelFilter::Error,
            (false, 0) => LevelFilter::Info,
            (false, 1) => LevelFilter::Debug,
            (false, _) => LevelFilter::Trace,
        };
        assert_eq!(level, LevelFilter::Error);

        // Test verbose mode
        let level = match (false, 2) {
            (true, _) => LevelFilter::Error,
            (false, 0) => LevelFilter::Info,
            (false, 1) => LevelFilter::Debug,
            (false, _) => LevelFilter::Trace,
        };
        assert_eq!(level, LevelFilter::Trace);
    }
}
