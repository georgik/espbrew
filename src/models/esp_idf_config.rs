//! ESP-IDF configuration models for environment detection and management

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// EIM (ESP-IDF Installation Manager) configuration structure
/// Based on the format found in C:\Espressif\tools\eim_idf.json
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EimConfig {
    /// List of installed ESP-IDF versions
    #[serde(default)]
    pub idf_installed: Vec<EspIdfInstallation>,

    /// Other EIM configuration fields that might be present
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

/// Represents a single ESP-IDF installation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EspIdfInstallation {
    /// ESP-IDF version (e.g., "v5.3.1", "v5.2.3")
    pub version: String,

    /// Installation path
    pub path: PathBuf,

    /// Python executable path for this installation
    #[serde(default)]
    pub python: Option<PathBuf>,

    /// Tool paths specific to this installation
    #[serde(default)]
    pub tools: Option<HashMap<String, PathBuf>>,

    /// Whether this is the active/default installation
    #[serde(default)]
    pub active: Option<bool>,

    /// Installation state (installed, downloading, etc.)
    #[serde(default)]
    pub state: Option<String>,

    /// Additional installation-specific fields
    #[serde(flatten)]
    pub additional_fields: HashMap<String, serde_json::Value>,
}

/// Detected ESP-IDF installation with runtime information
#[derive(Debug, Clone)]
pub struct DetectedEspIdfInstallation {
    /// Basic installation information
    pub installation: EspIdfInstallation,

    /// Source of detection (EIM, PATH, manual, etc.)
    pub detection_source: EspIdfDetectionSource,

    /// Resolved command name (idf.py or idf.py.exe)
    pub command_name: String,

    /// Environment variables to set for this installation
    pub environment: HashMap<String, String>,

    /// Whether this installation is currently available/working
    pub is_available: bool,
}

/// Source of ESP-IDF installation detection
#[derive(Debug, Clone, PartialEq)]
pub enum EspIdfDetectionSource {
    /// Detected via EIM configuration file
    Eim,
    /// Found in system PATH
    SystemPath,
    /// Found via IDF_PATH environment variable
    EnvironmentVariable,
    /// Found in standard installation locations
    StandardLocation,
    /// Manually configured by user
    Manual,
}

/// ESP-IDF detection result
#[derive(Debug, Clone)]
pub struct EspIdfDetectionResult {
    /// All detected installations
    pub installations: Vec<DetectedEspIdfInstallation>,

    /// Default/preferred installation to use
    pub default_installation: Option<DetectedEspIdfInstallation>,

    /// Detection errors or warnings
    pub warnings: Vec<String>,
}

impl EspIdfInstallation {
    /// Create a new ESP-IDF installation entry
    pub fn new(version: String, path: PathBuf) -> Self {
        Self {
            version,
            path,
            python: None,
            tools: None,
            active: None,
            state: None,
            additional_fields: HashMap::new(),
        }
    }

    /// Get the idf.py command path for this installation
    pub fn get_idf_py_path(&self) -> PathBuf {
        self.path.join("tools").join("idf.py")
    }

    /// Check if this installation appears to be valid
    pub fn is_valid(&self) -> bool {
        self.path.exists()
            && (self.get_idf_py_path().exists()
                || self.get_idf_py_path().with_extension("exe").exists())
    }
}

impl DetectedEspIdfInstallation {
    /// Create a new detected installation
    pub fn new(
        installation: EspIdfInstallation,
        detection_source: EspIdfDetectionSource,
        command_name: String,
    ) -> Self {
        let mut environment = HashMap::new();
        environment.insert(
            "IDF_PATH".to_string(),
            installation.path.to_string_lossy().to_string(),
        );

        Self {
            installation,
            detection_source,
            command_name,
            environment,
            is_available: true,
        }
    }

    /// Get a human-readable description of this installation
    pub fn get_description(&self) -> String {
        format!(
            "ESP-IDF {} at {} (detected via {})",
            self.installation.version,
            self.installation.path.display(),
            match self.detection_source {
                EspIdfDetectionSource::Eim => "EIM",
                EspIdfDetectionSource::SystemPath => "PATH",
                EspIdfDetectionSource::EnvironmentVariable => "IDF_PATH",
                EspIdfDetectionSource::StandardLocation => "standard location",
                EspIdfDetectionSource::Manual => "manual configuration",
            }
        )
    }
}

impl Default for EimConfig {
    fn default() -> Self {
        Self {
            idf_installed: Vec::new(),
            additional_fields: HashMap::new(),
        }
    }
}

impl Default for EspIdfDetectionResult {
    fn default() -> Self {
        Self {
            installations: Vec::new(),
            default_installation: None,
            warnings: Vec::new(),
        }
    }
}
