# D-Bus Interface Examples

## Service Details
- **Bus Name**: `org.kairpods`
- **Object Path**: `/org/kairpods/manager`
- **Interface**: `org.kairpods.manager`

## Using busctl

### List all methods and signals
```bash
busctl --user introspect org.kairpods /org/kairpods/manager
```

### Get all connected devices
```bash
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager GetDevices
```

### Get specific device information
```bash
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager GetDevice s "AA:BB:CC:DD:EE:FF"
```

### Set noise control mode
```bash
# Set to ANC
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "anc"  # 1 = dict length

# Set to Transparency
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "transparency"

# Set to Off
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "off"
```

### Toggle features
```bash
# Enable ear detection
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_feature" 2 "feature" s "ear_detection" "enabled" b true  # 2 = dict length

# Disable ear detection
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_feature" 2 "feature" s "ear_detection" "enabled" b false
```

### Connect/Disconnect device
```bash
# Connect
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager ConnectDevice s "AA:BB:CC:DD:EE:FF"

# Disconnect
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager DisconnectDevice s "AA:BB:CC:DD:EE:FF"
```

### Passthrough command
```bash
# Send raw passthrough command (advanced use)
busctl --user call org.kairpods /org/kairpods/manager \
    org.kairpods.manager Passthrough ss "AA:BB:CC:DD:EE:FF" "raw_command_data"
```

### Get connected device count
```bash
# Get the ConnectedCount property
busctl --user get-property org.kairpods /org/kairpods/manager \
    org.kairpods.manager ConnectedCount
```

### Monitor signals
```bash
# Monitor all signals from the service
busctl --user monitor org.kairpods

# Example signal outputs:
# BatteryUpdated: address="AA:BB:CC:DD:EE:FF" battery="{\"left\":85,\"right\":90,\"case\":75}"
# NoiseControlChanged: address="AA:BB:CC:DD:EE:FF" mode="anc"
# DeviceConnected: address="AA:BB:CC:DD:EE:FF"
```

## Using gdbus

### Get device list
```bash
gdbus call --session --dest org.kairpods \
    --object-path /org/kairpods/manager \
    --method org.kairpods.manager.GetDevices
```

### Set noise mode with gdbus
```bash
gdbus call --session --dest org.kairpods \
    --object-path /org/kairpods/manager \
    --method org.kairpods.manager.SendCommand \
    "AA:BB:CC:DD:EE:FF" "set_noise_mode" "{'value': <'anc'>}"
```

### Monitor with gdbus
```bash
gdbus monitor --session --dest org.kairpods
```

## Using D-Bus from Python

```python
import dbus

# Connect to session bus
bus = dbus.SessionBus()

# Get the service
service = bus.get_object('org.kairpods', '/org/kairpods/manager')
interface = dbus.Interface(service, 'org.kairpods.manager')

# Get devices
devices_json = interface.GetDevices()
print(f"Devices: {devices_json}")

# Set noise mode
address = "AA:BB:CC:DD:EE:FF"
params = {"value": "transparency"}
interface.SendCommand(address, "set_noise_mode", params)

# Connect signal handler
def on_battery_update(address, battery):
    print(f"Battery update for {address}: {battery}")

bus.add_signal_receiver(
    on_battery_update,
    dbus_interface="org.kairpods.manager",
    signal_name="BatteryUpdated"
)

# Note: To receive signals, you need to run a GLib MainLoop:
# from gi.repository import GLib
# GLib.MainLoop().run()
```

## Return Format

The `GetDevices` and `GetDevice` methods return JSON strings. Example:

### AirPods Pro / Regular AirPods
```json
[
  {
    "address": "AA:BB:CC:DD:EE:FF",
    "name": "John's AirPods Pro",
    "model": "AirPods Pro",
    "battery": {
      "left": {"level": 85, "charging": false},
      "right": {"level": 90, "charging": false},
      "case": {"level": 75, "charging": true},
      "headphone": null
    },
    "noise_control": "anc",
    "ear_detection": {
      "left": true,
      "right": true
    },
    "features": {
      "ear_detection": true,
      "noise_control": true,
      "spatial_audio": false
    }
  }
]
```

### AirPods Max
```json
[
  {
    "address": "BB:CC:DD:EE:FF:AA",
    "name": "John's AirPods Max",
    "model": "AirPods Max",
    "battery": {
      "left": null,
      "right": null,
      "case": null,
      "headphone": {"level": 95, "charging": false}
    },
    "noise_control": "transparency",
    "ear_detection": {
      "left": true,
      "right": true
    },
    "features": {
      "ear_detection": true,
      "noise_control": true,
      "spatial_audio": true
    }
  }
]
```