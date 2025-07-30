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
mod bluetooth;
mod config;
mod dbus;
mod error;
mod event;

use crate::{airpods::device::AirPods, dbus::AirPodsServiceSignals, error::Result};

#[tokio::main]
async fn main() -> Result<()> {
   env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

   info!("Starting kAirPods D-Bus service...");

   // Load configuration
   let config = config::Config::load()?;
   info!(
      "Loaded configuration with {} known devices",
      config.known_devices.len()
   );

   // Create event channel
   let event_bus = EventProcessor::new();

   // Create Bluetooth manager with event sender and config
   let bluetooth_manager = BluetoothManager::new(event_bus.clone(), config).await?;

   // Create D-Bus service
   let service = AirPodsService::new(bluetooth_manager);

   // Build D-Bus connection
   let connection = connection::Builder::session()?
      .name("org.kairpods")?
      .serve_at("/org/kairpods/manager", service)?
      .build()
      .await?;

   info!("kAirPods D-Bus service started at org.kairpods");

   // Start event processor
   event_bus.spawn_dispatcher(connection).await?;

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
   ) -> Result<()> {
      let addr_str = device.address_str();
      match event {
         AirPodsEvent::DeviceConnected => {
            iface.device_connected(addr_str).await?;
         },
         AirPodsEvent::DeviceDisconnected => {
            iface.device_disconnected(addr_str).await?;
         },
         AirPodsEvent::BatteryUpdated(battery) => {
            iface
               .battery_updated(addr_str, &battery.to_json().to_string())
               .await?;
         },
         AirPodsEvent::NoiseControlChanged(mode) => {
            iface.noise_control_changed(addr_str, mode.to_str()).await?;
         },
         AirPodsEvent::EarDetectionChanged(ear_detection) => {
            iface
               .ear_detection_changed(addr_str, &ear_detection.to_json().to_string())
               .await?;
         },
         AirPodsEvent::DeviceNameChanged(name) => {
            iface.device_name_changed(addr_str, &name).await?;
         },
         AirPodsEvent::DeviceError => {
            iface.device_error(addr_str).await?;
         },
      }
      Ok(())
   }

   async fn spawn_dispatcher(self: Arc<Self>, connection: Connection) -> Result<()> {
      let iface = connection
         .object_server()
         .interface::<_, AirPodsService>("/org/kairpods/manager")
         .await?;
      tokio::spawn(async move {
         while let Some(event) = self.recv().await {
            if let Err(e) = self.dispatch(&iface, event).await {
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
