#!/usr/bin/env bash
# Builds and installs the cli-utils binaries to /usr/local/bin.
#
# Supported: Debian/Ubuntu and macOS (should also work on most other
# Linux distros that have bash + a C toolchain).

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" &>/dev/null && pwd)"
# Every workspace crate that builds a binary.
TOOLS=()
for main in "$SCRIPT_DIR"/src/*/src/main.rs; do
    TOOLS+=("$(basename "${main%/src/main.rs}")")
done
INSTALL_DIR="/usr/local/bin"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn()  { printf '\033[1;33m==>\033[0m %s\n' "$1"; }
error() { printf '\033[1;31m==>\033[0m %s\n' "$1" >&2; }

ensure_cargo() {
    if command -v cargo &>/dev/null; then
        return
    fi

    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    fi

    if command -v cargo &>/dev/null; then
        return
    fi

    warn "Rust's toolchain (cargo) was not found."
    read -r -p "Install it now via rustup (https://rustup.rs)? [y/N] " reply
    case "$reply" in
        [yY][eE][sS]|[yY])
            curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
            # shellcheck disable=SC1091
            source "$HOME/.cargo/env"
            ;;
        *)
            error "cargo is required. Install Rust from https://rustup.rs and re-run this script."
            exit 1
            ;;
    esac
}

ensure_cargo

info "Building: ${TOOLS[*]}"
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

SUDO=""
if [ ! -w "$INSTALL_DIR" ]; then
    SUDO="sudo"
fi

info "Installing to $INSTALL_DIR (may prompt for your password)"
for tool in "${TOOLS[@]}"; do
    $SUDO install -m 755 "$SCRIPT_DIR/target/release/$tool" "$INSTALL_DIR/$tool"
done

info "Installed to $INSTALL_DIR:"
for tool in "${TOOLS[@]}"; do
    printf '  %s\n' "$tool"
done
