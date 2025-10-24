//! URL handler for processing espbrew:// custom protocol URLs
//!
//! This module handles parsing and processing of espbrew:// URLs that allow
//! web interfaces (like ESP Launchpad) to trigger espbrew operations.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

/// Parsed espbrew:// URL with all parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EspbrewUrl {
    /// Action to perform (flash, discover, monitor)
    pub action: String,
    /// Target espbrew-server URL or "local" for localhost:8080
    pub server: Option<String>,
    /// Board identifier (name or MAC address)
    pub board: Option<String>,
    /// Git repository URL for auto-build projects
    pub project: Option<String>,
    /// Additional parameters
    pub params: HashMap<String, String>,
}

/// URL handler for espbrew:// protocol
pub struct UrlHandler;

impl UrlHandler {
    /// Parse an espbrew:// URL into structured data
    pub fn parse_url(url_str: &str) -> Result<EspbrewUrl> {
        // Ensure it's an espbrew:// URL
        if !url_str.starts_with("espbrew://") {
            return Err(anyhow!("Invalid URL scheme: expected espbrew://"));
        }

        let url =
            Url::parse(url_str).map_err(|e| anyhow!("Failed to parse URL '{}': {}", url_str, e))?;

        // Extract the action from the host part
        let action = url
            .host_str()
            .ok_or_else(|| anyhow!("No action specified in URL"))?
            .to_string();

        // Validate action
        if !Self::is_valid_action(&action) {
            return Err(anyhow!(
                "Invalid action '{}'. Supported actions: flash, discover, monitor",
                action
            ));
        }

        let mut params = HashMap::new();
        let mut server = None;
        let mut board = None;
        let mut project = None;

        // Parse query parameters
        for (key, value) in url.query_pairs() {
            match key.as_ref() {
                "server" => {
                    server = Some(Self::normalize_server_url(&value)?);
                }
                "board" => {
                    board = Some(value.to_string());
                }
                "project" => {
                    project = Some(value.to_string());
                }
                _ => {
                    params.insert(key.to_string(), value.to_string());
                }
            }
        }

        // Set default server if not specified
        if server.is_none() && (action == "flash" || action == "discover" || action == "monitor") {
            server = Some("http://localhost:8080".to_string());
        }

        Ok(EspbrewUrl {
            action,
            server,
            board,
            project,
            params,
        })
    }

    /// Check if the action is valid
    fn is_valid_action(action: &str) -> bool {
        matches!(action, "flash" | "discover" | "monitor")
    }

    /// Normalize server URL (handle "local" alias and ensure proper format)
    fn normalize_server_url(server: &str) -> Result<String> {
        match server {
            "local" | "localhost" => Ok("http://localhost:8080".to_string()),
            url if url.starts_with("http://") || url.starts_with("https://") => {
                // Validate URL format
                Url::parse(url).map_err(|e| anyhow!("Invalid server URL '{}': {}", url, e))?;
                Ok(url.to_string())
            }
            url if url.contains(':') => {
                // Assume it's host:port, add http://
                let full_url = format!("http://{}", url);
                Url::parse(&full_url)
                    .map_err(|e| anyhow!("Invalid server URL '{}': {}", full_url, e))?;
                Ok(full_url)
            }
            _ => Err(anyhow!(
                "Invalid server format '{}'. Use 'local', 'host:port', or full URL",
                server
            )),
        }
    }

    /// Generate an espbrew:// URL from components
    pub fn generate_url(
        action: &str,
        server: Option<&str>,
        board: Option<&str>,
        project: Option<&str>,
        additional_params: Option<&HashMap<String, String>>,
    ) -> Result<String> {
        if !Self::is_valid_action(action) {
            return Err(anyhow!("Invalid action '{}'", action));
        }

        let mut url = format!("espbrew://{}", action);
        let mut query_params = Vec::new();

        if let Some(server) = server {
            query_params.push(format!("server={}", urlencoding::encode(server)));
        }

        if let Some(board) = board {
            query_params.push(format!("board={}", urlencoding::encode(board)));
        }

        if let Some(project) = project {
            query_params.push(format!("project={}", urlencoding::encode(project)));
        }

        if let Some(params) = additional_params {
            for (key, value) in params {
                query_params.push(format!(
                    "{}={}",
                    urlencoding::encode(key),
                    urlencoding::encode(value)
                ));
            }
        }

        if !query_params.is_empty() {
            url.push('?');
            url.push_str(&query_params.join("&"));
        }

        Ok(url)
    }

    /// Process an espbrew:// URL and execute the requested action
    pub async fn handle_url(url_str: &str) -> Result<()> {
        let parsed_url = Self::parse_url(url_str)?;

        log::info!("Processing espbrew:// URL: {}", url_str);
        log::debug!("Parsed URL: {:?}", parsed_url);

        // Security check - validate the URL before processing
        crate::security::url_validator::UrlValidator::validate_espbrew_url(&parsed_url)?;

        match parsed_url.action.as_str() {
            "flash" => Self::handle_flash_action(&parsed_url).await,
            "discover" => Self::handle_discover_action(&parsed_url).await,
            "monitor" => Self::handle_monitor_action(&parsed_url).await,
            _ => Err(anyhow!("Unsupported action: {}", parsed_url.action)),
        }
    }

    /// Handle flash action
    async fn handle_flash_action(parsed_url: &EspbrewUrl) -> Result<()> {
        log::info!("Handling flash action");

        // Show user confirmation
        if !Self::confirm_action("flash", parsed_url)? {
            log::info!("Flash action cancelled by user");
            return Ok(());
        }

        // Check if we have a project URL to build
        if let Some(project_url) = &parsed_url.project {
            log::info!("Project URL specified: {}", project_url);
            Self::handle_project_flash(project_url, parsed_url).await
        } else {
            // Flash existing project in current directory
            Self::handle_local_project_flash(parsed_url).await
        }
    }

    /// Handle discover action
    async fn handle_discover_action(parsed_url: &EspbrewUrl) -> Result<()> {
        log::info!("Handling discover action");

        if !Self::confirm_action("discover", parsed_url)? {
            log::info!("Discover action cancelled by user");
            return Ok(());
        }

        let server_url = parsed_url
            .server
            .as_ref()
            .ok_or_else(|| anyhow!("Server URL required for discover action"))?;

        // Use existing remote discover command
        crate::cli::commands::discover::execute_discover_command_with_server(server_url).await
    }

    /// Handle monitor action
    async fn handle_monitor_action(parsed_url: &EspbrewUrl) -> Result<()> {
        log::info!("Handling monitor action");

        if !Self::confirm_action("monitor", parsed_url)? {
            log::info!("Monitor action cancelled by user");
            return Ok(());
        }

        // TODO: Implement remote monitor functionality
        log::warn!("Monitor action not yet implemented");
        println!("Monitor action will be implemented in a future version");
        Ok(())
    }

    /// Handle flashing a project from a Git URL
    async fn handle_project_flash(project_url: &str, parsed_url: &EspbrewUrl) -> Result<()> {
        println!(
            "🔄 Preparing to build and flash project from: {}",
            project_url
        );

        // TODO: Implement project auto-building
        // For now, show what would be done
        println!("📋 Would clone and build project: {}", project_url);
        if let Some(board) = &parsed_url.board {
            println!("🎯 Target board: {}", board);
        }
        if let Some(server) = &parsed_url.server {
            println!("🌐 Target server: {}", server);
        }

        log::warn!("Project auto-build not yet implemented");
        println!("❌ Project auto-build feature will be implemented in Phase 3");

        Ok(())
    }

    /// Handle flashing local project
    async fn handle_local_project_flash(parsed_url: &EspbrewUrl) -> Result<()> {
        println!("🔄 Preparing to flash current project");

        let server = parsed_url
            .server
            .as_ref()
            .ok_or_else(|| anyhow!("Server URL required for flash action"))?;

        // Use existing remote flash command
        let board_mac = parsed_url.board.clone();
        let board_name = parsed_url.params.get("name").cloned();

        crate::cli::commands::remote_flash::execute_remote_flash_command_url(
            server,
            board_mac.as_deref(),
            board_name.as_deref(),
        )
        .await
    }

    /// Show confirmation dialog to user
    fn confirm_action(action: &str, parsed_url: &EspbrewUrl) -> Result<bool> {
        // Try to use GUI first, fall back to terminal if GUI is not available
        match Self::show_gui_confirmation(action, parsed_url) {
            Ok(result) => Ok(result),
            Err(gui_error) => {
                log::debug!(
                    "GUI confirmation failed, falling back to terminal: {}",
                    gui_error
                );
                Self::show_terminal_confirmation(action, parsed_url)
            }
        }
    }

    /// Show GUI confirmation dialog using Slint
    fn show_gui_confirmation(action: &str, parsed_url: &EspbrewUrl) -> Result<bool> {
        crate::ui::show_confirmation_dialog(
            action,
            parsed_url.server.as_deref(),
            parsed_url.board.as_deref(),
            parsed_url.project.as_deref(),
            &parsed_url.params,
        )
    }

    /// Show terminal confirmation dialog (fallback)
    fn show_terminal_confirmation(action: &str, parsed_url: &EspbrewUrl) -> Result<bool> {
        println!("\n🍺 ESPBrew URL Handler");
        println!("═══════════════════════");
        println!("Action: {}", action.to_uppercase());

        if let Some(server) = &parsed_url.server {
            println!("Server: {}", server);
        }

        if let Some(board) = &parsed_url.board {
            println!("Board: {}", board);
        }

        if let Some(project) = &parsed_url.project {
            println!("Project: {}", project);
        }

        if !parsed_url.params.is_empty() {
            println!("Additional parameters:");
            for (key, value) in &parsed_url.params {
                println!("  {}: {}", key, value);
            }
        }

        println!("\n⚠️  This will {} using the espbrew system.", action);
        print!("Do you want to continue? [y/N]: ");

        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let response = input.trim().to_lowercase();
        Ok(response == "y" || response == "yes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flash_url() {
        let url = "espbrew://flash?server=http://192.168.1.100:8080&board=esp32-s3-box-3";
        let parsed = UrlHandler::parse_url(url).unwrap();

        assert_eq!(parsed.action, "flash");
        assert_eq!(parsed.server, Some("http://192.168.1.100:8080".to_string()));
        assert_eq!(parsed.board, Some("esp32-s3-box-3".to_string()));
        assert_eq!(parsed.project, None);
    }

    #[test]
    fn test_parse_project_flash_url() {
        let url = "espbrew://flash?server=local&board=AA:BB:CC:DD:EE:FF&project=https://github.com/georgik/OpenTyrian";
        let parsed = UrlHandler::parse_url(url).unwrap();

        assert_eq!(parsed.action, "flash");
        assert_eq!(parsed.server, Some("http://localhost:8080".to_string()));
        assert_eq!(parsed.board, Some("AA:BB:CC:DD:EE:FF".to_string()));
        assert_eq!(
            parsed.project,
            Some("https://github.com/georgik/OpenTyrian".to_string())
        );
    }

    #[test]
    fn test_parse_discover_url() {
        let url = "espbrew://discover?server=http://192.168.1.100:8080";
        let parsed = UrlHandler::parse_url(url).unwrap();

        assert_eq!(parsed.action, "discover");
        assert_eq!(parsed.server, Some("http://192.168.1.100:8080".to_string()));
        assert_eq!(parsed.board, None);
    }

    #[test]
    fn test_generate_url() {
        let url =
            UrlHandler::generate_url("flash", Some("local"), Some("esp32-s3-box-3"), None, None)
                .unwrap();

        assert_eq!(url, "espbrew://flash?server=local&board=esp32-s3-box-3");
    }

    #[test]
    fn test_invalid_scheme() {
        let url = "http://example.com/flash";
        let result = UrlHandler::parse_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_action() {
        let url = "espbrew://invalid?server=local";
        let result = UrlHandler::parse_url(url);
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_server_url() {
        assert_eq!(
            UrlHandler::normalize_server_url("local").unwrap(),
            "http://localhost:8080"
        );
        assert_eq!(
            UrlHandler::normalize_server_url("192.168.1.100:8080").unwrap(),
            "http://192.168.1.100:8080"
        );
        assert_eq!(
            UrlHandler::normalize_server_url("https://example.com:8080").unwrap(),
            "https://example.com:8080"
        );
    }
}
