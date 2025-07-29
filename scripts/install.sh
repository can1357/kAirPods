#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo -e "${GREEN}KDE AirPods Installation Script${NC}"
echo "================================"

# Check prerequisites
echo -e "\n${YELLOW}Checking prerequisites...${NC}"

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Rust toolchain not found. Please install Rust first.${NC}"
    echo "Visit: https://rustup.rs/"
    exit 1
fi

# Check for KDE Plasma 6
if ! command -v kpackagetool6 &> /dev/null; then
    echo -e "${RED}Error: KDE Plasma 6 tools not found.${NC}"
    exit 1
fi

# Check for systemd
if ! command -v systemctl &> /dev/null; then
    echo -e "${RED}Error: systemd not found.${NC}"
    exit 1
fi

echo -e "${GREEN}✓ All prerequisites met${NC}"

# Build Rust service
echo -e "\n${YELLOW}Building Rust service...${NC}"
cd "$PROJECT_ROOT/service"
cargo build --release
echo -e "${GREEN}✓ Service built successfully${NC}"

# Install service binary
echo -e "\n${YELLOW}Installing service binary...${NC}"
sudo install -Dm755 target/release/kde-airpods-service /usr/bin/kde-airpods-service
echo -e "${GREEN}✓ Service binary installed${NC}"

# Install systemd user service
echo -e "\n${YELLOW}Installing systemd service...${NC}"
mkdir -p ~/.config/systemd/user/
install -Dm644 systemd/user/kde-airpods-service.service ~/.config/systemd/user/
systemctl --user daemon-reload
echo -e "${GREEN}✓ Systemd service installed${NC}"

# Install plasmoid
echo -e "\n${YELLOW}Installing Plasma widget...${NC}"
cd "$PROJECT_ROOT"
kpackagetool6 --type Plasma/Applet --install plasmoid || \
    kpackagetool6 --type Plasma/Applet --upgrade plasmoid
echo -e "${GREEN}✓ Plasma widget installed${NC}"

# Enable and start service
echo -e "\n${YELLOW}Starting service...${NC}"
systemctl --user enable kde-airpods-service
systemctl --user restart kde-airpods-service

# Check service status
if systemctl --user is-active --quiet kde-airpods-service; then
    echo -e "${GREEN}✓ Service is running${NC}"
else
    echo -e "${RED}⚠ Service failed to start. Check logs with:${NC}"
    echo "  journalctl --user -u kde-airpods-service -f"
fi

echo -e "\n${GREEN}Installation complete!${NC}"
echo -e "\nTo add the widget to your panel:"
echo "1. Right-click on your Plasma panel"
echo "2. Select 'Add Widgets'"
echo "3. Search for 'KDE AirPods'"
echo "4. Drag the widget to your panel"

echo -e "\n${YELLOW}Note:${NC} Make sure your AirPods are already paired via KDE Bluetooth settings."