//! Error types for the `AirPods` service.
//!
//! This module defines all error types that can occur during the operation
//! of the `AirPods` service, including Bluetooth, D-Bus, I/O, and protocol
//! errors.

use bluer::Address;
use thiserror::Error;
use tokio::task::JoinError;

/// Main error type for the `AirPods` service.
#[derive(Error, Debug)]
pub enum AirPodsError {
   #[error("Bluetooth error: {0}")]
   Bluetooth(#[from] bluer::Error),

   #[error("D-Bus error: {0}")]
   DBus(#[from] zbus::Error),

   #[error("D-Bus connection error: {0}")]
   DBusConnection(#[from] zbus::fdo::Error),

   #[error("I/O error: {0}")]
   Io(#[from] std::io::Error),

   #[error("Device not found: {0}")]
   DeviceNotFound(Address),

   #[error("Device not connected")]
   DeviceNotConnected,

   #[error("Invalid packet: {0}")]
   InvalidPacket(String),

   #[error("Feature not supported: {0}")]
   FeatureNotSupported(String),

   #[error("Connection lost")]
   ConnectionLost,

   #[error("Actor panicked: {0}")]
   ActorPanicked(JoinError),

   #[error("Connection closed")]
   ConnectionClosed,

   #[error("Request timeout")]
   RequestTimeout,

   #[error("Could not determine config directory")]
   ConfigDirNotFound,

   #[error("TOML parsing error: {0}")]
   TomlParse(#[from] toml::de::Error),

   #[error("TOML serialization error: {0}")]
   TomlSerialize(#[from] toml::ser::Error),

   #[error("Manager has been shut down")]
   ManagerShutdown,

   #[error("Already connecting to device")]
   AlreadyConnecting,

   #[error("Adapter not found")]
   AdapterNotFound,

   #[error("Adapter not available")]
   AdapterNotAvailable,
}

/// Convenience type alias for Results with `AirPodsError`.
pub type Result<T> = std::result::Result<T, AirPodsError>;
