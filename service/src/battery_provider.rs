use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use log::{info, warn};
use tokio::sync::Mutex;
use zbus::fdo::{ObjectManager, ObjectManagerProxy};
use zbus::object_server::InterfaceRef;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zbus::{interface, Connection, Proxy, Result};

pub struct DeviceBatteryProvider {
    percentage: Arc<AtomicU8>,
    device_path: OwnedObjectPath,
}

impl DeviceBatteryProvider {
    pub fn new(device_path: OwnedObjectPath, initial_percentage: u8) -> Self {
        Self {
            percentage: Arc::new(AtomicU8::new(initial_percentage)),
            device_path,
        }
    }
}

#[interface(name = "org.bluez.BatteryProvider1")]
impl DeviceBatteryProvider {
    #[zbus(property)]
    fn percentage(&self) -> u8 {
        self.percentage.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn device(&self) -> OwnedObjectPath {
        self.device_path.clone()
    }

    #[zbus(property)]
    fn source(&self) -> &'static str {
        "AirPods"
    }
}

pub struct BluezBatteryManager {
    system_conn: Connection,
    provider_root: &'static str,
    registered_adapters: Mutex<Vec<OwnedObjectPath>>,
    devices: Mutex<HashMap<String, (OwnedObjectPath, InterfaceRef<DeviceBatteryProvider>)>>,
}

impl BluezBatteryManager {
    pub async fn new(system_conn: Connection) -> Result<Arc<Self>> {
        let provider_root = "/org/kairpods/battery_provider";

        // Register ObjectManager at provider root
        system_conn.object_server().at(provider_root, ObjectManager).await?;

        let manager = Arc::new(Self {
            system_conn,
            provider_root,
            registered_adapters: Mutex::new(Vec::new()),
            devices: Mutex::new(HashMap::new()),
        });

        manager.discover_and_register_adapters().await?;

        Ok(manager)
    }

    /// Discovers all BlueZ adapters and registers the BatteryProvider on each.
    async fn discover_and_register_adapters(&self) -> Result<()> {
        let root_proxy = match ObjectManagerProxy::builder(&self.system_conn)
            .destination("org.bluez")?
            .path("/")?
            .build()
            .await
        {
            Ok(p) => p,
            Err(e) => {
                warn!("Could not connect to BlueZ ObjectManager: {e}");
                return Ok(());
            }
        };

        let managed_objects = match root_proxy.get_managed_objects().await {
            Ok(objs) => objs,
            Err(e) => {
                warn!("Failed to query BlueZ managed objects: {e}");
                return Ok(());
            }
        };

        let mut adapters = Vec::new();
        for (path, ifaces) in &managed_objects {
            if ifaces.keys().any(|k| k.as_str() == "org.bluez.Adapter1" || k.as_str() == "org.bluez.BatteryProviderManager1") {
                adapters.push(path.clone());
            }
        }

        if adapters.is_empty() {
            // Fallback default
            if let Ok(default_path) = OwnedObjectPath::try_from("/org/bluez/hci0") {
                adapters.push(default_path);
            }
        }

        let root_path = ObjectPath::try_from(self.provider_root)
            .map_err(|e| zbus::Error::Failure(e.to_string()))?;

        let mut registered = self.registered_adapters.lock().await;
        for adapter_path in adapters {
            match Proxy::new(
                &self.system_conn,
                "org.bluez",
                adapter_path.as_str(),
                "org.bluez.BatteryProviderManager1",
            )
            .await
            {
                Ok(proxy) => {
                    match proxy.call_method("RegisterBatteryProvider", &(root_path.clone(),)).await {
                        Ok(_) => {
                            info!("Registered BatteryProvider with BlueZ on {adapter_path}");
                            registered.push(adapter_path);
                        }
                        Err(e) => {
                            warn!("Failed to register BatteryProvider on {adapter_path}: {e}. (Ensure 'Experimental = true' in /etc/bluetooth/main.conf)");
                        }
                    }
                }
                Err(e) => {
                    warn!("Could not create BatteryProviderManager1 proxy on {adapter_path}: {e}");
                }
            }
        }

        Ok(())
    }

    /// Resolves the BlueZ device path for a given Bluetooth MAC address.
    async fn resolve_device_path(&self, addr_str: &str) -> Option<OwnedObjectPath> {
        if let Ok(builder) = ObjectManagerProxy::builder(&self.system_conn).destination("org.bluez") {
            if let Ok(builder) = builder.path("/") {
                if let Ok(root_proxy) = builder.build().await {
                    if let Ok(managed) = root_proxy.get_managed_objects().await {
                        for (path, ifaces) in managed {
                            if let Some(dev_props) = ifaces.get("org.bluez.Device1") {
                                if let Some(addr_val) = dev_props.get("Address") {
                                    if let Ok(addr) = <&str>::try_from(addr_val) {
                                        if addr.eq_ignore_ascii_case(addr_str) {
                                            return Some(path);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: use first registered adapter or default hci0
        let formatted_addr = addr_str.replace(':', "_");
        let adapters = self.registered_adapters.lock().await;
        let adapter_prefix = adapters.first().map(|p| p.as_str()).unwrap_or("/org/bluez/hci0");
        OwnedObjectPath::try_from(format!("{adapter_prefix}/dev_{formatted_addr}")).ok()
    }

    pub async fn update_device_battery(&self, addr_str: &str, level: u8) -> Result<()> {
        let mut devices = self.devices.lock().await;

        if let Some((_, iface_ref)) = devices.get(addr_str) {
            let provider = iface_ref.get_mut().await;
            provider.percentage.store(level, Ordering::Relaxed);
            let _ = provider.percentage_changed(iface_ref.signal_emitter()).await;
            info!("Updated BlueZ battery for {addr_str}: {level}%");
        } else {
            let Some(dev_path) = self.resolve_device_path(addr_str).await else {
                warn!("Could not resolve BlueZ device path for {addr_str}");
                return Ok(());
            };

            let formatted_addr = addr_str.replace(':', "_");
            let provider_path_str = format!("{}/dev_{formatted_addr}", self.provider_root);
            let Ok(provider_path) = OwnedObjectPath::try_from(provider_path_str.clone()) else {
                return Ok(());
            };

            let provider = DeviceBatteryProvider::new(dev_path.clone(), level);
            if self.system_conn.object_server().at(&provider_path, provider).await.is_ok() {
                if let Ok(iface_ref) = self.system_conn.object_server().interface::<_, DeviceBatteryProvider>(&provider_path).await {
                    devices.insert(addr_str.to_string(), (provider_path, iface_ref));
                    info!("Exported BlueZ BatteryProvider for {addr_str} (device: {dev_path}) at {provider_path_str}: {level}%");
                }
            }
        }

        Ok(())
    }

    pub async fn remove_device(&self, addr_str: &str) {
        let mut devices = self.devices.lock().await;
        if let Some((path, _)) = devices.remove(addr_str) {
            let _ = self.system_conn.object_server().remove::<DeviceBatteryProvider, _>(&path).await;
            info!("Removed BlueZ battery provider for {addr_str} at {path}");
        }
    }
}
