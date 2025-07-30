//! Event handling system for `AirPods` status updates.
//!
//! This module provides the event infrastructure for notifying about
//! `AirPods` state changes such as battery updates, connection status,
//! and feature changes.

use std::sync::Arc;

use smol_str::SmolStr;

use crate::airpods::{
   device::AirPods,
   protocol::{BatteryInfo, EarDetectionStatus, NoiseControlMode},
};

/// Events that can be emitted by the `AirPods` service.
#[derive(Debug, Clone)]
pub enum AirPodsEvent {
   DeviceConnected,
   DeviceDisconnected,
   DeviceError,
   BatteryUpdated(BatteryInfo),
   NoiseControlChanged(NoiseControlMode),
   EarDetectionChanged(EarDetectionStatus),
   DeviceNameChanged(SmolStr),
}

/// Trait for implementing event emission.
pub trait EventBus: Send + Sync {
   /// Emits an event to all registered listeners.
   fn emit(&self, device: &AirPods, event: AirPodsEvent);
}

/// Type alias for a thread-safe event sender.
pub type EventSender = Arc<dyn EventBus>;
