//! Packet parsing utilities for AirPods protocol.
//!
//! This module contains functions to parse various packet types received
//! from AirPods devices over the L2CAP connection.

use std::collections::HashMap;

use log::{debug, warn};

use crate::{
   airpods::protocol::{
      BatteryInfo, BatteryState, BatteryStatus, Component, EarDetectionStatus, HDR_BATTERY_STATE,
      HDR_EAR_DETECTION, HDR_METADATA, NoiseControlMode,
   },
   error::{AirPodsError, Result},
};

/// Parses a battery status packet from AirPods.
///
/// The packet format contains battery information for up to 3 components
/// (left, right, case).
pub fn parse_battery_status(data: &[u8]) -> Result<BatteryInfo> {
   if !data.starts_with(HDR_BATTERY_STATE) {
      return Err(AirPodsError::InvalidPacket(
         "Not a battery status packet".into(),
      ));
   }

   if data.len() < 7 {
      return Err(AirPodsError::InvalidPacket(format!(
         "Battery packet too short: {} bytes",
         data.len()
      )));
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

   if battery_count > 3 || data.len() != expected_length {
      return Err(AirPodsError::InvalidPacket(format!(
         "Invalid battery count ({}) or size mismatch (expected {}, got {})",
         battery_count,
         expected_length,
         data.len()
      )));
   }

   let mut battery_info = BatteryInfo::new();
   let mut pods_in_packet = Vec::new();

   for i in 0..battery_count {
      let offset = 7 + (5 * i) as usize;

      if offset + 4 >= data.len() {
         warn!("Not enough data for component {i} at offset {offset}");
         continue;
      }

      let component_type = data[offset];
      let spacer1 = data[offset + 1];
      let level = data[offset + 2];
      let status = data[offset + 3];
      let spacer2 = data[offset + 4];

      debug!(
         "Component {i}: type=0x{component_type:02x}, spacer1=0x{spacer1:02x}, level={level}, status=0x{status:02x}, spacer2=0x{spacer2:02x}"
      );

      if spacer1 != 0x01 || spacer2 != 0x01 {
         warn!(
            "Invalid spacer bytes for component {i}: spacer1=0x{spacer1:02x}, spacer2=0x{spacer2:02x}"
         );
         return Err(AirPodsError::InvalidPacket("Invalid spacer bytes".into()));
      }

      let component = match component_type {
         0x02 => Component::Right,
         0x04 => Component::Left,
         0x08 => Component::Case,
         _ => {
            warn!("Unknown component type 0x{component_type:02x}");
            continue;
         },
      };

      let bat_status = match status {
         0x00 => BatteryStatus::Normal,
         0x01 => BatteryStatus::Charging,
         0x02 => BatteryStatus::Discharging,
         0x04 => BatteryStatus::Disconnected,
         _ => {
            warn!(
               "Unknown battery status 0x{status:02x} for component {component}, treating as Normal"
            );
            BatteryStatus::Normal
         },
      };

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
         }

         if matches!(component, Component::Left | Component::Right) {
            pods_in_packet.push(component);
         }
      }
   }

   // Set primary and secondary pods based on order
   /*
   if !pods_in_packet.is_empty() {
       battery_info.primary_pod = Some(pods_in_packet[0]);
       if pods_in_packet.len() >= 2 {
           battery_info.secondary_pod = Some(pods_in_packet[1]);
       }
   }
   */

   debug!(
      "Battery parsed - L:{}%({}) R:{}%({}) C:{}%({})",
      battery_info.left.level,
      battery_info.left.status,
      battery_info.right.level,
      battery_info.right.status,
      battery_info.case.level,
      battery_info.case.status
   );

   Ok(battery_info)
}

pub fn parse_noise_mode(data: &[u8]) -> Result<NoiseControlMode> {
   if data.len() < 8 {
      return Err(AirPodsError::InvalidPacket(
         "Noise control packet too short".into(),
      ));
   }

   let mode = data[7] - 1;
   match mode + 1 {
      0x01 => Ok(NoiseControlMode::Off),
      0x02 => Ok(NoiseControlMode::NC),
      0x03 => Ok(NoiseControlMode::Trans),
      0x04 => Ok(NoiseControlMode::Adapt),
      _ => Err(AirPodsError::InvalidPacket(format!(
         "Unknown noise mode: {}",
         mode + 1
      ))),
   }
}

pub fn parse_ear_detection(data: &[u8]) -> Result<EarDetectionStatus> {
   if !data.starts_with(HDR_EAR_DETECTION) || data.len() < 8 {
      return Err(AirPodsError::InvalidPacket(
         "Invalid ear detection packet".into(),
      ));
   }
   let left_out = data[6] == 0x01;
   let right_out = data[7] == 0x01;
   Ok(EarDetectionStatus::new(!left_out, !right_out))
}

pub fn parse_metadata(data: &[u8]) -> Result<HashMap<String, serde_json::Value>> {
   if !data.starts_with(HDR_METADATA) || data.len() < 20 {
      return Err(AirPodsError::InvalidPacket(
         "Invalid metadata packet".into(),
      ));
   }

   let mut metadata = HashMap::new();
   metadata.insert(
      "packet_type".to_string(),
      serde_json::Value::String("metadata".to_string()),
   );
   metadata.insert(
      "raw_data".to_string(),
      serde_json::Value::String(hex::encode(data)),
   );
   metadata.insert(
      "length".to_string(),
      serde_json::Value::Number(data.len().into()),
   );

   // Try to extract device name if present
   if data.len() > 15 {
      let payload = &data[6..];
      for i in 0..payload.len().saturating_sub(5) {
         let chunk = &payload[i..i.min(payload.len()).min(i + 10)];
         if let Ok(text) = std::str::from_utf8(chunk)
            && text.chars().any(|c| c.is_alphabetic())
            && text.trim().len() > 2
         {
            metadata.insert(
               "possible_name".to_string(),
               serde_json::Value::String(text.trim().to_string()),
            );
            break;
         }
      }
   }

   Ok(metadata)
}
