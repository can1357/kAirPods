//! Bluetooth device manager for `AirPods`.
//!
//! This module handles Bluetooth adapter management, device discovery,
//! and connection lifecycle for `AirPods` devices.

use std::{
   collections::{HashMap, HashSet},
   time::Duration,
};

use bluer::{Adapter, AdapterEvent, Address, Session};
use futures::stream::StreamExt;
use log::{debug, error, info, warn};
use smol_str::SmolStr;
use tokio::{
   select,
   sync::{mpsc, oneshot},
   task::JoinHandle,
   time::{self, MissedTickBehavior},
};

use crate::{
   airpods::{self, device::AirPods},
   battery_study::BatteryStudy,
   config::Config,
   error::{AirPodsError, Result},
   event::{AirPodsEvent, EventSender},
};
use rand::Rng;

/// Interval to poll for new devices and check connection health
const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(5);
/// Interval to check for new adapters
const ADAPTER_CHECK_INTERVAL: Duration = Duration::from_secs(10);
/// Delay before retrying adapter operations after failure
const ADAPTER_RECOVERY_DELAY: Duration = Duration::from_secs(5);
/// Maximum time to wait for AAP connection
const AAP_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
/// Maximum AAP connection retry delay
const MAX_AAP_RETRY_DELAY: Duration = Duration::from_secs(120);
/// Device tick interval
const DEVICE_TICK_INTERVAL: Duration = Duration::from_secs(10);
/// Channel buffer size
const CHANNEL_BUFFER_SIZE: usize = 1000;

// === Adapter Management ===

#[derive(Debug, Clone, PartialEq)]
enum AdapterState {
   Active,
   Lost,
   Failed(String),
}

struct AdapterInfo {
   adapter: Adapter,
   state: AdapterState,
   monitor_handle: Option<JoinHandle<()>>,
   retry_count: u32,
   name: SmolStr,
}

// === Device Management ===

#[derive(Debug, Copy, Clone, PartialEq)]
enum BluetoothState {
   Connected,
   Disconnected,
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum AAPState {
   Disconnected,
   Connecting,
   Connected,
   Failed(&'static str),
   WaitingToReconnect,
}

struct ManagedDevice {
   device: AirPods,
   bluetooth_state: BluetoothState,
   aap_state: AAPState,
   adapter_name: SmolStr,
   aap_retry_count: u32,
   last_aap_error: Option<String>,
   aap_handle: Option<JoinHandle<()>>,
}

// === Commands ===

#[derive(Debug)]
enum ManagerCommand {
   // Adapter events
   AdapterAvailable(SmolStr, Adapter),
   AdapterLost(SmolStr),
   AdapterError(SmolStr, String), // adapter_name, error

   // Device events
   DeviceDiscovered(Address, SmolStr), // address, adapter_name
   BluetoothConnected(Address),
   BluetoothDisconnected(Address),
   AAPConnected(Address),
   AAPDisconnected(Address, bool), // address, is_error
   DeviceLost(Address),

   // User commands
   EstablishAAP(Address, Option<oneshot::Sender<Result<()>>>),
   DisconnectAAP(Address, Option<oneshot::Sender<Result<()>>>),
   GetDeviceState(Address, oneshot::Sender<Option<AirPods>>),
   GetAllDeviceStates(oneshot::Sender<Vec<AirPods>>),
   CountDevices(oneshot::Sender<u32>),
}

// === Main Manager ===

/// Main Bluetooth manager that handles device discovery and connections.
///
/// This type provides a high-level interface for managing `AirPods` devices
/// across all available Bluetooth adapters.
pub struct BluetoothManager {
   inbox: mpsc::Sender<ManagerCommand>,
}

impl BluetoothManager {
   pub async fn new(
      event_tx: EventSender,
      config: Config,
      battery_study: Option<BatteryStudy>,
   ) -> Result<Self> {
      let (command_tx, command_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
      tokio::spawn(
         ManagerActor::new(config, event_tx, command_rx, battery_study)
            .await
            .run(),
      );
      Ok(Self { inbox: command_tx })
   }

   pub async fn establish_aap(&self, address: Address) -> Result<()> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::EstablishAAP(address, Some(tx)))
         .await
         .map_err(|_| AirPodsError::ManagerShutdown)?;
      rx.await.map_err(|_| AirPodsError::ManagerShutdown)?
   }

   pub async fn disconnect_aap(&self, address: Address) -> Result<()> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::DisconnectAAP(address, Some(tx)))
         .await
         .map_err(|_| AirPodsError::ManagerShutdown)?;
      rx.await.map_err(|_| AirPodsError::ManagerShutdown)?
   }

   pub async fn get_device(&self, address: Address) -> Result<AirPods> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::GetDeviceState(address, tx))
         .await
         .map_err(|_| AirPodsError::DeviceNotFound(address))?;

      rx.await
         .ok()
         .flatten()
         .ok_or(AirPodsError::DeviceNotFound(address))
   }

   pub async fn all_devices(&self) -> Vec<AirPods> {
      let (tx, rx) = oneshot::channel();
      if self
         .inbox
         .send(ManagerCommand::GetAllDeviceStates(tx))
         .await
         .is_err()
      {
         return Vec::new();
      }
      rx.await.unwrap_or_default()
   }

   pub async fn count_devices(&self) -> u32 {
      let (tx, rx) = oneshot::channel();
      if self
         .inbox
         .send(ManagerCommand::CountDevices(tx))
         .await
         .is_err()
      {
         return 0;
      }
      rx.await.unwrap_or_default()
   }
}

// === Manager Actor ===

struct ManagerActor {
   config: Config,
   event_tx: EventSender,
   command_rx: mpsc::Receiver<ManagerCommand>,
   loopback_rx: mpsc::Receiver<ManagerCommand>,
   loopback_tx: mpsc::Sender<ManagerCommand>,
   session: Session,
   battery_study: Option<BatteryStudy>,

   // State
   adapters: HashMap<SmolStr, AdapterInfo>,
   devices: HashMap<Address, ManagedDevice>,
   aap_connecting: HashSet<Address>, // Prevent duplicate AAP connections
}

impl ManagerActor {
   async fn new(
      config: Config,
      event_tx: EventSender,
      command_rx: mpsc::Receiver<ManagerCommand>,
      battery_study: Option<BatteryStudy>,
   ) -> Self {
      let session = Session::new()
         .await
         .expect("Failed to create Bluetooth session");

      let (loopback_tx, loopback_rx) = mpsc::channel(CHANNEL_BUFFER_SIZE);
      Self {
         config,
         event_tx,
         command_rx,
         loopback_rx,
         loopback_tx,
         session,
         battery_study,
         adapters: HashMap::new(),
         devices: HashMap::new(),
         aap_connecting: HashSet::new(),
      }
   }

   async fn run(mut self) {
      info!("Bluetooth manager starting up");

      // Initialize adapters
      self.initialize_adapters().await;

      // Start periodic checks
      let mut health_check_interval = time::interval(HEALTH_CHECK_INTERVAL);
      health_check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

      let mut adapter_check_interval = time::interval(ADAPTER_CHECK_INTERVAL);
      adapter_check_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

      let mut device_tick_interval = time::interval(DEVICE_TICK_INTERVAL);
      device_tick_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

      // Main event loop
      loop {
         select! {
             _ = health_check_interval.tick() => {
                 // Check connection health and scan for new devices
                 self.check_connection_health().await;
                 self.scan_for_connected_airpods().await;
             }
             _ = adapter_check_interval.tick() => {
                 // Check for new adapters
                 self.discover_new_adapters().await;
             }
             _ = device_tick_interval.tick() => {
                 // Tick all devices
                 self.tick_all_devices();
             }
             cmd = self.command_rx.recv() => {
                 let Some(cmd) = cmd else {
                     info!("Bluetooth manager shutting down");
                     break;
                 };
                 if !self.handle_command(cmd).await {
                     break;
                 }
             }
             Some(cmd) = self.loopback_rx.recv() => {
                 if !self.handle_command(cmd).await {
                     break;
                 }
             }
         }
      }

      // Cleanup
      self.cleanup().await;
   }

   async fn initialize_adapters(&mut self) {
      match self.session.adapter_names().await {
         Ok(names) => {
            for name in names {
               self.initialize_adapter(name.into()).await;
            }
         },
         Err(e) => {
            error!("Failed to get adapter names: {e}");
         },
      }

      // If no adapters found, try default
      if self.adapters.is_empty() {
         self.initialize_adapter(SmolStr::new_static("hci0")).await;
      }
   }

   async fn initialize_adapter(&mut self, name: SmolStr) {
      match self.session.adapter(&name) {
         Ok(adapter) => {
            info!("Initializing adapter: {name}");

            // Ensure adapter is powered on
            if let Ok(powered) = adapter.is_powered().await
               && !powered
            {
               if let Err(e) = adapter.set_powered(true).await {
                  warn!("Failed to power on adapter {name}: {e}");
                  // Schedule retry
                  let loopback = self.loopback_tx.clone();
                  let name_clone = name.clone();
                  let adapter_clone = adapter.clone();
                  tokio::spawn(async move {
                     time::sleep(ADAPTER_RECOVERY_DELAY).await;
                     let _ = loopback
                        .send(ManagerCommand::AdapterAvailable(name_clone, adapter_clone))
                        .await;
                  });
                  return;
               }
               info!("Powered on adapter: {name}");
            }

            // Start monitoring this adapter
            self.adapters.insert(
               name.clone(),
               AdapterInfo {
                  state: AdapterState::Active,
                  monitor_handle: Some(Self::start_adapter_monitor(
                     self.loopback_tx.clone(),
                     name.clone(),
                     adapter.clone(),
                  )),
                  adapter,
                  retry_count: 0,
                  name: name.clone(),
               },
            );

            // Check for already connected devices
            self.check_connected_devices(&name).await;
         },
         Err(e) => {
            warn!("Failed to initialize adapter {name}: {e}");
         },
      }
   }

   fn start_adapter_monitor(
      loopback: mpsc::Sender<ManagerCommand>,
      name: SmolStr,
      adapter: Adapter,
   ) -> JoinHandle<()> {
      tokio::spawn(async move {
         let Ok(mut events) = adapter.events().await else {
            if let Err(e) = loopback
               .send(ManagerCommand::AdapterError(
                  name.clone(),
                  "Failed to get adapter events".to_string(),
               ))
               .await
            {
               warn!("Channel overflow sending adapter error: {e}");
            }
            return;
         };

         while let Some(event) = events.next().await {
            match event {
               AdapterEvent::DeviceAdded(addr) => {
                  debug!("Device added on {name}: {addr}");
                  let _ = loopback
                     .send(ManagerCommand::DeviceDiscovered(addr, name.clone()))
                     .await;
               },
               AdapterEvent::DeviceRemoved(addr) => {
                  debug!("Device removed on {name}: {addr}");
                  let _ = loopback.send(ManagerCommand::DeviceLost(addr)).await;
               },
               // Note: bluer doesn't provide DeviceConnected/Disconnected events
               // We'll detect connection changes through periodic scanning
               _ => {},
            }
         }

         // If we exit the event loop, adapter is probably gone
         if let Err(e) = loopback.send(ManagerCommand::AdapterLost(name)).await {
            warn!("Channel overflow sending adapter lost: {e}");
         }
      })
   }

   async fn check_connected_devices(&self, adapter_name: &SmolStr) {
      let Some(adapter_info) = self.adapters.get(adapter_name) else {
         return;
      };

      let Ok(addresses) = adapter_info.adapter.device_addresses().await else {
         return;
      };

      for addr in addresses {
         if let Ok(device) = adapter_info.adapter.device(addr)
            && device.is_connected().await == Ok(true)
            && self.is_airpods_device(&device).await
            && !self.devices.contains_key(&addr)
         {
            // Found a connected AirPods device without AAP connection
            let _ = self
               .loopback_tx
               .send(ManagerCommand::DeviceDiscovered(addr, adapter_name.clone()))
               .await;
         }
      }
   }

   async fn is_airpods_device(&self, device: &bluer::Device) -> bool {
      // Check known addresses
      let addr = device.address();
      if self.config.is_known_device(&addr.to_string()).is_some() {
         return true;
      }
      airpods::recognition::is_device_airpods(device).await
   }

   async fn handle_command(&mut self, cmd: ManagerCommand) -> bool {
      match cmd {
         ManagerCommand::AdapterAvailable(name, adapter) => {
            self.handle_adapter_available(name, adapter).await;
         },
         ManagerCommand::AdapterLost(name) => {
            self.handle_adapter_lost(name);
         },
         ManagerCommand::AdapterError(name, error) => {
            self.handle_adapter_error(&name, error);
         },
         ManagerCommand::DeviceDiscovered(addr, adapter_name) => {
            self.handle_device_discovered(addr, adapter_name).await;
         },
         ManagerCommand::BluetoothConnected(addr) => {
            self.handle_bluetooth_connected(addr).await;
         },
         ManagerCommand::BluetoothDisconnected(addr) => {
            self.handle_bluetooth_disconnected(addr);
         },
         ManagerCommand::AAPConnected(addr) => {
            self.handle_aap_connected(addr);
         },
         ManagerCommand::AAPDisconnected(addr, is_error) => {
            self.handle_aap_disconnected(addr, is_error);
         },
         ManagerCommand::DeviceLost(addr) => {
            self.handle_device_lost(addr);
         },
         ManagerCommand::EstablishAAP(addr, reply) => {
            let result = self.establish_aap_connection(addr).await;
            if let Some(reply) = reply {
               let _ = reply.send(result);
            }
         },
         ManagerCommand::DisconnectAAP(addr, reply) => {
            let result = self.disconnect_aap(addr).await;
            if let Some(reply) = reply {
               let _ = reply.send(result);
            }
         },
         ManagerCommand::GetDeviceState(addr, reply) => {
            let state = self.devices.get(&addr).map(|d| d.device.clone());
            let _ = reply.send(state);
         },
         ManagerCommand::GetAllDeviceStates(reply) => {
            let states = self.devices.values().map(|d| d.device.clone()).collect();
            let _ = reply.send(states);
         },
         ManagerCommand::CountDevices(reply) => {
            let count = self.devices.len() as u32;
            let _ = reply.send(count);
         },
      }
      true
   }

   async fn handle_adapter_available(&mut self, name: SmolStr, adapter: Adapter) {
      info!("Adapter available: {name}");

      if let Some(info) = self.adapters.get_mut(&name) {
         info.adapter = adapter;
         info.state = AdapterState::Active;
         info.retry_count = 0; // Reset retry count on success

         // Restart monitor if needed
         if info.monitor_handle.is_none() {
            info.monitor_handle = Some(Self::start_adapter_monitor(
               self.loopback_tx.clone(),
               name.clone(),
               info.adapter.clone(),
            ));
         }

         // Re-check connected devices and trigger reconnects
         self.check_connected_devices(&name).await;

         // Try to reconnect failed AAP connections on this adapter
         let devices_to_reconnect: Vec<Address> = self
            .devices
            .iter()
            .filter(|(_, d)| {
               d.adapter_name == name
                  && d.bluetooth_state == BluetoothState::Connected
                  && matches!(d.aap_state, AAPState::Failed(_) | AAPState::Disconnected)
            })
            .map(|(addr, _)| *addr)
            .collect();

         for addr in devices_to_reconnect {
            let _ = self.establish_aap_connection(addr).await;
         }
      } else {
         self.initialize_adapter(name).await;
      }
   }

   fn handle_adapter_lost(&mut self, name: SmolStr) {
      warn!("Adapter lost: {name}");

      if let Some(info) = self.adapters.get_mut(&name) {
         info.state = AdapterState::Lost;
         info.retry_count += 1;

         // Abort the monitor handle
         if let Some(handle) = info.monitor_handle.take() {
            handle.abort();
         }

         // Mark all AAP connections on this adapter as failed
         for device in self.devices.values_mut() {
            if device.adapter_name == name {
               device.aap_state = AAPState::Failed("Adapter lost");
               // Abort AAP handle if it exists
               if let Some(handle) = device.aap_handle.take() {
                  handle.abort();
               }
               self
                  .event_tx
                  .emit(&device.device, AirPodsEvent::DeviceError);
            }
         }

         // Schedule adapter recovery with exponential backoff
         let loopback = self.loopback_tx.clone();
         let session = self.session.clone();
         let retry_count = info.retry_count;
         let delay = calc_retry_delay(retry_count);

         tokio::spawn(async move {
            time::sleep(delay).await;

            match session.adapter(&name) {
               Ok(adapter) => {
                  let _ = loopback
                     .send(ManagerCommand::AdapterAvailable(name, adapter))
                     .await;
               },
               Err(e) => {
                  let _ = loopback
                     .send(ManagerCommand::AdapterError(
                        name,
                        format!("Recovery failed: {e}"),
                     ))
                     .await;
               },
            }
         });
      }
   }

   fn handle_adapter_error(&mut self, name: &SmolStr, error: String) {
      error!("Adapter error on {name}: {error}");

      if let Some(info) = self.adapters.get_mut(name) {
         info.state = AdapterState::Failed(error);
      }
   }

   async fn handle_device_discovered(&mut self, addr: Address, adapter_name: SmolStr) {
      // Check if we already know about this device
      if self.devices.contains_key(&addr) {
         return;
      }

      // Verify it's an AirPods device
      let Some(adapter_info) = self.adapters.get(&adapter_name) else {
         return;
      };

      let Ok(device) = adapter_info.adapter.device(addr) else {
         return;
      };

      if !self.is_airpods_device(&device).await {
         return;
      }

      // Only proceed if already connected by bluetoothd
      if !device.is_connected().await.unwrap_or(false) {
         debug!("Discovered AirPods at {addr} but not connected by system");
         return;
      }

      let name = device
         .name()
         .await
         .ok()
         .flatten()
         .unwrap_or_else(|| addr.to_string());
      info!("Found connected AirPods: {name} ({addr})");

      // Create managed device
      let airpods = AirPods::new(addr, name, self.battery_study.clone());
      let managed = ManagedDevice {
         device: airpods,
         bluetooth_state: BluetoothState::Connected,
         aap_state: AAPState::Disconnected,
         adapter_name,
         aap_retry_count: 0,
         last_aap_error: None,
         aap_handle: None,
      };

      self.devices.insert(addr, managed);

      // Establish AAP connection for already-connected device
      let _ = self.establish_aap_connection(addr).await;
   }

   async fn handle_bluetooth_connected(&mut self, addr: Address) {
      // Check if this is an AirPods device
      let is_airpods = if let Some(device) = self.devices.get_mut(&addr) {
         device.bluetooth_state = BluetoothState::Connected;
         true
      } else {
         // Check if this is a newly connected AirPods
         for (adapter_name, adapter_info) in &self.adapters {
            if let Ok(device) = adapter_info.adapter.device(addr)
               && self.is_airpods_device(&device).await
            {
               // Discovered a new connected AirPods
               let _ = self
                  .loopback_tx
                  .send(ManagerCommand::DeviceDiscovered(addr, adapter_name.clone()))
                  .await;
               return;
            }
         }
         false
      };

      if is_airpods {
         // Automatically establish AAP connection
         let _ = self.establish_aap_connection(addr).await;
      }
   }

   fn handle_bluetooth_disconnected(&mut self, addr: Address) {
      if let Some(device) = self.devices.get_mut(&addr) {
         device.bluetooth_state = BluetoothState::Disconnected;

         // Clean up AAP connection
         if let Some(handle) = device.aap_handle.take() {
            handle.abort();
         }
         device.aap_state = AAPState::Disconnected;

         self
            .event_tx
            .emit(&device.device, AirPodsEvent::DeviceDisconnected);
      }

      self.aap_connecting.remove(&addr);
   }

   fn handle_aap_connected(&mut self, addr: Address) {
      if let Some(device) = self.devices.get_mut(&addr) {
         device.aap_state = AAPState::Connected;
         device.aap_retry_count = 0;
         device.last_aap_error = None;

         self
            .event_tx
            .emit(&device.device, AirPodsEvent::DeviceConnected);
      }

      self.aap_connecting.remove(&addr);
   }

   fn handle_aap_disconnected(&mut self, addr: Address, is_error: bool) {
      if let Some(device) = self.devices.get_mut(&addr) {
         if is_error && device.bluetooth_state == BluetoothState::Connected {
            // Only retry AAP if Bluetooth is still connected
            device.aap_state = AAPState::WaitingToReconnect;
            device.aap_retry_count += 1;

            // Schedule AAP reconnection with backoff
            let loopback = self.loopback_tx.clone();
            let delay = calc_retry_delay(device.aap_retry_count);
            info!("AAP connection to {addr} failed, retrying in {delay:?}");

            tokio::spawn(async move {
               time::sleep(delay).await;
               let _ = loopback
                  .send(ManagerCommand::EstablishAAP(addr, None))
                  .await;
            });
         } else {
            device.aap_state = AAPState::Disconnected;
            device.aap_retry_count = 0;
         }
      }

      self.aap_connecting.remove(&addr);
   }

   fn handle_device_lost(&mut self, addr: Address) {
      if let Some(device) = self.devices.remove(&addr) {
         self
            .event_tx
            .emit(&device.device, AirPodsEvent::DeviceDisconnected);
      }
      self.aap_connecting.remove(&addr);
   }

   async fn establish_aap_connection(&mut self, addr: Address) -> Result<()> {
      // Check if already connecting
      if self.aap_connecting.contains(&addr) {
         return Err(AirPodsError::AlreadyConnecting);
      }

      let device = self
         .devices
         .get_mut(&addr)
         .ok_or(AirPodsError::DeviceNotFound(addr))?;

      // Check adapter is available
      let adapter_info = self
         .adapters
         .get(&device.adapter_name)
         .ok_or(AirPodsError::AdapterNotFound)?;

      if adapter_info.state != AdapterState::Active {
         return Err(AirPodsError::AdapterNotAvailable);
      }

      // Only work with bluetooth-connected devices
      if device.bluetooth_state != BluetoothState::Connected {
         return Err(AirPodsError::DeviceNotConnected);
      }

      // Get BlueZ device to verify it's paired
      let bluer_device = adapter_info.adapter.device(addr)?;
      if !bluer_device.is_paired().await.unwrap_or(false) {
         // Clean up on early exit
         self.aap_connecting.remove(&addr);
         return Err(AirPodsError::DeviceNotPaired);
      }

      // Spawn AAP connection task
      let airpods = device.device.clone();
      let event_tx = self.event_tx.clone();
      let loopback = self.loopback_tx.clone();

      let handle = tokio::spawn(async move {
         let err = match time::timeout(AAP_CONNECTION_TIMEOUT, airpods.connect(&event_tx)).await {
            Ok(Err(e)) => {
               warn!("Failed to establish AAP connection to {addr}: {e}");
               Some(e)
            },
            Err(_) => {
               warn!("AAP connection to {addr} timed out");
               Some(AirPodsError::RequestTimeout)
            },
            Ok(Ok(jhandle)) => {
               if let Err(e) = loopback.send(ManagerCommand::AAPConnected(addr)).await {
                  warn!("Channel overflow sending AAP connected: {e}");
                  return;
               }

               let err = match jhandle.await {
                  Ok(x) => x,
                  Err(x) => Some(AirPodsError::ActorPanicked(x)),
               };

               if let Some(err) = &err {
                  warn!("AAP connection to {addr} terminated: {err:?}");
               } else {
                  info!("AAP connection to {addr} closed cleanly");
               }
               err
            },
         };
         if let Err(e) = loopback
            .send(ManagerCommand::AAPDisconnected(addr, err.is_some()))
            .await
         {
            warn!("Channel overflow sending AAP disconnected: {e}");
         }
      });

      // Track AAP handle
      device.aap_handle = Some(handle);

      // Mark as connecting only after spawn succeeds
      self.aap_connecting.insert(addr);
      device.aap_state = AAPState::Connecting;

      Ok(())
   }

   async fn disconnect_aap(&mut self, addr: Address) -> Result<()> {
      let device = self
         .devices
         .get_mut(&addr)
         .ok_or(AirPodsError::DeviceNotFound(addr))?;

      // Abort AAP connection if active
      if let Some(handle) = device.aap_handle.take() {
         handle.abort();
      }

      device.aap_state = AAPState::Disconnected;
      device.device.disconnect().await;

      self.aap_connecting.remove(&addr);
      self
         .event_tx
         .emit(&device.device, AirPodsEvent::DeviceDisconnected);

      Ok(())
   }

   async fn cleanup(&mut self) {
      use tokio::time::timeout;
      info!("Cleaning up Bluetooth manager");

      // Abort adapter monitors with timeout
      for info in self.adapters.values_mut() {
         if let Some(handle) = info.monitor_handle.take() {
            handle.abort();
            // Give it a moment to finish
            let _ = timeout(Duration::from_secs(1), handle).await;
         }
      }

      // Abort AAP handles and disconnect all devices
      for device in self.devices.values_mut() {
         if let Some(handle) = device.aap_handle.take() {
            handle.abort();
            // Give it a moment to finish
            let _ = timeout(Duration::from_secs(1), handle).await;
         }
         device.device.disconnect().await;
      }
   }

   async fn discover_new_adapters(&mut self) {
      match self.session.adapter_names().await {
         Ok(names) => {
            for name in names.into_iter().map(SmolStr::from) {
               if !self.adapters.contains_key(&name)
                  || matches!(
                     self.adapters.get(&name).map(|info| &info.state),
                     Some(AdapterState::Lost | AdapterState::Failed(_))
                  )
               {
                  self.initialize_adapter(name).await;
               }
            }
         },
         Err(e) => {
            warn!("Failed to poll adapter names: {e}. Retrying later.");
         },
      }
   }

   async fn scan_for_connected_airpods(&self) {
      for adapter_info in self.adapters.values() {
         if adapter_info.state != AdapterState::Active {
            continue;
         }

         // Check all connected devices
         if let Ok(addresses) = adapter_info.adapter.device_addresses().await {
            for addr in addresses {
               if let Ok(device) = adapter_info.adapter.device(addr)
                  && device.is_connected().await.unwrap_or(false)
                  && self.is_airpods_device(&device).await
                  && !self.has_aap_connection(addr)
               {
                  // Found connected AirPods without AAP connection
                  let _ = self
                     .loopback_tx
                     .send(ManagerCommand::DeviceDiscovered(
                        addr,
                        adapter_info.name.clone(),
                     ))
                     .await;
               }
            }
         }
      }
   }

   fn has_aap_connection(&self, addr: Address) -> bool {
      self
         .devices
         .get(&addr)
         .is_some_and(|d| matches!(d.aap_state, AAPState::Connected | AAPState::Connecting))
   }

   fn tick_all_devices(&self) {
      for device in self.devices.values() {
         device.device.tick();
      }
   }

   async fn check_connection_health(&self) {
      for (addr, device) in &self.devices {
         if let Some(adapter_info) = self.adapters.get(&device.adapter_name)
            && let Ok(bluer_device) = adapter_info.adapter.device(*addr)
         {
            let is_connected = bluer_device.is_connected().await.unwrap_or(false);

            match (device.bluetooth_state, is_connected) {
               (BluetoothState::Connected, false) => {
                  let _ = self
                     .loopback_tx
                     .send(ManagerCommand::BluetoothDisconnected(*addr))
                     .await;
               },
               (BluetoothState::Disconnected, true) => {
                  let _ = self
                     .loopback_tx
                     .send(ManagerCommand::BluetoothConnected(*addr))
                     .await;
               },
               _ => {},
            }
         }
      }
   }
}

fn calc_retry_delay(retry_count: u32) -> Duration {
   let base_delay = Duration::from_secs(2);
   let exponential = base_delay * (1 << retry_count.min(4));
   let delay = exponential.min(MAX_AAP_RETRY_DELAY);
   let jitter = rand::thread_rng().gen_range(0..1000);
   delay + Duration::from_millis(jitter)
}
