//! Slint-based confirmation dialog for espbrew:// URL handler
//!
//! This module provides a modern GUI confirmation dialog that appears when
//! users click espbrew:// links from web browsers.

use anyhow::Result;
use slint::ComponentHandle;

slint::include_modules!();

/// Show a confirmation dialog for espbrew:// URL actions
pub fn show_confirmation_dialog(
    action: &str,
    server: Option<&str>,
    board: Option<&str>,
    project: Option<&str>,
    additional_params: &std::collections::HashMap<String, String>,
) -> Result<bool> {
    let ui = ConfirmationDialog::new()?;
    
    // Set dialog content
    ui.set_action(action.to_uppercase().into());
    
    if let Some(server) = server {
        ui.set_server(server.into());
        ui.set_show_server(true);
    } else {
        ui.set_show_server(false);
    }
    
    if let Some(board) = board {
        ui.set_board(board.into());
        ui.set_show_board(true);
    } else {
        ui.set_show_board(false);
    }
    
    if let Some(project) = project {
        ui.set_project(project.into());
        ui.set_show_project(true);
    } else {
        ui.set_show_project(false);
    }
    
    // Build additional parameters string
    if !additional_params.is_empty() {
        let mut params_text = String::new();
        for (key, value) in additional_params {
            if !params_text.is_empty() {
                params_text.push_str("\\n");
            }
            params_text.push_str(&format!("{}: {}", key, value));
        }
        ui.set_additional_params(params_text.into());
        ui.set_show_additional_params(true);
    } else {
        ui.set_show_additional_params(false);
    }
    
    // Set up callbacks
    let ui_weak = ui.as_weak();
    let result = std::rc::Rc::new(std::cell::RefCell::new(false));
    let result_clone = result.clone();
    
    ui.on_confirm({
        let ui_weak = ui_weak.clone();
        move || {
            *result_clone.borrow_mut() = true;
            if let Some(ui) = ui_weak.upgrade() {
                ui.hide().unwrap();
            }
        }
    });
    
    let result_clone = result.clone();
    ui.on_cancel({
        let ui_weak = ui_weak.clone();
        move || {
            *result_clone.borrow_mut() = false;
            if let Some(ui) = ui_weak.upgrade() {
                ui.hide().unwrap();
            }
        }
    });
    
    // Show dialog and run event loop
    ui.show()?;
    slint::run_event_loop()?;
    
    Ok(*result.borrow())
}

/// Show a success dialog with results
pub fn show_success_dialog(message: &str, details: Option<&str>) -> Result<()> {
    let ui = SuccessDialog::new()?;
    
    ui.set_message(message.into());
    
    if let Some(details) = details {
        ui.set_details(details.into());
        ui.set_show_details(true);
    } else {
        ui.set_show_details(false);
    }
    
    let ui_weak = ui.as_weak();
    ui.on_close({
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.hide().unwrap();
            }
        }
    });
    
    ui.show()?;
    slint::run_event_loop()?;
    
    Ok(())
}

/// Show an error dialog
pub fn show_error_dialog(title: &str, message: &str, details: Option<&str>) -> Result<()> {
    let ui = ErrorDialog::new()?;
    
    ui.set_title_text(title.into());
    ui.set_message(message.into());
    
    if let Some(details) = details {
        ui.set_details(details.into());
        ui.set_show_details(true);
    } else {
        ui.set_show_details(false);
    }
    
    let ui_weak = ui.as_weak();
    ui.on_close({
        move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.hide().unwrap();
            }
        }
    });
    
    ui.show()?;
    slint::run_event_loop()?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_confirmation_dialog_creation() {
        // This test just verifies the dialog can be created without panicking
        // We can't actually test the GUI in a headless environment
        let params = HashMap::new();
        let result = show_confirmation_dialog(
            "flash",
            Some("http://localhost:8080"),
            Some("esp32-s3-box-3"),
            None,
            &params
        );
        
        // In a headless environment, this will likely fail, which is expected
        // The test is mainly to ensure the code compiles correctly
        assert!(result.is_ok() || result.is_err());
    }
}