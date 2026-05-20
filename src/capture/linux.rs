//! Linux camera capture using v4l2
//!
//! Uses v4l2 devices or ffmpeg for camera capture on Linux.

use super::{CameraCapture, CameraInfo, CapturedImage, ImageFormat};
use anyhow::{Context, Result};
use base64::Engine;
use log::{debug, info};
use std::path::Path;

/// Linux camera capture implementation
pub struct LinuxCapture {
    cameras: Vec<CameraInfo>,
}

impl LinuxCapture {
    pub fn new() -> Result<Self> {
        info!("Initializing Linux camera capture");

        let cameras = Self::enumerate_cameras()?;

        info!("Found {} cameras", cameras.len());

        Ok(Self { cameras })
    }

    fn enumerate_cameras() -> Result<Vec<CameraInfo>> {
        let mut cameras = Vec::new();

        // Scan /dev/video* devices
        for idx in 0..10 {
            let device_path = format!("/dev/video{}", idx);
            if Path::new(&device_path).exists() {
                // Get USB info from sysfs
                let vid = Self::read_usb_sysfs(&device_path, "idVendor");
                let pid = Self::read_usb_sysfs(&device_path, "idProduct");
                let bus = Self::read_usb_sysfs(&device_path, "busnum")
                    .and_then(|s| u8::from_str_radix(&s, 16).ok());
                let device_num = Self::read_usb_sysfs(&device_path, "devnum")
                    .and_then(|s| u8::from_str_radix(&s, 16).ok());

                // Get device name from v4l2
                let name = Self::get_v4l2_device_name(&device_path)
                    .unwrap_or_else(|_| format!("Camera {}", idx));

                cameras.push(CameraInfo {
                    id: device_path.clone(),
                    name,
                    vid,
                    pid,
                    bus,
                    device: device_num,
                });
            }
        }

        Ok(cameras)
    }

    fn read_usb_sysfs(device_path: &str, file: &str) -> Option<u16> {
        // Extract device number from path
        if let Some(idx) = device_path.strip_prefix("/dev/video") {
            let sysfs_path = format!("/sys/class/video4linux/video{}/device/../..", idx);
            let target_path = std::fs::read_link(&sysfs_path).ok()?;

            let product_path = target_path.join(file);
            if product_path.exists() {
                let content = std::fs::read_to_string(product_path).ok()?;
                u16::from_str_radix(content.trim(), 16).ok()
            } else {
                None
            }
        } else {
            None
        }
    }

    fn get_v4l2_device_name(device_path: &str) -> Result<String> {
        // Try using v4l2-ctl
        let output = std::process::Command::new("v4l2-ctl")
            .args(&["--device", device_path, "--info"])
            .output()?;

        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if line.contains("Card type") {
                    let name = line
                        .split("Card type")
                        .nth(1)
                        .unwrap_or("")
                        .trim()
                        .trim_start_matches(':')
                        .trim()
                        .to_string();
                    if !name.is_empty() {
                        return Ok(name);
                    }
                }
            }
        }

        // Fallback: use device name
        Ok(device_path.to_string())
    }
}

#[async_trait::async_trait]
impl CameraCapture for LinuxCapture {
    async fn list_cameras(&self) -> Result<Vec<CameraInfo>> {
        Ok(self.cameras.clone())
    }

    async fn capture(&self, camera_id: &str) -> Result<CapturedImage> {
        debug!("Capturing from camera: {}", camera_id);

        // Use ffmpeg for capture
        let output = std::process::Command::new("ffmpeg")
            .args(&[
                "-f",
                "v4l2",
                "-framerate",
                "30",
                "-video_size",
                "640x480",
                "-i",
                camera_id,
                "-vframes",
                "1",
                "-f",
                "image2pipe",
                "-vcodec",
                "mjpeg",
                "-",
            ])
            .output()
            .context("ffmpeg failed. Install with: apt install ffmpeg")?;

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
        self.cameras.iter().any(|c| c.id == camera_id) && std::path::Path::new(camera_id).exists()
    }
}
