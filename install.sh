#!/bin/sh
# token-saver installer for Linux and macOS.
#
# Downloads the latest (or a pinned) prebuilt release archive from GitHub,
# verifies its SHA-256 checksum, and installs the `token-saver` and `tks`
# binaries into a directory on your PATH.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/congiuluc/token-saver/main/install.sh | sh
#
# Environment variables:
#   TOKEN_SAVER_VERSION   Tag to install (e.g. v0.1.0). Defaults to the latest release.
#   TOKEN_SAVER_BIN_DIR   Install directory. Defaults to ~/.local/bin.

set -eu

REPO="congiuluc/token-saver"
BIN_DIR="${TOKEN_SAVER_BIN_DIR:-$HOME/.local/bin}"
VERSION="${TOKEN_SAVER_VERSION:-latest}"

err() {
    printf 'error: %s\n' "$1" >&2
    exit 1
}

need() {
    command -v "$1" >/dev/null 2>&1 || err "missing required command: $1"
}

need uname
need tar
need mktemp

install_from_source() {
    command -v cargo >/dev/null 2>&1 || err "release archive unavailable and cargo is not installed; install Rust or publish a GitHub release."
    command -v git >/dev/null 2>&1 || err "release archive unavailable and git is not installed; install Git or publish a GitHub release."

    cargo_root="$tmp/cargo-root"
    printf 'Release archive unavailable; building token-saver from source with cargo.\n'
    if [ "$VERSION" = "latest" ]; then
        cargo install --locked --force --root "$cargo_root" --git "https://github.com/${REPO}" || err "cargo install failed"
    else
        cargo install --locked --force --root "$cargo_root" --git "https://github.com/${REPO}" --tag "$VERSION" || err "cargo install failed"
    fi

    mkdir -p "$BIN_DIR"
    install -m 0755 "$cargo_root/bin/token-saver" "$BIN_DIR/token-saver"
    install -m 0755 "$cargo_root/bin/tks" "$BIN_DIR/tks"

    printf 'Installed token-saver and tks to %s\n' "$BIN_DIR"

    case ":$PATH:" in
        *":$BIN_DIR:"*) ;;
        *)
            printf '\nNote: %s is not on your PATH. Add this to your shell profile:\n' "$BIN_DIR"
            printf '  export PATH="%s:$PATH"\n' "$BIN_DIR"
            ;;
    esac

    printf '\nRun "token-saver --help" to get started.\n'
}

# Pick a downloader.
if command -v curl >/dev/null 2>&1; then
    download() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
    download() { wget -qO "$2" "$1"; }
else
    err "need either curl or wget installed"
fi

# Detect operating system.
os="$(uname -s)"
case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) err "unsupported operating system: $os" ;;
esac

# Detect CPU architecture.
arch="$(uname -m)"
case "$arch" in
    x86_64 | amd64) arch="x86_64" ;;
    arm64 | aarch64) arch="aarch64" ;;
    *) err "unsupported architecture: $arch" ;;
esac

target="${arch}-${os}"
asset="token-saver-${target}.tar.gz"

if [ "$VERSION" = "latest" ]; then
    base="https://github.com/${REPO}/releases/latest/download"
else
    base="https://github.com/${REPO}/releases/download/${VERSION}"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

printf 'Downloading %s ...\n' "$asset"
if ! download "${base}/${asset}" "${tmp}/${asset}"; then
    install_from_source
    exit 0
fi

# Verify checksum when a checksum tool and the .sha256 file are available.
if download "${base}/${asset}.sha256" "${tmp}/${asset}.sha256" 2>/dev/null; then
    if command -v shasum >/dev/null 2>&1; then
        ( cd "$tmp" && shasum -a 256 -c "${asset}.sha256" >/dev/null 2>&1 ) \
            && printf 'Checksum verified.\n' || err "checksum verification failed"
    elif command -v sha256sum >/dev/null 2>&1; then
        ( cd "$tmp" && sha256sum -c "${asset}.sha256" >/dev/null 2>&1 ) \
            && printf 'Checksum verified.\n' || err "checksum verification failed"
    fi
fi

tar -xzf "${tmp}/${asset}" -C "$tmp"

mkdir -p "$BIN_DIR"
install -m 0755 "${tmp}/token-saver-${target}/token-saver" "${BIN_DIR}/token-saver"
install -m 0755 "${tmp}/token-saver-${target}/tks" "${BIN_DIR}/tks"

printf 'Installed token-saver and tks to %s\n' "$BIN_DIR"

case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *)
        printf '\nNote: %s is not on your PATH. Add this to your shell profile:\n' "$BIN_DIR"
        printf '  export PATH="%s:$PATH"\n' "$BIN_DIR"
        ;;
esac

printf '\nRun "token-saver --help" to get started.\n'
