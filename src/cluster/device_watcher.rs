//! Cross-platform USB device watcher
//!
//! Uses platform-specific APIs for efficient device change notifications:
//! - macOS: IOKit USB notifications
//! - Linux: udev device events
//! - Fallback: Polling (for unsupported platforms)

use anyhow::Result;
#[allow(unused_imports)]
use log::{debug, info, warn};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::mpsc;

/// Device change event
#[derive(Debug, Clone)]
pub enum DeviceEvent {
    /// Device added (port name)
    Added(String),
    /// Device removed (port name)
    Removed(String),
}

/// Create a device watcher channel
///
/// Returns a receiver for device events and a shutdown flag.
/// Spawns a background task that monitors for device changes.
pub fn spawn_device_watcher() -> (mpsc::UnboundedReceiver<DeviceEvent>, Arc<AtomicBool>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let shutdown = Arc::new(AtomicBool::new(false));

    #[cfg(target_os = "linux")]
    {
        use crate::cluster::device_watcher::linux::UdevWatcher;
        let mut watcher = UdevWatcher::new(tx.clone());
        let shutdown_clone = shutdown.clone();
        let shutdown_fallback = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = watcher.run(shutdown_clone).await {
                warn!("Linux device watcher error: {}, falling back to polling", e);
                // Fall back to polling
                drop(watcher);
                crate::cluster::device_watcher::polling::poll_devices(tx, shutdown_fallback).await;
            }
        });
    }

    #[cfg(target_os = "macos")]
    {
        use crate::cluster::device_watcher::macos::IOKitWatcher;
        let mut watcher = IOKitWatcher::new(tx.clone());
        let shutdown_clone = shutdown.clone();
        let shutdown_fallback = shutdown.clone();
        tokio::spawn(async move {
            if let Err(e) = watcher.run(shutdown_clone).await {
                warn!("macOS device watcher error: {}, falling back to polling", e);
                // Fall back to polling
                drop(watcher);
                crate::cluster::device_watcher::polling::poll_devices(tx, shutdown_fallback).await;
            }
        });
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        info!("Using polling for device detection (platform-specific watchers not implemented)");
        tokio::spawn(async move {
            crate::cluster::device_watcher::polling::poll_devices(tx, shutdown).await;
        });
    }

    (rx, shutdown)
}

/// Polling-based device watcher (fallback for unsupported platforms)
pub mod polling {
    use super::*;
    use crate::cluster::backends::usb::detect_usb_devices;
    use std::collections::HashSet;

    pub async fn poll_devices(tx: mpsc::UnboundedSender<DeviceEvent>, shutdown: Arc<AtomicBool>) {
        let mut known_ports = HashSet::new();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));

        loop {
            if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                debug!("Device watcher shutting down (polling)");
                break;
            }

            interval.tick().await;

            match detect_usb_devices() {
                Ok(devices) => {
                    let current_ports: HashSet<_> =
                        devices.iter().map(|d| d.location.clone()).collect();

                    // Detect added devices
                    for port in current_ports.difference(&known_ports) {
                        debug!("Device added: {}", port);
                        let _ = tx.send(DeviceEvent::Added(port.clone()));
                    }

                    // Detect removed devices
                    for port in known_ports.difference(&current_ports) {
                        debug!("Device removed: {}", port);
                        let _ = tx.send(DeviceEvent::Removed(port.clone()));
                    }

                    known_ports = current_ports;
                }
                Err(e) => {
                    debug!("Device scan failed: {}", e);
                }
            }
        }
    }
}

#[cfg(all(target_os = "linux", feature = "device-monitor-linux"))]
pub mod linux {
    use super::*;
    use std::os::unix::io::AsRawFd;

    /// udev-based device watcher for Linux
    pub struct UdevWatcher {
        tx: mpsc::UnboundedSender<DeviceEvent>,
        udev: Option<udev::Udev>,
        monitor: Option<udev::Monitor>,
    }

    impl UdevWatcher {
        pub fn new(tx: mpsc::UnboundedSender<DeviceEvent>) -> Self {
            let udev = udev::Udev::new().expect("Failed to create udev context");
            let monitor = udev
                .monitor()
                .match_subsystem_devtype("tty", None)
                .and_then(|m| m.listen())
                .ok();

            if monitor.is_some() {
                info!("Using udev for USB device notifications");
            } else {
                warn!("Failed to create udev monitor, will fall back to polling");
            }

            Self {
                tx,
                udev: Some(udev),
                monitor,
            }
        }

        pub async fn run(&mut self, shutdown: Arc<AtomicBool>) -> Result<()> {
            let monitor = self
                .monitor
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("udev monitor not available"))?;

            // Set monitor fd to non-blocking
            let fd = monitor.as_raw_fd();

            loop {
                if shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                    debug!("Device watcher shutting down (udev)");
                    break;
                }

                // Small timeout to allow checking shutdown
                let mut poll_fds = [libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                }];

                let result = unsafe {
                    libc::poll(
                        poll_fds.as_mut_ptr(),
                        1,
                        500, // 500ms timeout
                    )
                };

                if result > 0 && poll_fds[0].revents & libc::POLLIN != 0 {
                    if let Some(event) = monitor.receive_event() {
                        self.handle_udev_event(&event);
                    }
                }

                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }

            Ok(())
        }

        fn handle_udev_event(&self, event: &udev::Event) {
            let devnode = event.devnode();
            let action = event.action();

            if let Some(node) = devnode {
                // Filter for USB serial devices
                if node.contains("/dev/ttyUSB") || node.contains("/dev/ttyACM") {
                    match action {
                        udev::Action::Add => {
                            debug!("udev: Device added {}", node);
                            let _ = self.tx.send(DeviceEvent::Added(node.to_string()));
                        }
                        udev::Action::Remove => {
                            debug!("udev: Device removed {}", node);
                            let _ = self.tx.send(DeviceEvent::Removed(node.to_string()));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

// On Linux without device-monitor-linux feature, use polling
#[cfg(all(target_os = "linux", not(feature = "device-monitor-linux")))]
pub mod linux {
    use super::*;

    /// Polling-based watcher for Linux (when udev feature not enabled)
    pub struct UdevWatcher {
        _tx: mpsc::UnboundedSender<DeviceEvent>,
    }

    impl UdevWatcher {
        pub fn new(tx: mpsc::UnboundedSender<DeviceEvent>) -> Self {
            warn!("udev feature not enabled, using polling for device detection");
            Self { _tx: tx }
        }

        pub async fn run(&mut self, _shutdown: Arc<AtomicBool>) -> Result<()> {
            // Fall back to polling immediately
            Err(anyhow::anyhow!("udev feature not enabled"))
        }
    }
}

#[cfg(all(target_os = "macos", feature = "device-monitor-macos"))]
pub mod macos {
    use super::*;

    /// IOKit-based device watcher for macOS
    pub struct IOKitWatcher {
        tx: mpsc::UnboundedSender<DeviceEvent>,
    }

    impl IOKitWatcher {
        pub fn new(tx: mpsc::UnboundedSender<DeviceEvent>) -> Self {
            info!("IOKit device watcher enabled (experimental)");
            Self { tx }
        }

        pub async fn run(&mut self, _shutdown: Arc<AtomicBool>) -> Result<()> {
            // TODO: Implement full IOKit notifications
            // For now, fall back to polling
            warn!("IOKit watcher not fully implemented, using polling");
            Err(anyhow::anyhow!("IOKit watcher not implemented"))
        }
    }
}

// On macOS without device-monitor-macos feature, use polling
#[cfg(all(target_os = "macos", not(feature = "device-monitor-macos")))]
pub mod macos {
    use super::*;

    /// Polling-based watcher for macOS (when IOKit feature not enabled)
    pub struct IOKitWatcher {
        _tx: mpsc::UnboundedSender<DeviceEvent>,
    }

    impl IOKitWatcher {
        pub fn new(tx: mpsc::UnboundedSender<DeviceEvent>) -> Self {
            warn!("IOKit feature not enabled, using polling for device detection");
            Self { _tx: tx }
        }

        pub async fn run(&mut self, _shutdown: Arc<AtomicBool>) -> Result<()> {
            // Fall back to polling immediately
            Err(anyhow::anyhow!("IOKit feature not enabled"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_event_creation() {
        let added = DeviceEvent::Added("/dev/ttyUSB0".to_string());
        let removed = DeviceEvent::Removed("/dev/ttyUSB0".to_string());

        match added {
            DeviceEvent::Added(port) => assert_eq!(port, "/dev/ttyUSB0"),
            _ => panic!("Expected Added event"),
        }

        match removed {
            DeviceEvent::Removed(port) => assert_eq!(port, "/dev/ttyUSB0"),
            _ => panic!("Expected Removed event"),
        }
    }
}
