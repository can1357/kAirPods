# D-Bus Interface Examples

## Service Details
- **Bus Name**: `org.kde.plasma.airpods`
- **Object Path**: `/org/kde/plasma/airpods`
- **Interface**: `org.kde.plasma.airpods`

## Using busctl

### List all methods and signals
```bash
busctl --user introspect org.kde.plasma.airpods /org/kde/plasma/airpods
```

### Get all connected devices
```bash
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods GetDevices
```

### Get specific device information
```bash
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods GetDevice s "AA:BB:CC:DD:EE:FF"
```

### Set noise control mode
```bash
# Set to ANC
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "anc"

# Set to Transparency
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "transparency"

# Set to Off
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_noise_mode" 1 "value" s "off"
```

### Toggle features
```bash
# Enable ear detection
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_feature" 2 "feature" s "ear_detection" "enabled" b true

# Disable ear detection
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods SendCommand ssa{sv} "AA:BB:CC:DD:EE:FF" "set_feature" 2 "feature" s "ear_detection" "enabled" b false
```

### Connect/Disconnect device
```bash
# Connect
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods ConnectDevice s "AA:BB:CC:DD:EE:FF"

# Disconnect
busctl --user call org.kde.plasma.airpods /org/kde/plasma/airpods \
    org.kde.plasma.airpods DisconnectDevice s "AA:BB:CC:DD:EE:FF"
```

### Monitor signals
```bash
# Monitor all signals from the service
busctl --user monitor org.kde.plasma.airpods

# Example signal outputs:
# BatteryUpdated: address="AA:BB:CC:DD:EE:FF" battery="{\"left\":85,\"right\":90,\"case\":75}"
# NoiseControlChanged: address="AA:BB:CC:DD:EE:FF" mode="anc"
# DeviceConnected: address="AA:BB:CC:DD:EE:FF"
```

## Using gdbus

### Get device list
```bash
gdbus call --session --dest org.kde.plasma.airpods \
    --object-path /org/kde/plasma/airpods \
    --method org.kde.plasma.airpods.GetDevices
```

### Set noise mode with gdbus
```bash
gdbus call --session --dest org.kde.plasma.airpods \
    --object-path /org/kde/plasma/airpods \
    --method org.kde.plasma.airpods.SendCommand \
    "AA:BB:CC:DD:EE:FF" "set_noise_mode" "{'value': <'anc'>}"
```

### Monitor with gdbus
```bash
gdbus monitor --session --dest org.kde.plasma.airpods
```

## Using D-Bus from Python

```python
import dbus

# Connect to session bus
bus = dbus.SessionBus()

# Get the service
service = bus.get_object('org.kde.plasma.airpods', '/org/kde/plasma/airpods')
interface = dbus.Interface(service, 'org.kde.plasma.airpods')

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
    dbus_interface="org.kde.plasma.airpods",
    signal_name="BatteryUpdated"
)
```

## Return Format

The `GetDevices` and `GetDevice` methods return JSON strings. Example:

```json
[
  {
    "address": "AA:BB:CC:DD:EE:FF",
    "name": "John's AirPods Pro",
    "model": "AirPods Pro",
    "battery": {
      "left": 85,
      "right": 90,
      "case": 75
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