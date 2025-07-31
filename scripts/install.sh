#!/usr/bin/env bash
set -Eeuo pipefail
IFS=$'\n\t'

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Service identifiers
SERVICE_ID="kairpodsd"
PLASMOID_ID="org.kairpods.plasma"
OLD_SERVICE_ID="kde-airpods-service"
OLD_PLASMOID_ID="org.kde.plasma.airpods"

# Build mode
BUILD_MODE="release"
INSTALL_SERVICE=true
INSTALL_WIDGET=true
PREFIX="/usr"
DEBUG=false

# Colors - readonly for safety
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# Pre-build tags for performance
readonly TAG_INFO="${GREEN}[INFO]${NC}"
readonly TAG_WARN="${YELLOW}[WARN]${NC}"
readonly TAG_ERROR="${RED}[ERROR]${NC}"
readonly TAG_STEP="${BLUE}==>${NC}"

# Logging functions
log_info() { printf '%b\n' "$TAG_INFO $*"; }
log_warn() { printf '%b\n' "$TAG_WARN $*"; }
log_error() { printf '%b\n' "$TAG_ERROR $*" >&2; }
log_step() { printf '%b\n' "$TAG_STEP $*"; }

# Trap errors and interrupts
trap 'log_error "Installation aborted (line $LINENO)"' ERR
trap 'log_error "Installation cancelled"; exit 130' INT

# Usage
usage() {
    cat << EOF
kAirPods Installation Script

Usage: $0 [OPTIONS]

Options:
    -h, --help          Show this help message
    -d, --debug         Build in debug mode (default: release)
    -x, --verbose       Enable verbose output for debugging
    --no-service        Skip service installation
    --no-widget         Skip widget installation
    --prefix PATH       Installation prefix (default: /usr)
    --uninstall         Uninstall kAirPods

Examples:
    $0                  # Standard installation
    $0 --debug          # Install debug build
    $0 --uninstall      # Remove kAirPods

EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -d|--debug)
            BUILD_MODE="debug"
            shift
            ;;
        -x|--verbose)
            DEBUG=true
            shift
            ;;
        --no-service)
            INSTALL_SERVICE=false
            shift
            ;;
        --no-widget)
            INSTALL_WIDGET=false
            shift
            ;;
        --prefix)
            PREFIX="$2"
            shift 2
            ;;
        --uninstall)
            UNINSTALL=true
            shift
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Uninstall function
uninstall_kairpods() {
    log_info "Uninstalling kAirPods..."

    # Stop and disable service
    if systemctl --user is-enabled "$SERVICE_ID" &>/dev/null; then
        log_step "Stopping and disabling service..."
        systemctl --user stop "$SERVICE_ID" || true
        systemctl --user disable "$SERVICE_ID" || true
    fi

    # Remove service files
    log_step "Removing service files..."
    sudo rm -f "$PREFIX/bin/$SERVICE_ID"
    rm -f "$HOME/.config/systemd/user/${SERVICE_ID}.service"
    systemctl --user daemon-reload

    # Remove widget
    if kpackagetool6 --type Plasma/Applet --list | grep -q "$PLASMOID_ID"; then
        log_step "Removing widget..."
        kpackagetool6 --type Plasma/Applet --remove "$PLASMOID_ID" || true
    fi

    log_info "Uninstallation complete!"
    exit 0
}

# Handle uninstall
if [[ "${UNINSTALL:-false}" == "true" ]]; then
    uninstall_kairpods
fi

# Main installation
log_info "kAirPods Installation Script"
echo "================================"

# Check if running as root
if [[ $EUID -eq 0 ]]; then
    log_error "This script should not be run as root!"
    log_error "It needs to add your regular user to groups."
    exit 1
fi

# Sudo privilege check and caching
log_step "Checking sudo privileges..."
if ! sudo -v; then
    log_error "Sudo privileges required for installation"
    exit 1
fi

# Enable verbose mode if requested
if [[ "$DEBUG" == "true" ]]; then
    PS4='+ ${BASH_SOURCE}:${LINENO}: '  # Clean trace output
    set -x
    log_info "Verbose mode enabled"
fi

# Check prerequisites
log_step "Checking prerequisites..."

# Check Rust version
if ! command -v cargo &>/dev/null; then
    log_error "Rust toolchain not found. Please install Rust first."
    echo "Visit: https://rustup.rs/"
    exit 1
fi

RUST_VERSION=$(rustc --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "0.0.0")
REQUIRED_VERSION="1.88.0"
if [[ "$(LC_ALL=C printf '%s\n' "$REQUIRED_VERSION" "$RUST_VERSION" | sort -V | head -n1)" != "$REQUIRED_VERSION" ]]; then
    log_error "Rust version $RUST_VERSION is too old. Minimum required: $REQUIRED_VERSION"
    exit 1
fi
log_info "✓ Rust $RUST_VERSION"

# Check for KDE Plasma 6
if ! command -v kpackagetool6 &>/dev/null; then
    log_error "KDE Plasma 6 tools not found."
    exit 1
fi
log_info "✓ KDE Plasma 6"

# Check for systemd
if ! command -v systemctl &>/dev/null; then
    log_error "systemd not found."
    exit 1
fi
log_info "✓ systemd"

# Check for required development packages
log_step "Checking development dependencies..."
declare -a missing_deps=()

# Check for pkg-config
if ! command -v pkg-config &>/dev/null; then
    missing_deps+=("pkg-config")
fi

# Check for dbus development files
if ! pkg-config --exists dbus-1 2>/dev/null; then
    missing_deps+=("libdbus-1-dev")
fi

# Check for bluetooth development files (some distros use bluez, others libbluetooth)
if ! pkg-config --exists bluez 2>/dev/null && ! pkg-config --exists libbluetooth 2>/dev/null; then
    if [[ "$DEBUG" == "true" ]]; then
        log_warn "Neither 'pkg-config --exists bluez' nor 'pkg-config --exists libbluetooth' succeeded"
    fi
    missing_deps+=("libbluetooth-dev")
fi

if [[ ${#missing_deps[@]} -gt 0 ]]; then
    log_error "Missing development packages: ${missing_deps[*]}"
    echo -e "\nPlease install them using:"
    if command -v apt &>/dev/null; then
        log_warn "sudo apt install ${missing_deps[*]}"
    elif command -v dnf &>/dev/null; then
        log_warn "sudo dnf install gcc pkg-config dbus-devel bluez-libs-devel"
    elif command -v pacman &>/dev/null; then
        log_warn "sudo pacman -S base-devel pkgconf dbus bluez-libs"
    fi
    exit 1
fi
log_info "✓ All development dependencies present"

# Check bluetooth group membership
log_step "Checking bluetooth permissions..."
BLUETOOTH_GROUP_EXISTS=false
SET_CAPABILITIES=false

if getent group bluetooth >/dev/null 2>&1; then
    BLUETOOTH_GROUP_EXISTS=true
    if ! groups | grep -q -- 'bluetooth'; then
        log_warn "Adding user to bluetooth group..."
        sudo usermod -aG bluetooth "$USER"
        log_info "✓ User added to bluetooth group"
        log_warn "Note: You'll need to log out and back in for this to take effect."
        NEED_RELOGIN=true
    else
        log_info "✓ User is in bluetooth group"
    fi
else
    log_warn "Bluetooth group does not exist on this system"
    log_warn "This is normal on some distributions (e.g., Fedora)"
    log_info "Will set capabilities on the binary for Bluetooth access"
    SET_CAPABILITIES=true
fi

# Clean up old installation
log_step "Checking for previous installation..."

# Old service cleanup
if [[ -f "/usr/bin/$OLD_SERVICE_ID" ]]; then
    log_warn "Found old $OLD_SERVICE_ID, removing..."
    sudo rm -f "/usr/bin/$OLD_SERVICE_ID"
fi

if systemctl --user is-enabled "$OLD_SERVICE_ID" &>/dev/null; then
    log_warn "Disabling old systemd service..."
    systemctl --user stop "$OLD_SERVICE_ID" || true
    systemctl --user disable "$OLD_SERVICE_ID" || true
fi

if [[ -f "$HOME/.config/systemd/user/${OLD_SERVICE_ID}.service" ]]; then
    log_warn "Removing old systemd service file..."
    rm -f "$HOME/.config/systemd/user/${OLD_SERVICE_ID}.service"
    systemctl --user daemon-reload
fi

# Remove old plasmoid if exists
if kpackagetool6 --type Plasma/Applet --list | grep -E "^(${OLD_PLASMOID_ID}|${PLASMOID_ID})\s" >/dev/null 2>&1; then
    log_warn "Removing old plasmoid..."
    kpackagetool6 --type Plasma/Applet --remove "$OLD_PLASMOID_ID" 2>/dev/null || true
    kpackagetool6 --type Plasma/Applet --remove "$PLASMOID_ID" 2>/dev/null || true
fi

log_info "✓ Old installation cleaned up"

# Build Rust service
if [[ "$INSTALL_SERVICE" == "true" ]]; then
    log_step "Building Rust service ($BUILD_MODE mode)..."
    cd "$PROJECT_ROOT/service"

    if [[ "$BUILD_MODE" == "release" ]]; then
        cargo build --release --locked
        BINARY_PATH="target/release/$SERVICE_ID"
    else
        cargo build
        BINARY_PATH="target/debug/$SERVICE_ID"
    fi

    log_info "✓ Service built successfully"

    # Install service binary
    log_step "Installing service binary..."
    sudo install -Dm755 "$BINARY_PATH" "$PREFIX/bin/$SERVICE_ID"
    log_info "✓ Service binary installed"

    # Set capabilities if bluetooth group doesn't exist
    if [[ "$SET_CAPABILITIES" == "true" ]]; then
        if command -v setcap &>/dev/null; then
            log_step "Setting capabilities for Bluetooth access..."
            sudo setcap 'cap_net_raw,cap_net_admin+eip' "$PREFIX/bin/$SERVICE_ID" || {
                log_warn "Failed to set capabilities - service may have limited functionality"
            }
            log_info "✓ Capabilities set for raw socket access"
        else
            log_warn "setcap not found - install libcap package for capability support"
            log_warn "Service may have limited functionality without bluetooth group membership"
        fi
    fi

    # Install systemd user service
    log_step "Installing systemd service..."
    mkdir -p "$HOME/.config/systemd/user/"
    # If PREFIX is not /usr, update the service file path
    if [[ "$PREFIX" != "/usr" ]]; then
        sed "s:/usr/bin:$PREFIX/bin:g" "systemd/user/${SERVICE_ID}.service" > "$HOME/.config/systemd/user/${SERVICE_ID}.service"
        chmod 644 "$HOME/.config/systemd/user/${SERVICE_ID}.service"
    else
        install -Dm644 "systemd/user/${SERVICE_ID}.service" "$HOME/.config/systemd/user/"
    fi
    systemctl --user daemon-reload
    log_info "✓ Systemd service installed"

    # Return to project root
    cd "$PROJECT_ROOT"

    # Enable and start service
    log_step "Starting service..."
    systemctl --user enable --now "$SERVICE_ID"

    # Check service status
    sleep 1
    if systemctl --user is-active --quiet "$SERVICE_ID"; then
        log_info "✓ Service is running"
    else
        log_error "Service failed to start. Check logs with:"
        echo "  journalctl --user -u $SERVICE_ID -f"
        echo -e "\n${YELLOW}Common issues:${NC}"
        echo "- Ensure you're in the bluetooth group (see above)"
        echo "- Make sure Bluetooth is enabled"
        echo "- Check that AirPods are paired via KDE settings"
    fi
fi

# Install plasmoid
if [[ "$INSTALL_WIDGET" == "true" ]]; then
    log_step "Installing Plasma widget..."
    cd "$PROJECT_ROOT"
    kpackagetool6 --type Plasma/Applet --install plasmoid 2>/dev/null || \
        kpackagetool6 --type Plasma/Applet --upgrade plasmoid
    log_info "✓ Plasma widget installed"
fi

# Final message
echo
log_info "Installation complete!"
echo -e "\nTo add the widget to your panel:"
echo "1. Right-click on your Plasma panel"
echo "2. Select 'Add Widgets'"
echo "3. Search for 'kAirPods'"
echo "4. Drag the widget to your panel"

echo -e "\n${YELLOW}Important:${NC}"
echo "- Make sure your AirPods are already paired via KDE Bluetooth settings"
if [[ "${NEED_RELOGIN:-false}" == "true" ]]; then
    log_warn "You need to log out and back in for bluetooth group changes to take effect!"
fi

# Script completed successfully
