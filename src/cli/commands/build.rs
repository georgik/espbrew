//! Build command implementation

use crate::cli::args::Cli;
use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub async fn execute_build_command(cli: &Cli) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let project_dir = cli.project_dir.as_ref().unwrap_or(&current_dir);

    if !project_dir.exists() {
        return Err(anyhow::anyhow!(
            "Project directory does not exist: {:?}",
            project_dir
        ));
    }

    log::info!("🔨 ESPBrew Build Command");
    log::info!("📁 Project directory: {}", project_dir.display());

    // Simple approach: just run cargo build --release
    log::info!("🚀 Executing: cargo build --release");

    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_dir)
        .args(["build", "--release"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        log::error!("Failed to start cargo build: {}", e);
        anyhow::anyhow!("Failed to start cargo build: {}", e)
    })?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Handle stdout
    tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut buffer = String::new();
        while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
            let line = buffer.trim();
            log::info!("[cargo] {}", line);
            buffer.clear();
        }
    });

    // Handle stderr
    tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut buffer = String::new();
        while reader.read_line(&mut buffer).await.unwrap_or(0) > 0 {
            let line = buffer.trim();
            log::warn!("[cargo] {}", line);
            buffer.clear();
        }
    });

    let status = child.wait().await.map_err(|e| {
        log::error!("Failed to wait for cargo build: {}", e);
        anyhow::anyhow!("Failed to wait for cargo build: {}", e)
    })?;

    if status.success() {
        log::info!("✅ Build completed successfully");
        Ok(())
    } else {
        log::error!("❌ Build failed with exit code: {:?}", status.code());
        Err(anyhow::anyhow!("Build failed"))
    }
}
