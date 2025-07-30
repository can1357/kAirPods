//! `AirPods` protocol definitions and data structures.
//!
//! This module contains all the protocol-specific constants, packet
//! definitions, and data structures for communicating with `AirPods` devices.

use std::{fmt, num::NonZeroU8, str::FromStr, sync::LazyLock};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::bluetooth::l2cap::Packet;

pub const PKT_HANDSHAKE: &[u8] = &[
   0x00, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
pub const PKT_SET_FEATURES: &[u8] = &[
   0x04, 0x00, 0x04, 0x00, 0x4d, 0x00, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];
pub const PKT_REQUEST_NOTIFY: &[u8] = &[
   0x04, 0x00, 0x04, 0x00, 0x0f, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff,
];

// Parsing headers
pub const HDR_BATTERY_STATE: &[u8] = b"\x04\x00\x04\x00\x04\x00";
pub const HDR_NOISE_CTL: &[u8] = b"\x04\x00\x04\x00\x09\x00\x0D";
pub const HDR_CMD_CTL: &[u8] = b"\x04\x00\x04\x00\x09\x00";

// ACK packet headers
pub const HDR_ACK_HANDSHAKE: &[u8] = b"\x01\x00\x04\x00";
pub const HDR_ACK_FEATURES: &[u8] = b"\x04\x00\x04\x00\x2b";
pub const HDR_METADATA: &[u8] = b"\x04\x00\x04\x00\x1d";
pub const HDR_EAR_DETECTION: &[u8] = b"\x04\x00\x04\x00\x06\x00";

/// Represents different components of `AirPods`.
#[repr(u8)]
#[derive(
   Debug,
   Clone,
   Copy,
   PartialEq,
   Eq,
   Serialize,
   Deserialize,
   strum::FromRepr,
   strum::Display,
   strum::EnumString,
)]
pub enum Component {
   Right = 0x02,
   Left = 0x04,
   Case = 0x08,
}

/// Battery status for `AirPods` components.
#[derive(
   Debug,
   Clone,
   Copy,
   PartialEq,
   Eq,
   Serialize,
   Deserialize,
   strum::FromRepr,
   strum::Display,
   strum::EnumString,
)]
#[repr(u8)]
pub enum BatteryStatus {
   Normal = 0x00,
   Charging = 0x01,
   Discharging = 0x02,
   Disconnected = 0x04,
}

/// Noise control modes supported by `AirPods`.
#[derive(
   Debug,
   Clone,
   Copy,
   PartialEq,
   Eq,
   Serialize,
   Deserialize,
   strum::FromRepr,
   strum::Display,
   strum::EnumString,
   strum::IntoStaticStr,
)]
#[repr(u32)]
pub enum NoiseControlMode {
   #[strum(serialize = "off")]
   Off = 0x01,
   #[strum(serialize = "nc")]
   NC = 0x02,
   #[strum(serialize = "trans", serialize = "transparency")]
   Trans = 0x03,
   #[strum(serialize = "adapt", serialize = "adaptive")]
   Adapt = 0x04,
}

impl NoiseControlMode {
   pub fn to_str(self) -> &'static str {
      self.into()
   }
}

pub const KNOWN_FEATURES: &[(u8, &str)] = &[
   (FeatureId::NOISE_CONTROL.id(), "noise_control"),
   (FeatureId::ONE_BUD_ANC.id(), "one_bud_anc"),
   (FeatureId::VOLUME_SWIPE.id(), "volume_swipe"),
   (FeatureId::VOLUME_INTERVAL.id(), "volume_interval"),
   (FeatureId::ADAPTIVE_VOLUME.id(), "adaptive_volume"),
   (FeatureId::CONVERSATIONAL.id(), "conversational"),
   (FeatureId::HEARING_ASSIST.id(), "hearing_assist"),
   (FeatureId::ALLOW_OFF.id(), "allow_off"),
];

/// Represents a feature command that can be sent to `AirPods`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct FeatureId(u8);

impl FromStr for FeatureId {
   type Err = strum::ParseError;

   fn from_str(s: &str) -> Result<Self, Self::Err> {
      for (repr, name) in KNOWN_FEATURES {
         if name.eq_ignore_ascii_case(s) {
            return Ok(Self(*repr));
         }
      }
      Err(strum::ParseError::VariantNotFound)
   }
}

static U8_TO_HEX: LazyLock<[[u8; 2]; 256]> = LazyLock::new(|| {
   let mut featids = [[0u8; 2]; 256];
   for i in 0..=255u8 {
      const fn nibble_to_hex(n: u8) -> u8 {
         if n < 10 { n + b'0' } else { n - 10 + b'a' }
      }
      featids[i as usize] = [nibble_to_hex(i >> 4), nibble_to_hex(i & 0x0f)];
   }
   featids
});

impl FeatureId {
   pub const NOISE_CONTROL: Self = Self(0x0D);
   pub const ONE_BUD_ANC: Self = Self(0x1B);
   pub const VOLUME_SWIPE: Self = Self(0x25);
   pub const VOLUME_INTERVAL: Self = Self(0x23);
   pub const ADAPTIVE_VOLUME: Self = Self(0x26);
   pub const CONVERSATIONAL: Self = Self(0x28);
   pub const HEARING_ASSIST: Self = Self(0x33);
   pub const ALLOW_OFF: Self = Self(0x34);

   pub const fn from_id(repr: u8) -> Self {
      Self(repr)
   }

   pub const fn id(self) -> u8 {
      self.0
   }

   pub const fn bitpos(self) -> (usize, u64) {
      let idx = self.0 as usize >> 6;
      let mask = 1 << (self.0 as usize & 0x3f);
      (idx, mask)
   }

   pub fn try_to_str(self) -> Option<&'static str> {
      let Ok(i) = KNOWN_FEATURES.binary_search_by_key(&self.0, |(repr, _)| *repr) else {
         return None;
      };
      let (_, name) = KNOWN_FEATURES[i];
      Some(name)
   }

   pub fn to_str(self) -> &'static str {
      if let Some(name) = self.try_to_str() {
         name
      } else {
         let bytes = &U8_TO_HEX[self.0 as usize];
         str::from_utf8(bytes).unwrap_or("??")
      }
   }
}

impl fmt::Display for FeatureId {
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      f.write_str(self.to_str())
   }
}

/// Battery state for a single `AirPods` component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryState {
   pub level: u8,
   pub status: BatteryStatus,
}

impl BatteryState {
   pub const fn new() -> Self {
      Self {
         level: 0,
         status: BatteryStatus::Disconnected,
      }
   }

   pub fn is_charging(&self) -> bool {
      self.status == BatteryStatus::Charging
   }

   pub fn is_available(&self) -> bool {
      self.status != BatteryStatus::Disconnected
   }
}

/// Complete battery information for all `AirPods` components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatteryInfo {
   pub left: BatteryState,
   pub right: BatteryState,
   pub case: BatteryState,
}

impl BatteryInfo {
   pub const fn new() -> Self {
      Self {
         left: BatteryState::new(),
         right: BatteryState::new(),
         case: BatteryState::new(),
      }
   }

   pub fn to_json(self) -> serde_json::Value {
      json!({
          "left_level": u32::from(self.left.level),
          "right_level": u32::from(self.right.level),
          "case_level": u32::from(self.case.level),
          "left_charging": self.left.is_charging(),
          "right_charging": self.right.is_charging(),
          "case_charging": self.case.is_charging(),
          "left_available": self.left.is_available(),
          "right_available": self.right.is_available(),
          "case_available": self.case.is_available(),
      })
   }
}

/// Ear detection status for left and right `AirPods`.
#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct EarDetectionStatus(NonZeroU8);

impl EarDetectionStatus {
   pub const LEFT: u8 = 1 << 0;
   pub const RIGHT: u8 = 1 << 1;
   pub const VALID: u8 = 0x80;

   pub const fn new(left_in_ear: bool, right_in_ear: bool) -> Self {
      let mut flags = Self::VALID;
      if left_in_ear {
         flags |= Self::LEFT;
      }
      if right_in_ear {
         flags |= Self::RIGHT;
      }
      Self(NonZeroU8::new(flags).expect("(x|valid) != 0"))
   }

   pub const fn is_left_in_ear(&self) -> bool {
      self.0.get() & Self::LEFT != 0
   }
   pub const fn is_right_in_ear(&self) -> bool {
      self.0.get() & Self::RIGHT != 0
   }

   pub fn to_json(self) -> serde_json::Value {
      json!({
          "left_in_ear": self.is_left_in_ear(),
          "right_in_ear": self.is_right_in_ear(),
      })
   }
}

/// Builds a control packet for sending commands to `AirPods`.
pub fn build_control_packet(cmd: u8, data: [u8; 4]) -> Packet {
   HDR_CMD_CTL
      .iter()
      .copied()
      .chain([cmd])
      .chain(data.iter().copied())
      .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FeatureCmd {
   Query = 0,
   Enable = 1,
   Disable = 2,
}

impl FeatureCmd {
   pub fn build(self, feature: u8) -> Packet {
      let data = self as u32;
      build_control_packet(feature, data.to_le_bytes())
   }
   pub fn parse(data: &[u8]) -> Option<(FeatureId, Self)> {
      let rest = data.strip_prefix(HDR_CMD_CTL)?;
      let (feature, rest) = rest.split_first()?;
      let u: u32 = u32::from_le_bytes(rest.try_into().ok()?);
      match u {
         0 => Some((FeatureId::from_id(*feature), Self::Query)),
         1 => Some((FeatureId::from_id(*feature), Self::Enable)),
         2 => Some((FeatureId::from_id(*feature), Self::Disable)),
         _ => None,
      }
   }
}
