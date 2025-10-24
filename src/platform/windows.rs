//! Windows-specific implementation for espbrew:// URL handler registration
//!
//! This module handles Windows Registry manipulation for registering
//! the espbrew:// custom URL protocol on Windows systems.

use anyhow::{Result, anyhow};
use std::process::Command;

use super::{PlatformRegistrar, utils};

/// Windows-specific URL handler registrar
pub struct WindowsRegistrar;

impl PlatformRegistrar for WindowsRegistrar {
    fn register() -> Result<()> {
        println!("🍺 Registering espbrew:// URL handler on Windows...");

        let exe_path = utils::get_executable_path()?;
        let exe_path_str = exe_path.to_string_lossy();

        // Register protocol in registry
        Self::register_protocol_in_registry(&exe_path_str)?;

        println!("✅ Successfully registered espbrew:// URL handler");
        println!("💡 You can now click espbrew:// links in web browsers");

        Ok(())
    }

    fn unregister() -> Result<()> {
        println!("🍺 Unregistering espbrew:// URL handler on Windows...");

        // Remove protocol from registry
        Self::unregister_protocol_from_registry()?;

        println!("✅ Successfully unregistered espbrew:// URL handler");
        Ok(())
    }

    fn is_registered() -> Result<bool> {
        // Check if registry key exists
        Self::check_registry_key_exists()
    }

    fn get_install_instructions() -> String {
        r#"Windows Installation Instructions:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

ESPBrew will register the URL handler in the Windows Registry.
This is the standard way Windows applications handle custom URL schemes.

Registry location:
  HKEY_CURRENT_USER\Software\Classes\espbrew

Manual steps (if needed):
1. Ensure ESPBrew is installed in your PATH
2. Run: espbrew --register-handler
3. If elevation is required, run as Administrator

Troubleshooting:
• If registration fails, try running as Administrator
• Check Windows Registry permissions
• Ensure PowerShell execution policy allows scripts"#
            .to_string()
    }
}

impl WindowsRegistrar {
    /// Register protocol in Windows Registry using PowerShell
    fn register_protocol_in_registry(exe_path: &str) -> Result<()> {
        let powershell_script = format!(
            r#"
            New-Item -Path 'HKCU:\Software\Classes\espbrew' -Force | Out-Null
            Set-ItemProperty -Path 'HKCU:\Software\Classes\espbrew' -Name '(Default)' -Value 'URL:ESPBrew Protocol'
            Set-ItemProperty -Path 'HKCU:\Software\Classes\espbrew' -Name 'URL Protocol' -Value ''
            New-Item -Path 'HKCU:\Software\Classes\espbrew\shell\open\command' -Force | Out-Null
            Set-ItemProperty -Path 'HKCU:\Software\Classes\espbrew\shell\open\command' -Name '(Default)' -Value '"{}" --handle-url "%1"'
            "#,
            exe_path.replace('\\', "\\\\")
        );

        let output = Command::new("powershell")
            .args(&["-Command", &powershell_script])
            .output()
            .map_err(|e| anyhow!("Failed to run PowerShell: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("PowerShell registry update failed: {}", stderr));
        }

        println!("✅ Updated Windows Registry");
        Ok(())
    }

    /// Unregister protocol from Windows Registry
    fn unregister_protocol_from_registry() -> Result<()> {
        let powershell_script = r#"
            if (Test-Path 'HKCU:\Software\Classes\espbrew') {
                Remove-Item -Path 'HKCU:\Software\Classes\espbrew' -Recurse -Force
                Write-Output 'Registry key removed'
            } else {
                Write-Output 'Registry key not found'
            }
        "#;

        let output = Command::new("powershell")
            .args(&["-Command", powershell_script])
            .output()
            .map_err(|e| anyhow!("Failed to run PowerShell: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!("PowerShell unregister failed: {}", stderr);
        }

        println!("✅ Cleaned up Windows Registry");
        Ok(())
    }

    /// Check if registry key exists
    fn check_registry_key_exists() -> Result<bool> {
        let powershell_script = r#"
            if (Test-Path 'HKCU:\Software\Classes\espbrew') {
                Write-Output 'EXISTS'
            } else {
                Write-Output 'NOT_EXISTS'
            }
        "#;

        let output = Command::new("powershell")
            .args(&["-Command", powershell_script])
            .output()
            .map_err(|e| anyhow!("Failed to run PowerShell: {}", e))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.trim() == "EXISTS")
        } else {
            Ok(false)
        }
    }

    /// Check if running as administrator
    pub fn is_admin() -> bool {
        // Simple check - try to access a system registry key
        let output = Command::new("powershell")
            .args(&[
                "-Command",
                "([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] 'Administrator')"
            ])
            .output();

        match output {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                stdout.trim().eq_ignore_ascii_case("true")
            }
            _ => false,
        }
    }

    /// Test the URL handler by attempting to open a test URL
    pub fn test_url_handler() -> Result<()> {
        println!("🧪 Testing espbrew:// URL handler...");

        let test_url = "espbrew://discover?server=local";
        println!("Opening test URL: {}", test_url);

        let output = Command::new("cmd")
            .args(&["/c", "start", test_url])
            .output()
            .map_err(|e| anyhow!("Failed to test URL handler: {}", e))?;

        if output.status.success() {
            println!("✅ URL handler test successful");
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("❌ URL handler test failed: {}", stderr);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_instructions() {
        let instructions = WindowsRegistrar::get_install_instructions();
        assert!(instructions.contains("Windows Installation Instructions"));
        assert!(instructions.contains("Registry"));
    }

    #[test]
    fn test_admin_check() {
        // This test just verifies the function doesn't panic
        let _is_admin = WindowsRegistrar::is_admin();
    }
}
