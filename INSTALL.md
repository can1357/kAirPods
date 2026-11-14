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

### Stem presses do not control playback

kAirPods uses MPRIS for its optional ear-detection play/pause feature; it does not handle stem presses in its AirPods protocol parser. Stem playback controls use Bluetooth AVRCP and an MPRIS-compatible player. If they do not work, BlueZ's `mpris-proxy` can expose the player to Bluetooth.

Install the package for your distribution:

- Arch: [`bluez-utils`](https://archlinux.org/packages/extra/x86_64/bluez-utils/files/) (`/usr/bin/mpris-proxy`)
- Fedora: [`bluez`](https://packages.fedoraproject.org/pkgs/bluez/bluez/fedora-44-updates.html) (`/usr/bin/mpris-proxy`)
- Debian/Ubuntu: `bluez` (for example, [Debian bookworm](https://packages.debian.org/bookworm/armhf/bluez/filelist) and [Ubuntu jammy](https://packages.ubuntu.com/jammy/amd64/bluez/filelist) install `/usr/lib/bluetooth/mpris-proxy`)

Check your package's file list if the path differs. Create `~/.config/systemd/user/mpris-proxy.service`, using the path for your system in `ExecStart`:

```ini
[Unit]
Description=Bluetooth MPRIS proxy
After=bluetooth.target

[Service]
ExecStart=/usr/bin/mpris-proxy
Restart=on-failure

[Install]
WantedBy=default.target
```

Then enable it for your user:

```bash
systemctl --user daemon-reload
systemctl --user enable --now mpris-proxy.service
```

Some packages already provide this user unit; check `systemctl --user cat mpris-proxy.service` before creating a local override. If it still fails, check `journalctl --user -u mpris-proxy.service` and confirm your player supports MPRIS.

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
