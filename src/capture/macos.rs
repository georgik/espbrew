//! macOS camera capture using external tools
//!
//! Uses `imagesnap` or `ffmpeg` for camera capture on macOS.
//! This prototype approach avoids complex Objective-C bindings.

use super::{CameraCapture, CameraInfo, CapturedImage, ImageFormat};
use anyhow::{Context, Result};
use base64::Engine;
use log::{debug, info};

/// macOS camera capture implementation
pub struct MacOSCapture {
    cameras: Vec<CameraInfo>,
}

impl MacOSCapture {
    pub fn new() -> Result<Self> {
        info!("Initializing macOS camera capture");

        // Check for imagesnap
        let has_imagesnap = std::process::Command::new("which")
            .arg("imagesnap")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        // Check for ffmpeg
        let has_ffmpeg = std::process::Command::new("which")
            .arg("ffmpeg")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !has_imagesnap && !has_ffmpeg {
            return Err(anyhow::anyhow!(
                "No camera capture tool found. Install with: brew install imagesnap"
            ));
        }

        // Enumerate cameras using system_profiler
        let cameras = Self::enumerate_cameras()?;

        info!("Found {} cameras", cameras.len());

        Ok(Self { cameras })
    }

    fn enumerate_cameras() -> Result<Vec<CameraInfo>> {
        let output = std::process::Command::new("system_profiler")
            .args(&["SPCameraDataType", "-json"])
            .output()
            .context("Failed to run system_profiler")?;

        if !output.status.success() {
            // Fallback: return a default camera
            return Ok(vec![CameraInfo {
                id: "default".to_string(),
                name: "Default Camera".to_string(),
                vid: None,
                pid: None,
                bus: None,
                device: None,
            }]);
        }

        // Parse JSON output (simplified for prototype)
        let json: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("Failed to parse system_profiler output")?;

        let mut cameras = Vec::new();

        if let Some(items) = json["SPCameraDataType"].as_array() {
            for (idx, item) in items.iter().enumerate() {
                if let Some(name) = item["_name"].as_str() {
                    let vid = item["vendor_id"]
                        .as_str()
                        .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok());
                    let pid = item["product_id"]
                        .as_str()
                        .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok());

                    cameras.push(CameraInfo {
                        id: format!("camera-{}", idx),
                        name: name.to_string(),
                        vid,
                        pid,
                        bus: None,
                        device: None,
                    });
                }
            }
        }

        // Fallback if no cameras found
        if cameras.is_empty() {
            cameras.push(CameraInfo {
                id: "default".to_string(),
                name: "Default Camera".to_string(),
                vid: None,
                pid: None,
                bus: None,
                device: None,
            });
        }

        Ok(cameras)
    }
}

#[async_trait::async_trait]
impl CameraCapture for MacOSCapture {
    async fn list_cameras(&self) -> Result<Vec<CameraInfo>> {
        Ok(self.cameras.clone())
    }

    async fn capture(&self, camera_id: &str) -> Result<CapturedImage> {
        debug!("Capturing from camera: {}", camera_id);

        // Create temp file for capture
        let temp_file =
            std::env::temp_dir().join(format!("espbrew_capture_{}.jpg", uuid::Uuid::new_v4()));

        // Try imagesnap first
        let result = std::process::Command::new("imagesnap")
            .arg("-q")
            .arg(&temp_file)
            .output();

        if let Ok(output) = result {
            if output.status.success() && temp_file.exists() {
                let data = std::fs::read(&temp_file)?;
                let _ = std::fs::remove_file(&temp_file);

                // Get dimensions from image
                let img = image::load_from_memory(&data);
                let (width, height) = if let Ok(i) = img {
                    (i.width(), i.height())
                } else {
                    (0, 0)
                };

                return Ok(CapturedImage {
                    data: base64::engine::general_purpose::STANDARD.encode(&data),
                    width,
                    height,
                    format: ImageFormat::Jpeg,
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        // Fallback to ffmpeg
        let output = std::process::Command::new("ffmpeg")
            .args(&[
                "-f",
                "avfoundation",
                "-framerate",
                "30",
                "-video_size",
                "640x480",
                "-i",
                "0",
                "-vframes",
                "1",
                "-f",
                "image2pipe",
                "-vcodec",
                "mjpeg",
                "-",
            ])
            .output()
            .context("ffmpeg failed. Install with: brew install ffmpeg")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "Capture failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(CapturedImage {
            data: base64::engine::general_purpose::STANDARD.encode(&output.stdout),
            width: 640,
            height: 480,
            format: ImageFormat::Jpeg,
            timestamp: chrono::Utc::now(),
        })
    }

    async fn capture_to_file(&self, camera_id: &str, path: &std::path::Path) -> Result<()> {
        let image = self.capture(camera_id).await?;

        let data = base64::engine::general_purpose::STANDARD
            .decode(&image.data)
            .context("Failed to decode base64 image")?;

        std::fs::write(path, data).context("Failed to write image file")?;

        Ok(())
    }

    async fn is_available(&self, camera_id: &str) -> bool {
        self.cameras.iter().any(|c| c.id == camera_id)
    }
}
