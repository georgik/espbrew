//! Platform-specific implementations for espbrew:// URL handler registration
//!
//! This module provides cross-platform support for registering and unregistering
//! the espbrew:// custom URL protocol handler with the operating system.

use anyhow::Result;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Platform-specific URL handler registration
pub struct UrlHandlerRegistrar;

impl UrlHandlerRegistrar {
    /// Register espbrew:// URL handler with the operating system
    pub fn register() -> Result<()> {
        log::info!(
            "Registering espbrew:// URL handler for platform: {}",
            std::env::consts::OS
        );

        #[cfg(target_os = "macos")]
        {
            macos::MacOSRegistrar::register()
        }

        #[cfg(target_os = "linux")]
        {
            linux::LinuxRegistrar::register()
        }

        #[cfg(target_os = "windows")]
        {
            windows::WindowsRegistrar::register()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(anyhow::anyhow!(
                "URL handler registration not supported on this platform: {}",
                std::env::consts::OS
            ))
        }
    }

    /// Unregister espbrew:// URL handler from the operating system
    pub fn unregister() -> Result<()> {
        log::info!(
            "Unregistering espbrew:// URL handler for platform: {}",
            std::env::consts::OS
        );

        #[cfg(target_os = "macos")]
        {
            macos::MacOSRegistrar::unregister()
        }

        #[cfg(target_os = "linux")]
        {
            linux::LinuxRegistrar::unregister()
        }

        #[cfg(target_os = "windows")]
        {
            windows::WindowsRegistrar::unregister()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err(anyhow::anyhow!(
                "URL handler unregistration not supported on this platform: {}",
                std::env::consts::OS
            ))
        }
    }

    /// Check if espbrew:// URL handler is currently registered
    pub fn is_registered() -> Result<bool> {
        #[cfg(target_os = "macos")]
        {
            macos::MacOSRegistrar::is_registered()
        }

        #[cfg(target_os = "linux")]
        {
            linux::LinuxRegistrar::is_registered()
        }

        #[cfg(target_os = "windows")]
        {
            windows::WindowsRegistrar::is_registered()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Ok(false)
        }
    }

    /// Get platform-specific installation instructions
    pub fn get_install_instructions() -> String {
        #[cfg(target_os = "macos")]
        {
            macos::MacOSRegistrar::get_install_instructions()
        }

        #[cfg(target_os = "linux")]
        {
            linux::LinuxRegistrar::get_install_instructions()
        }

        #[cfg(target_os = "windows")]
        {
            windows::WindowsRegistrar::get_install_instructions()
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            format!(
                "Platform {} is not supported for URL handler registration",
                std::env::consts::OS
            )
        }
    }

    /// Show status information about URL handler registration
    pub fn show_status() -> Result<()> {
        println!("🍺 ESPBrew URL Handler Status");
        println!("═══════════════════════════");
        println!("Platform: {}", std::env::consts::OS);

        match Self::is_registered() {
            Ok(true) => {
                println!("Status: ✅ Registered");
                println!("Protocol: espbrew://");

                // Show additional platform-specific info
                #[cfg(target_os = "macos")]
                {
                    macos::MacOSRegistrar::show_detailed_status()?;
                }

                println!("\n💡 You can now use espbrew:// links from web browsers");
                println!("💡 Try: espbrew://discover?server=local");
            }
            Ok(false) => {
                println!("Status: ❌ Not registered");
                println!("\nTo register the URL handler, run:");
                println!("  espbrew --register-handler");
                println!("\n{}", Self::get_install_instructions());
            }
            Err(e) => {
                println!("Status: ⚠️  Error checking registration: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}

/// Common trait for platform-specific registrars
pub trait PlatformRegistrar {
    /// Register the URL handler
    fn register() -> Result<()>;

    /// Unregister the URL handler
    fn unregister() -> Result<()>;

    /// Check if the URL handler is registered
    fn is_registered() -> Result<bool>;

    /// Get platform-specific installation instructions
    fn get_install_instructions() -> String;
}

/// Utility functions for cross-platform operations
pub mod utils {
    use anyhow::Result;
    use std::path::PathBuf;

    /// Get the current executable path
    pub fn get_executable_path() -> Result<PathBuf> {
        std::env::current_exe()
            .map_err(|e| anyhow::anyhow!("Failed to get current executable path: {}", e))
    }

    /// Get the executable name without extension
    pub fn get_executable_name() -> Result<String> {
        let exe_path = get_executable_path()?;
        let name = exe_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Failed to get executable name"))?;
        Ok(name.to_string())
    }

    /// Check if running with elevated privileges
    pub fn is_elevated() -> bool {
        #[cfg(target_os = "windows")]
        {
            // On Windows, check if running as admin
            windows::WindowsRegistrar::is_admin()
        }

        #[cfg(unix)]
        {
            // On Unix systems, check if running as root by checking environment
            std::env::var("USER").map_or(false, |user| user == "root")
                || std::env::var("SUDO_UID").is_ok()
        }
    }

    /// Create a backup of configuration files before making changes
    pub fn backup_config_file(file_path: &std::path::Path) -> Result<PathBuf> {
        if !file_path.exists() {
            return Ok(file_path.to_path_buf());
        }

        let backup_path = file_path.with_extension(format!(
            "{}.espbrew-backup",
            file_path.extension().and_then(|s| s.to_str()).unwrap_or("")
        ));

        std::fs::copy(file_path, &backup_path).map_err(|e| {
            anyhow::anyhow!("Failed to create backup of {}: {}", file_path.display(), e)
        })?;

        log::info!("Created backup: {}", backup_path.display());
        Ok(backup_path)
    }

    /// Restore configuration file from backup
    pub fn restore_config_file(
        original_path: &std::path::Path,
        backup_path: &std::path::Path,
    ) -> Result<()> {
        if backup_path.exists() {
            std::fs::copy(backup_path, original_path)
                .map_err(|e| anyhow::anyhow!("Failed to restore backup: {}", e))?;
            std::fs::remove_file(backup_path)
                .map_err(|e| anyhow::anyhow!("Failed to remove backup file: {}", e))?;
            log::info!("Restored configuration from backup");
        }
        Ok(())
    }
}
