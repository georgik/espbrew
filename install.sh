#!/usr/bin/env bash

# espbrew installer script
# Usage: curl -L https://georgik.github.io/espbrew/install.sh | bash

set -euo pipefail

# Configuration
GITHUB_REPO="georgik/espbrew"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="espbrew"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Detect platform and architecture
detect_platform() {
    local os
    local arch
    local platform

    # Detect OS
    case "$(uname -s)" in
        Darwin*)
            os="macos"
            ;;
        Linux*)
            os="linux"
            ;;
        CYGWIN*|MINGW*|MSYS*)
            os="windows"
            ;;
        *)
            log_error "Unsupported operating system: $(uname -s)"
            exit 1
            ;;
    esac

    # Detect architecture
    case "$(uname -m)" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            if [[ "$os" == "macos" ]]; then
                arch="arm64"
            else
                arch="x86_64"  # Fallback to x86_64 for non-macOS ARM systems
                log_warn "ARM64 detected on $os, falling back to x86_64 binary"
            fi
            ;;
        *)
            log_warn "Unsupported architecture: $(uname -m), falling back to x86_64"
            arch="x86_64"
            ;;
    esac

    # Construct platform string
    if [[ "$os" == "macos" && "$arch" == "arm64" ]]; then
        platform="macos-arm64"
    elif [[ "$os" == "linux" ]]; then
        platform="linux-x86_64"
    elif [[ "$os" == "windows" ]]; then
        platform="windows-x86_64"
    else
        log_error "Unsupported platform combination: $os-$arch"
        exit 1
    fi

    echo "$platform"
}

# Get the latest release version from GitHub API
get_latest_version() {
    local latest_release_url="https://api.github.com/repos/$GITHUB_REPO/releases/latest"
    
    if command -v curl >/dev/null 2>&1; then
        curl -s "$latest_release_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    elif command -v wget >/dev/null 2>&1; then
        wget -qO- "$latest_release_url" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    else
        log_error "Neither curl nor wget is available. Please install one of them."
        exit 1
    fi
}

# Download and extract the binary
download_and_install() {
    local platform="$1"
    local version="$2"
    local archive_name
    local download_url
    local temp_dir

    # Determine archive name and extension
    if [[ "$platform" == *"windows"* ]]; then
        archive_name="espbrew-${platform}.zip"
    else
        archive_name="espbrew-${platform}.tar.gz"
    fi

    download_url="https://github.com/$GITHUB_REPO/releases/download/$version/$archive_name"
    temp_dir=$(mktemp -d)

    log_info "Downloading espbrew $version for $platform..."
    log_info "Download URL: $download_url"

    # Download the archive
    if command -v curl >/dev/null 2>&1; then
        if ! curl -sL "$download_url" -o "$temp_dir/$archive_name"; then
            log_error "Failed to download $archive_name"
            exit 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if ! wget -q "$download_url" -O "$temp_dir/$archive_name"; then
            log_error "Failed to download $archive_name"
            exit 1
        fi
    else
        log_error "Neither curl nor wget is available. Please install one of them."
        exit 1
    fi

    # Extract the archive
    log_info "Extracting archive..."
    
    if [[ "$platform" == *"windows"* ]]; then
        if command -v unzip >/dev/null 2>&1; then
            unzip -q "$temp_dir/$archive_name" -d "$temp_dir"
        else
            log_error "unzip is required to extract Windows archives"
            exit 1
        fi
        binary_path="$temp_dir/${BINARY_NAME}.exe"
    else
        tar -xzf "$temp_dir/$archive_name" -C "$temp_dir"
        binary_path="$temp_dir/$BINARY_NAME"
    fi

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install the binary
    if [[ -f "$binary_path" ]]; then
        cp "$binary_path" "$INSTALL_DIR/$BINARY_NAME"
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
        log_success "espbrew installed to $INSTALL_DIR/$BINARY_NAME"
    else
        log_error "Binary not found in archive"
        exit 1
    fi

    # Cleanup
    rm -rf "$temp_dir"
}

# Check if espbrew is already installed
check_existing_installation() {
    if command -v espbrew >/dev/null 2>&1; then
        local current_version
        current_version=$(espbrew --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' || echo "unknown")
        log_warn "espbrew is already installed (version: $current_version)"
        echo -n "Do you want to overwrite it? [y/N] "
        read -r response
        case "$response" in
            [yY][eE][sS]|[yY])
                log_info "Proceeding with installation..."
                ;;
            *)
                log_info "Installation cancelled."
                exit 0
                ;;
        esac
    fi
}

# Add to PATH if necessary
add_to_path() {
    local shell_rc=""
    
    # Check if INSTALL_DIR is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        log_warn "$INSTALL_DIR is not in your PATH"
        
        # Detect shell and appropriate RC file
        case "$SHELL" in
            */bash)
                shell_rc="$HOME/.bashrc"
                ;;
            */zsh)
                shell_rc="$HOME/.zshrc"
                ;;
            */fish)
                shell_rc="$HOME/.config/fish/config.fish"
                ;;
            *)
                log_warn "Unknown shell: $SHELL"
                ;;
        esac
        
        if [[ -n "$shell_rc" ]]; then
            echo -n "Do you want to add $INSTALL_DIR to your PATH in $shell_rc? [y/N] "
            read -r response
            case "$response" in
                [yY][eE][sS]|[yY])
                    if [[ "$SHELL" == */fish ]]; then
                        echo "set -gx PATH $INSTALL_DIR \$PATH" >> "$shell_rc"
                    else
                        echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$shell_rc"
                    fi
                    log_success "Added $INSTALL_DIR to PATH in $shell_rc"
                    log_info "Please restart your shell or run: source $shell_rc"
                    ;;
                *)
                    log_info "You can manually add $INSTALL_DIR to your PATH"
                    ;;
            esac
        fi
    fi
}

# Main installation function
main() {
    log_info "Starting espbrew installation..."
    
    # Check for existing installation
    check_existing_installation
    
    # Detect platform
    local platform
    platform=$(detect_platform)
    log_info "Detected platform: $platform"
    
    # Get latest version
    local version
    version=$(get_latest_version)
    if [[ -z "$version" ]]; then
        log_error "Failed to get latest version"
        exit 1
    fi
    log_info "Latest version: $version"
    
    # Download and install
    download_and_install "$platform" "$version"
    
    # Add to PATH if necessary
    add_to_path
    
    # Verify installation
    if [[ -x "$INSTALL_DIR/$BINARY_NAME" ]]; then
        log_success "Installation completed successfully!"
        log_info "Run '$BINARY_NAME --help' to get started"
        
        # Show version if possible
        if [[ ":$PATH:" == *":$INSTALL_DIR:"* ]]; then
            log_info "Version: $("$INSTALL_DIR/$BINARY_NAME" --version 2>/dev/null || echo "Unable to get version")"
        fi
    else
        log_error "Installation verification failed"
        exit 1
    fi
}

# Run main function
main "$@"