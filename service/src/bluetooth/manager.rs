//! Bluetooth device manager for AirPods.
//!
//! This module handles Bluetooth adapter management, device discovery,
//! and connection lifecycle for AirPods devices.

use std::{
   collections::{HashMap, HashSet},
   time::Duration,
};

use bluer::{Adapter, AdapterEvent, Address, Session};
use futures::stream::StreamExt;
use log::{debug, error, info, warn};
use tokio::{
   select,
   sync::{mpsc, oneshot},
   task::JoinHandle,
   time,
};

use crate::{
   airpods::device::AirPods,
   config::Config,
   error::{AirPodsError, Result},
   event::{AirPodsEvent, EventSender},
};

/// Device name patterns to identify AirPods and compatible devices
const AIRPOD_PATTERNS: &[&str] = &["AirPods", "Beats", "Powerbeats"];
/// Delay before retrying adapter operations after failure
const ADAPTER_RECOVERY_DELAY: Duration = Duration::from_secs(5);
/// Maximum time to wait for a device connection
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

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
}

// === Device Management ===

#[derive(Debug, Clone, PartialEq)]
enum DeviceConnectionState {
   Discovered,
   Connecting,
   Connected,
   Disconnecting,
   Disconnected,
   Failed(String),
   WaitingToReconnect,
}

struct ManagedDevice {
   device: AirPods,
   state: DeviceConnectionState,
   adapter_name: String,
   retry_count: u32,
   last_error: Option<String>,
}

// === Commands ===

#[derive(Debug)]
enum ManagerCommand {
   // Adapter events
   AdapterAvailable(String, Adapter),
   AdapterLost(String),
   AdapterError(String, String), // adapter_name, error

   // Device events
   DeviceDiscovered(Address, String), // address, adapter_name
   DeviceConnected(Address),
   DeviceDisconnected(Address, bool), // address, is_error
   DeviceLost(Address),

   // User commands
   ConnectDevice(Address, Option<oneshot::Sender<Result<()>>>),
   DisconnectDevice(Address, Option<oneshot::Sender<Result<()>>>),
   GetDeviceState(Address, oneshot::Sender<Option<AirPods>>),
   GetAllDeviceStates(oneshot::Sender<Vec<AirPods>>),
   CountDevices(oneshot::Sender<u32>),
}

// === Main Manager ===

/// Main Bluetooth manager that handles device discovery and connections.
///
/// This type provides a high-level interface for managing AirPods devices
/// across all available Bluetooth adapters.
pub struct BluetoothManager {
   inbox: mpsc::Sender<ManagerCommand>,
}

impl BluetoothManager {
   pub async fn new(event_tx: EventSender, config: Config) -> Result<Self> {
      let (command_tx, command_rx) = mpsc::channel(100);
      tokio::spawn(ManagerActor::new(config, event_tx, command_rx).await.run());
      Ok(Self { inbox: command_tx })
   }

   pub async fn connect_device(&self, address: Address) -> Result<()> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::ConnectDevice(address, Some(tx)))
         .await
         .map_err(|_| AirPodsError::ManagerShutdown)?;
      rx.await.map_err(|_| AirPodsError::ManagerShutdown)?
   }

   pub async fn disconnect_device(&self, address: Address) -> Result<()> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::DisconnectDevice(address, Some(tx)))
         .await
         .map_err(|_| AirPodsError::ManagerShutdown)?;
      rx.await.map_err(|_| AirPodsError::ManagerShutdown)?
   }

   pub async fn get_device(&self, address: Address) -> Option<AirPods> {
      let (tx, rx) = oneshot::channel();
      self
         .inbox
         .send(ManagerCommand::GetDeviceState(address, tx))
         .await
         .ok()?;
      rx.await.ok()?
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

   // State
   adapters: HashMap<String, AdapterInfo>,
   devices: HashMap<Address, ManagedDevice>,
   connecting_devices: HashSet<Address>, // Prevent duplicate connections

   // Task handles
   discovery_handle: Option<JoinHandle<()>>,
   reconnect_handle: Option<JoinHandle<()>>,
}

impl ManagerActor {
   async fn new(
      config: Config,
      event_tx: EventSender,
      command_rx: mpsc::Receiver<ManagerCommand>,
   ) -> Self {
      let session = Session::new()
         .await
         .expect("Failed to create Bluetooth session");

      let (loopback_tx, loopback_rx) = mpsc::channel(100);
      Self {
         config,
         event_tx,
         command_rx,
         loopback_rx,
         loopback_tx,
         session,
         adapters: HashMap::new(),
         devices: HashMap::new(),
         connecting_devices: HashSet::new(),
         discovery_handle: None,
         reconnect_handle: None,
      }
   }

   async fn run(mut self) {
      info!("Bluetooth manager starting up");

      // Initialize adapters
      self.initialize_adapters().await;

      // Main event loop
      loop {
         select! {
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
               self.initialize_adapter(name).await;
            }
         },
         Err(e) => {
            error!("Failed to get adapter names: {e}");
         },
      }

      // If no adapters found, try default
      if self.adapters.is_empty() {
         self.initialize_adapter("hci0".to_string()).await;
      }
   }

   async fn initialize_adapter(&mut self, name: String) {
      match self.session.adapter(&name) {
         Ok(adapter) => {
            info!("Initializing adapter: {name}");

            // Start monitoring this adapter
            let monitor_handle = self.start_adapter_monitor(name.clone(), adapter.clone());

            self.adapters.insert(
               name.clone(),
               AdapterInfo {
                  adapter,
                  state: AdapterState::Active,
                  monitor_handle: Some(monitor_handle),
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

   fn start_adapter_monitor(&self, name: String, adapter: Adapter) -> JoinHandle<()> {
      let loopback = self.loopback_tx.clone();
      tokio::spawn(async move {
         let Ok(mut events) = adapter.events().await else {
            let _ = loopback
               .send(ManagerCommand::AdapterError(
                  name.clone(),
                  "Failed to get adapter events".to_string(),
               ))
               .await;
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
               _ => {},
            }
         }

         // If we exit the event loop, adapter is probably gone
         let _ = loopback.send(ManagerCommand::AdapterLost(name)).await;
      })
   }

   async fn check_connected_devices(&mut self, adapter_name: &str) {
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
         {
            let _ = self
               .loopback_tx
               .send(ManagerCommand::DeviceDiscovered(
                  addr,
                  adapter_name.to_string(),
               ))
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

      // Check name patterns
      if let Ok(Some(name)) = device.name().await {
         return AIRPOD_PATTERNS.iter().any(|pat| name.contains(pat));
      }

      false
   }

   async fn handle_command(&mut self, cmd: ManagerCommand) -> bool {
      match cmd {
         ManagerCommand::AdapterAvailable(name, adapter) => {
            self.handle_adapter_available(name, adapter).await;
         },
         ManagerCommand::AdapterLost(name) => {
            self.handle_adapter_lost(name).await;
         },
         ManagerCommand::AdapterError(name, error) => {
            self.handle_adapter_error(name, error).await;
         },
         ManagerCommand::DeviceDiscovered(addr, adapter_name) => {
            self.handle_device_discovered(addr, adapter_name).await;
         },
         ManagerCommand::DeviceConnected(addr) => {
            self.handle_device_connected(addr).await;
         },
         ManagerCommand::DeviceDisconnected(addr, is_error) => {
            self.handle_device_disconnected(addr, is_error).await;
         },
         ManagerCommand::DeviceLost(addr) => {
            self.handle_device_lost(addr).await;
         },
         ManagerCommand::ConnectDevice(addr, reply) => {
            let result = self.connect_device(addr).await;
            if let Some(reply) = reply {
               let _ = reply.send(result);
            }
         },
         ManagerCommand::DisconnectDevice(addr, reply) => {
            let result = self.disconnect_device(addr).await;
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

   async fn handle_adapter_available(&mut self, name: String, adapter: Adapter) {
      info!("Adapter available: {name}");

      if let Some(info) = self.adapters.get_mut(&name) {
         info.adapter = adapter;
         info.state = AdapterState::Active;
      } else {
         self.initialize_adapter(name).await;
      }
   }

   async fn handle_adapter_lost(&mut self, name: String) {
      warn!("Adapter lost: {name}");

      if let Some(info) = self.adapters.get_mut(&name) {
         info.state = AdapterState::Lost;

         // Mark all devices on this adapter as lost
         for device in self.devices.values_mut() {
            if device.adapter_name == name {
               device.state = DeviceConnectionState::Failed("Adapter lost".to_string());
               self
                  .event_tx
                  .emit(&device.device, AirPodsEvent::DeviceError);
            }
         }
      }

      // Schedule adapter recovery
      let loopback = self.loopback_tx.clone();
      let session = self.session.clone();
      tokio::spawn(async move {
         tokio::time::sleep(ADAPTER_RECOVERY_DELAY).await;

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

   async fn handle_adapter_error(&mut self, name: String, error: String) {
      error!("Adapter error on {name}: {error}");

      if let Some(info) = self.adapters.get_mut(&name) {
         info.state = AdapterState::Failed(error);
      }
   }

   async fn handle_device_discovered(&mut self, addr: Address, adapter_name: String) {
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

      let name = device
         .name()
         .await
         .ok()
         .flatten()
         .unwrap_or_else(|| addr.to_string());
      info!("Discovered AirPods: {name} ({addr})");

      // Create managed device
      let airpods = AirPods::new(addr, name);
      let managed = ManagedDevice {
         device: airpods,
         state: DeviceConnectionState::Discovered,
         adapter_name,
         retry_count: 0,
         last_error: None,
      };

      self.devices.insert(addr, managed);

      // Auto-connect if configured
      if self.config.is_known_device(&addr.to_string()).is_some() {
         let _ = self.connect_device(addr).await;
      }
   }

   async fn handle_device_connected(&mut self, addr: Address) {
      if let Some(device) = self.devices.get_mut(&addr) {
         device.state = DeviceConnectionState::Connected;
         device.retry_count = 0;
         device.last_error = None;

         self
            .event_tx
            .emit(&device.device, AirPodsEvent::DeviceConnected);
      }

      self.connecting_devices.remove(&addr);
   }

   async fn handle_device_disconnected(&mut self, addr: Address, is_error: bool) {
      if let Some(device) = self.devices.get_mut(&addr) {
         if is_error && device.retry_count < self.config.connection_retry_count {
            device.state = DeviceConnectionState::WaitingToReconnect;
            device.retry_count += 1;

            // Schedule reconnection
            let loopback = self.loopback_tx.clone();
            let delay = Duration::from_secs(2u64.pow(device.retry_count)); // Exponential backoff

            tokio::spawn(async move {
               tokio::time::sleep(delay).await;
               let _ = loopback
                  .send(ManagerCommand::ConnectDevice(addr, None))
                  .await;
            });
         } else {
            device.state = DeviceConnectionState::Disconnected;
            self
               .event_tx
               .emit(&device.device, AirPodsEvent::DeviceDisconnected);
         }
      }

      self.connecting_devices.remove(&addr);
   }

   async fn handle_device_lost(&mut self, addr: Address) {
      if let Some(device) = self.devices.remove(&addr) {
         self
            .event_tx
            .emit(&device.device, AirPodsEvent::DeviceDisconnected);
      }
      self.connecting_devices.remove(&addr);
   }

   async fn connect_device(&mut self, addr: Address) -> Result<()> {
      // Check if already connecting
      if self.connecting_devices.contains(&addr) {
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

      // Mark as connecting
      self.connecting_devices.insert(addr);
      device.state = DeviceConnectionState::Connecting;

      // Spawn connection task
      let airpods = device.device.clone();
      let event_tx = self.event_tx.clone();
      let loopback = self.loopback_tx.clone();

      tokio::spawn(async move {
         let err = match time::timeout(CONNECTION_TIMEOUT, airpods.connect(&event_tx)).await {
            Ok(Err(e)) => Some(e),
            Err(_) => Some(AirPodsError::RequestTimeout),
            Ok(Ok(jhandle)) => {
               if let Err(e) = loopback.send(ManagerCommand::DeviceConnected(addr)).await {
                  warn!("Failed to send device connected event: {e:?}");
                  return;
               }

               let err = match jhandle.await {
                  Ok(x) => x,
                  Err(x) => Some(AirPodsError::ActorPanicked(x)),
               };

               if let Some(err) = &err {
                  warn!("Connection to device {addr} terminated: {err:?}");
               } else {
                  info!("Connection to device {addr} closed");
               }
               err
            },
         };
         let _ = loopback
            .send(ManagerCommand::DeviceDisconnected(addr, err.is_some()))
            .await;
      });

      Ok(())
   }

   async fn disconnect_device(&mut self, addr: Address) -> Result<()> {
      let device = self
         .devices
         .get_mut(&addr)
         .ok_or(AirPodsError::DeviceNotFound(addr))?;

      device.state = DeviceConnectionState::Disconnecting;
      device.device.disconnect().await;
      device.state = DeviceConnectionState::Disconnected;

      self.connecting_devices.remove(&addr);
      self
         .event_tx
         .emit(&device.device, AirPodsEvent::DeviceDisconnected);

      Ok(())
   }

   async fn cleanup(&mut self) {
      info!("Cleaning up Bluetooth manager");

      // Cancel all tasks
      if let Some(handle) = self.discovery_handle.take() {
         handle.abort();
      }
      if let Some(handle) = self.reconnect_handle.take() {
         handle.abort();
      }

      // Abort adapter monitors
      for info in self.adapters.values_mut() {
         if let Some(handle) = info.monitor_handle.take() {
            handle.abort();
         }
      }

      // Disconnect all devices
      for device in self.devices.values() {
         device.device.disconnect().await;
      }
   }
}
