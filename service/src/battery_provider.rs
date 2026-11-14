//! Exports `AirPods` battery levels to `BlueZ` through `org.bluez.BatteryProvider1`,
//! so `UPower` and desktop battery widgets can show them.

use std::{collections::HashMap, sync::Arc};

use futures::stream::StreamExt;
use log::{debug, info, warn};
use tokio::sync::Mutex;
use zbus::{
   Connection, Result,
   fdo::{ObjectManager, ObjectManagerProxy},
   interface,
   object_server::InterfaceRef,
   zvariant::{ObjectPath, OwnedObjectPath},
};

/// Object path under which battery objects are exported and registered with `BlueZ`.
const PROVIDER_ROOT: &str = "/org/kairpods/battery_provider";
const BATTERY_PROVIDER_MANAGER: &str = "org.bluez.BatteryProviderManager1";

/// A single device battery exposed to `BlueZ`.
pub struct DeviceBatteryProvider {
   percentage: u8,
   device_path: OwnedObjectPath,
}

#[interface(name = "org.bluez.BatteryProvider1")]
impl DeviceBatteryProvider {
   #[zbus(property)]
   const fn percentage(&self) -> u8 {
      self.percentage
   }

   #[zbus(property)]
   fn device(&self) -> OwnedObjectPath {
      self.device_path.clone()
   }

   #[zbus(property)]
   const fn source(&self) -> &'static str {
      "AirPods"
   }
}

/// Registers the battery provider with every `BlueZ` adapter and keeps the exported batteries in
/// sync with connected `AirPods`.
pub struct BluezBatteryManager {
   system_conn: Connection,
   devices: Mutex<HashMap<String, InterfaceRef<DeviceBatteryProvider>>>,
}

impl BluezBatteryManager {
   /// Exports the provider root on the system bus and registers it with current and future
   /// `BlueZ` adapters.
   pub async fn new() -> Result<Arc<Self>> {
      let system_conn = Connection::system().await?;
      system_conn
         .object_server()
         .at(PROVIDER_ROOT, ObjectManager)
         .await?;

      let bluez = ObjectManagerProxy::builder(&system_conn)
         .destination("org.bluez")?
         .path("/")?
         .build()
         .await?;

      // Subscribe before listing so adapters that appear later (hotplug, bluetoothd restart)
      // are registered too.
      let mut added = bluez.receive_interfaces_added().await?;
      match bluez.get_managed_objects().await {
         Ok(objects) => {
            let mut supported = false;
            for (path, ifaces) in &objects {
               if ifaces.contains_key(BATTERY_PROVIDER_MANAGER) {
                  supported = true;
                  register_provider(&system_conn, path).await;
               }
            }
            if !supported {
               info!(
                  "BlueZ does not offer {BATTERY_PROVIDER_MANAGER} (BlueZ < 5.71 needs \
                   experimental mode); battery levels are not exported to BlueZ"
               );
            }
         },
         Err(e) => debug!("BlueZ not reachable yet, waiting for adapters: {e}"),
      }

      let conn = system_conn.clone();
      tokio::spawn(async move {
         while let Some(signal) = added.next().await {
            let Ok(args) = signal.args() else {
               continue;
            };
            if args
               .interfaces_and_properties()
               .contains_key(BATTERY_PROVIDER_MANAGER)
            {
               register_provider(&conn, args.object_path()).await;
            }
         }
      });

      Ok(Arc::new(Self {
         system_conn,
         devices: Mutex::new(HashMap::new()),
      }))
   }

   /// Resolves the `BlueZ` device object path for a Bluetooth address.
   async fn resolve_device_path(&self, addr_str: &str) -> Option<OwnedObjectPath> {
      let bluez = ObjectManagerProxy::builder(&self.system_conn)
         .destination("org.bluez")
         .ok()?
         .path("/")
         .ok()?
         .build()
         .await
         .ok()?;
      let objects = bluez.get_managed_objects().await.ok()?;
      objects.into_iter().find_map(|(path, ifaces)| {
         let addr = ifaces.get("org.bluez.Device1")?.get("Address")?;
         <&str>::try_from(addr)
            .is_ok_and(|addr| addr.eq_ignore_ascii_case(addr_str))
            .then_some(path)
      })
   }

   /// Exports or updates the battery level of the device at `addr_str`.
   pub async fn update_device_battery(&self, addr_str: &str, level: u8) {
      if let Err(e) = self.try_update_device_battery(addr_str, level).await {
         warn!("Failed to export BlueZ battery for {addr_str}: {e}");
      }
   }

   async fn try_update_device_battery(&self, addr_str: &str, level: u8) -> Result<()> {
      let mut devices = self.devices.lock().await;

      if let Some(iface_ref) = devices.get(addr_str) {
         let mut provider = iface_ref.get_mut().await;
         provider.percentage = level;
         provider
            .percentage_changed(iface_ref.signal_emitter())
            .await?;
         debug!("Updated BlueZ battery for {addr_str}: {level}%");
         return Ok(());
      }

      let Some(device_path) = self.resolve_device_path(addr_str).await else {
         debug!("No BlueZ device object for {addr_str}; battery not exported");
         return Ok(());
      };

      let provider_path = ObjectPath::try_from(format!(
         "{PROVIDER_ROOT}/dev_{}",
         addr_str.replace(':', "_")
      ))?;
      let object_server = self.system_conn.object_server();
      object_server
         .at(
            &provider_path,
            DeviceBatteryProvider {
               percentage: level,
               device_path,
            },
         )
         .await?;
      let iface_ref = object_server
         .interface::<_, DeviceBatteryProvider>(&provider_path)
         .await?;
      devices.insert(addr_str.to_owned(), iface_ref);
      info!("Exported BlueZ battery for {addr_str} at {provider_path}: {level}%");
      Ok(())
   }

   /// Removes the exported battery of the device at `addr_str`, if any.
   pub async fn remove_device(&self, addr_str: &str) {
      let Some(iface_ref) = self.devices.lock().await.remove(addr_str) else {
         return;
      };
      let path = iface_ref.signal_emitter().path().to_owned();
      if let Err(e) = self
         .system_conn
         .object_server()
         .remove::<DeviceBatteryProvider, _>(&path)
         .await
      {
         warn!("Failed to remove BlueZ battery for {addr_str}: {e}");
      }
      info!("Removed BlueZ battery for {addr_str}");
   }
}

/// Registers the provider root with the `BatteryProviderManager1` at `adapter`.
async fn register_provider(conn: &Connection, adapter: &ObjectPath<'_>) {
   let root = ObjectPath::from_static_str_unchecked(PROVIDER_ROOT);
   match conn
      .call_method(
         Some("org.bluez"),
         adapter,
         Some(BATTERY_PROVIDER_MANAGER),
         "RegisterBatteryProvider",
         &(root,),
      )
      .await
   {
      Ok(_) => info!("Registered battery provider with BlueZ adapter {adapter}"),
      Err(e) => warn!("Failed to register battery provider with {adapter}: {e}"),
   }
}
