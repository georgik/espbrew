//! Device reservation system for cluster operations
//!
//! Manages device locking, reservations, and pool management

use crate::cluster::messaging::{DeviceId, JobId, NodeId};
use anyhow::Result;
use chrono::{DateTime, Utc};
use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Device reservation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceReservation {
    /// Device being reserved
    pub device_id: DeviceId,
    /// Node holding the reservation
    pub node_id: NodeId,
    /// Job ID using this reservation
    pub job_id: JobId,
    /// When the reservation was created
    pub reserved_at: DateTime<Utc>,
    /// When the reservation expires
    pub expires_at: DateTime<Utc>,
    /// Current state of the reservation
    pub state: ReservationState,
}

/// Reservation state
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReservationState {
    /// Reservation requested, pending confirmation
    Pending,
    /// Active reservation
    Active,
    /// Released back to pool
    Released,
    /// Expired
    Expired,
}

/// Device pool manager
#[derive(Debug)]
pub struct DevicePool {
    /// Device reservations by device ID
    reservations: HashMap<DeviceId, DeviceReservation>,
    /// Default reservation timeout
    default_timeout: Duration,
}

impl DevicePool {
    /// Create new device pool
    pub fn new() -> Self {
        Self {
            reservations: HashMap::new(),
            default_timeout: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Create device pool with custom timeout
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            reservations: HashMap::new(),
            default_timeout: timeout,
        }
    }

    /// Reserve a device for a job
    pub fn reserve(
        &mut self,
        device_id: DeviceId,
        node_id: NodeId,
        job_id: JobId,
    ) -> Result<DeviceReservation> {
        if let Some(existing) = self.reservations.get(&device_id) {
            if existing.state == ReservationState::Active && !Self::is_expired(existing) {
                return Err(anyhow::anyhow!(
                    "Device {} already reserved by {} for job {}",
                    device_id,
                    existing.node_id,
                    existing.job_id
                ));
            }
        }

        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.default_timeout).unwrap();

        let reservation = DeviceReservation {
            device_id: device_id.clone(),
            node_id,
            job_id: job_id.clone(),
            reserved_at: now,
            expires_at,
            state: ReservationState::Active,
        };

        self.reservations.insert(device_id.clone(), reservation.clone());
        debug!(
            "Reserved device {} for job {} on node {}",
            device_id, job_id, reservation.node_id
        );

        Ok(reservation)
    }

    /// Release a device reservation
    pub fn release(&mut self, device_id: &str) -> Result<()> {
        if let Some(_reservation) = self.reservations.remove(device_id) {
            debug!("Released reservation for device {}", device_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("No reservation found for device {}", device_id))
        }
    }

    /// Extend a reservation timeout
    pub fn extend(&mut self, device_id: &str, additional: Duration) -> Result<()> {
        if let Some(reservation) = self.reservations.get_mut(device_id) {
            reservation.expires_at = reservation.expires_at
                + chrono::Duration::from_std(additional).unwrap();
            debug!(
                "Extended reservation for device {} until {}",
                device_id, reservation.expires_at
            );
            Ok(())
        } else {
            Err(anyhow::anyhow!("No reservation found for device {}", device_id))
        }
    }

    /// Check if a device is available
    pub fn is_available(&self, device_id: &str) -> bool {
        if let Some(reservation) = self.reservations.get(device_id) {
            reservation.state != ReservationState::Active || Self::is_expired(reservation)
        } else {
            true
        }
    }

    /// Get reservation for a device
    pub fn get_reservation(&self, device_id: &str) -> Option<&DeviceReservation> {
        self.reservations.get(device_id)
    }

    /// Clean up expired reservations
    pub fn cleanup_expired(&mut self) -> Vec<DeviceId> {
        let mut expired = Vec::new();
        let now = Utc::now();

        for (device_id, reservation) in &self.reservations {
            if reservation.expires_at < now {
                expired.push(device_id.clone());
            }
        }

        for device_id in &expired {
            if let Some(mut reservation) = self.reservations.remove(device_id) {
                reservation.state = ReservationState::Expired;
                debug!("Expired reservation for device {}", device_id);
            }
        }

        expired
    }

    /// Get all active reservations for a node
    pub fn get_node_reservations(&self, node_id: &NodeId) -> Vec<&DeviceReservation> {
        self.reservations
            .values()
            .filter(|r| r.node_id.as_str() == *node_id && r.state == ReservationState::Active)
            .collect()
    }

    /// Release all reservations for a node
    pub fn release_node(&mut self, node_id: &NodeId) -> Vec<DeviceId> {
        let mut released = Vec::new();

        self.reservations.retain(|device_id, reservation| {
            if reservation.node_id.as_str() == *node_id {
                released.push(device_id.clone());
                false
            } else {
                true
            }
        });

        debug!("Released {} reservations for node {}", released.len(), node_id);
        released
    }

    /// Get count of active reservations
    pub fn active_count(&self) -> usize {
        self.reservations
            .values()
            .filter(|r| r.state == ReservationState::Active && !Self::is_expired(r))
            .count()
    }

    /// Get total reservations
    pub fn total_count(&self) -> usize {
        self.reservations.len()
    }

    /// Check if reservation is expired
    fn is_expired(reservation: &DeviceReservation) -> bool {
        Utc::now() > reservation.expires_at
    }
}

impl Default for DevicePool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_pool() -> DevicePool {
        DevicePool::with_timeout(Duration::from_secs(60))
    }

    #[test]
    fn test_pool_creation() {
        let pool = DevicePool::new();
        assert_eq!(pool.default_timeout.as_secs(), 300);
        assert_eq!(pool.active_count(), 0);
    }

    #[test]
    fn test_reserve_device() {
        let mut pool = create_test_pool();

        let reservation = pool
            .reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();

        assert_eq!(reservation.device_id, "device-1");
        assert_eq!(reservation.node_id, "node-1");
        assert_eq!(reservation.job_id, "job-1");
        assert_eq!(reservation.state, ReservationState::Active);
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn test_reserve_duplicate_device() {
        let mut pool = create_test_pool();

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();

        let result = pool.reserve(
            "device-1".to_string(),
            "node-2".to_string(),
            "job-2".to_string(),
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already reserved"));
    }

    #[test]
    fn test_release_device() {
        let mut pool = create_test_pool();

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();

        assert!(pool.release(&"device-1".to_string()).is_ok());
        assert_eq!(pool.active_count(), 0);
        assert!(pool.is_available("device-1"));
    }

    #[test]
    fn test_extend_reservation() {
        let mut pool = create_test_pool();

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();

        let original_expires = pool
            .get_reservation(&"device-1".to_string())
            .unwrap()
            .expires_at;

        assert!(pool
            .extend(&"device-1".to_string(), Duration::from_secs(30))
            .is_ok());

        let new_expires = pool
            .get_reservation(&"device-1".to_string())
            .unwrap()
            .expires_at;

        assert!(new_expires > original_expires);
    }

    #[test]
    fn test_get_node_reservations() {
        let mut pool = create_test_pool();

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();
        pool.reserve("device-2".to_string(), "node-1".to_string(), "job-2".to_string())
            .unwrap();
        pool.reserve("device-3".to_string(), "node-2".to_string(), "job-3".to_string())
            .unwrap();

        let node1_reservations = pool.get_node_reservations(&"node-1".to_string());
        assert_eq!(node1_reservations.len(), 2);

        let node2_reservations = pool.get_node_reservations(&"node-2".to_string());
        assert_eq!(node2_reservations.len(), 1);
    }

    #[test]
    fn test_release_node() {
        let mut pool = create_test_pool();

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();
        pool.reserve("device-2".to_string(), "node-1".to_string(), "job-2".to_string())
            .unwrap();
        pool.reserve("device-3".to_string(), "node-2".to_string(), "job-3".to_string())
            .unwrap();

        let released = pool.release_node(&"node-1".to_string());
        assert_eq!(released.len(), 2);
        assert_eq!(pool.active_count(), 1);
    }

    #[test]
    fn test_is_available() {
        let mut pool = create_test_pool();

        assert!(pool.is_available("device-1"));

        pool.reserve("device-1".to_string(), "node-1".to_string(), "job-1".to_string())
            .unwrap();

        assert!(!pool.is_available("device-1"));

        pool.release(&"device-1".to_string()).unwrap();

        assert!(pool.is_available("device-1"));
    }
}
