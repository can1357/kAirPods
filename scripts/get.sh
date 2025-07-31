#!/usr/bin/env bash
# kAirPods Quick Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/can1357/kAirPods/master/scripts/get.sh | bash

set -euo pipefail

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Logging
log_info() { printf '%b\n' "${GREEN}[INFO]${NC} $*"; }
log_error() { printf '%b\n' "${RED}[ERROR]${NC} $*" >&2; }
log_warn() { printf '%b\n' "${YELLOW}[WARN]${NC} $*"; }

# Check prerequisites
if ! command -v git &>/dev/null; then
    log_error "git is required but not installed"
    exit 1
fi

# Temporary directory for cloning
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT

log_info "kAirPods Quick Installer"
log_info "========================"

# Check if we're already in a kAirPods repository
if [[ -f "./scripts/install.sh" ]] && [[ -f "./service/Cargo.toml" ]] && grep -q "kairpodsd" "./service/Cargo.toml" 2>/dev/null; then
    log_info "Found existing kAirPods repository"
    log_info "Updating to latest version..."
    git pull --rebase || {
        log_warn "Failed to update repository, continuing with current version"
    }
    log_info "Running installer..."
    bash ./scripts/install.sh "$@"
else
    # Clone repository
    log_info "Cloning kAirPods repository..."
    git clone --depth 1 https://github.com/can1357/kAirPods.git "$TEMP_DIR/kairpods" || {
        log_error "Failed to clone repository"
        exit 1
    }

    # Run installer
    cd "$TEMP_DIR/kairpods"
    log_info "Running installer..."
    bash ./scripts/install.sh "$@"
fi
