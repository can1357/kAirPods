# Installation Guide

## Prerequisites

- KDE Plasma 6 or later
- Rust toolchain (1.70+)
- BlueZ 5.50 or later
- systemd (for user services)
- Development packages:
  ```bash
  # Debian/Ubuntu
  sudo apt install build-essential pkg-config libdbus-1-dev libbluetooth-dev

  # Fedora
  sudo dnf install gcc pkg-config dbus-devel bluez-libs-devel

  # Arch
  sudo pacman -S base-devel pkgconf dbus bluez-libs
  ```

## Building from Source

1. **Clone the repository**
   ```bash
   git clone https://github.com/can1357/kde-airpods.git
   cd kde-airpods
   ```

2. **Build the Rust service**
   ```bash
   cd service
   cargo build --release
   cd ..
   ```

3. **Install components**
   ```bash
   # Install the service binary
   sudo install -Dm755 service/target/release/kde-airpods-service /usr/bin/kde-airpods-service

   # Install systemd user service
   install -Dm644 service/systemd/user/kde-airpods-service.service \
     ~/.config/systemd/user/kde-airpods-service.service

   # Install the plasmoid
   kpackagetool6 --type Plasma/Applet --install plasmoid
   ```

4. **Enable and start the service**
   ```bash
   systemctl --user daemon-reload
   systemctl --user enable --now kde-airpods-service
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
   systemctl --user status kde-airpods-service
   ```

2. **Test D-Bus interface**
   ```bash
   busctl --user introspect org.kde.plasma.airpods /org/kde/plasma/airpods
   ```

3. **Add widget to panel**
   - Right-click on your Plasma panel
   - Select "Add Widgets"
   - Search for "KDE AirPods"
   - Drag to panel

## Troubleshooting

### Service fails to start
- Check logs: `journalctl --user -u kde-airpods-service -f`
- Ensure your user is in the `bluetooth` group: `sudo usermod -aG bluetooth $USER`
- Logout and login again for group changes to take effect

### AirPods not detected
- Ensure AirPods are paired via KDE Bluetooth settings first
- Check Bluetooth is enabled: `bluetoothctl power on`
- Verify L2CAP support: `lsmod | grep bluetooth`

### Permission issues
- The service needs access to Bluetooth and D-Bus
- SELinux/AppArmor may need configuration on some distributions

## Uninstalling

```bash
# Stop and disable service
systemctl --user stop kde-airpods-service
systemctl --user disable kde-airpods-service

# Remove files
sudo rm /usr/bin/kde-airpods-service
rm ~/.config/systemd/user/kde-airpods-service.service

# Remove plasmoid
kpackagetool6 --type Plasma/Applet --remove org.kde.plasma.airpods
```