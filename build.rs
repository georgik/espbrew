//! Build script for ESPBrew
//!
//! ## WASM Dashboard Build
//!
//! This script builds the cluster dashboard WASM module using `trunk`.
//!
//! ### Critical: Workspace Cargo Deadlock
//!
//! The `dashboard-wasm` crate is a workspace member. When trunk builds WASM,
//! it internally runs `cargo build` for the wasm32 target. If trunk's inner
//! cargo uses the same target directory as the outer cargo build, they will
//! deadlock waiting for the same lock files.
//!
//! **Solution**: Set `CARGO_TARGET_DIR=.trunk-target` so trunk's cargo uses
//! an isolated target directory, preventing the deadlock.
//!
//! ### Skipping WASM Build
//!
//! For faster iteration during development, set `ESPBREW_SKIP_WASM=1` to skip
//! the trunk build. Build WASM separately:
//!
//! ```bash
//! cd dashboard-wasm
//! trunk build --release --dist ../target/dashboard-wasm
//! ```

use std::process::Command;

fn main() {
    // Only build WASM dashboard if we're not building for docs or wasm32 target itself
    let is_wasm_target = std::env::var("CARGO_CFG_TARGET_ARCH")
        .map(|v| v == "wasm32")
        .unwrap_or(false);
    // Skip WASM build if ESPBREW_SKIP_WASM is set
    let skip_wasm = std::env::var("ESPBREW_SKIP_WASM").is_ok();

    if !is_wasm_target && !skip_wasm {
        println!("cargo:rerun-if-changed=dashboard-wasm/src");
        println!("cargo:rerun-if-changed=dashboard-wasm/index.html");
        println!("cargo:rerun-if-changed=dashboard-wasm/Cargo.toml");

        // Build WASM dashboard with trunk
        //
        // CRITICAL: Must use isolated CARGO_TARGET_DIR to avoid deadlock.
        // trunk internally runs `cargo build` for wasm32. Since dashboard-wasm
        // is a workspace member, sharing the target dir causes cargo lock
        // contention between outer (espbrew) and inner (trunk) cargo processes.
        let output = Command::new("trunk")
            .args([
                "build",
                "--release",
                "--dist",
                "../target/dashboard-wasm",
                "--public-url",
                "/",
            ])
            .env("CARGO_TARGET_DIR", ".trunk-target") // Isolated target prevents deadlock
            .current_dir("dashboard-wasm")
            .output()
            .expect("Failed to execute trunk. Is it installed? (cargo install trunk)");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Trunk build failed:\n{}", stderr);
            std::process::exit(1);
        }
    } else if skip_wasm {
        println!("cargo:warning=Skipping WASM dashboard build (ESPBREW_SKIP_WASM set)");
        println!(
            "cargo:warning=Run manually: cd dashboard-wasm && trunk build --release --dist ../target/dashboard-wasm"
        );
    }
}
