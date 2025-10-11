//! CI validation tests for println! usage
//!
//! These tests ensure that no println! statements are accidentally introduced
//! into the codebase where they shouldn't be.

use std::fs;
use std::path::Path;

/// Test that TUI components don't contain println! or eprintln! statements
#[test]
fn test_no_println_in_tui_components() {
    let tui_files = [
        "src/cli/tui/event_loop.rs",
        "src/cli/tui/app.rs",
        "src/cli/tui/main_app.rs",
        "src/cli/tui/ui.rs",
    ];

    for file_path in &tui_files {
        if Path::new(file_path).exists() {
            let content = fs::read_to_string(file_path)
                .unwrap_or_else(|_| panic!("Failed to read {}", file_path));

            // Check for println! macros
            let println_lines: Vec<_> = content
                .lines()
                .enumerate()
                .filter(|(_, line)| {
                    line.contains("println!") && !line.trim_start().starts_with("//")
                })
                .collect();

            if !println_lines.is_empty() {
                panic!(
                    "Found println! statements in TUI file {}: lines {:?}",
                    file_path,
                    println_lines.iter().map(|(n, _)| n + 1).collect::<Vec<_>>()
                );
            }

            // Check for eprintln! macros (more critical for TUI)
            let eprintln_lines: Vec<_> = content
                .lines()
                .enumerate()
                .filter(|(_, line)| {
                    line.contains("eprintln!") && !line.trim_start().starts_with("//")
                })
                .collect();

            if !eprintln_lines.is_empty() {
                panic!(
                    "Found eprintln! statements in TUI file {}: lines {:?}\\n\\
                     TUI components must not write directly to stderr as it breaks the interface.\\n\\
                     Use AppEvent::Error, AppEvent::Warning, or TuiLogger instead.",
                    file_path,
                    eprintln_lines
                        .iter()
                        .map(|(n, _)| n + 1)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// Test that certain utility modules use proper logging instead of println!
#[test]
fn test_no_println_in_utility_modules() {
    let utility_files = [
        "src/utils/espflash_utils.rs",
        "src/espflash_local/flash_data.rs",
        "src/remote/discovery.rs",
    ];

    for file_path in &utility_files {
        if Path::new(file_path).exists() {
            let content = fs::read_to_string(file_path)
                .unwrap_or_else(|_| panic!("Failed to read {}", file_path));

            // Count println! statements (some might be acceptable for user feedback)
            let println_count = content
                .lines()
                .filter(|line| line.contains("println!") && !line.trim_start().starts_with("//"))
                .count();

            // Utilities should have minimal or no direct user output
            // This is a warning rather than an error for now
            if println_count > 10 {
                eprintln!(
                    "Warning: {} contains {} println! statements. Consider using log macros instead.",
                    file_path, println_count
                );
            }
        }
    }
}

/// Count total println! usage across the codebase and ensure it doesn't increase
#[test]
fn test_println_usage_not_increasing() {
    let src_dir = Path::new("src");
    if !src_dir.exists() {
        return; // Skip if not in right directory
    }

    let mut total_count = 0;
    let mut file_counts = Vec::new();

    fn count_println_in_dir(
        dir: &Path,
        total_count: &mut usize,
        file_counts: &mut Vec<(String, usize)>,
    ) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    count_println_in_dir(&path, total_count, file_counts);
                } else if let Some(ext) = path.extension() {
                    if ext == "rs" {
                        if let Ok(content) = fs::read_to_string(&path) {
                            let count = content
                                .lines()
                                .filter(|line| {
                                    (line.contains("println!") || line.contains("eprintln!"))
                                        && !line.trim_start().starts_with("//")
                                })
                                .count();

                            if count > 0 {
                                file_counts.push((path.to_string_lossy().to_string(), count));
                                *total_count += count;
                            }
                        }
                    }
                }
            }
        }
    }

    count_println_in_dir(src_dir, &mut total_count, &mut file_counts);

    // Sort by count for easier analysis
    file_counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    println!("Current println!/eprintln! usage summary:");
    println!("Total statements: {}", total_count);
    println!("Top 10 files:");
    for (file, count) in file_counts.iter().take(10) {
        println!("  {}: {} statements", file, count);
    }

    // We know the current state is 459, so let's ensure it doesn't increase significantly
    // As we implement the migration plan, this number should decrease
    const CURRENT_BASELINE: usize = 459;
    const ALLOWED_INCREASE: usize = 10; // Small buffer for legitimate additions

    if total_count > CURRENT_BASELINE + ALLOWED_INCREASE {
        panic!(
            "println!/eprintln! usage has increased from baseline {} to {}. \\n\\
             This suggests new println! statements have been added. \\n\\
             Please use appropriate logging mechanisms instead. \\n\\
             See docs/logging_architecture.md for guidance.",
            CURRENT_BASELINE, total_count
        );
    }

    // If count has decreased significantly, update the test
    if total_count < CURRENT_BASELINE - 50 {
        println!(
            "🎉 Great! println! usage has decreased from {} to {}. \\n\\
             Consider updating CURRENT_BASELINE in this test to reflect the improvement.",
            CURRENT_BASELINE, total_count
        );
    }
}

/// Test that specific patterns of good logging are used
#[test]
fn test_proper_logging_patterns() {
    let check_files = [
        "src/utils/logging.rs",
        "src/main.rs",
        "src/bin/espbrew-server.rs",
    ];

    for file_path in &check_files {
        if Path::new(file_path).exists() {
            let content = fs::read_to_string(file_path)
                .unwrap_or_else(|_| panic!("Failed to read {}", file_path));

            // Should have proper logging imports
            let has_log_import = content.contains("use log::") || content.contains("log::");
            let has_logging_init = content.contains("init_")
                && (content.contains("logging") || content.contains("log"));

            if file_path.contains("main.rs") || file_path.contains("server.rs") {
                assert!(
                    has_logging_init,
                    "File {} should initialize logging but doesn't appear to",
                    file_path
                );
            }

            if file_path.contains("logging.rs") {
                assert!(
                    has_log_import,
                    "Logging utility file {} should import log crate",
                    file_path
                );
            }
        }
    }
}

/// Test that AppEvent has the required logging variants
#[test]
fn test_app_event_has_logging_variants() {
    use espbrew::models::AppEvent;

    // These should compile without errors
    let _error = AppEvent::Error("test".to_string());
    let _warning = AppEvent::Warning("test".to_string());
    let _info = AppEvent::Info("test".to_string());

    // Pattern matching should work
    match AppEvent::Error("test".to_string()) {
        AppEvent::Error(_) => {}
        _ => panic!("AppEvent::Error variant not found"),
    }
}

/// Test that CLI args support logging configuration
#[test]
fn test_cli_logging_arguments() {
    use clap::Parser;
    use espbrew::cli::args::Cli;

    // Test verbose flag parsing
    let args = ["espbrew", "-v"];
    let cli = Cli::try_parse_from(args).expect("Should parse verbose flag");
    assert_eq!(cli.verbose, 1);

    // Test quiet flag parsing
    let args = ["espbrew", "--quiet"];
    let cli = Cli::try_parse_from(args).expect("Should parse quiet flag");
    assert!(cli.quiet);

    // Test multiple verbose flags
    let args = ["espbrew", "-vv"];
    let cli = Cli::try_parse_from(args).expect("Should parse multiple verbose flags");
    assert_eq!(cli.verbose, 2);
}
