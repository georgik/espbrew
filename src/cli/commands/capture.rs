//! Camera capture command for HIL testing

use anyhow::Result;
use std::path::PathBuf;

#[cfg(feature = "capture")]
use crate::capture::{CaptureService, ImageComparator};
#[cfg(feature = "capture")]
use base64::Engine;

/// Execute capture command
#[cfg(feature = "capture")]
pub async fn execute_capture_command(
    list: bool,
    device: Option<String>,
    output: Option<PathBuf>,
    compare: Option<PathBuf>,
    threshold: f32,
) -> Result<()> {
    let service = CaptureService::new()?;

    // List cameras
    if list {
        println!("Available cameras:");
        let cameras = service.list_cameras().await?;
        if cameras.is_empty() {
            println!("  No cameras found");
            return Ok(());
        }
        for camera in cameras {
            println!(
                "  {} - {} (VID: {:?}, PID: {:?})",
                camera.id, camera.name, camera.vid, camera.pid
            );
        }
        return Ok(());
    }

    // Capture from device
    let camera_id = device.unwrap_or_else(|| {
        // Try to get first camera
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let cameras = service.list_cameras().await.unwrap();
            if cameras.is_empty() {
                panic!("No cameras available");
            }
            cameras[0].id.clone()
        })
    });

    println!("Capturing from camera: {}", camera_id);
    let image = service.capture(&camera_id).await?;
    println!(
        "Captured: {}x{}, {} bytes",
        image.width,
        image.height,
        image.data.len()
    );

    // Save to file if specified
    if let Some(output_path) = output {
        service.capture_to_file(&camera_id, &output_path).await?;
        println!("Saved to: {}", output_path.display());
    }

    // Compare with reference if specified
    if let Some(ref_path) = compare {
        let ref_data = std::fs::read(&ref_path)?;
        let ref_image = crate::capture::CapturedImage {
            data: base64::engine::general_purpose::STANDARD.encode(&ref_data),
            width: 0,
            height: 0,
            format: crate::capture::ImageFormat::Jpeg,
            timestamp: chrono::Utc::now(),
        };

        let result = ImageComparator::compare_pixel(&image, &ref_image, threshold)?;
        println!(
            "Comparison: {:.2}% similar - {}",
            result.similarity * 100.0,
            if result.matches { "MATCH" } else { "NO MATCH" }
        );
    }

    Ok(())
}

/// Execute capture command when capture feature is not available
#[cfg(not(feature = "capture"))]
pub async fn execute_capture_command(
    _list: bool,
    _device: Option<String>,
    _output: Option<PathBuf>,
    _compare: Option<PathBuf>,
    _threshold: f32,
) -> Result<()> {
    Err(anyhow::anyhow!(
        "Camera capture not supported. Build with --features capture-macos or capture-linux"
    ))
}
