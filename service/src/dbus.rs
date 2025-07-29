use std::{collections::HashMap, str::FromStr};

use bluer::Address;
use log::info;
use zbus::{interface, object_server::SignalEmitter, zvariant};

use crate::{airpods::protocol::NoiseControlMode, bluetooth::manager::BluetoothManager};

pub struct AirPodsService {
   bluetooth_manager: BluetoothManager,
}

impl AirPodsService {
   pub const fn new(bluetooth_manager: BluetoothManager) -> Self {
      Self { bluetooth_manager }
   }
}

#[interface(name = "org.kde.plasma.airpods")]
impl AirPodsService {
   async fn get_devices(&self) -> zbus::fdo::Result<String> {
      let states: Vec<serde_json::Value> = self
         .bluetooth_manager
         .all_devices()
         .await
         .into_iter()
         .map(|d| d.to_json())
         .collect();
      Ok(serde_json::to_string(&states).unwrap())
   }

   async fn get_device(&self, address: String) -> zbus::fdo::Result<String> {
      let addr =
         Address::from_str(&address).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      let dev = self
         .bluetooth_manager
         .get_device(addr)
         .await
         .ok_or_else(|| zbus::fdo::Error::Failed("Device not found".into()))?;
      Ok(dev.to_json().to_string())
   }

   async fn passthrough(&self, address: String, packet: String) -> zbus::fdo::Result<bool> {
      let addr =
         Address::from_str(&address).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      let dev = self
         .bluetooth_manager
         .get_device(addr)
         .await
         .ok_or_else(|| zbus::fdo::Error::Failed("Device not found".into()))?;

      let packet = hex::decode(packet).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      dev.passthrough(&packet)
         .await
         .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

      Ok(true)
   }

   async fn send_command(
      &self,
      address: String,
      action: String,
      params: HashMap<String, zvariant::Value<'_>>,
   ) -> zbus::fdo::Result<bool> {
      let addr =
         Address::from_str(&address).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      let dev = self
         .bluetooth_manager
         .get_device(addr)
         .await
         .ok_or_else(|| zbus::fdo::Error::Failed("Device not found".into()))?;

      match action.as_str() {
         "set_noise_mode" => {
            let mode_str = params
               .get("value")
               .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Missing 'value' parameter".into()))?
               .downcast_ref::<String>()
               .map_err(|e| {
                  zbus::fdo::Error::InvalidArgs(format!("Invalid 'value' parameter: {e}"))
               })?;

            let mode = NoiseControlMode::from_str(mode_str.as_str()).ok_or_else(|| {
               zbus::fdo::Error::InvalidArgs(format!("Invalid noise mode: {mode_str}"))
            })?;

            dev.set_noise_control(mode)
               .await
               .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

            info!("Set noise mode to {mode} for {address}");
         },

         "set_feature" => {
            let feature = params
               .get("feature")
               .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Missing 'feature' parameter".into()))?
               .downcast_ref::<String>()
               .map_err(|e| {
                  zbus::fdo::Error::InvalidArgs(format!("Invalid 'feature' parameter: {e}"))
               })?;

            let enabled = params
               .get("enabled")
               .ok_or_else(|| zbus::fdo::Error::InvalidArgs("Missing 'enabled' parameter".into()))?
               .downcast_ref::<bool>()
               .map_err(|e| {
                  zbus::fdo::Error::InvalidArgs(format!(
                     "Invalid 'enabled' value for feature: {feature}: {e}"
                  ))
               })?;

            dev.set_feature(feature.as_str(), enabled)
               .await
               .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

            info!("Set feature {feature} to {enabled} for {address}");
         },

         _ => {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
               "Unknown action: {action}"
            )));
         },
      }

      Ok(true)
   }

   async fn connect_device(&self, address: String) -> zbus::fdo::Result<bool> {
      let addr =
         Address::from_str(&address).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      self
         .bluetooth_manager
         .connect_device(addr)
         .await
         .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

      Ok(true)
   }

   async fn disconnect_device(&self, address: String) -> zbus::fdo::Result<bool> {
      let addr =
         Address::from_str(&address).map_err(|e| zbus::fdo::Error::InvalidArgs(e.to_string()))?;

      self
         .bluetooth_manager
         .disconnect_device(addr)
         .await
         .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))?;

      Ok(true)
   }

   // Signals
   #[zbus(signal)]
   pub async fn device_connected(emitter: &SignalEmitter<'_>, address: &str) -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn device_disconnected(emitter: &SignalEmitter<'_>, address: &str)
   -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn battery_updated(
      emitter: &SignalEmitter<'_>,
      address: &str,
      battery: &str,
   ) -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn noise_control_changed(
      emitter: &SignalEmitter<'_>,
      address: &str,
      mode: &str,
   ) -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn ear_detection_changed(
      emitter: &SignalEmitter<'_>,
      address: &str,
      ear_detection: &str,
   ) -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn device_name_changed(
      emitter: &SignalEmitter<'_>,
      address: &str,
      name: &str,
   ) -> zbus::Result<()>;

   #[zbus(signal)]
   pub async fn device_error(emitter: &SignalEmitter<'_>, address: &str) -> zbus::Result<()>;

   // Properties for polling-free updates
   #[zbus(property)]
   async fn devices(&self) -> String {
      self.get_devices().await.unwrap_or_default()
   }

   #[zbus(property)]
   async fn connected_count(&self) -> u32 {
      self.bluetooth_manager.count_devices().await
   }
}
