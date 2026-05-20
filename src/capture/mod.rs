//! Camera capture service for HIL testing
//!
//! This module provides image capture from cameras connected via USB.
//! Supports both macOS (AVFoundation) and Linux (v4l2) platforms.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[cfg(feature = "capture")]
use base64::Engine;

/// Camera device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraInfo {
    /// Unique identifier for the camera
    pub id: String,
    /// Camera name/manufacturer
    pub name: String,
    /// USB Vendor ID
    pub vid: Option<u16>,
    /// USB Product ID
    pub pid: Option<u16>,
    /// USB bus number (Linux only)
    pub bus: Option<u8>,
    /// USB device number (Linux only)
    pub device: Option<u8>,
}

/// Captured image data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedImage {
    /// Image data as base64 string
    pub data: String,
    /// Image width in pixels
    pub width: u32,
    /// Image height in pixels
    pub height: u32,
    /// Image format (png, jpeg)
    pub format: ImageFormat,
    /// Timestamp of capture
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Image format
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
}

/// Capture result with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    /// The captured image
    pub image: CapturedImage,
    /// Camera used for capture
    pub camera_id: String,
    /// Whether the capture was successful
    pub success: bool,
    /// Error message if capture failed
    pub error: Option<String>,
}

/// Comparison result between two images
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    /// Similarity score (0.0 - 1.0)
    pub similarity: f32,
    /// Whether images match within threshold
    pub matches: bool,
    /// Differential image (base64 encoded)
    pub diff_image: Option<String>,
}

/// Camera capture trait - platform-specific implementation
#[async_trait::async_trait]
pub trait CameraCapture: Send + Sync {
    /// List available cameras
    async fn list_cameras(&self) -> Result<Vec<CameraInfo>>;

    /// Capture a single frame from the specified camera
    async fn capture(&self, camera_id: &str) -> Result<CapturedImage>;

    /// Capture and save to file
    async fn capture_to_file(&self, camera_id: &str, path: &Path) -> Result<()>;

    /// Check if camera is available
    async fn is_available(&self, camera_id: &str) -> bool;
}

/// Capture service - main entry point
pub struct CaptureService {
    capture: Box<dyn CameraCapture>,
}

impl CaptureService {
    /// Create a new capture service with platform-specific implementation
    pub fn new() -> Result<Self> {
        #[cfg(all(feature = "capture", target_os = "macos"))]
        {
            Ok(Self {
                capture: Box::new(macos::MacOSCapture::new()?),
            })
        }

        #[cfg(all(feature = "capture", target_os = "linux"))]
        {
            Ok(Self {
                capture: Box::new(linux::LinuxCapture::new()?),
            })
        }

        #[cfg(not(any(
            all(feature = "capture", target_os = "macos"),
            all(feature = "capture", target_os = "linux")
        )))]
        {
            Err(anyhow::anyhow!(
                "Camera capture not supported on this platform. Enable 'capture-macos' or 'capture-linux' feature."
            ))
        }
    }

    /// List all available cameras
    pub async fn list_cameras(&self) -> Result<Vec<CameraInfo>> {
        self.capture.list_cameras().await
    }

    /// Capture an image from the specified camera
    pub async fn capture(&self, camera_id: &str) -> Result<CapturedImage> {
        self.capture.capture(camera_id).await
    }

    /// Capture and save to file
    pub async fn capture_to_file(&self, camera_id: &str, path: &Path) -> Result<()> {
        self.capture.capture_to_file(camera_id, path).await
    }

    /// Check if a camera is available
    pub async fn is_available(&self, camera_id: &str) -> bool {
        self.capture.is_available(camera_id).await
    }
}

/// Image comparison utilities
pub struct ImageComparator;

impl ImageComparator {
    /// Compare two images using SSIM (Structural Similarity Index)
    pub fn compare_ssim(
        image1: &CapturedImage,
        image2: &CapturedImage,
        threshold: f32,
    ) -> Result<ComparisonResult> {
        // Decode base64 images
        let data1 = base64_decode(&image1.data)?;
        let data2 = base64_decode(&image2.data)?;

        // Load images
        let img1 = image::load_from_memory(&data1).context("Failed to load first image")?;
        let img2 = image::load_from_memory(&data2).context("Failed to load second image")?;

        // Convert to grayscale for comparison
        let gray1 = self::to_grayscale(&img1);
        let gray2 = self::to_grayscale(&img2);

        // Calculate SSIM
        let similarity = calculate_ssim(&gray1, &gray2)?;

        let matches = similarity >= threshold;

        Ok(ComparisonResult {
            similarity,
            matches,
            diff_image: None, // TODO: Generate differential image
        })
    }

    /// Simple pixel-by-pixel comparison
    pub fn compare_pixel(
        image1: &CapturedImage,
        image2: &CapturedImage,
        threshold: f32,
    ) -> Result<ComparisonResult> {
        let data1 = base64_decode(&image1.data)?;
        let data2 = base64_decode(&image2.data)?;

        let img1 = image::load_from_memory(&data1)?;
        let img2 = image::load_from_memory(&data2)?;

        if (img1.width(), img1.height()) != (img2.width(), img2.height()) {
            return Ok(ComparisonResult {
                similarity: 0.0,
                matches: false,
                diff_image: None,
            });
        }

        let mut different_pixels = 0;
        let total_pixels = img1.width() * img2.height();

        // Convert to RGBA8 for easier comparison
        let rgba1 = img1.to_rgba8();
        let rgba2 = img2.to_rgba8();

        // Compare pixel buffers directly
        for (p1, p2) in rgba1.chunks(4).zip(rgba2.chunks(4)) {
            if p1 != p2 {
                different_pixels += 1;
            }
        }

        let similarity = 1.0 - (different_pixels as f32 / total_pixels as f32);
        let matches = similarity >= threshold;

        Ok(ComparisonResult {
            similarity,
            matches,
            diff_image: None,
        })
    }
}

fn base64_decode(data: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| anyhow::anyhow!("Base64 decode error: {}", e))
}

fn to_grayscale(img: &image::DynamicImage) -> Vec<f32> {
    let gray = img.to_luma8();
    // Get the raw pixel data
    let raw = gray.as_raw();
    raw.iter().map(|&p| p as f32 / 255.0).collect()
}

fn calculate_ssim(img1: &[f32], img2: &[f32]) -> Result<f32> {
    if img1.len() != img2.len() {
        return Ok(0.0);
    }

    // Simplified SSIM calculation
    let mean1: f32 = img1.iter().sum::<f32>() / img1.len() as f32;
    let mean2: f32 = img2.iter().sum::<f32>() / img2.len() as f32;

    let var1: f32 = img1.iter().map(|x| (x - mean1).powi(2)).sum::<f32>() / img1.len() as f32;
    let var2: f32 = img2.iter().map(|x| (x - mean2).powi(2)).sum::<f32>() / img2.len() as f32;

    let cov: f32 = img1
        .iter()
        .zip(img2.iter())
        .map(|(x, y)| (x - mean1) * (y - mean2))
        .sum::<f32>()
        / img1.len() as f32;

    let c1 = 0.01_f32.powi(2);
    let c2 = 0.03_f32.powi(2);

    let numerator = (2.0 * mean1 * mean2 + c1) * (2.0 * cov + c2);
    let denominator = (mean1.powi(2) + mean2.powi(2) + c1) * (var1 + var2 + c2);

    Ok(numerator / denominator)
}

// Platform-specific implementations
#[cfg(all(feature = "capture", target_os = "macos"))]
pub mod macos;

#[cfg(all(feature = "capture", target_os = "linux"))]
pub mod linux;
