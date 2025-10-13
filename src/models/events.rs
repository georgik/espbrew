//! Application events for TUI and CLI operations

use crate::models::board::RemoteBoard;
use crate::models::server::DiscoveredServer;

/// Application events for communication between components
#[derive(Debug)]
pub enum AppEvent {
    // Build events
    BuildOutput(String, String),          // board_name, line
    BuildFinished(String, bool),          // board_name, success
    BuildCompleted,                       // All builds completed
    ActionFinished(String, String, bool), // board_name, action, success

    // Component events
    ComponentActionStarted(String, String), // component_name, action_name
    ComponentActionProgress(String, String), // component_name, progress_message
    ComponentActionFinished(String, String, bool), // component_name, action_name, success

    // Monitoring events
    MonitorLogReceived(String), // log_line
    MonitorConnected(String),   // session_id
    MonitorDisconnected,        // monitoring session ended
    MonitorError(String),       // error_message

    // Remote board events
    RemoteBoardsFetched(Vec<RemoteBoard>), // successful fetch result
    RemoteBoardsFetchFailed(String),       // error message

    // Remote flash events
    RemoteFlashCompleted,      // remote flash completed successfully
    RemoteFlashFailed(String), // remote flash failed with error

    // Remote monitor events
    RemoteMonitorStarted(String), // session_id
    RemoteMonitorFailed(String),  // error message

    // Server discovery events
    ServerDiscoveryStarted,
    ServerDiscovered(DiscoveredServer),
    ServerDiscoveryCompleted(Vec<DiscoveredServer>),
    ServerDiscoveryFailed(String),

    // General events
    Tick,

    // User feedback events for TUI
    Error(String),   // Error message to display in TUI
    Warning(String), // Warning message to display in TUI
    Info(String),    // Info message to display in TUI
}
