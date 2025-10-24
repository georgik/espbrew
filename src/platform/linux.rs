//! Linux-specific implementation for espbrew:// URL handler registration
//!
//! This module handles .desktop file creation and XDG MIME type registration
//! for the espbrew:// custom URL protocol on Linux systems.

use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use super::{PlatformRegistrar, utils};

/// Linux-specific URL handler registrar
pub struct LinuxRegistrar;

impl PlatformRegistrar for LinuxRegistrar {
    fn register() -> Result<()> {
        println!("🍺 Registering espbrew:// URL handler on Linux...");

        let exe_path = utils::get_executable_path()?;
        let desktop_file_path = Self::get_desktop_file_path()?;

        // Create .desktop file
        Self::create_desktop_file(&desktop_file_path, &exe_path)?;

        // Update desktop database
        Self::update_desktop_database()?;

        // Set as default handler
        Self::set_default_handler()?;

        println!("✅ Successfully registered espbrew:// URL handler");
        println!("💡 You can now click espbrew:// links in web browsers");

        Ok(())
    }

    fn unregister() -> Result<()> {
        println!("🍺 Unregistering espbrew:// URL handler on Linux...");

        let desktop_file_path = Self::get_desktop_file_path()?;

        if desktop_file_path.exists() {
            fs::remove_file(&desktop_file_path)
                .map_err(|e| anyhow!("Failed to remove desktop file: {}", e))?;
            println!("✅ Removed desktop file: {}", desktop_file_path.display());
        }

        // Update desktop database
        Self::update_desktop_database()?;

        println!("✅ Successfully unregistered espbrew:// URL handler");
        Ok(())
    }

    fn is_registered() -> Result<bool> {
        let desktop_file_path = Self::get_desktop_file_path()?;
        Ok(desktop_file_path.exists())
    }

    fn get_install_instructions() -> String {
        r#"Linux Installation Instructions:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

ESPBrew will create a .desktop file to register the URL handler.
This is the standard way Linux applications handle custom URL schemes.

The desktop file will be created at:
  ~/.local/share/applications/espbrew.desktop

Manual steps (if needed):
1. Ensure ESPBrew is installed in your PATH
2. Run: espbrew --register-handler
3. Update desktop database: update-desktop-database ~/.local/share/applications

Troubleshooting:
• If registration fails, check file permissions
• Ensure XDG desktop utilities are installed
• Verify the desktop environment supports custom URL handlers"#
            .to_string()
    }
}

impl LinuxRegistrar {
    /// Get the path for the .desktop file
    fn get_desktop_file_path() -> Result<PathBuf> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;

        let desktop_file_path = home_dir
            .join(".local")
            .join("share")
            .join("applications")
            .join("espbrew.desktop");

        Ok(desktop_file_path)
    }

    /// Create the .desktop file
    fn create_desktop_file(desktop_file_path: &PathBuf, exe_path: &std::path::Path) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = desktop_file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| anyhow!("Failed to create directory: {}", e))?;
        }

        let desktop_content = format!(
            r#"[Desktop Entry]
Name=ESPBrew
Comment=ESP32 Multi-Board Development Platform
Exec={} --handle-url %u
Type=Application
Terminal=false
MimeType=x-scheme-handler/espbrew;
Icon=application-x-executable
Categories=Development;
Keywords=ESP32;embedded;development;flash;
StartupNotify=false
NoDisplay=true
"#,
            exe_path.display()
        );

        fs::write(desktop_file_path, desktop_content)
            .map_err(|e| anyhow!("Failed to write desktop file: {}", e))?;

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(desktop_file_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(desktop_file_path, perms)?;
        }

        println!("✅ Created desktop file: {}", desktop_file_path.display());
        Ok(())
    }

    /// Update the desktop database
    fn update_desktop_database() -> Result<()> {
        let home_dir =
            dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;

        let applications_dir = home_dir.join(".local").join("share").join("applications");

        let output = Command::new("update-desktop-database")
            .arg(&applications_dir)
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Updated desktop database");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::warn!("Failed to update desktop database: {}", stderr);
                }
            }
            Err(e) => {
                log::warn!("Could not run update-desktop-database: {}", e);
                println!("⚠️  Could not update desktop database automatically");
            }
        }

        Ok(())
    }

    /// Set as default handler for espbrew:// URLs
    fn set_default_handler() -> Result<()> {
        let output = Command::new("xdg-mime")
            .args(&["default", "espbrew.desktop", "x-scheme-handler/espbrew"])
            .output();

        match output {
            Ok(output) => {
                if output.status.success() {
                    println!("✅ Set as default handler for espbrew:// URLs");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::warn!("Failed to set default handler: {}", stderr);
                }
            }
            Err(e) => {
                log::warn!("Could not run xdg-mime: {}", e);
                println!("⚠️  Could not set default handler automatically");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_file_path() {
        let result = LinuxRegistrar::get_desktop_file_path();
        assert!(result.is_ok());

        let path = result.unwrap();
        assert!(path.to_string_lossy().ends_with("espbrew.desktop"));
        assert!(path.to_string_lossy().contains(".local/share/applications"));
    }

    #[test]
    fn test_install_instructions() {
        let instructions = LinuxRegistrar::get_install_instructions();
        assert!(instructions.contains("Linux Installation Instructions"));
        assert!(instructions.contains(".desktop"));
    }
}
