//! Device recognition logic for `AirPods` devices.
//!
//! This module contains the logic for recognizing `AirPods` devices
//! based on various criteria such as modalias, manufacturer data,
//! services, and name/alias patterns.

use uuid::Uuid;

/// Patterns to match `AirPods` devices (case-insensitive)
const AIRPOD_PATTERNS: &[&str] = &["airpods", "beats", "powerbeats"];
// Note: "earpods" are wired earphones, not Bluetooth AirPods

/// Apple vendor ID
const APPLE_VID: u32 = 0x004C;

/// Apple company ID for manufacturer data (u16)
const APPLE_CID: u16 = 0x004C;

/// Proximity-pairing message type in manufacturer data
const PP_TYPE: u8 = 0x07;

/// Offset of the little-endian product ID in the proximity-pairing TLV.
const PID_OFFSET: usize = 3;

/// Known Apple headphone Bluetooth product IDs.
const AIRPOD_PIDS: &[u32] = &[
   0x2002, // AirPods (1st generation)
   0x200A, // AirPods Max (Lightning)
   0x200E, // AirPods Pro (1st generation)
   0x200F, // AirPods (2nd generation)
   0x2012, // Beats Fit Pro
   0x2013, // AirPods (3rd generation)
   0x2014, // AirPods Pro 2 (Lightning)
   0x2019, // AirPods 4
   0x201B, // AirPods 4 (ANC)
   0x201F, // AirPods Max (USB-C)
   0x2024, // AirPods Pro 2 (USB-C)
   0x2027, // AirPods Pro 3
];

/// Noise-control capabilities inferred from a Bluetooth product ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoiseCapabilities {
   pub noise_control: bool,
   pub adaptive_audio: bool,
}

impl NoiseCapabilities {
   /// Unknown products retain all controls.
   pub fn from_product_id(product_id: Option<u32>) -> Self {
      match product_id {
         Some(0x2002 | 0x200F | 0x2013 | 0x2019) => Self {
            noise_control: false,
            adaptive_audio: false,
         },
         Some(0x200A | 0x200E | 0x2012 | 0x201F) => Self {
            noise_control: true,
            adaptive_audio: false,
         },
         _ => Self {
            noise_control: true,
            adaptive_audio: true,
         },
      }
   }
}

/// Returns the Apple Bluetooth product ID when available.
pub async fn product_id(dev: &bluer::Device) -> Option<u32> {
   if let Ok(Some(modalias)) = dev.modalias().await
      && modalias.vendor == APPLE_VID
   {
      return Some(modalias.product);
   }

   if let Ok(Some(data)) = dev.manufacturer_data().await
      && let Some(apple_data) = data.get(&APPLE_CID)
   {
      return manufacturer_product_id(apple_data);
   }
   None
}

/// Apple service UUIDs - Note: Not always advertised by AirPods
static APPLE_SERVICES: [Uuid; 3] = [
   Uuid::from_u128(0x0000fd6f_0000_1000_8000_00805f9b34fb), // Find My
   Uuid::from_u128(0x0000fd39_0000_1000_8000_00805f9b34fb), // Apple service
   Uuid::from_u128(0x0000fd32_0000_1000_8000_00805f9b34fb), // Apple service
];

/// Extract the product ID from an Apple proximity-pairing message.
fn manufacturer_product_id(data: &[u8]) -> Option<u32> {
   if data.len() <= PID_OFFSET + 1 || data[0] != PP_TYPE || data[1] != 0x19 {
      return None;
   }
   let product_id = u32::from(u16::from_le_bytes([data[PID_OFFSET], data[PID_OFFSET + 1]]));
   AIRPOD_PIDS.contains(&product_id).then_some(product_id)
}

pub async fn is_device_airpods(dev: &bluer::Device) -> bool {
   // 1. Check modalias (most reliable for connected devices)
   if let Ok(Some(modalias)) = dev.modalias().await
      && modalias.vendor == APPLE_VID
      && AIRPOD_PIDS.contains(&modalias.product)
   {
      log::debug!(
         "AirPods detected via modalias: vendor={:#06x}, product={:#06x}",
         modalias.vendor,
         modalias.product
      );
      return true;
   }

   // 2. Check manufacturer data (useful for advertising/unconnected devices)
   if let Ok(Some(mfg_data)) = dev.manufacturer_data().await
      && let Some(apple_data) = mfg_data.get(&APPLE_CID)
      && manufacturer_product_id(apple_data).is_some()
   {
      log::debug!("AirPods detected via manufacturer data");
      return true;
   }

   // 3. Check service UUIDs (not always present, but definitive when found)
   if let Ok(Some(uuids)) = dev.uuids().await
      && uuids.iter().any(|u| APPLE_SERVICES.contains(u))
   {
      log::debug!("AirPods detected via Apple service UUID");
      return true;
   }

   // 4. Last-chance name/alias pattern matching
   if let Ok(Some(mut name)) = dev.name().await {
      name.make_ascii_lowercase();
      for pattern in AIRPOD_PATTERNS {
         if name.contains(pattern) {
            log::debug!("AirPods detected via name pattern: {name} => {pattern}");
            return true;
         }
      }
   }
   if let Ok(mut alias) = dev.alias().await {
      alias.make_ascii_lowercase();
      for pattern in AIRPOD_PATTERNS {
         if alias.contains(pattern) {
            log::debug!("AirPods detected via alias pattern: {alias} => {pattern}");
            return true;
         }
      }
   }
   false
}

#[cfg(test)]
mod tests {
   use super::{NoiseCapabilities, manufacturer_product_id};

   #[test]
   fn reads_proximity_product_id_not_battery() {
      let data = [0x07, 0x19, 0x01, 0x0a, 0x20, 0x00, 0x13];
      assert_eq!(manufacturer_product_id(&data), Some(0x200A));
      assert_eq!(manufacturer_product_id(&data[..4]), None);
   }

   #[test]
   fn capabilities_follow_product_id() {
      for pid in [0x2002, 0x200F, 0x2013, 0x2019] {
         let capabilities = NoiseCapabilities::from_product_id(Some(pid));
         assert!(!capabilities.noise_control && !capabilities.adaptive_audio);
      }
      for pid in [0x200A, 0x200E, 0x2012, 0x201F] {
         let capabilities = NoiseCapabilities::from_product_id(Some(pid));
         assert!(capabilities.noise_control && !capabilities.adaptive_audio);
      }
      for pid in [
         None,
         Some(0x2014),
         Some(0x2024),
         Some(0x201B),
         Some(0x2027),
         Some(0xFFFF),
      ] {
         let capabilities = NoiseCapabilities::from_product_id(pid);
         assert!(capabilities.noise_control && capabilities.adaptive_audio);
      }
   }
}
