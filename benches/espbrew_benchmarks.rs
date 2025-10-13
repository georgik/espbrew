//! Performance benchmarks for critical espbrew operations
//!
//! These benchmarks help track performance regressions and optimization opportunities
//! in key espbrew functionality.

use criterion::{Criterion, criterion_group, criterion_main};
use espbrew::projects::ProjectRegistry;
use std::hint::black_box;
use tempfile::TempDir;

/// Benchmark project detection performance
fn benchmark_project_detection(c: &mut Criterion) {
    // Create test projects for benchmarking
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let rust_project_path = temp_dir.path().join("rust_project");

    // Create a simple Rust project structure
    std::fs::create_dir_all(rust_project_path.join("src")).unwrap();
    std::fs::write(
        rust_project_path.join("Cargo.toml"),
        r#"[package]
name = "benchmark-project"
version = "0.1.0"
edition = "2021"

[dependencies]
esp-hal = "1.0.0"
"#,
    )
    .unwrap();

    std::fs::write(
        rust_project_path.join("src/main.rs"),
        r#"#![no_std]
#![no_main]

use esp_hal::prelude::*;

#[no_mangle]
pub extern "C" fn main() {
    loop {}
}
"#,
    )
    .unwrap();

    let registry = ProjectRegistry::new();

    c.bench_function("project_detection_rust", |b| {
        b.iter(|| {
            let result = registry.detect_project(black_box(&rust_project_path));
            black_box(result);
        });
    });
}

/// Benchmark registry initialization performance
fn benchmark_registry_creation(c: &mut Criterion) {
    c.bench_function("registry_creation", |b| {
        b.iter(|| {
            let registry = ProjectRegistry::new();
            black_box(registry);
        });
    });
}

/// Benchmark path validation performance
fn benchmark_path_operations(c: &mut Criterion) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_path = temp_dir.path();

    c.bench_function("path_exists_check", |b| {
        b.iter(|| {
            let exists = test_path.exists();
            black_box(exists);
        });
    });

    c.bench_function("path_is_dir_check", |b| {
        b.iter(|| {
            let is_dir = test_path.is_dir();
            black_box(is_dir);
        });
    });
}

/// Benchmark file system operations that espbrew commonly performs
fn benchmark_fs_operations(c: &mut Criterion) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let test_file = temp_dir.path().join("test_file.txt");

    // Create test file
    std::fs::write(&test_file, "test content for benchmarking").unwrap();

    c.bench_function("file_read_small", |b| {
        b.iter(|| {
            let content = std::fs::read_to_string(black_box(&test_file)).unwrap();
            black_box(content);
        });
    });

    c.bench_function("directory_listing", |b| {
        b.iter(|| {
            let entries: Vec<_> = std::fs::read_dir(black_box(temp_dir.path()))
                .unwrap()
                .collect();
            black_box(entries);
        });
    });
}

/// Benchmark multiple project detection scenarios
fn benchmark_multiple_project_detection(c: &mut Criterion) {
    let temp_workspace = TempDir::new().expect("Failed to create temp workspace");
    let workspace_path = temp_workspace.path();

    // Create multiple different project types
    for i in 0..10 {
        let project_path = workspace_path.join(format!("project_{}", i));
        std::fs::create_dir_all(project_path.join("src")).unwrap();
        std::fs::write(
            project_path.join("Cargo.toml"),
            format!(
                r#"[package]
name = "benchmark-project-{}"
version = "0.1.0"
edition = "2021"
"#,
                i
            ),
        )
        .unwrap();
    }

    let registry = ProjectRegistry::new();

    c.bench_function("detect_multiple_projects", |b| {
        b.iter(|| {
            for i in 0..10 {
                let project_path = workspace_path.join(format!("project_{}", i));
                let result = registry.detect_project(black_box(&project_path));
                black_box(result);
            }
        });
    });
}

/// Benchmark configuration parsing (if applicable)
fn benchmark_config_operations(c: &mut Criterion) {
    use espbrew::config::AppConfig;

    c.bench_function("app_config_default", |b| {
        b.iter(|| {
            let config = AppConfig::default();
            black_box(config);
        });
    });
}

criterion_group!(
    benches,
    benchmark_project_detection,
    benchmark_registry_creation,
    benchmark_path_operations,
    benchmark_fs_operations,
    benchmark_multiple_project_detection,
    benchmark_config_operations
);

criterion_main!(benches);
