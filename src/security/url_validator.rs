//! URL validation and security for espbrew:// custom protocol
//!
//! This module provides validation, sanitization, and security checks
//! for espbrew:// URLs to prevent injection attacks and ensure safe operation.

use anyhow::{Result, anyhow};
use url::Url;

use crate::cli::url_handler::EspbrewUrl;

/// URL validator with security checks for espbrew:// protocol
pub struct UrlValidator;

impl UrlValidator {
    /// Validate an espbrew URL for security and correctness
    pub fn validate_espbrew_url(parsed_url: &EspbrewUrl) -> Result<()> {
        // Validate action
        Self::validate_action(&parsed_url.action)?;

        // Validate server URL if present
        if let Some(server) = &parsed_url.server {
            Self::validate_server_url(server)?;
        }

        // Validate board identifier if present
        if let Some(board) = &parsed_url.board {
            Self::validate_board_identifier(board)?;
        }

        // Validate project URL if present
        if let Some(project) = &parsed_url.project {
            Self::validate_project_url(project)?;
        }

        // Validate additional parameters
        Self::validate_parameters(&parsed_url.params)?;

        // Check for suspicious combinations
        Self::check_suspicious_combinations(parsed_url)?;

        log::debug!("URL validation passed for: {:?}", parsed_url);
        Ok(())
    }

    /// Validate the action parameter
    fn validate_action(action: &str) -> Result<()> {
        let allowed_actions = ["flash", "discover", "monitor"];

        if !allowed_actions.contains(&action) {
            return Err(anyhow!(
                "Invalid action '{}'. Allowed actions: {:?}",
                action,
                allowed_actions
            ));
        }

        // Check for suspicious patterns
        if action.contains("..") || action.contains("/") || action.contains("\\") {
            return Err(anyhow!("Action contains suspicious characters: {}", action));
        }

        Ok(())
    }

    /// Validate server URL for security
    fn validate_server_url(server_url: &str) -> Result<()> {
        // Parse URL to validate format
        let parsed =
            Url::parse(server_url).map_err(|e| anyhow!("Invalid server URL format: {}", e))?;

        // Check scheme
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(anyhow!(
                "Invalid URL scheme '{}'. Only http and https are allowed",
                parsed.scheme()
            ));
        }

        // Validate host
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow!("Server URL must have a valid host"))?;

        Self::validate_host(host)?;

        // Validate port if present
        if let Some(port) = parsed.port() {
            Self::validate_port(port)?;
        }

        // Check for suspicious patterns in URL
        if server_url.contains("..") || server_url.contains("@") {
            return Err(anyhow!("Server URL contains suspicious patterns"));
        }

        // Ensure URL doesn't contain fragments or excessive query parameters
        if parsed.fragment().is_some() {
            return Err(anyhow!("Server URL should not contain fragments"));
        }

        Ok(())
    }

    /// Validate host address (IP or domain name)
    fn validate_host(host: &str) -> Result<()> {
        // Check for localhost variations
        let localhost_variants = [
            "localhost",
            "127.0.0.1",
            "::1",
            "0.0.0.0",
            "127.0.0.0",
            "127.255.255.255",
        ];

        if localhost_variants.contains(&host) {
            return Ok(()); // Localhost is always trusted
        }

        // Check for private network ranges
        if Self::is_private_network_host(host) {
            return Ok(()); // Private networks are generally trusted
        }

        // For public hosts, apply stricter validation
        Self::validate_public_host(host)?;

        Ok(())
    }

    /// Check if host is in private network ranges
    fn is_private_network_host(host: &str) -> bool {
        // Check for private IPv4 ranges
        if let Ok(ip) = host.parse::<std::net::Ipv4Addr>() {
            return ip.is_private() || ip.is_loopback();
        }

        // Check for private IPv6
        if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
            return ip.is_loopback();
        }

        // Check common private domain patterns
        host.ends_with(".local") || host.ends_with(".lan") || host.ends_with(".internal")
    }

    /// Validate public host addresses with stricter rules
    fn validate_public_host(host: &str) -> Result<()> {
        // For now, we're being conservative with public hosts
        // In a production environment, you might want to:
        // 1. Check against a whitelist of allowed domains
        // 2. Require user confirmation for public hosts
        // 3. Implement certificate pinning for HTTPS

        log::warn!("Public host detected: {}. Use with caution.", host);

        // Basic domain name validation
        if host.len() > 253 {
            return Err(anyhow!("Host name too long"));
        }

        // Check for suspicious patterns
        if host.contains("..") || host.starts_with('-') || host.ends_with('-') {
            return Err(anyhow!("Host name contains suspicious patterns"));
        }

        Ok(())
    }

    /// Validate port number
    fn validate_port(port: u16) -> Result<()> {
        // Common espbrew-server ports
        let common_ports = [8080, 8443, 3000, 8000, 9000];

        if common_ports.contains(&port) {
            return Ok(());
        }

        // Allow user ports (1024-65535)
        if port >= 1024 {
            return Ok(());
        }

        // Warn about privileged ports
        log::warn!("Using privileged port {}, ensure this is intentional", port);
        Ok(())
    }

    /// Validate board identifier
    fn validate_board_identifier(board: &str) -> Result<()> {
        // Check length
        if board.len() > 100 {
            return Err(anyhow!("Board identifier too long"));
        }

        // Check for MAC address pattern
        if Self::looks_like_mac_address(board) {
            return Self::validate_mac_address(board);
        }

        // Validate as board name
        Self::validate_board_name(board)?;

        Ok(())
    }

    /// Check if string looks like a MAC address
    fn looks_like_mac_address(s: &str) -> bool {
        s.len() == 17 && s.chars().filter(|&c| c == ':').count() == 5
    }

    /// Validate MAC address format
    fn validate_mac_address(mac: &str) -> Result<()> {
        let parts: Vec<&str> = mac.split(':').collect();

        if parts.len() != 6 {
            return Err(anyhow!("Invalid MAC address format"));
        }

        for part in parts {
            if part.len() != 2 {
                return Err(anyhow!("Invalid MAC address format"));
            }

            if !part.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(anyhow!("Invalid MAC address format"));
            }
        }

        Ok(())
    }

    /// Validate board name
    fn validate_board_name(name: &str) -> Result<()> {
        // Check for allowed characters (alphanumeric, hyphens, underscores)
        if !name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow!("Board name contains invalid characters"));
        }

        // Check for suspicious patterns
        if name.contains("..") || name.starts_with('-') || name.ends_with('-') {
            return Err(anyhow!("Board name contains suspicious patterns"));
        }

        Ok(())
    }

    /// Validate project URL or path (Git repositories or local paths)
    fn validate_project_url(project_url: &str) -> Result<()> {
        // Check if it's a local path (starts with / or ./)
        if project_url.starts_with('/')
            || project_url.starts_with("./")
            || project_url.starts_with("../")
        {
            return Self::validate_local_project_path(project_url);
        }

        // Check for path traversal patterns before URL parsing (which normalizes paths)
        if project_url.contains("/../") || project_url.contains("..\\") {
            return Err(anyhow!("Project URL contains path traversal patterns"));
        }

        // Try to parse as URL for Git repositories
        let parsed =
            Url::parse(project_url).map_err(|e| anyhow!("Invalid project URL format: {}", e))?;

        // Allow only specific schemes for project URLs
        match parsed.scheme() {
            "https" => {} // Preferred for security
            "http" => {
                // Allow HTTP for internal/development use, but warn
                log::warn!(
                    "HTTP project URL detected, HTTPS recommended: {}",
                    project_url
                );
            }
            "git" => {
                // Allow git protocol for compatibility
                log::info!("Using git protocol for project URL: {}", project_url);
            }
            scheme => {
                return Err(anyhow!(
                    "Unsupported project URL scheme '{}'. Allowed: https, http, git",
                    scheme
                ));
            }
        }

        // Validate host for project URLs
        if let Some(host) = parsed.host_str() {
            Self::validate_project_host(host)?;
        }

        // Check for suspicious patterns in URLs only (not local paths)
        if project_url.contains("@") {
            return Err(anyhow!("Project URL contains suspicious patterns"));
        }

        Ok(())
    }

    /// Validate local project path
    fn validate_local_project_path(path: &str) -> Result<()> {
        // Check path length
        if path.len() > 1000 {
            return Err(anyhow!("Local project path too long"));
        }

        // Check for obvious dangerous patterns
        if path.contains("../../../") {
            return Err(anyhow!(
                "Local project path contains excessive directory traversal"
            ));
        }

        // Allow common safe patterns for development paths
        // This is lenient since the project URL is used for reference, not direct file access
        log::debug!("Validated local project path: {}", path);
        Ok(())
    }

    /// Validate project host (Git hosting services)
    fn validate_project_host(host: &str) -> Result<()> {
        // Common Git hosting services
        let trusted_hosts = [
            "github.com",
            "gitlab.com",
            "bitbucket.org",
            "codeberg.org",
            "git.sr.ht",
            "gitea.com",
            "gitee.com",
        ];

        // Check for trusted public hosts
        for trusted_host in &trusted_hosts {
            if host == *trusted_host || host.ends_with(&format!(".{}", trusted_host)) {
                return Ok(());
            }
        }

        // Allow private networks
        if Self::is_private_network_host(host) {
            return Ok(());
        }

        // For other hosts, log a warning but allow
        log::warn!(
            "Unknown Git host: {}. Ensure this is a trusted source.",
            host
        );
        Ok(())
    }

    /// Validate additional URL parameters
    fn validate_parameters(params: &std::collections::HashMap<String, String>) -> Result<()> {
        const MAX_PARAM_COUNT: usize = 10;
        const MAX_PARAM_LENGTH: usize = 1000;

        if params.len() > MAX_PARAM_COUNT {
            return Err(anyhow!(
                "Too many URL parameters (max: {})",
                MAX_PARAM_COUNT
            ));
        }

        for (key, value) in params {
            // Validate key
            if key.len() > MAX_PARAM_LENGTH {
                return Err(anyhow!("Parameter key too long: {}", key));
            }

            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err(anyhow!(
                    "Parameter key contains invalid characters: {}",
                    key
                ));
            }

            // Validate value
            if value.len() > MAX_PARAM_LENGTH {
                return Err(anyhow!("Parameter value too long for key: {}", key));
            }

            // Check for suspicious patterns in values
            if value.contains("../") || value.contains("..\\") {
                return Err(anyhow!(
                    "Parameter value contains suspicious patterns: {}",
                    key
                ));
            }
        }

        Ok(())
    }

    /// Check for suspicious parameter combinations
    fn check_suspicious_combinations(parsed_url: &EspbrewUrl) -> Result<()> {
        // Check if action and parameters make sense together
        match parsed_url.action.as_str() {
            "discover" => {
                if parsed_url.board.is_some() {
                    log::warn!("Discover action typically doesn't need board parameter");
                }
                if parsed_url.project.is_some() {
                    return Err(anyhow!(
                        "Discover action should not include project parameter"
                    ));
                }
            }
            "flash" => {
                // Flash can have board and/or project, this is normal
            }
            "monitor" => {
                if parsed_url.project.is_some() {
                    return Err(anyhow!(
                        "Monitor action should not include project parameter"
                    ));
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Check if a server URL is in the trusted list
    pub fn is_trusted_server(server_url: &str) -> bool {
        // Always trust localhost
        if server_url.contains("localhost") || server_url.contains("127.0.0.1") {
            return true;
        }

        // Parse URL to check host
        if let Ok(parsed) = Url::parse(server_url) {
            if let Some(host) = parsed.host_str() {
                return Self::is_private_network_host(host);
            }
        }

        false
    }

    /// Get list of trusted server patterns for configuration
    pub fn get_default_trusted_patterns() -> Vec<String> {
        vec![
            "http://localhost:*".to_string(),
            "https://localhost:*".to_string(),
            "http://127.0.0.1:*".to_string(),
            "https://127.0.0.1:*".to_string(),
            "http://192.168.*:*".to_string(),
            "https://192.168.*:*".to_string(),
            "http://10.*:*".to_string(),
            "https://10.*:*".to_string(),
            "http://*.local:*".to_string(),
            "https://*.local:*".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_validate_action() {
        assert!(UrlValidator::validate_action("flash").is_ok());
        assert!(UrlValidator::validate_action("discover").is_ok());
        assert!(UrlValidator::validate_action("monitor").is_ok());
        assert!(UrlValidator::validate_action("invalid").is_err());
        assert!(UrlValidator::validate_action("flash../").is_err());
    }

    #[test]
    fn test_validate_server_url() {
        assert!(UrlValidator::validate_server_url("http://localhost:8080").is_ok());
        assert!(UrlValidator::validate_server_url("https://192.168.1.100:8080").is_ok());
        assert!(UrlValidator::validate_server_url("ftp://localhost:8080").is_err());
        assert!(UrlValidator::validate_server_url("http://localhost:8080/../").is_err());
    }

    #[test]
    fn test_validate_board_identifier() {
        assert!(UrlValidator::validate_board_identifier("esp32-s3-box-3").is_ok());
        assert!(UrlValidator::validate_board_identifier("AA:BB:CC:DD:EE:FF").is_ok());
        assert!(UrlValidator::validate_board_identifier("invalid..name").is_err());
        assert!(UrlValidator::validate_board_identifier("AA:BB:CC:DD:EE").is_err()); // Invalid MAC
    }

    #[test]
    fn test_validate_project_url() {
        assert!(UrlValidator::validate_project_url("https://github.com/user/repo").is_ok());
        assert!(UrlValidator::validate_project_url("http://gitlab.local/project").is_ok());
        assert!(UrlValidator::validate_project_url("ftp://example.com/repo").is_err());
        assert!(UrlValidator::validate_project_url("https://github.com/../malicious").is_err());
    }

    #[test]
    fn test_mac_address_validation() {
        assert!(UrlValidator::looks_like_mac_address("AA:BB:CC:DD:EE:FF"));
        assert!(!UrlValidator::looks_like_mac_address("esp32-board"));

        assert!(UrlValidator::validate_mac_address("AA:BB:CC:DD:EE:FF").is_ok());
        assert!(UrlValidator::validate_mac_address("00:11:22:33:44:55").is_ok());
        assert!(UrlValidator::validate_mac_address("AA:BB:CC:DD:EE").is_err());
        assert!(UrlValidator::validate_mac_address("GG:BB:CC:DD:EE:FF").is_err());
    }

    #[test]
    fn test_is_trusted_server() {
        assert!(UrlValidator::is_trusted_server("http://localhost:8080"));
        assert!(UrlValidator::is_trusted_server("http://127.0.0.1:8080"));
        assert!(UrlValidator::is_trusted_server("http://192.168.1.100:8080"));
    }

    #[test]
    fn test_validate_parameters() {
        let mut params = HashMap::new();
        params.insert("key1".to_string(), "value1".to_string());
        params.insert("key_2".to_string(), "value-2".to_string());
        assert!(UrlValidator::validate_parameters(&params).is_ok());

        let mut bad_params = HashMap::new();
        bad_params.insert("key".to_string(), "../malicious".to_string());
        assert!(UrlValidator::validate_parameters(&bad_params).is_err());
    }
}
