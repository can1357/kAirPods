//! `AirPods` device implementation and state management.
//!
//! This module provides the core `AirPods` type which represents a connected
//! `AirPods` device, manages its state, and handles communication over L2CAP.

use core::fmt;
use std::{
   borrow::Borrow,
   collections::HashMap,
   mem,
   sync::{
      Arc, Weak,
      atomic::{AtomicBool, AtomicU64, Ordering},
   },
   time::{Duration, Instant},
};

use bluer::Address;
use crossbeam::atomic::AtomicCell;
use log::{debug, error, info, warn};
use serde_json::json;
use smol_str::{SmolStr, ToSmolStr};
use tokio::{
   sync::{RwLock, oneshot},
   task::{JoinHandle, JoinSet},
   time,
};

use crate::{
   airpods::{
      parser,
      protocol::{
         BatteryInfo, EarDetectionStatus, FeatureCmd, FeatureId, HDR_ACK_FEATURES,
         HDR_ACK_HANDSHAKE, HDR_BATTERY_STATE, HDR_EAR_DETECTION, HDR_METADATA, HDR_NOISE_CTL,
         NoiseControlMode, PKT_HANDSHAKE, PKT_REQUEST_NOTIFY, PKT_SET_FEATURES,
         build_control_packet,
      },
   },
   bluetooth::l2cap::{self, L2CapReceiver, L2CapSender, Packet},
   error::{AirPodsError, Result},
   event::{AirPodsEvent, EventSender},
};

/// Internal state for an active L2CAP connection.
#[derive(Debug)]
struct ConnectionState {
   sender: l2cap::L2CapSender,
   jset: JoinSet<()>,
}

impl Drop for ConnectionState {
   fn drop(&mut self) {
      self.jset.abort_all();
   }
}

/// Ring buffer for tracking battery history.
const BATTERY_HISTORY_SIZE: usize = 32;

#[derive(Debug, Clone, Copy)]
struct BatteryHistory {
   base_time: Instant,
   timestamps: [u32; BATTERY_HISTORY_SIZE], // ms since base_time
   levels: [u8; BATTERY_HISTORY_SIZE],
   head: usize,
   count: usize,
}

impl Default for BatteryHistory {
   fn default() -> Self {
      let now = Instant::now();
      Self {
         base_time: now,
         timestamps: [0; BATTERY_HISTORY_SIZE],
         levels: [0; BATTERY_HISTORY_SIZE],
         head: 0,
         count: 0,
      }
   }
}

impl BatteryHistory {
   fn push(&mut self, timestamp: Instant, level: u8) {
      self.timestamps[self.head] = timestamp
         .saturating_duration_since(self.base_time)
         .as_millis()
         .try_into()
         .unwrap_or(u32::MAX);
      self.levels[self.head] = level;
      self.head = (self.head + 1) % BATTERY_HISTORY_SIZE;
      self.count = self.count.saturating_add(1).min(BATTERY_HISTORY_SIZE);
   }

   fn iter(&self) -> impl ExactSizeIterator<Item = (Instant, u8)> + Clone + '_ {
      // Start from oldest entry
      let start = if self.count < BATTERY_HISTORY_SIZE {
         0
      } else {
         self.head
      };

      (0..self.count).map(move |i| {
         let idx = (start + i) % BATTERY_HISTORY_SIZE;
         let timestamp = self.base_time + Duration::from_millis(self.timestamps[idx] as u64);
         (timestamp, self.levels[idx])
      })
   }

   fn last_level(&self) -> Option<u8> {
      if self.count == 0 {
         return None;
      }
      let last_idx = if self.head == 0 {
         BATTERY_HISTORY_SIZE - 1
      } else {
         self.head - 1
      };
      Some(self.levels[last_idx])
   }

   fn clear(&mut self) {
      self.base_time = Instant::now();
      self.timestamps.fill(0);
      self.levels.fill(0);
      self.head = 0;
      self.count = 0;
   }

   fn record_battery_drop(&mut self, level: u8, timestamp: Instant) {
      // Not charging, record battery level
      if let Some(last_level) = self.last_level() {
         if level < last_level {
            debug!("Battery dropped from {last_level} to {level}");
            self.push(timestamp, level);
         }
      } else {
         // First recording
         self.push(timestamp, level);
      }
   }

   fn calculate_drain_rate_and_alpha(
      &self,
      min_samples: usize,
      max_age: Option<Instant>,
   ) -> Option<(f64, f64)> {
      if self.count < min_samples {
         None
      } else {
         let samples: heapless::Vec<_, BATTERY_HISTORY_SIZE> = self
            .iter()
            .filter(|(timestamp, _)| max_age.is_none_or(|s| *timestamp >= s))
            .collect();
         if samples.len() < min_samples {
            None
         } else {
            let rate = calculate_slope(&samples)?;
            let alpha = if samples.len() >= 10 { 0.3 } else { 0.1 };
            Some((rate, alpha))
         }
      }
   }
}

/// Internal shared state for an `AirPods` device.
#[derive(Debug, Default)]
struct AirPodsInner {
   address: Address,
   address_str: SmolStr,
   name: parking_lot::Mutex<SmolStr>,
   battery: AtomicCell<Option<BatteryInfo>>,
   is_connected: AtomicBool,
   ear_detection: AtomicCell<Option<EarDetectionStatus>>,
   noise_mode: AtomicCell<Option<NoiseControlMode>>,
   features: [AtomicU64; 256 / 64],
   features_seen: [AtomicU64; 256 / 64],
   conn: RwLock<Option<ConnectionState>>,
   battery_history: parking_lot::Mutex<[BatteryHistory; 2]>, // (left, right)
   last_ttl_estimate: AtomicCell<Option<u32>>,
}

/// Represents a connected `AirPods` device.
///
/// This type is cheaply cloneable and thread-safe.
#[derive(Clone)]
pub struct AirPods(Arc<AirPodsInner>);

/// Weak reference to an `AirPods` device.
#[derive(Debug, Clone)]
pub struct WeakAirPods(Weak<AirPodsInner>);

impl WeakAirPods {
   pub fn new(airpods: &AirPods) -> Self {
      Self(Arc::downgrade(&airpods.0))
   }

   pub fn upgrade(&self) -> Option<AirPods> {
      self.0.upgrade().map(AirPods)
   }
}

impl fmt::Debug for AirPods {
   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      fmt::Debug::fmt(&self.0, f)
   }
}

/// Represents the result of an update operation on device state.
#[derive(Debug, Clone, Copy)]
pub enum UpdateOp<T> {
   /// No change occurred
   Noop,
   /// A new value was inserted (None -> Some)
   Inserted,
   /// A value was deleted (Some -> None)
   Deleted(T),
   /// An existing value was updated
   Updated(T),
}

impl<T: PartialEq> UpdateOp<T> {
   fn apply_atomic(dst: &AtomicCell<Option<T>>, new: Option<T>) -> Self
   where
      T: Copy,
   {
      Self::new(dst.swap(new), new)
   }

   fn new(prev: Option<T>, new: Option<T>) -> Self {
      match (prev, new) {
         (Some(p), Some(n)) if p == n => Self::Noop,
         (None, Some(_)) => Self::Inserted,
         (Some(p), None) => Self::Deleted(p),
         (Some(_), Some(n)) => Self::Updated(n),
         (None, None) => Self::Noop,
      }
   }

   const fn is_updated(&self) -> bool {
      matches!(self, Self::Inserted | Self::Updated(_))
   }
}

impl AirPods {
   /// Creates a new `AirPods` device instance.
   pub fn new(address: Address, name: String) -> Self {
      Self(Arc::new(AirPodsInner {
         address,
         address_str: address.to_smolstr(),
         name: parking_lot::Mutex::new(name.into()),
         ..Default::default()
      }))
   }

   /// Gets the address of the Airpod.
   pub fn address(&self) -> Address {
      self.0.address
   }

   /// Gets the address string of the Airpod.
   pub fn address_str(&self) -> &SmolStr {
      &self.0.address_str
   }

   /// Gets the name of the Airpod.
   pub fn name(&self) -> SmolStr {
      self.0.name.lock().clone()
   }

   /// Updates the name of the Airpod.
   pub fn update_name(&self, name: SmolStr) -> UpdateOp<SmolStr> {
      let mut lock = self.0.name.lock();
      if lock.as_str() == name.as_str() {
         return UpdateOp::Noop;
      }
      UpdateOp::Updated(mem::replace(&mut *lock, name))
   }

   /// Gets the battery information of the Airpod.
   pub fn battery_info(&self) -> Option<BatteryInfo> {
      self.0.battery.load()
   }

   /// Replaces the battery information of the Airpod.
   pub fn update_battery_info(
      &self,
      battery: impl Into<Option<BatteryInfo>>,
   ) -> UpdateOp<BatteryInfo> {
      UpdateOp::apply_atomic(&self.0.battery, battery.into())
   }

   /// Checks if the Airpod is connected.
   pub fn is_connected(&self) -> bool {
      self.0.is_connected.load(Ordering::Relaxed)
   }

   /// Gets the ear detection status of the Airpod.
   pub fn ear_detection(&self) -> Option<EarDetectionStatus> {
      self.0.ear_detection.load()
   }

   /// Sets the ear detection status of the Airpod.
   pub fn update_ear_detection(
      &self,
      status: impl Into<Option<EarDetectionStatus>>,
   ) -> UpdateOp<EarDetectionStatus> {
      UpdateOp::apply_atomic(&self.0.ear_detection, status.into())
   }

   /// Gets the noise control mode of the Airpod.
   pub fn noise_mode(&self) -> Option<NoiseControlMode> {
      self.0.noise_mode.load()
   }

   /// Sets the noise control mode of the Airpod.
   pub fn update_noise_mode(
      &self,
      mode: impl Into<Option<NoiseControlMode>>,
   ) -> UpdateOp<NoiseControlMode> {
      UpdateOp::apply_atomic(&self.0.noise_mode, mode.into())
   }

   /// Converts the device state to a JSON representation.
   pub fn to_json(&self) -> serde_json::Value {
      let mut info = json!({
          "address": self.address_str().as_str(),
          "name": self.name().as_str(),
          "connected": self.is_connected(),
      });

      if let Some(battery) = self.battery_info() {
         info["battery"] = battery.to_json();
      }

      // Add battery TTL estimate
      info["battery_ttl_estimate"] = match self.estimate_battery_ttl() {
         Some(minutes) => json!(minutes),
         None => json!(null),
      };

      if let Some(mode) = self.noise_mode() {
         info["noise_mode"] = json!(mode.to_str());
      }

      if let Some(ear) = self.ear_detection() {
         info["ear_detection"] = ear.to_json();
      }

      let features_dict: HashMap<_, _> = self
         .features()
         .into_iter()
         .map(|(k, v)| (k.to_str(), v))
         .collect();
      info["features"] = json!(features_dict);
      info
   }

   pub fn feature_enabled(&self, feature: FeatureId) -> bool {
      let (idx, mask) = feature.bitpos();
      self.0.features[idx].load(Ordering::Relaxed) & mask != 0
   }

   pub fn seen_feature(&self, feature: FeatureId) -> bool {
      let (idx, mask) = feature.bitpos();
      self.0.features_seen[idx].load(Ordering::Relaxed) & mask != 0
   }

   pub fn features(&self) -> Vec<(FeatureId, bool)> {
      let mut features = Vec::new();
      for i in 0..=0xff {
         let feat = FeatureId::from_id(i);
         if self.seen_feature(feat) {
            features.push((feat, self.feature_enabled(feat)));
         }
      }
      features
   }

   pub fn set_feature_enabled(&self, feature: FeatureId, enabled: bool) -> bool {
      let (idx, mask) = feature.bitpos();
      {
         self.0.features_seen[idx].fetch_or(mask, Ordering::Relaxed);
      }
      let prev = if enabled {
         self.0.features[idx].fetch_or(mask, Ordering::Relaxed)
      } else {
         self.0.features[idx].fetch_and(!mask, Ordering::Relaxed)
      };
      prev & mask != 0
   }

   /// Establishes an L2CAP connection to the `AirPods` device.
   ///
   /// Returns a join handle that resolves when the connection is closed.
   pub async fn connect(&self, event_tx: &EventSender) -> Result<JoinHandle<Option<AirPodsError>>> {
      info!("Connecting to AirPods at {}", self.address());
      let mut conn = self.0.conn.write().await;
      let _ = conn.take();

      // Create L2CAP connection
      let mut jset = JoinSet::new();

      // Perform handshake
      let (receiver, sender) = self.start_connection(&mut jset).await?;

      // Start packet processor with direct access to fields
      let jhandle = self.start_packet_processor(receiver, event_tx.clone());

      // Store connection state
      *conn = Some(ConnectionState { sender, jset });
      self.0.is_connected.store(true, Ordering::Relaxed);

      info!("Successfully connected to {}", self.address());
      Ok(jhandle)
   }

   pub async fn disconnect(&self) {
      self.0.is_connected.store(false, Ordering::Relaxed);
      let _ = self.0.conn.write().await.take();
      info!("Disconnected from {}", self.address());
   }

   async fn notify_disconnected(&self, event_tx: &EventSender) {
      self.0.is_connected.store(false, Ordering::Relaxed);
      let _ = self.0.conn.write().await.take();
      info!("Disconnected from {}", self.address());
      event_tx.emit(self, AirPodsEvent::DeviceDisconnected);
   }

   async fn start_connection(
      &self,
      jset: &mut JoinSet<()>,
   ) -> Result<(L2CapReceiver, L2CapSender)> {
      let (hs_ack_tx, mut hs_ack_rx) = oneshot::channel();
      let (feat_ack_tx, mut feat_ack_rx) = oneshot::channel();

      async fn wait_for_ack<T>(tx: &mut oneshot::Receiver<T>) -> Result<T> {
         time::timeout(Duration::from_secs(5), tx)
            .await
            .map_err(|_| AirPodsError::RequestTimeout)?
            .map_err(|_| AirPodsError::ConnectionClosed)
      }

      let hooks = l2cap::Hooks::new()
         .prefix_once(HDR_ACK_HANDSHAKE, |_| {
            let _ = hs_ack_tx.send(());
         })
         .prefix_once(HDR_ACK_FEATURES, |_| {
            let _ = feat_ack_tx.send(());
         });

      let (receiver, sender) = l2cap::connect(jset, hooks, self.address(), None).await?;
      info!("Starting handshake sequence...");

      // Send handshake
      if let Err(e) = sender.send(PKT_HANDSHAKE).await {
         error!("Failed to send handshake: {e:?}");
         return Err(e);
      } else if let Err(e) = wait_for_ack(&mut hs_ack_rx).await {
         warn!("No handshake acknowledgment received ({e:?}), continuing anyway...");
      } else {
         info!("Handshake acknowledged");
      }

      // Send features
      if let Err(e) = sender.send(PKT_SET_FEATURES).await {
         error!("Failed to send features: {e:?}");
         return Err(e);
      } else if let Err(e) = wait_for_ack(&mut feat_ack_rx).await {
         warn!("No features acknowledgment received ({e:?}), continuing anyway...");
      } else {
         info!("Features acknowledged");
      }

      // Request notifications
      if let Err(e) = sender.send(PKT_REQUEST_NOTIFY).await {
         error!("Failed to send notification request: {e:?}");
         return Err(e);
      }

      // Schedule retry for notifications with battery status check
      let weak = WeakAirPods::new(self);
      let mac = self.address();
      info!("{mac}: Handshake sequence completed");
      jset.spawn({
            let sender = sender.clone();
            async move {
                time::sleep(Duration::from_secs(1)).await;

                const RETRY_SCHEDULE: &[Duration] = &[
                    Duration::from_secs(2),
                    Duration::from_secs(3),
                    Duration::from_secs(5),
                    Duration::from_secs(10),
                ];

                for (i, delay) in RETRY_SCHEDULE.iter().enumerate() {
                    if let Some(this) = weak.upgrade()
                        && this.battery_info().is_some() {
                            info!("{mac}: Battery status established after {i} retries!");
                            return;
                        }
                    warn!(
                        "{mac}: [Retry {i}] No battery status received after notification request, retrying in {delay:?}..."
                    );
                    let _ = sender.send(PKT_REQUEST_NOTIFY).await;
                    time::sleep(*delay).await;
                }
            }
        });
      Ok((receiver, sender))
   }

   fn start_packet_processor(
      &self,
      mut rx: l2cap::L2CapReceiver,
      event_tx: EventSender,
   ) -> JoinHandle<Option<AirPodsError>> {
      let addr = self.address();
      let event_tx = event_tx.clone();
      let weak = WeakAirPods::new(self);
      tokio::spawn(async move {
         let mut err = None;
         loop {
            match rx.recv().await {
               Ok(packet) => {
                  if let Some(this) = weak.upgrade() {
                     this.process_packet(addr, packet, &event_tx).await;
                  } else {
                     warn!("{addr}: Airpod instance was dropped");
                     break;
                  }
               },
               Err(e) => {
                  if let Some(this) = weak.upgrade() {
                     this.notify_disconnected(&event_tx).await;
                  } else {
                     warn!("{addr}: Connection closed: {e:?}");
                  }
                  err = Some(e);
                  break;
               },
            }
         }
         err
      })
   }

   pub async fn set_noise_control(&self, mode: NoiseControlMode) -> Result<()> {
      let conn = self.0.conn.read().await;
      if let Some(conn) = conn.as_ref() {
         let packet = build_control_packet(0x0D, (mode as u32).to_le_bytes());
         conn.sender.send(&packet).await?;
         self.0.noise_mode.store(Some(mode));
         Ok(())
      } else {
         Err(AirPodsError::DeviceNotConnected)
      }
   }

   pub async fn passthrough(&self, packet: &[u8]) -> Result<()> {
      let conn = self.0.conn.read().await;
      if let Some(conn) = conn.as_ref() {
         conn.sender.send(packet).await?;
         Ok(())
      } else {
         Err(AirPodsError::DeviceNotConnected)
      }
   }

   pub async fn set_feature(&self, feature: FeatureId, enabled: bool) -> Result<()> {
      let conn = self.0.conn.read().await;
      if let Some(conn) = conn.as_ref() {
         let packet = if enabled {
            FeatureCmd::Enable.build(feature.id())
         } else {
            FeatureCmd::Disable.build(feature.id())
         };
         conn.sender.send(&packet).await?;
         self.set_feature_enabled(feature, enabled);
         Ok(())
      } else {
         Err(AirPodsError::DeviceNotConnected)
      }
   }

   async fn process_packet(&self, address: Address, packet: Packet, event_tx: &EventSender) {
      // Battery status
      if packet.starts_with(HDR_BATTERY_STATE) {
         match parser::parse_battery_status(&packet) {
            Ok(battery) => {
               debug!(
                  "Battery updated for {}: L:{}% R:{}% C:{}%",
                  address, battery.left.level, battery.right.level, battery.case.level
               );

               // Record battery drops for TTL estimation
               self.record_battery_drop(
                  battery.left.level,
                  battery.right.level,
                  battery.left.is_charging(),
                  battery.right.is_charging(),
               );

               // Send event if battery changed
               if self.update_battery_info(battery).is_updated() {
                  event_tx.emit(self, AirPodsEvent::BatteryUpdated(battery));
               }
            },
            Err(e) => warn!("Failed to parse battery: {e}"),
         }
      }
      // Noise control mode
      else if packet.starts_with(HDR_NOISE_CTL) {
         match parser::parse_noise_mode(&packet) {
            Ok(mode) => {
               debug!("Noise mode updated for {address}: {mode}");
               if self.update_noise_mode(mode).is_updated() {
                  event_tx.emit(self, AirPodsEvent::NoiseControlChanged(mode));
               }
            },
            Err(e) => warn!("Failed to parse noise mode: {e}"),
         }
      }
      // Ear detection
      else if packet.starts_with(HDR_EAR_DETECTION) {
         match parser::parse_ear_detection(&packet) {
            Ok(status) => {
               debug!(
                  "Ear detection updated for {}: L:{} R:{}",
                  address,
                  status.is_left_in_ear(),
                  status.is_right_in_ear()
               );

               if self.update_ear_detection(status).is_updated() {
                  event_tx.emit(self, AirPodsEvent::EarDetectionChanged(status));
               }
            },
            Err(e) => warn!("Failed to parse ear detection: {e}"),
         }
      }
      // Metadata packets
      else if packet.starts_with(HDR_METADATA) {
         if let Ok(metadata) = parser::parse_metadata(&packet) {
            debug!("Device metadata for {address}: {metadata:?}");

            if let Some(new_name) = metadata.name_candidate
               && self.update_name(new_name.clone()).is_updated()
            {
               event_tx.emit(self, AirPodsEvent::DeviceNameChanged(new_name));
            }
         }
      }
      // Other packets
      else if packet.starts_with(HDR_ACK_HANDSHAKE) {
         debug!("Received handshake ACK from {address}");
      } else if packet.starts_with(HDR_ACK_FEATURES) {
         debug!("Received features ACK from {address}");
      } else if let Some((cmd, op)) = FeatureCmd::parse(&packet) {
         debug!("Received feature command from {address}: {cmd} {op:?}");
         if matches!(op, FeatureCmd::Enable | FeatureCmd::Disable) {
            self.set_feature_enabled(cmd, matches!(op, FeatureCmd::Enable));
         }
      } else {
         let data = if packet.len() < 16 {
            hex::encode(&packet)
         } else {
            format!(
               "{}..{}",
               hex::encode(&packet[..8]),
               hex::encode(&packet[8..])
            )
         };

         debug!(
            "Unknown packet from {} | {} bytes => {}",
            address,
            packet.len(),
            data
         );
      }
   }

   /// Records a battery drop for the specified component if the level decreased.
   /// Clears history when charging starts to avoid using stale drain rates.
   fn record_battery_drop(
      &self,
      left_level: u8,
      right_level: u8,
      left_charging: bool,
      right_charging: bool,
   ) {
      let now = Instant::now();

      let mut history = self.0.battery_history.lock();
      let [left, right] = &mut *history;
      let sides = [
         ("left", left_level, left_charging, left),
         ("right", right_level, right_charging, right),
      ];
      for (name, level, charging, history) in sides {
         if charging && history.last_level().is_some() {
            debug!("{name} bud started charging, clearing battery history");
            history.clear();
         } else if !charging {
            history.record_battery_drop(level, now);
         }
      }
   }

   /// Calculates the battery drain rate using linear regression for better accuracy.
   fn calculate_drain_rate_and_alpha(&self) -> Option<(f64, f64)> {
      const MIN_SAMPLES: usize = 4;
      const MAX_AGE_SECS: u64 = 2 * 60 * 60; // 2 hours

      let now = Instant::now();

      // Calculate drain rate for left bud
      let max_age = now.checked_sub(Duration::from_secs(MAX_AGE_SECS));
      let [left, right] = { *self.0.battery_history.lock() };

      let left = left.calculate_drain_rate_and_alpha(MIN_SAMPLES, max_age);
      let right = right.calculate_drain_rate_and_alpha(MIN_SAMPLES, max_age);

      // Return the maximum drain rate (most conservative estimate)
      match (left, right) {
         (Some((l, la)), Some((r, ra))) => Some((l.max(r), f64::min(la, ra))),
         (Some((l, la)), None) => Some((l, la)),
         (None, Some((r, ra))) => Some((r, ra)),
         (None, None) => None,
      }
   }

   /// Estimates battery time-to-live in minutes based on current levels and drain rate.
   pub fn estimate_battery_ttl(&self) -> Option<u32> {
      let battery = self.battery_info()?;
      let prev_estimate = self.0.last_ttl_estimate.load();

      // Don't estimate if either bud is charging
      if battery.left.is_charging() || battery.right.is_charging() {
         if prev_estimate.is_some() {
            debug!("Battery TTL estimation unavailable: AirPods are charging");
            self.0.last_ttl_estimate.store(None);
         }
         return None;
      }

      // Don't estimate if either bud is disconnected
      if !battery.left.is_available() || !battery.right.is_available() {
         if prev_estimate.is_some() {
            debug!("Battery TTL estimation unavailable: One or both buds disconnected");
            self.0.last_ttl_estimate.store(None);
         }
         return None;
      }

      let (drain_rate, alpha) = match self.calculate_drain_rate_and_alpha() {
         Some((rate, alpha)) => (rate, alpha),
         None => {
            if prev_estimate.is_some() {
               self.0.last_ttl_estimate.store(None);
            }
            return None;
         },
      };

      // Use the minimum battery level for conservative estimate
      let min_level = battery.left.level.min(battery.right.level) as f64;

      // Calculate hours remaining
      let hours_remaining = min_level / drain_rate;

      // Convert to minutes
      let new_minutes = (hours_remaining * 60.0) as u32;

      if new_minutes > 0 && new_minutes < 24 * 60 {
         // Apply hysteresis to avoid jumpy estimates
         let smoothed_minutes = if let Some(last_estimate) = prev_estimate {
            let smoothed = (new_minutes as f64) * alpha + (last_estimate as f64) * (1.0 - alpha);
            smoothed.round() as u32
         } else {
            info!("Battery TTL estimation now available: {new_minutes} minutes remaining");
            new_minutes
         };

         // Cache the smoothed estimate
         self.0.last_ttl_estimate.store(Some(smoothed_minutes));
         Some(smoothed_minutes)
      } else {
         if prev_estimate.is_some() {
            debug!(
               "Battery TTL estimation unavailable: Unreasonable estimate ({new_minutes} minutes)"
            );
            self.0.last_ttl_estimate.store(None);
         }
         None
      }
   }
}

// Helper function to calculate linear regression slope
fn calculate_slope<I>(samples: I) -> Option<f64>
where
   I: IntoIterator<Item: Borrow<(Instant, u8)>>,
   I::IntoIter: ExactSizeIterator,
{
   let samples = samples.into_iter();
   let len = samples.len();
   if len < 2 {
      return None;
   }

   let n = len as f64;
   let mut sum_x = 0.0;
   let mut sum_y = 0.0;
   let mut sum_xy = 0.0;
   let mut sum_xx = 0.0;
   let mut base_time = None;

   for v in samples {
      let (timestamp, level) = v.borrow();

      let since = if let Some(base_time) = base_time {
         timestamp.duration_since(base_time).as_secs_f64() / 3600.0
      } else {
         base_time = Some(*timestamp);
         0.0
      };

      let x = since;
      let y = *level as f64;

      sum_x += x;
      sum_y += y;
      sum_xy += x * y;
      sum_xx += x * x;
   }

   let denominator = n * sum_xx - sum_x * sum_x;
   if denominator.abs() < f64::EPSILON {
      return None;
   }

   // Slope represents battery change per hour (negative for drain)
   let slope = (n * sum_xy - sum_x * sum_y) / denominator;

   // Convert to positive drain rate
   if slope < 0.0 {
      Some(-slope)
   } else {
      None // Battery not draining
   }
}

#[cfg(test)]
mod tests {
   use super::*;
   use crate::airpods::protocol::{BatteryState, BatteryStatus};
   use std::time::Duration as StdDuration;

   #[test]
   fn test_battery_history_ring_buffer() {
      let mut history = BatteryHistory::default();
      let base_time = Instant::now();

      // Test initial state
      assert_eq!(history.count, 0);
      assert!(history.last_level().is_none());

      // Add some samples
      for i in 0..5 {
         history.push(base_time + StdDuration::from_secs(i * 60), 100 - i as u8);
      }

      assert_eq!(history.count, 5);
      assert_eq!(history.last_level(), Some(96));

      // Test iterator
      let samples: Vec<_> = history.iter().collect();
      assert_eq!(samples.len(), 5);
      assert_eq!(samples[0].1, 100);
      assert_eq!(samples[4].1, 96);
   }

   #[test]
   fn test_battery_history_wraparound() {
      let mut history = BatteryHistory::default();
      let base_time = Instant::now();

      // Fill beyond capacity
      for i in 0..80 {
         history.push(base_time + StdDuration::from_secs(i * 60), 100 - i as u8);
      }

      assert_eq!(history.count, BATTERY_HISTORY_SIZE);

      // Check that we have the most recent samples
      let samples: Vec<_> = history.iter().collect();
      assert_eq!(samples.len(), BATTERY_HISTORY_SIZE);

      // The oldest sample should be from index 48 (80 - 32)
      assert_eq!(samples[0].1, 52);
   }

   #[test]
   fn test_drain_rate_calculation() {
      let airpods = AirPods::new(Address::any(), "Test AirPods".to_string());

      // Add battery history with known drain pattern
      let now = Instant::now();
      {
         let mut history = airpods.0.battery_history.lock();
         let [left_history, right_history] = &mut *history;

         // Clear history and set base_time to 50 minutes ago
         left_history.clear();
         right_history.clear();

         // Set base_time to the start of our simulated history
         let start_time = now - StdDuration::from_secs(3000);
         left_history.base_time = start_time;
         right_history.base_time = start_time;

         // Push 6 samples, each 10 minutes apart, with 2% drop each time
         for i in 0..6 {
            let time = start_time + StdDuration::from_secs(i * 600);
            let level = (100 - (i * 2)) as u8;
            left_history.push(time, level);
            right_history.push(time, level);
         }
      }

      // Calculate drain rate
      let (rate, alpha) = airpods
         .calculate_drain_rate_and_alpha()
         .expect("rate was none");
      assert!(alpha > 0.0, "Alpha was 0.0");

      // Should be approximately 12% per hour (allowing for some floating point error)
      assert!(rate > 11.0 && rate < 13.0, "Rate was: {rate}");
   }

   #[test]
   fn test_ttl_estimation() {
      let airpods = AirPods::new(Address::any(), "Test AirPods".to_string());

      // Set current battery levels
      let battery = BatteryInfo {
         left: BatteryState {
            level: 50,
            status: BatteryStatus::Normal,
         },
         right: BatteryState {
            level: 60,
            status: BatteryStatus::Normal,
         },
         case: BatteryState {
            level: 80,
            status: BatteryStatus::Normal,
         },
      };
      airpods.update_battery_info(battery);

      // Add battery history with 12% per hour drain
      let now = Instant::now();
      {
         let mut history = airpods.0.battery_history.lock();
         let [left_history, right_history] = &mut *history;

         // Clear history and set base_time to 50 minutes ago
         left_history.clear();
         right_history.clear();

         // Set base_time to the start of our simulated history
         let start_time = now - StdDuration::from_secs(3000);
         left_history.base_time = start_time;
         right_history.base_time = start_time;

         // Push 6 samples, each 10 minutes apart
         for i in 0..6 {
            let time = start_time + StdDuration::from_secs(i * 600);
            let left_level = (60 - (i * 2)) as u8; // From 60 to 50
            let right_level = (70 - (i * 2)) as u8; // From 70 to 60
            left_history.push(time, left_level);
            right_history.push(time, right_level);
         }
      }

      // Estimate TTL
      let ttl = airpods.estimate_battery_ttl();
      assert!(ttl.is_some());

      // With 50% battery and ~12% per hour drain, should be around 250 minutes (4.16 hours)
      let ttl_value = ttl.unwrap();
      assert!(
         ttl_value > 200 && ttl_value < 300,
         "TTL was: {ttl_value} minutes"
      );
   }

   #[test]
   fn test_no_ttl_when_charging() {
      let airpods = AirPods::new(Address::any(), "Test AirPods".to_string());

      // Set battery with one bud charging
      let battery = BatteryInfo {
         left: BatteryState {
            level: 50,
            status: BatteryStatus::Charging,
         },
         right: BatteryState {
            level: 60,
            status: BatteryStatus::Normal,
         },
         case: BatteryState {
            level: 80,
            status: BatteryStatus::Normal,
         },
      };
      airpods.update_battery_info(battery);

      // Should return None when charging
      assert!(airpods.estimate_battery_ttl().is_none());
   }

   #[test]
   fn test_no_ttl_with_insufficient_data() {
      let airpods = AirPods::new(Address::any(), "Test AirPods".to_string());

      // Set battery levels
      let battery = BatteryInfo {
         left: BatteryState {
            level: 50,
            status: BatteryStatus::Normal,
         },
         right: BatteryState {
            level: 60,
            status: BatteryStatus::Normal,
         },
         case: BatteryState {
            level: 80,
            status: BatteryStatus::Normal,
         },
      };
      airpods.update_battery_info(battery);

      // Add only 3 samples (less than MIN_SAMPLES)
      let now = Instant::now();
      {
         let mut history = airpods.0.battery_history.lock();
         let [left_history, _] = &mut *history;
         for i in 0..3 {
            left_history.push(now - StdDuration::from_secs(i * 600), 50 - i as u8);
         }
      }

      // Should return None with insufficient data
      assert!(airpods.estimate_battery_ttl().is_none());
   }
}
