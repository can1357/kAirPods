#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}KDE AirPods Uninstallation Script${NC}"
echo "=================================="

# Stop and disable service
echo -e "\n${YELLOW}Stopping service...${NC}"
systemctl --user stop kde-airpods-service 2>/dev/null || true
systemctl --user disable kde-airpods-service 2>/dev/null || true
echo -e "${GREEN}✓ Service stopped${NC}"

# Remove service files
echo -e "\n${YELLOW}Removing service files...${NC}"
sudo rm -f /usr/bin/kde-airpods-service
rm -f ~/.config/systemd/user/kde-airpods-service.service
systemctl --user daemon-reload
echo -e "${GREEN}✓ Service files removed${NC}"

# Remove plasmoid
echo -e "\n${YELLOW}Removing Plasma widget...${NC}"
kpackagetool6 --type Plasma/Applet --remove org.kde.plasma.airpods 2>/dev/null || \
    echo -e "${YELLOW}Widget was not installed or already removed${NC}"

echo -e "\n${GREEN}Uninstallation complete!${NC}"