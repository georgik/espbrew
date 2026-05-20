//! Test camera capture functionality
//!
//! Run with: cargo run --example capture_test

#[cfg(feature = "capture")]
fn main() -> anyhow::Result<()> {
    use espbrew::capture::{CameraInfo, CaptureService};

    println!("ESPBrew Camera Capture Test");
    println!("================================");

    // Create capture service
    let service = CaptureService::new()?;

    // List available cameras
    println!("\nScanning for cameras...");
    let cameras =
        tokio::runtime::Runtime::new()?.block_on(async { service.list_cameras().await })?;

    if cameras.is_empty() {
        println!("No cameras found!");
        return Ok(());
    }

    println!("Found {} camera(s):", cameras.len());
    for camera in &cameras {
        println!("  - {} ({})", camera.name, camera.id);
    }

    // Capture from first camera
    let camera_id = &cameras[0].id;
    println!("\nCapturing from {}...", camera_id);

    let rt = tokio::runtime::Runtime::new()?;
    let image = rt.block_on(async { service.capture(camera_id).await })?;

    println!(
        "Captured: {}x{}, {} bytes",
        image.width,
        image.height,
        image.data.len()
    );

    // Save to file
    let output_path = "/tmp/espbrew_capture.jpg";
    rt.block_on(async {
        service
            .capture_to_file(camera_id, std::path::Path::new(output_path))
            .await
    })?;

    println!("Saved to: {}", output_path);

    Ok(())
}

#[cfg(not(feature = "capture"))]
fn main() {
    println!(
        "Capture feature not enabled. Run with: cargo run --example capture_test --features capture"
    );
}
