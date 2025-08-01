//! `AirPods` device implementation and state management.
//!
//! This module provides the core `AirPods` type which represents a connected
//! `AirPods` device, manages its state, and handles communication over L2CAP.

use core::fmt;
use std::{
   collections::HashMap,
   mem,
   sync::{
      Arc, Weak,
      atomic::{AtomicBool, Ordering},
   },
   time::Duration,
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
         BatteryInfo, EarDetectionStatus, FeatureBitmap, FeatureCmd, FeatureId, HDR_ACK_FEATURES,
         HDR_ACK_HANDSHAKE, HDR_BATTERY_STATE, HDR_EAR_DETECTION, HDR_METADATA, HDR_NOISE_CTL,
         NoiseControlMode, PKT_HANDSHAKE, PKT_REQUEST_NOTIFY, PKT_SET_FEATURES,
         build_control_packet,
      },
   },
   battery_study::{BatteryStudy, BatteryTracker},
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
   features: FeatureBitmap,
   features_present: FeatureBitmap,
   conn: RwLock<Option<ConnectionState>>,
   battery_tracker: parking_lot::Mutex<BatteryTracker>,
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
   pub fn new(address: Address, name: String, battery_study: Option<BatteryStudy>) -> Self {
      Self(Arc::new(AirPodsInner {
         address,
         address_str: address.to_smolstr(),
         name: parking_lot::Mutex::new(name.into()),
         battery_tracker: parking_lot::Mutex::new(BatteryTracker::new(battery_study)),
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
      self.0.features.get(feature)
   }

   pub fn features(&self) -> Vec<(FeatureId, bool)> {
      self
         .0
         .features_present
         .iter()
         .map(|feat| (feat, self.feature_enabled(feat)))
         .collect()
   }

   pub fn set_feature_enabled(&self, feature: FeatureId, enabled: bool) -> bool {
      self.0.features_present.set(feature, true);
      self.0.features.set(feature, enabled)
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

      // Initialize battery study session
      self
         .0
         .battery_tracker
         .lock()
         .init_session(self.address(), &self.name());

      info!("Successfully connected to {}", self.address());
      Ok(jhandle)
   }

   pub async fn disconnect(&self) {
      // Save battery study data before disconnecting
      self.save_battery_study();

      self.0.is_connected.store(false, Ordering::Relaxed);
      let _ = self.0.conn.write().await.take();
      info!("Disconnected from {}", self.address());
   }

   async fn notify_disconnected(&self, event_tx: &EventSender) {
      // Save battery study data before disconnecting
      self.save_battery_study();

      self.0.is_connected.store(false, Ordering::Relaxed);
      let _ = self.0.conn.write().await.take();
      info!("Disconnected from {}", self.address());
      event_tx.emit(self, AirPodsEvent::DeviceDisconnected);
   }

   async fn start_connection(
      &self,
      jset: &mut JoinSet<()>,
   ) -> Result<(L2CapReceiver, L2CapSender)> {
      async fn wait_for_ack<T>(tx: &mut oneshot::Receiver<T>) -> Result<T> {
         time::timeout(Duration::from_secs(5), tx)
            .await
            .map_err(|_| AirPodsError::RequestTimeout)?
            .map_err(|_| AirPodsError::ConnectionClosed)
      }

      let (hs_ack_tx, mut hs_ack_rx) = oneshot::channel();
      let (feat_ack_tx, mut feat_ack_rx) = oneshot::channel();

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
                const RETRY_SCHEDULE: &[Duration] = &[
                    Duration::from_secs(2),
                    Duration::from_secs(3),
                    Duration::from_secs(5),
                    Duration::from_secs(10),
                ];

                time::sleep(Duration::from_secs(1)).await;
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
      let weak = WeakAirPods::new(self);
      tokio::spawn(async move {
         let mut err = None;
         loop {
            match rx.recv().await {
               Ok(packet) => {
                  if let Some(this) = weak.upgrade() {
                     this.process_packet(addr, packet, &event_tx);
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

   fn process_packet(&self, address: Address, packet: Packet, event_tx: &EventSender) {
      // Battery status
      if packet.starts_with(HDR_BATTERY_STATE) {
         match parser::parse_battery_status(&packet) {
            Ok(battery) => {
               debug!(
                  "Battery updated for {}: L:{}% R:{}% C:{}%",
                  address, battery.left.level, battery.right.level, battery.case.level
               );

               // Send event if battery changed
               if self.update_battery_info(battery).is_updated() {
                  self
                     .0
                     .battery_tracker
                     .lock()
                     .record_battery_drop(battery.left, battery.right);
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

   /// Estimates battery time-to-live in minutes based on current levels and drain rate.
   pub fn estimate_battery_ttl(&self) -> Option<u32> {
      const DEFAULT_DRAIN_RATE: f64 = 16.9; // 16.9%/hr

      let battery = self.battery_info()?;
      let mode = self.noise_mode();

      // Get estimate from battery tracker, or use default
      self
         .0
         .battery_tracker
         .lock()
         .estimate_ttl(&battery, mode, self.address())
         .or_else(|| {
            // Default to 16.9%/hr drain rate if no estimate available
            let min_level = f64::from(battery.left.level.min(battery.right.level));
            let hours_remaining = min_level / DEFAULT_DRAIN_RATE;
            Some((hours_remaining * 60.0) as u32)
         })
   }

   /// Saves the current battery study data to the database.
   fn save_battery_study(&self) {
      let mode = self.noise_mode().unwrap_or_default();
      self
         .0
         .battery_tracker
         .lock()
         .save_to_study(self.address(), mode);
   }

   /// Checks if enough time has passed and samples collected to warrant a periodic save.
   /// Returns true if data should be saved.
   fn should_save_battery_study(&self, interval_minutes: u32) -> bool {
      if let Some(battery) = self.battery_info() {
         self
            .0
            .battery_tracker
            .lock()
            .should_save(interval_minutes, &battery)
      } else {
         false
      }
   }

   /// Performs a periodic save of battery study data if conditions are met.
   fn periodic_battery_save(&self) {
      let mode = self.noise_mode().unwrap_or_default();
      self
         .0
         .battery_tracker
         .lock()
         .save_to_study(self.address(), mode);
   }

   /// Performs all periodic tasks for the device.
   pub fn tick(&self) {
      if self.is_connected() {
         if self.should_save_battery_study(5) {
            debug!("Performing periodic battery save for {}", self.address());
            self.periodic_battery_save();
         } else {
            debug!("Battery save check for {} returned false", self.address());
         }
      } else {
         debug!("Device {} not connected, skipping tick", self.address());
      }
   }
}
