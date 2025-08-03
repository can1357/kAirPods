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

/// Offset of the product-id byte inside the manufacturer data TLV
const PID_OFFSET: usize = 6;

/// All Apple headphone PIDs known
/// Based on real device testing and reverse engineering
const AIRPOD_PIDS: &[u32] = &[
   0x2002, // Beats (also some AirPods variants)
   0x200E, // AirPods (2nd gen)
   0x200A, // AirPods (3rd gen)
   0x200F, // Beats Solo Pro
   0x2012, // PowerBeats Pro
   0x2013, // AirPods Max
   0x2014, // AirPods Pro (2nd gen)
   0x2024, // AirPods Pro (1st gen)
];

/// Apple service UUIDs - Note: Not always advertised by AirPods
static APPLE_SERVICES: [Uuid; 3] = [
   Uuid::from_u128(0x0000fd6f_0000_1000_8000_00805f9b34fb), // Find My
   Uuid::from_u128(0x0000fd39_0000_1000_8000_00805f9b34fb), // Apple service
   Uuid::from_u128(0x0000fd32_0000_1000_8000_00805f9b34fb), // Apple service
];

/// Check if device is AirPods based on manufacturer data
fn check_manufacturer_data(data: &[u8]) -> bool {
   // Apple TLV format: [0] type, [1] len, [2..5] ?, [6] product_id, ...
   if data.len() > PID_OFFSET && data[0] == PP_TYPE {
      let product_id = data[PID_OFFSET];
      return AIRPOD_PIDS.iter().any(|&x| (x & 0xFF) as u8 == product_id);
   }
   false
}

pub async fn is_device_airpods(dev: &bluer::Device) -> bool {
   // 1. Check modalias (most reliable for connected devices)
   if let Ok(Some(modalias)) = dev.modalias().await {
      if modalias.vendor == APPLE_VID && AIRPOD_PIDS.contains(&modalias.product) {
         log::debug!(
            "AirPods detected via modalias: vendor={:#06x}, product={:#06x}",
            modalias.vendor,
            modalias.product
         );
         return true;
      }
   }

   // 2. Check manufacturer data (useful for advertising/unconnected devices)
   if let Ok(Some(mfg_data)) = dev.manufacturer_data().await {
      if let Some(apple_data) = mfg_data.get(&APPLE_CID) {
         if check_manufacturer_data(apple_data) {
            log::debug!("AirPods detected via manufacturer data");
            return true;
         }
      }
   }

   // 3. Check service UUIDs (not always present, but definitive when found)
   if let Ok(Some(uuids)) = dev.uuids().await {
      if uuids.iter().any(|u| APPLE_SERVICES.contains(u)) {
         log::debug!("AirPods detected via Apple service UUID");
         return true;
      }
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
