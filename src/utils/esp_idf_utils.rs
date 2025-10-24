//! ESP-IDF utilities for cross-platform command detection and environment management

use crate::models::esp_idf_config::{
    DetectedEspIdfInstallation, EimConfig, EspIdfDetectionResult, EspIdfDetectionSource,
    EspIdfInstallation,
};

use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Main function to detect all available ESP-IDF installations
pub fn detect_esp_idf_installations() -> EspIdfDetectionResult {
    let mut result = EspIdfDetectionResult::default();

    // Try EIM detection first (Windows)
    if let Some(eim_installations) = detect_from_eim() {
        result.installations.extend(eim_installations);
    }

    // Try PATH detection
    if let Some(path_installation) = detect_from_path() {
        result.installations.push(path_installation);
    }

    // Try environment variable detection
    if let Some(env_installation) = detect_from_environment() {
        result.installations.push(env_installation);
    }

    // Try standard location detection
    result
        .installations
        .extend(detect_from_standard_locations());

    // Remove duplicates based on path
    result.installations = deduplicate_installations(result.installations);

    // Set default installation
    result.default_installation = select_default_installation(&result.installations);

    // Validate installations and add warnings
    for installation in &mut result.installations {
        if !installation.installation.is_valid() {
            installation.is_available = false;
            result.warnings.push(format!(
                "ESP-IDF installation at {} appears to be invalid or incomplete",
                installation.installation.path.display()
            ));
        }
    }

    if result.installations.is_empty() {
        result.warnings.push(
            "No ESP-IDF installations found. Please install ESP-IDF or check your environment."
                .to_string(),
        );
    }

    result
}

/// Get the best available ESP-IDF command for the current system
pub fn get_esp_idf_command() -> Result<String> {
    let detection_result = detect_esp_idf_installations();

    if let Some(default_installation) = detection_result.default_installation {
        Ok(default_installation.command_name)
    } else {
        Err(anyhow::anyhow!(
            "No ESP-IDF installation found. Please install ESP-IDF and ensure it's available in PATH or via EIM."
        ))
    }
}

/// Check if ESP-IDF is available on the system
pub fn is_esp_idf_available() -> bool {
    let detection_result = detect_esp_idf_installations();
    detection_result.default_installation.is_some()
}

/// Get environment variables needed for ESP-IDF commands
pub fn get_esp_idf_environment() -> Result<HashMap<String, String>> {
    let detection_result = detect_esp_idf_installations();

    if let Some(default_installation) = detection_result.default_installation {
        Ok(default_installation.environment)
    } else {
        Ok(HashMap::new())
    }
}

/// Detect ESP-IDF installations from EIM configuration
fn detect_from_eim() -> Option<Vec<DetectedEspIdfInstallation>> {
    let eim_config = find_eim_configuration()?;
    let mut installations = Vec::new();

    for installation in eim_config.idf_installed {
        if installation.is_valid() {
            let command_name = resolve_command_name(&installation.path);
            installations.push(DetectedEspIdfInstallation::new(
                installation,
                EspIdfDetectionSource::Eim,
                command_name,
            ));
        }
    }

    Some(installations)
}

/// Find and parse EIM configuration file
pub fn find_eim_configuration() -> Option<EimConfig> {
    // Standard EIM configuration location on Windows
    let eim_path = PathBuf::from("C:\\Espressif\\tools\\eim_idf.json");

    if eim_path.exists() {
        match fs::read_to_string(&eim_path) {
            Ok(content) => match serde_json::from_str::<EimConfig>(&content) {
                Ok(config) => {
                    log::debug!(
                        "Found EIM configuration with {} installations",
                        config.idf_installed.len()
                    );
                    Some(config)
                }
                Err(e) => {
                    log::warn!("Failed to parse EIM configuration: {}", e);
                    None
                }
            },
            Err(e) => {
                log::warn!("Failed to read EIM configuration file: {}", e);
                None
            }
        }
    } else {
        None
    }
}

/// Detect ESP-IDF from system PATH
fn detect_from_path() -> Option<DetectedEspIdfInstallation> {
    // Try both idf.py and idf.py.exe
    let candidates = ["idf.py", "idf.py.exe"];

    for candidate in candidates {
        if let Some(idf_path) = find_in_path(candidate) {
            // Try to determine ESP-IDF root from the idf.py location
            if let Some(esp_idf_root) = find_esp_idf_root_from_command(&idf_path) {
                let version =
                    get_esp_idf_version(&esp_idf_root).unwrap_or_else(|| "unknown".to_string());
                let installation = EspIdfInstallation::new(version, esp_idf_root);

                return Some(DetectedEspIdfInstallation::new(
                    installation,
                    EspIdfDetectionSource::SystemPath,
                    candidate.to_string(),
                ));
            }
        }
    }

    None
}

/// Detect ESP-IDF from environment variables
fn detect_from_environment() -> Option<DetectedEspIdfInstallation> {
    if let Ok(idf_path) = env::var("IDF_PATH") {
        let esp_idf_root = PathBuf::from(idf_path);
        if esp_idf_root.exists() {
            let version =
                get_esp_idf_version(&esp_idf_root).unwrap_or_else(|| "unknown".to_string());
            let installation = EspIdfInstallation::new(version, esp_idf_root.clone());
            let command_name = resolve_command_name(&esp_idf_root);

            return Some(DetectedEspIdfInstallation::new(
                installation,
                EspIdfDetectionSource::EnvironmentVariable,
                command_name,
            ));
        }
    }

    None
}

/// Detect ESP-IDF from standard installation locations
fn detect_from_standard_locations() -> Vec<DetectedEspIdfInstallation> {
    let mut installations = Vec::new();
    let standard_locations = get_standard_esp_idf_locations();

    for location in standard_locations {
        if location.exists() {
            let version = get_esp_idf_version(&location).unwrap_or_else(|| "unknown".to_string());
            let installation = EspIdfInstallation::new(version, location.clone());
            let command_name = resolve_command_name(&location);

            installations.push(DetectedEspIdfInstallation::new(
                installation,
                EspIdfDetectionSource::StandardLocation,
                command_name,
            ));
        }
    }

    installations
}

/// Get standard ESP-IDF installation locations for the current platform
fn get_standard_esp_idf_locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();

    if cfg!(windows) {
        // Windows standard locations
        if let Ok(user_profile) = env::var("USERPROFILE") {
            locations.push(PathBuf::from(user_profile).join("esp").join("esp-idf"));
        }

        // EIM framework installations
        let espressif_frameworks = PathBuf::from("C:\\Espressif\\frameworks");
        if espressif_frameworks.exists() {
            if let Ok(entries) = fs::read_dir(&espressif_frameworks) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with("esp-idf-") {
                            locations.push(entry.path());
                        }
                    }
                }
            }
        }
    } else {
        // Unix-like systems (macOS, Linux)
        if let Ok(home) = env::var("HOME") {
            locations.push(PathBuf::from(home).join("esp").join("esp-idf"));
        }

        // System-wide installation
        locations.push(PathBuf::from("/opt/esp-idf"));
    }

    locations
}

/// Resolve the correct command name for an ESP-IDF installation
fn resolve_command_name(esp_idf_root: &Path) -> String {
    let idf_py_path = esp_idf_root.join("tools").join("idf.py");
    let idf_py_exe_path = idf_py_path.with_extension("exe");

    if idf_py_exe_path.exists() {
        "idf.py.exe".to_string()
    } else if idf_py_path.exists() {
        "idf.py".to_string()
    } else {
        // Fallback - might be in PATH
        if cfg!(windows) {
            "idf.py.exe".to_string()
        } else {
            "idf.py".to_string()
        }
    }
}

/// Find ESP-IDF root directory from idf.py command location
fn find_esp_idf_root_from_command(idf_py_path: &Path) -> Option<PathBuf> {
    // idf.py is typically at ESP-IDF/tools/idf.py
    if let Some(parent) = idf_py_path.parent() {
        if parent.file_name()? == "tools" {
            if let Some(root) = parent.parent() {
                return Some(root.to_path_buf());
            }
        }
    }

    None
}

/// Get ESP-IDF version from installation directory
fn get_esp_idf_version(esp_idf_root: &Path) -> Option<String> {
    // Try to read version from version.txt file
    let version_file = esp_idf_root.join("version.txt");
    if version_file.exists() {
        if let Ok(version) = fs::read_to_string(version_file) {
            return Some(version.trim().to_string());
        }
    }

    // Try to get version from idf.py --version
    let idf_py_path = esp_idf_root.join("tools").join("idf.py");
    let command_name = if idf_py_path.with_extension("exe").exists() {
        idf_py_path.with_extension("exe")
    } else {
        idf_py_path
    };

    if command_name.exists() {
        if let Ok(output) = Command::new(command_name).arg("--version").output() {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                // Parse version from output like "ESP-IDF v5.3.1"
                if let Some(version_part) = version_output.split_whitespace().nth(1) {
                    return Some(version_part.to_string());
                }
            }
        }
    }

    None
}

/// Find a command in the system PATH
pub fn find_in_path(command: &str) -> Option<PathBuf> {
    if let Ok(path_var) = env::var("PATH") {
        let paths = env::split_paths(&path_var);

        for path in paths {
            let command_path = path.join(command);
            if command_path.exists() && command_path.is_file() {
                return Some(command_path);
            }

            // On Windows, also try with .exe extension
            if cfg!(windows) && !command.ends_with(".exe") {
                let command_exe_path = path.join(format!("{}.exe", command));
                if command_exe_path.exists() && command_exe_path.is_file() {
                    return Some(command_exe_path);
                }
            }
        }
    }

    None
}

/// Check if a command is available in PATH or as a file
pub fn is_command_available(command: &str) -> bool {
    // Try direct path first
    let command_path = PathBuf::from(command);
    if command_path.exists() && command_path.is_file() {
        return true;
    }

    // Try finding in PATH
    find_in_path(command).is_some()
}

/// Remove duplicate installations based on path
fn deduplicate_installations(
    installations: Vec<DetectedEspIdfInstallation>,
) -> Vec<DetectedEspIdfInstallation> {
    let mut unique_installations = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for installation in installations {
        let canonical_path = installation
            .installation
            .path
            .canonicalize()
            .unwrap_or_else(|_| installation.installation.path.clone());

        if seen_paths.insert(canonical_path) {
            unique_installations.push(installation);
        }
    }

    unique_installations
}

/// Select the default ESP-IDF installation from available options
fn select_default_installation(
    installations: &[DetectedEspIdfInstallation],
) -> Option<DetectedEspIdfInstallation> {
    // Priority order:
    // 1. EIM installations marked as active
    // 2. Environment variable installations
    // 3. PATH installations
    // 4. Most recent EIM installation
    // 5. Most recent standard location installation

    // Check for active EIM installation
    for installation in installations {
        if installation.detection_source == EspIdfDetectionSource::Eim {
            if let Some(true) = installation.installation.active {
                return Some(installation.clone());
            }
        }
    }

    // Check for environment variable installation
    for installation in installations {
        if installation.detection_source == EspIdfDetectionSource::EnvironmentVariable {
            return Some(installation.clone());
        }
    }

    // Check for PATH installation
    for installation in installations {
        if installation.detection_source == EspIdfDetectionSource::SystemPath {
            return Some(installation.clone());
        }
    }

    // Use the first available installation as fallback
    installations.first().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_availability() {
        // Test with a command that should always be available
        #[cfg(unix)]
        assert!(is_command_available("ls"));

        #[cfg(windows)]
        assert!(is_command_available("dir"));
    }

    #[test]
    fn test_esp_idf_detection() {
        let result = detect_esp_idf_installations();
        eprintln!(
            "Detected {} ESP-IDF installations",
            result.installations.len()
        );

        for installation in &result.installations {
            eprintln!("  - {}", installation.get_description());
        }

        if let Some(default) = &result.default_installation {
            eprintln!("Default: {}", default.get_description());
        }

        for warning in &result.warnings {
            eprintln!("Warning: {}", warning);
        }
    }

    #[test]
    fn test_eim_detection() {
        if let Some(eim_config) = find_eim_configuration() {
            eprintln!(
                "Found EIM config with {} installations",
                eim_config.idf_installed.len()
            );
            for installation in &eim_config.idf_installed {
                eprintln!(
                    "  - {} at {}",
                    installation.version,
                    installation.path.display()
                );
            }
        } else {
            eprintln!(
                "No EIM configuration found (this is normal on non-Windows or systems without EIM)"
            );
        }
    }
}
