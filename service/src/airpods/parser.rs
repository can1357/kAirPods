//! Packet parsing utilities for `AirPods` protocol.
//!
//! This module contains functions to parse various AAP packet types received
//! from `AirPods` devices over the L2CAP connection.

use std::str;

use log::{debug, warn};
use smol_str::SmolStr;

use crate::{
   airpods::protocol::{
      BatteryInfo, BatteryState, BatteryStatus, Component, EarDetectionStatus, HDR_BATTERY_STATE,
      HDR_EAR_DETECTION, HDR_METADATA, NoiseControlMode,
   },
   error::Result,
};

use thiserror::Error;

/// Error type for protocol parsing.
#[derive(Error, Debug)]
pub enum ProtoError {
   /// Packet is not of the expected type
   #[error("Not a {expected} packet")]
   WrongPacketType { expected: &'static str },

   /// Packet is too short for the expected format
   #[error("Packet too short: expected at least {expected} bytes, got {actual}")]
   PacketTooShort { expected: usize, actual: usize },

   /// Invalid battery count in battery status packet
   #[error("Invalid battery count: {count} (must be 0-3)")]
   InvalidBatteryCount { count: u8 },

   /// Packet size doesn't match expected size based on content
   #[error("Packet size mismatch: expected {expected} bytes, got {actual} bytes")]
   PacketSizeMismatch { expected: usize, actual: usize },

   /// Unknown component type in battery status
   #[error("Unknown component type: 0x{component_type:02x}")]
   UnknownComponentType { component_type: u8 },

   /// Unknown noise control mode
   #[error("Unknown noise control mode: 0x{mode:02x}")]
   UnknownNoiseMode { mode: u32 },

   /// Generic invalid packet format
   #[error("Invalid packet format: {reason}")]
   InvalidFormat { reason: &'static str },
}

/// Parses a battery status packet from `AirPods`.
///
/// The packet format contains battery information for up to 3 components
/// (left, right, case).
pub fn parse_battery_status(data: &[u8]) -> Result<BatteryInfo> {
   if !data.starts_with(HDR_BATTERY_STATE) {
      return Err(
         ProtoError::WrongPacketType {
            expected: "battery status",
         }
         .into(),
      );
   }

   if data.len() < 7 {
      return Err(
         ProtoError::PacketTooShort {
            expected: 7,
            actual: data.len(),
         }
         .into(),
      );
   }

   let battery_count = data[6];
   let expected_length = 7 + 5 * battery_count as usize;

   debug!("Battery packet: {}", hex::encode(data));
   debug!(
      "Battery count: {}, expected length: {}, actual: {}",
      battery_count,
      expected_length,
      data.len()
   );

   if battery_count > 3 {
      return Err(
         ProtoError::InvalidBatteryCount {
            count: battery_count,
         }
         .into(),
      );
   }

   if data.len() != expected_length {
      return Err(
         ProtoError::PacketSizeMismatch {
            expected: expected_length,
            actual: data.len(),
         }
         .into(),
      );
   }

   let mut battery_info = BatteryInfo::new();

   for i in 0..battery_count {
      let offset = 7 + (5 * i) as usize;

      if offset + 4 >= data.len() {
         warn!("Not enough data for component {i} at offset {offset}");
         continue;
      }

      let id = data[offset];
      let pad1 = data[offset + 1];
      let level = data[offset + 2];
      let status = data[offset + 3];
      let pad2 = data[offset + 4];

      debug!(
         "Component {i}: type=0x{id:02x}, pad1=0x{pad1:02x}, level={level}, status=0x{status:02x}, pad2=0x{pad2:02x}"
      );

      let Some(component) = Component::from_repr(id) else {
         warn!("Unknown component type 0x{id:02x}");
         continue;
      };

      let bat_status = BatteryStatus::from_repr(status).unwrap_or_else(|| {
         warn!(
            "Unknown battery status 0x{status:02x} for component {component}, treating as Normal"
         );
         BatteryStatus::Normal
      });

      debug!("Parsed component: {component} = {level}% ({bat_status})");

      if bat_status != BatteryStatus::Disconnected {
         let battery_state = BatteryState {
            level,
            status: bat_status,
         };

         match component {
            Component::Left => battery_info.left = battery_state,
            Component::Right => battery_info.right = battery_state,
            Component::Case => battery_info.case = battery_state,
            Component::Headphone => battery_info.headphone = battery_state,
         }

         /*if matches!(component, Component::Left | Component::Right) {
            if battery_info.primary_pod.is_none() {
               battery_info.primary_pod = Some(component);
            } else {
               battery_info.secondary_pod = Some(component);
            }
         }*/
      }
   }
   debug!("Battery parsed - {battery_info}");
   Ok(battery_info)
}

pub fn parse_noise_mode(data: &[u8]) -> Result<NoiseControlMode> {
   if data.len() < 8 {
      return Err(
         ProtoError::PacketTooShort {
            expected: 8,
            actual: data.len(),
         }
         .into(),
      );
   }

   let mode = u32::from(data[7]);
   let Some(mode) = NoiseControlMode::from_repr(mode) else {
      return Err(ProtoError::UnknownNoiseMode { mode }.into());
   };
   Ok(mode)
}

pub fn parse_ear_detection(data: &[u8]) -> Result<EarDetectionStatus> {
   if !data.starts_with(HDR_EAR_DETECTION) {
      return Err(
         ProtoError::WrongPacketType {
            expected: "ear detection",
         }
         .into(),
      );
   }
   if data.len() < 8 {
      return Err(
         ProtoError::PacketTooShort {
            expected: 8,
            actual: data.len(),
         }
         .into(),
      );
   }
   let left_out = data[6] == 0x01;
   let right_out = data[7] == 0x01;
   Ok(EarDetectionStatus::new(!left_out, !right_out))
}

#[derive(Debug, Default)]
pub struct Metadata {
   pub name_candidate: Option<SmolStr>,
}

pub fn parse_metadata(data: &[u8]) -> Result<Metadata> {
   if !data.starts_with(HDR_METADATA) {
      return Err(
         ProtoError::WrongPacketType {
            expected: "metadata",
         }
         .into(),
      );
   }
   if data.len() < 20 {
      return Err(
         ProtoError::PacketTooShort {
            expected: 20,
            actual: data.len(),
         }
         .into(),
      );
   }

   // Try to extract device name if present
   let mut name_candidate = None;
   if data.len() > 15 {
      let payload = &data[6..];
      for i in 0..payload.len().saturating_sub(5) {
         let chunk = &payload[i..i.min(payload.len()).min(i + 10)];
         if let Ok(text) = str::from_utf8(chunk)
            && text.chars().any(|c| c.is_alphabetic())
            && text.trim().len() > 2
         {
            name_candidate = Some(text.trim().into());
            break;
         }
      }
   }

   Ok(Metadata { name_candidate })
}
