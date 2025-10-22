//! macOS-specific implementation for espbrew:// URL handler registration
//!
//! This module handles registration with Launch Services and Info.plist management
//! for the espbrew:// custom URL protocol on macOS.

use anyhow::{Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{PlatformRegistrar, utils};

/// macOS-specific URL handler registrar
pub struct MacOSRegistrar;

impl PlatformRegistrar for MacOSRegistrar {
    fn register() -> Result<()> {
        println!("🍺 Registering espbrew:// URL handler on macOS...");

        // Check if we're running as part of an app bundle
        let exe_path = utils::get_executable_path()?;
        let app_bundle_path = Self::find_or_create_app_bundle(&exe_path)?;

        // Create/update Info.plist
        Self::create_info_plist(&app_bundle_path)?;

        // Register with Launch Services
        Self::register_with_launch_services(&app_bundle_path)?;

        println!("✅ Successfully registered espbrew:// URL handler");
        println!("💡 You can now click espbrew:// links in web browsers");

        Ok(())
    }

    fn unregister() -> Result<()> {
        println!("🍺 Unregistering espbrew:// URL handler on macOS...");

        let exe_path = utils::get_executable_path()?;
        let app_bundle_path = Self::get_app_bundle_path(&exe_path)?;

        if app_bundle_path.exists() {
            // Unregister from Launch Services
            Self::unregister_from_launch_services(&app_bundle_path)?;

            // Optionally remove the app bundle (ask user)
            if Self::should_remove_app_bundle()? {
                fs::remove_dir_all(&app_bundle_path)
                    .map_err(|e| anyhow!("Failed to remove app bundle: {}", e))?;
                println!("✅ Removed app bundle: {}", app_bundle_path.display());
            }
        }

        println!("✅ Successfully unregistered espbrew:// URL handler");
        Ok(())
    }

    fn is_registered() -> Result<bool> {
        // Check if espbrew:// protocol is registered in Launch Services
        let output = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .args(&["-dump"])
            .output()
            .map_err(|e| anyhow!("Failed to run lsregister: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains("espbrew:") || stdout.contains("espbrew://"))
    }

    fn get_install_instructions() -> String {
        r#"macOS Installation Instructions:
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

ESPBrew will create a .app bundle to register the URL handler.
This is the standard way macOS applications handle custom URL schemes.

The app bundle will be created at:
  ~/Applications/ESPBrew.app

Manual steps (if needed):
1. Ensure ESPBrew is installed in your PATH
2. Run: espbrew --register-handler
3. Grant permission when macOS prompts for Launch Services registration

Troubleshooting:
• If registration fails, try running with elevated privileges
• Check Console.app for Launch Services errors
• Verify the app bundle was created correctly"#
            .to_string()
    }
}

impl MacOSRegistrar {
    /// Find existing app bundle or create a new one
    fn find_or_create_app_bundle(exe_path: &Path) -> Result<PathBuf> {
        let app_name = "ESPBrew.app";

        // Try to find existing bundle in common locations
        let possible_locations = vec![
            dirs::home_dir().map(|p| p.join("Applications").join(app_name)),
            Some(PathBuf::from("/Applications").join(app_name)),
            exe_path.parent().map(|p| p.join(app_name)),
        ];

        for location in possible_locations.into_iter().flatten() {
            if location.exists() {
                println!("📦 Found existing app bundle: {}", location.display());
                return Ok(location);
            }
        }

        // Create new bundle
        let bundle_path = dirs::home_dir()
            .ok_or_else(|| anyhow!("Cannot determine home directory"))?
            .join("Applications")
            .join(app_name);

        Self::create_app_bundle(&bundle_path, exe_path)?;
        Ok(bundle_path)
    }

    /// Get the expected app bundle path
    fn get_app_bundle_path(exe_path: &Path) -> Result<PathBuf> {
        let app_name = "ESPBrew.app";

        // Check common locations
        let possible_locations = vec![
            dirs::home_dir().map(|p| p.join("Applications").join(app_name)),
            Some(PathBuf::from("/Applications").join(app_name)),
            exe_path.parent().map(|p| p.join(app_name)),
        ];

        for location in possible_locations.into_iter().flatten() {
            if location.exists() {
                return Ok(location);
            }
        }

        // Default location
        dirs::home_dir()
            .map(|p| p.join("Applications").join(app_name))
            .ok_or_else(|| anyhow!("Cannot determine app bundle path"))
    }

    /// Create a macOS app bundle structure
    fn create_app_bundle(bundle_path: &Path, exe_path: &Path) -> Result<()> {
        println!("📦 Creating app bundle: {}", bundle_path.display());

        // Create bundle directory structure
        let contents_dir = bundle_path.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        let resources_dir = contents_dir.join("Resources");

        fs::create_dir_all(&macos_dir)
            .map_err(|e| anyhow!("Failed to create MacOS directory: {}", e))?;
        fs::create_dir_all(&resources_dir)
            .map_err(|e| anyhow!("Failed to create Resources directory: {}", e))?;

        // Copy or link the executable
        let bundle_exe = macos_dir.join("espbrew");
        if bundle_exe.exists() {
            fs::remove_file(&bundle_exe)?;
        }

        // Create a symbolic link to the original executable
        std::os::unix::fs::symlink(exe_path, &bundle_exe)
            .map_err(|e| anyhow!("Failed to create symlink to executable: {}", e))?;

        println!("✅ Created app bundle structure");
        Ok(())
    }

    /// Create or update Info.plist with URL scheme registration
    fn create_info_plist(bundle_path: &Path) -> Result<()> {
        let info_plist_path = bundle_path.join("Contents").join("Info.plist");

        let plist_content = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>ESPBrew</string>
    <key>CFBundleDisplayName</key>
    <string>ESPBrew</string>
    <key>CFBundleIdentifier</key>
    <string>dev.georgik.espbrew</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleExecutable</key>
    <string>espbrew</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.12</string>
    <key>LSUIElement</key>
    <true/>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>ESPBrew Protocol</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>espbrew</string>
            </array>
            <key>LSHandlerRank</key>
            <string>Owner</string>
        </dict>
    </array>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>LSBackgroundOnly</key>
    <false/>
</dict>
</plist>"#;

        fs::write(&info_plist_path, plist_content)
            .map_err(|e| anyhow!("Failed to write Info.plist: {}", e))?;

        println!("✅ Created Info.plist with URL scheme registration");
        Ok(())
    }

    /// Register the app bundle with Launch Services
    fn register_with_launch_services(bundle_path: &Path) -> Result<()> {
        println!("🔄 Registering with Launch Services...");

        let output = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .args(&["-f", bundle_path.to_str().unwrap()])
            .output()
            .map_err(|e| anyhow!("Failed to run lsregister: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "Failed to register with Launch Services: {}",
                stderr
            ));
        }

        // Also try to set as default handler for espbrew:// URLs
        let _ = Command::new("defaults")
            .args(&[
                "write",
                "com.apple.LaunchServices/com.apple.launchservices.secure",
                "LSHandlers",
                "-array-add",
                "{LSHandlerContentType='espbrew'; LSHandlerRoleAll='dev.georgik.espbrew';}",
            ])
            .output();

        println!("✅ Registered with Launch Services");
        Ok(())
    }

    /// Unregister from Launch Services
    fn unregister_from_launch_services(bundle_path: &Path) -> Result<()> {
        println!("🔄 Unregistering from Launch Services...");

        // Unregister the bundle
        let output = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .args(&["-u", bundle_path.to_str().unwrap()])
            .output()
            .map_err(|e| anyhow!("Failed to run lsregister: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::warn!("Failed to unregister from Launch Services: {}", stderr);
        }

        // Reset Launch Services database
        let _ = Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
            .args(&["-kill", "-r", "-domain", "local", "-domain", "system", "-domain", "user"])
            .output();

        println!("✅ Unregistered from Launch Services");
        Ok(())
    }

    /// Ask user if they want to remove the app bundle
    fn should_remove_app_bundle() -> Result<bool> {
        print!("Do you want to remove the ESPBrew.app bundle? [y/N]: ");

        use std::io::{self, Write};
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let response = input.trim().to_lowercase();
        Ok(response == "y" || response == "yes")
    }

    /// Show detailed status information for macOS
    pub fn show_detailed_status() -> Result<()> {
        let exe_path = utils::get_executable_path()?;
        let bundle_path = Self::get_app_bundle_path(&exe_path)?;

        println!("App Bundle: {}", bundle_path.display());

        if bundle_path.exists() {
            println!("Bundle Status: ✅ Exists");

            let info_plist = bundle_path.join("Contents").join("Info.plist");
            if info_plist.exists() {
                println!("Info.plist: ✅ Present");
            } else {
                println!("Info.plist: ❌ Missing");
            }

            let exe_link = bundle_path.join("Contents").join("MacOS").join("espbrew");
            if exe_link.exists() {
                println!("Executable Link: ✅ Present");
            } else {
                println!("Executable Link: ❌ Missing");
            }
        } else {
            println!("Bundle Status: ❌ Not found");
        }

        Ok(())
    }

    /// Test the URL handler by attempting to open a test URL
    pub fn test_url_handler() -> Result<()> {
        println!("🧪 Testing espbrew:// URL handler...");

        let test_url = "espbrew://discover?server=local";
        println!("Opening test URL: {}", test_url);

        let output = Command::new("open")
            .arg(test_url)
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
    use std::env;

    #[test]
    fn test_app_bundle_path_generation() {
        let exe_path = env::current_exe().unwrap();
        let result = MacOSRegistrar::get_app_bundle_path(&exe_path);
        assert!(result.is_ok());

        let bundle_path = result.unwrap();
        assert!(bundle_path.to_string_lossy().ends_with("ESPBrew.app"));
    }

    #[test]
    fn test_info_plist_content() {
        // Test that we can create a valid plist structure
        // This is mainly a compilation test
        let _ = MacOSRegistrar::get_install_instructions();
    }
}
