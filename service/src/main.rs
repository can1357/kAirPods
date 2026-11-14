//! `AirPods` D-Bus Service for KDE Plasma
//!
//! This service provides a D-Bus interface for managing `AirPods` devices
//! in KDE Plasma, including battery monitoring, noise control, and
//! feature management.

use std::{sync::Arc, time::Duration};

use crossbeam::queue::SegQueue;
use log::{info, warn};
use tokio::{signal, sync::Notify, time};
use zbus::{Connection, connection, object_server::InterfaceRef};

use bluetooth::manager::BluetoothManager;
use dbus::AirPodsService;
use event::{AirPodsEvent, EventBus};

mod airpods;
mod battery_study;
mod bluetooth;
mod config;
mod dbus;
mod error;
mod event;
mod media_control;
mod ringbuf;
mod battery_provider;

use battery_provider::BluezBatteryManager;

use crate::{airpods::device::AirPods, dbus::AirPodsServiceSignals, error::Result};

#[tokio::main]
async fn main() -> Result<()> {
   // Parse command line arguments
   let args: Vec<String> = std::env::args().collect();
   if args.len() > 1 {
      match args[1].as_str() {
         "--version" | "-v" => {
            println!("kairpodsd {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
         },
         "--help" | "-h" => {
            println!("Usage: {} [OPTIONS]", args[0]);
            println!();
            println!("Options:");
            println!("  -v, --version    Print version information and exit");
            println!("  -h, --help       Print this help message and exit");
            return Ok(());
         },
         arg => {
            eprintln!("Unknown argument: {arg}");
            eprintln!("Try '{} --help' for more information.", args[0]);
            std::process::exit(1);
         },
      }
   }

   let (config, config_err) = match config::Config::load() {
      Ok(config) => (config, None),
      Err(e) => (config::Config::default(), Some(e)),
   };

   let default_filter = config.log_filter.as_deref().unwrap_or("info");
   env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_filter))
      .init();
   info!("Starting kAirPods D-Bus service...");

   if let Some(err) = config_err {
      warn!("Failed to load configuration: {err:?}");
   } else {
      info!(
         "Loaded configuration with {} known devices",
         config.known_devices.len()
      );
   }

   // Create event channel
   let event_bus = EventProcessor::new();

   // Initialize battery study database
   let battery_study = match battery_study::BatteryStudy::open() {
      Ok(study) => {
         info!("Battery study database initialized");
         Some(study)
      },
      Err(e) => {
         warn!("Failed to initialize battery study database: {e}");
         None
      },
   };

   // Create Bluetooth manager with event sender and config
   let bluetooth_manager = BluetoothManager::new(event_bus.clone(), config.clone(), battery_study).await?;

   // Create D-Bus service
   let service = AirPodsService::new(bluetooth_manager);

   // Build D-Bus connection
   let connection = connection::Builder::session()?
      .name("org.kairpods")?
      .serve_at("/org/kairpods/manager", service)?
      .build()
      .await?;

   info!("kAirPods D-Bus service started at org.kairpods");

   // Initialize BlueZ system battery provider if enabled in configuration
   let bluez_battery = if config.export_bluez_battery {
      match connection::Builder::system() {
         Ok(builder) => match builder.build().await {
            Ok(sys_conn) => match BluezBatteryManager::new(sys_conn).await {
               Ok(mgr) => Some(mgr),
               Err(e) => {
                  warn!("Failed to initialize BlueZ battery manager: {e}");
                  None
               },
            },
            Err(e) => {
               warn!("Failed to connect to system D-Bus: {e}");
               None
            },
         },
         Err(e) => {
            warn!("Failed to create system D-Bus connection builder: {e}");
            None
         },
      }
   } else {
      info!("BlueZ battery provider export disabled by configuration");
      None
   };

   // Start event processor
   event_bus.spawn_dispatcher(connection, bluez_battery).await?;

   // Wait for shutdown signal
   signal::ctrl_c().await?;
   info!("Shutting down kAirPods service...");

   Ok(())
}

struct EventProcessor {
   queue: SegQueue<(AirPods, AirPodsEvent)>,
   notifier: Notify,
}

impl EventProcessor {
   fn new() -> Arc<Self> {
      Arc::new(Self {
         queue: SegQueue::new(),
         notifier: Notify::new(),
      })
   }
}

impl EventProcessor {
   async fn recv(self: &Arc<Self>) -> Option<(AirPods, AirPodsEvent)> {
      loop {
         if let Some(event) = self.queue.pop() {
            return Some(event);
         }
         let notify = self.notifier.notified();
         if let Some(event) = self.queue.pop() {
            return Some(event);
         }
         if Arc::strong_count(self) == 1 {
            return None;
         }
         let _ = time::timeout(Duration::from_secs(1), notify).await;
      }
   }

   async fn dispatch(
      &self,
      iface: &InterfaceRef<AirPodsService>,
      (device, event): (AirPods, AirPodsEvent),
      bluez_battery: Option<&BluezBatteryManager>,
   ) -> Result<()> {
      let addr_str = device.address_str();
      match event {
         AirPodsEvent::DeviceConnected => {
            iface.device_connected(addr_str).await?;
            if let (Some(mgr), Some(battery)) = (bluez_battery, device.battery_info()) {
               if let Some(level) = battery.aggregate_percentage() {
                  let _ = mgr.update_device_battery(addr_str, level).await;
               }
            }
            // Emit property changes
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
            iface
               .get_mut()
               .await
               .connected_count_changed(iface.signal_emitter())
               .await?;
         },
         AirPodsEvent::DeviceDisconnected => {
            if let Some(mgr) = bluez_battery {
               mgr.remove_device(addr_str).await;
            }
            iface.device_disconnected(addr_str).await?;
            // Emit property changes
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
            iface
               .get_mut()
               .await
               .connected_count_changed(iface.signal_emitter())
               .await?;
         },
         AirPodsEvent::BatteryUpdated(battery) => {
            if let Some(mgr) = bluez_battery {
               if let Some(level) = battery.aggregate_percentage() {
                  let _ = mgr.update_device_battery(addr_str, level).await;
               }
            }
            iface
               .battery_updated(addr_str, &battery.to_json().to_string())
               .await?;
            // Emit property change for devices (battery state changed)
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
         },
         AirPodsEvent::NoiseControlChanged(mode) => {
            iface.noise_control_changed(addr_str, mode.to_str()).await?;
            // Emit property change for devices (noise control state changed)
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
         },
         AirPodsEvent::EarDetectionChanged(ear_detection) => {
            iface
               .ear_detection_changed(addr_str, &ear_detection.to_json().to_string())
               .await?;
            // Emit property change for devices (ear detection state changed)
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;

            // Handle play/pause based on ear detection
            // Pause when at least one earbud is removed, play only when both are in
            let both_in_ear = ear_detection.is_left_in_ear() && ear_detection.is_right_in_ear();
            if both_in_ear {
               // Both AirPods are in ear - send play command
               media_control::send_play().await;
            } else {
               // At least one AirPod is out of ear - send pause command
               media_control::send_pause().await;
            }
         },
         AirPodsEvent::DeviceNameChanged(name) => {
            iface.device_name_changed(addr_str, &name).await?;
            // Emit property change for devices (name changed)
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
         },
         AirPodsEvent::DeviceError => {
            iface.device_error(addr_str).await?;
            // Emit property change for devices (error state might affect device info)
            iface
               .get_mut()
               .await
               .devices_changed(iface.signal_emitter())
               .await?;
         },
      }
      Ok(())
   }

   async fn spawn_dispatcher(
      self: Arc<Self>,
      connection: Connection,
      bluez_battery: Option<Arc<BluezBatteryManager>>,
   ) -> Result<()> {
      let iface = connection
         .object_server()
         .interface::<_, AirPodsService>("/org/kairpods/manager")
         .await?;
      tokio::spawn(async move {
         while let Some(event) = self.recv().await {
            if let Err(e) = self.dispatch(&iface, event, bluez_battery.as_deref()).await {
               warn!("Error dispatching event: {e}");
            }
         }
      });

      Ok(())
   }
}

impl EventBus for EventProcessor {
   fn emit(&self, device: &AirPods, event: AirPodsEvent) {
      self.queue.push((device.clone(), event));
      self.notifier.notify_waiters();
   }
}
