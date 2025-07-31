# Installation Guide

## Prerequisites

- KDE Plasma 6 or later
- Rust toolchain (1.88+)
- BlueZ 5.50 or later
- systemd (for user services)
- Plasma SDK (provides `kpackagetool6` for widget installation)
- Development packages:

  ```bash
  # Debian/Ubuntu
  sudo apt install build-essential pkg-config libdbus-1-dev libbluetooth-dev

  # Fedora
  sudo dnf install gcc pkg-config dbus-devel bluez-libs-devel

  # Arch
  sudo pacman -S base-devel pkgconf dbus bluez-libs
  ```

- Plasma SDK (provides `kpackagetool6` for widget installation)

## Building from Source

1. **Clone the repository**

   ```bash
   git clone https://github.com/can1357/kAirPods.git
   cd kAirPods
   ```

2. **Build the Rust service**

   ```bash
   cd service
   cargo build --release --locked
   cd ..
   ```

3. **Install components**

   ```bash
   # Install the service binary
   sudo install -Dm755 service/target/release/kairpodsd /usr/bin/kairpodsd

   # Install systemd user service
   install -Dm644 service/systemd/user/kairpodsd.service \
     ~/.config/systemd/user/kairpodsd.service

   # Install the plasmoid
   kpackagetool6 --type Plasma/Applet --install plasmoid
   ```

4. **Enable and start the service**
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now kairpodsd
   ```

## Quick Install Script

For convenience, use the provided install script:

```bash
./scripts/install.sh
```

This will build and install all components automatically.

## Verifying Installation

1. **Check service status**

   ```bash
   systemctl --user status kairpodsd
   ```

2. **Test D-Bus interface**

   ```bash
   busctl --user introspect org.kairpods /org/kairpods/manager
   ```

3. **Add widget to panel**
   - Right-click on your Plasma panel
   - Select "Add Widgets"
   - Search for "kAirPods"
   - Drag to panel

## Troubleshooting

### Service fails to start

- Check logs: `journalctl --user -u kairpodsd -f`
- Ensure your user is in the `bluetooth` group: `sudo usermod -aG bluetooth $USER`
- Logout and login again for group changes to take effect

### AirPods not detected

- Ensure AirPods are paired via KDE Bluetooth settings first
- Check Bluetooth is enabled: `bluetoothctl power on`
- Verify L2CAP support: `lsmod | grep bluetooth`

### Permission issues

- The service needs access to Bluetooth and D-Bus
- SELinux/AppArmor may need configuration on some distributions
- On systems without a `bluetooth` group, you may need to set capabilities:
  `sudo setcap 'cap_net_raw,cap_net_admin+eip' $(command -v kairpodsd)`

## Uninstalling

### Automated Uninstall

The easiest way is to use the installer script:

```bash
./scripts/install.sh --uninstall
```

### Manual Uninstall

If you need to manually remove kAirPods:

```bash
# Stop and disable service
systemctl --user stop kairpodsd
systemctl --user disable kairpodsd

# Remove service files
sudo rm /usr/bin/kairpodsd
rm ~/.config/systemd/user/kairpodsd.service

# Reload systemd
systemctl --user daemon-reload

# Remove plasmoid
kpackagetool6 --type Plasma/Applet --remove org.kairpods.plasma
```
