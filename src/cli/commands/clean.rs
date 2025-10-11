//! Clean command implementation
//! 
//! This command runs cargo clean for Rust projects to remove build artifacts.

use anyhow::Result;
use std::path::Path;
use std::process::Command;

use crate::cli::args::Cli;
use crate::projects::ProjectRegistry;

/// Execute clean command - runs cargo clean for Rust projects
pub async fn execute_clean_command(cli: &Cli) -> Result<()> {
    let current_dir = std::env::current_dir()?;
    let project_dir = cli.project_dir.as_ref().unwrap_or(&current_dir);
    
    println!("🧹 ESPBrew Clean Command");
    println!("📁 Project directory: {}", project_dir.display());

    // Detect project type
    let project_registry = ProjectRegistry::new();
    
    if let Some(handler) = project_registry.detect_project_boxed(project_dir) {
        println!("🔍 Detected project type: {:?}", handler.project_type());
        
        // Get boards for this project
        match handler.discover_boards(project_dir) {
            Ok(boards) => {
                if boards.is_empty() {
                    println!("⚠️ No boards/targets found in this project.");
                    return Ok(());
                }
                
                println!("🎯 Found {} board(s)/target(s)", boards.len());
                
                // For Rust projects, run cargo clean
                match handler.project_type() {
                    crate::models::project::ProjectType::RustNoStd => {
                        clean_rust_project(project_dir).await?;
                    },
                    _ => {
                        println!("ℹ️  Clean command currently only supports Rust no_std projects");
                        println!("   For other project types, please run the appropriate clean command manually:");
                        match handler.project_type() {
                            crate::models::project::ProjectType::EspIdf => {
                                println!("   - ESP-IDF: idf.py clean");
                            },
                            crate::models::project::ProjectType::Arduino => {
                                println!("   - Arduino: Remove build/ directory");
                            },
                            crate::models::project::ProjectType::PlatformIO => {
                                println!("   - PlatformIO: pio run -t clean");
                            },
                            _ => {
                                println!("   - Check your project's documentation for clean commands");
                            }
                        }
                    }
                }
            },
            Err(e) => {
                println!("❌ Error discovering boards: {}", e);
                return Err(e);
            }
        }
    } else {
        println!("⚠️ Unknown project type in {}", project_dir.display());
        println!("   Trying generic cargo clean...");
        clean_rust_project(project_dir).await?;
    }

    Ok(())
}

/// Clean a Rust project using cargo clean
async fn clean_rust_project(project_dir: &Path) -> Result<()> {
    println!("🦀 Running cargo clean...");
    
    let mut cmd = Command::new("cargo");
    cmd.current_dir(project_dir).arg("clean");
    
    let output = cmd.output()?;
    
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        
        if !stdout.is_empty() {
            println!("{}", stdout.trim());
        }
        if !stderr.is_empty() {
            println!("{}", stderr.trim());
        }
        
        println!("✅ Cargo clean completed successfully");
        
        // Show what was cleaned
        let target_dir = project_dir.join("target");
        if !target_dir.exists() {
            println!("📂 Removed target/ directory and all build artifacts");
        } else {
            println!("🧹 Cleaned build artifacts in target/ directory");
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("❌ Cargo clean failed: {}", stderr.trim());
        return Err(anyhow::anyhow!("Cargo clean failed"));
    }
    
    Ok(())
}