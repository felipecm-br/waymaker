#!/bin/sh
#
# Waymaker Installation & Configuration Script
# Installs the `wm` binary and deploys .toml configs and presets to ~/.config/waymaker/
#
# Usage:
#   ./install.sh                # Install binary and deploy .toml configs
#   ./install.sh --configs-only # Only deploy/update .toml configs and presets
#   ./install.sh --binary-only  # Only install/download the `wm` binary
#   ./install.sh --force        # Overwrite existing configs without backing up
#   ./install.sh --help         # Show this help message
#

set -e

REPO="fcmiranda/waymaker"
BRANCH="main"
BINARY_BASE_NAME="wm"

# Directories
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/waymaker"
PRESETS_DIR="$CONFIG_DIR/presets"
INSTALL_DIR_LOCAL="$HOME/.local/bin"
INSTALL_DIR_CARGO="$HOME/.cargo/bin"

# Options
INSTALL_BIN=true
INSTALL_CONFIG=true
FORCE_OVERWRITE=false

# Styling / Colors
BOLD='\033[1m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

error() { printf "%b[!] Error: %s%b\n" "${RED}" "$1" "${NC}" >&2; exit 1; }
info()  { printf "%b[✓] %s%b\n" "${GREEN}" "$1" "${NC}"; }
step()  { printf "%b[→] %s%b\n" "${BLUE}" "$1" "${NC}"; }
warn()  { printf "%b[!] Warning: %s%b\n" "${YELLOW}" "$1" "${NC}"; }

print_help() {
    printf "%bWaymaker Installer & Config Deployer%b\n\n" "${BOLD}" "${NC}"
    printf "Usage: %s [OPTIONS]\n\n" "$0"
    printf "Options:\n"
    printf "  -c, --configs-only    Deploy/update .toml configuration files and presets only\n"
    printf "  -b, --binary-only     Build or download the 'wm' binary only\n"
    printf "  -f, --force           Overwrite existing configuration files without backups\n"
    printf "  -h, --help            Show this help dialog\n\n"
    printf "Target Locations:\n"
    printf "  Binary:  %s/wm\n" "${INSTALL_DIR_LOCAL}"
    printf "  Config:  %s/config.toml\n" "${CONFIG_DIR}"
    printf "  Session: %s/session.toml\n" "${CONFIG_DIR}"
    printf "  Presets: %s/*.toml\n" "${PRESETS_DIR}"
    exit 0
}

# Parse command line flags
for arg in "$@"; do
    case "$arg" in
        -c|--configs-only)
            INSTALL_BIN=false
            INSTALL_CONFIG=true
            ;;
        -b|--binary-only)
            INSTALL_BIN=true
            INSTALL_CONFIG=false
            ;;
        -f|--force)
            FORCE_OVERWRITE=true
            ;;
        -h|--help)
            print_help
            ;;
        *)
            warn "Unknown argument: $arg"
            ;;
    esac
done

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "mac" ;;
        CYGWIN*|MINGW*|MSYS*) echo "windows" ;;
        *) error "Unsupported OS: $(uname -s)" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)   echo "x86_64" ;;
        arm64|aarch64)  echo "aarch64" ;;
        *) echo "x86_64" ;;
    esac
}

get_install_dir() {
    case ":$PATH:" in
        *":$INSTALL_DIR_LOCAL:"*)
            mkdir -p "$INSTALL_DIR_LOCAL" 2>/dev/null || true
            echo "$INSTALL_DIR_LOCAL"
            return
            ;;
        *":$INSTALL_DIR_CARGO:"*)
            mkdir -p "$INSTALL_DIR_CARGO" 2>/dev/null || true
            echo "$INSTALL_DIR_CARGO"
            return
            ;;
    esac

    if [ -d "$INSTALL_DIR_LOCAL" ] || mkdir -p "$INSTALL_DIR_LOCAL" 2>/dev/null; then
        echo "$INSTALL_DIR_LOCAL"
    elif [ "$OS" = "windows" ]; then
        _win_appdata="${LOCALAPPDATA:-}"
        [ -z "$_win_appdata" ] && error "LOCALAPPDATA not set"
        _win_path="$_win_appdata/Programs/waymaker"
        mkdir -p "$_win_path" || error "Could not create $_win_path"
        echo "$_win_path"
    else
        echo "/usr/local/bin"
    fi
}

get_latest_release() {
    _url="https://api.github.com/repos/$REPO/releases/latest"
    _version=$(curl -fsSL "$_url" 2>/dev/null | grep '"tag_name":' | sed 's/.*"tag_name": "//;s/".*//' || echo "")
    if [ -z "$_version" ]; then
        # Fallback to hardcoded current release tag or v0.1.1
        _version="v0.1.1"
    fi
    echo "$_version"
}

copy_or_backup() {
    src="$1"
    dst="$2"

    if [ -f "$dst" ] && ! cmp -s "$src" "$dst"; then
        if [ "$FORCE_OVERWRITE" = false ]; then
            backup="${dst}.bak.$(date +%Y%m%d%H%M%S)"
            cp "$dst" "$backup"
            warn "Existing file modified: backed up to $backup"
        fi
    fi

    cp "$src" "$dst"
}

download_or_backup() {
    url="$1"
    dst="$2"

    temp_file=$(mktemp 2>/dev/null || mktemp -t 'wm_conf')
    if ! curl -fsSL "$url" -o "$temp_file"; then
        rm -f "$temp_file"
        warn "Could not fetch $url (skipping)"
        return 1
    fi

    if [ -f "$dst" ] && ! cmp -s "$temp_file" "$dst"; then
        if [ "$FORCE_OVERWRITE" = false ]; then
            backup="${dst}.bak.$(date +%Y%m%d%H%M%S)"
            cp "$dst" "$backup"
            warn "Existing file modified: backed up to $backup"
        fi
    fi

    mv "$temp_file" "$dst"
}

install_configs_local() {
    assets_dir="$1"
    step "Deploying configuration files from local assets ($assets_dir)..."

    mkdir -p "$CONFIG_DIR"
    mkdir -p "$PRESETS_DIR"

    # 1. Base config.toml
    if [ -f "$assets_dir/config.toml" ]; then
        copy_or_backup "$assets_dir/config.toml" "$CONFIG_DIR/config.toml"
        info "Installed $CONFIG_DIR/config.toml"
    fi

    # 2. session.toml
    if [ -f "$assets_dir/session.toml" ]; then
        copy_or_backup "$assets_dir/session.toml" "$CONFIG_DIR/session.toml"
        info "Installed $CONFIG_DIR/session.toml"
    fi

    # 3. Presets
    if [ -d "$assets_dir/presets" ]; then
        count=0
        for preset in "$assets_dir/presets"/*.toml; do
            [ -f "$preset" ] || continue
            preset_name=$(basename "$preset")
            copy_or_backup "$preset" "$PRESETS_DIR/$preset_name"
            count=$((count + 1))
        done

        # Also copy preset subdirectories if any (e.g. git, ai, docker)
        for subdir in "$assets_dir/presets"/*/; do
            [ -d "$subdir" ] || continue
            sub_name=$(basename "$subdir")
            mkdir -p "$PRESETS_DIR/$sub_name"
            for sub_file in "$subdir"*.toml; do
                [ -f "$sub_file" ] || continue
                sub_preset_name=$(basename "$sub_file")
                copy_or_backup "$sub_file" "$PRESETS_DIR/$sub_name/$sub_preset_name"
                count=$((count + 1))
            done
        done
        info "Installed $count presets to $PRESETS_DIR/"
    fi
}

install_configs_remote() {
    step "Fetching configuration files and presets from GitHub ($REPO@$BRANCH)..."

    mkdir -p "$CONFIG_DIR"
    mkdir -p "$PRESETS_DIR"

    raw_base="https://raw.githubusercontent.com/$REPO/$BRANCH/waymaker-cli/assets"

    # Base config
    step "Fetching config.toml..."
    download_or_backup "$raw_base/config.toml" "$CONFIG_DIR/config.toml" || true

    # Session config
    step "Fetching session.toml..."
    download_or_backup "$raw_base/session.toml" "$CONFIG_DIR/session.toml" || true

    # Core Presets
    core_presets="jump.toml rg.toml workspace.toml yank.toml scrollback-picker.toml session-picker.toml ftb.toml kill.toml keybindings.toml pr.toml animations.toml borders.toml backgrounds.toml downloads.toml sounds.toml memory.toml ps.toml cargo-rx.toml"

    step "Fetching core presets..."
    for p in $core_presets; do
        download_or_backup "$raw_base/presets/$p" "$PRESETS_DIR/$p" || true
    done

    info "Configuration files deployed to $CONFIG_DIR"
}

install_binary_local() {
    bin_path="$1"
    step "Installing local binary from $bin_path..."

    SUDO=""
    if [ ! -w "$INSTALL_DIR" ]; then
        if [ "$OS" = "windows" ]; then
            error "No write permission for $INSTALL_DIR. Please run as Administrator."
        else
            warn "No write permission for $INSTALL_DIR. Requesting sudo..."
            SUDO="sudo"
        fi
    fi

    $SUDO cp "$bin_path" "$INSTALL_DIR/$BINARY_NAME"
    $SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"
    info "Successfully installed binary to $INSTALL_DIR/$BINARY_NAME"
}

install_binary_remote() {
    step "Fetching pre-compiled binary for $OS ($ARCH)..."

    VERSION=$(get_latest_release)
    step "Target release version: $VERSION"

    # Release asset mapping
    if [ "$OS" = "windows" ]; then
        ASSET_NAME="waymaker-cli-x86_64-pc-windows-msvc.zip"
    elif [ "$OS" = "mac" ]; then
        if [ "$ARCH" = "aarch64" ]; then
            ASSET_NAME="waymaker-cli-aarch64-apple-darwin.tar.xz"
        else
            ASSET_NAME="waymaker-cli-x86_64-apple-darwin.tar.xz"
        fi
    else
        # Linux (musl statically linked)
        if [ "$ARCH" = "aarch64" ]; then
            ASSET_NAME="waymaker-cli-aarch64-unknown-linux-musl.tar.xz"
        else
            ASSET_NAME="waymaker-cli-x86_64-unknown-linux-musl.tar.xz"
        fi
    fi

    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET_NAME"
    TEMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t 'wm_install')
    trap 'rm -rf "$TEMP_DIR"' EXIT INT TERM

    step "Downloading $DOWNLOAD_URL..."
    if ! curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$ASSET_NAME"; then
        warn "Direct asset download failed. Falling back to source build if cargo is present..."
        if command -v cargo >/dev/null 2>&1; then
            cargo install --git "https://github.com/$REPO.git" waymaker-cli
            return 0
        else
            error "Could not download binary asset and cargo is not installed."
        fi
    fi

    step "Extracting $ASSET_NAME..."
    case "$ASSET_NAME" in
        *.zip)
            if command -v unzip >/dev/null 2>&1; then
                unzip -q "$TEMP_DIR/$ASSET_NAME" -d "$TEMP_DIR"
            else
                tar -xf "$TEMP_DIR/$ASSET_NAME" -C "$TEMP_DIR"
            fi
            ;;
        *.tar.xz)
            tar -xJf "$TEMP_DIR/$ASSET_NAME" -C "$TEMP_DIR"
            ;;
        *)
            tar -xzf "$TEMP_DIR/$ASSET_NAME" -C "$TEMP_DIR"
            ;;
    esac

    FOUND_BIN=$(find "$TEMP_DIR" -name "$BINARY_NAME" -type f | head -n 1)
    [ -z "$FOUND_BIN" ] && error "Binary $BINARY_NAME not found inside archive"

    SUDO=""
    if [ ! -w "$INSTALL_DIR" ]; then
        if [ "$OS" = "windows" ]; then
            error "No write permission for $INSTALL_DIR. Please run as Administrator."
        else
            warn "No write permission for $INSTALL_DIR. Requesting sudo..."
            SUDO="sudo"
        fi
    fi

    $SUDO rm -f "$INSTALL_DIR/$BINARY_NAME"
    $SUDO mv "$FOUND_BIN" "$INSTALL_DIR/"
    $SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"

    info "Successfully installed $INSTALL_DIR/$BINARY_NAME"
}

main() {
    OS=$(detect_os)
    ARCH=$(detect_arch)
    BINARY_NAME="$BINARY_BASE_NAME"
    [ "$OS" = "windows" ] && BINARY_NAME="${BINARY_BASE_NAME}.exe"

    INSTALL_DIR=$(get_install_dir)

    printf "\n%b⚡ Waymaker Installer (%s %s)%b\n\n" "${CYAN}${BOLD}" "$OS" "$ARCH" "${NC}"

    # Determine local vs remote context
    LOCAL_ASSETS=""
    LOCAL_BIN=""

    # Check script's directory for repository assets
    SCRIPT_DIR="$(cd "$(dirname "$0")" 2>/dev/null && pwd)"
    if [ -d "$SCRIPT_DIR/waymaker-cli/assets" ]; then
        LOCAL_ASSETS="$SCRIPT_DIR/waymaker-cli/assets"
    elif [ -d "$PWD/waymaker-cli/assets" ]; then
        LOCAL_ASSETS="$PWD/waymaker-cli/assets"
    elif [ -d "$PWD/assets" ] && [ -f "$PWD/assets/config.toml" ]; then
        LOCAL_ASSETS="$PWD/assets"
    fi

    # Check for pre-built local binary
    if [ -f "$SCRIPT_DIR/target/release/$BINARY_NAME" ]; then
        LOCAL_BIN="$SCRIPT_DIR/target/release/$BINARY_NAME"
    elif [ -f "$PWD/target/release/$BINARY_NAME" ]; then
        LOCAL_BIN="$PWD/target/release/$BINARY_NAME"
    fi

    # 1. Install or Update Binary
    if [ "$INSTALL_BIN" = true ]; then
        if [ -n "$LOCAL_BIN" ]; then
            install_binary_local "$LOCAL_BIN"
        elif [ -n "$LOCAL_ASSETS" ] && command -v cargo >/dev/null 2>&1; then
            step "Building release binary via cargo..."
            (cd "$SCRIPT_DIR" && cargo build --release --workspace)
            install_binary_local "$SCRIPT_DIR/target/release/$BINARY_NAME"
        else
            install_binary_remote
        fi
    fi

    # 2. Install Configurations & Presets (.toml)
    if [ "$INSTALL_CONFIG" = true ]; then
        if [ -n "$LOCAL_ASSETS" ]; then
            install_configs_local "$LOCAL_ASSETS"
        else
            install_configs_remote
        fi
    fi

    # 3. Path Warning
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *) warn "$INSTALL_DIR is not currently in your PATH. Add it via: export PATH=\"\$PATH:$INSTALL_DIR\"" ;;
    esac

    printf "\n%b✨ Waymaker installation complete!%b\n" "${GREEN}${BOLD}" "${NC}"
    printf "Try running:\n"
    printf "  %bwm --help%b             # Verify installation & view options\n" "${CYAN}" "${NC}"
    printf "  %bwm -o jump%b            # Launch frecency file manager & jumper\n" "${CYAN}" "${NC}"
    printf "  %bwm -o workspace%b       # Inspect workspace files & markdown diagrams\n" "${CYAN}" "${NC}"
    printf "  %bwm session%b            # Manage and switch tmux sessions\n\n" "${CYAN}" "${NC}"
}

main "$@"
