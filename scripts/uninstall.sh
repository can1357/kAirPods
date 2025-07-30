#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}kAirPods Uninstallation Script${NC}"
echo "=================================="

# Stop and disable service (both old and new names)
echo -e "\n${YELLOW}Stopping services...${NC}"
# Stop new service
systemctl --user stop kairpodsd 2>/dev/null || true
systemctl --user disable kairpodsd 2>/dev/null || true
# Stop old service if exists
systemctl --user stop kde-airpods-service 2>/dev/null || true
systemctl --user disable kde-airpods-service 2>/dev/null || true
echo -e "${GREEN}✓ Services stopped${NC}"

# Remove service files (both old and new)
echo -e "\n${YELLOW}Removing service files...${NC}"
# Remove new service
sudo rm -f /usr/bin/kairpodsd
rm -f ~/.config/systemd/user/kairpodsd.service
# Remove old service
sudo rm -f /usr/bin/kde-airpods-service
rm -f ~/.config/systemd/user/kde-airpods-service.service
systemctl --user daemon-reload
echo -e "${GREEN}✓ Service files removed${NC}"

# Remove plasmoid (all versions)
echo -e "\n${YELLOW}Removing Plasma widgets...${NC}"
kpackagetool6 --type Plasma/Applet --remove org.kairpods.plasma 2>/dev/null || \
    echo -e "${YELLOW}Current widget was not installed or already removed${NC}"
# Remove old plasmoid
kpackagetool6 --type Plasma/Applet --remove org.kde.plasma.airpods 2>/dev/null || \
    echo -e "${YELLOW}Old widget was not installed or already removed${NC}"

echo -e "\n${GREEN}Uninstallation complete!${NC}"
