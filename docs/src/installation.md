# Installation

token-saver is cross-platform and runs on **Windows, Linux, and macOS** for both
`x86_64` and `arm64` (aarch64) CPUs. Choose whichever method fits your workflow.

## Prebuilt binaries (recommended)

Each tagged release publishes prebuilt archives for every supported platform on
the [GitHub Releases page](https://github.com/congiuluc/token-saver/releases). The
install scripts below pick the correct archive for your OS and architecture,
verify its SHA-256 checksum, and place the `token-saver` and `tks` binaries on your
`PATH`.
If the release archive is unavailable, the scripts fall back to building from
source with Cargo.

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/congiuluc/token-saver/main/install.sh | sh
```

The binaries are installed to `~/.local/bin` by default. Override the location
or version with environment variables:

```sh
TOKEN_SAVER_BIN_DIR=/usr/local/bin TOKEN_SAVER_VERSION=v0.1.0 \
  curl -fsSL https://raw.githubusercontent.com/congiuluc/token-saver/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/congiuluc/token-saver/main/install.ps1 | iex
```

The binaries are installed to `%LOCALAPPDATA%\Programs\token-saver` and that
directory is added to your user `PATH`. Restart your terminal afterwards. To pin
a version:

```powershell
$env:TOKEN_SAVER_VERSION = "v0.1.0"; irm https://raw.githubusercontent.com/congiuluc/token-saver/main/install.ps1 | iex
```

> Want to review the script first? Download
> [`install.sh`](https://github.com/congiuluc/token-saver/blob/main/install.sh) or
> [`install.ps1`](https://github.com/congiuluc/token-saver/blob/main/install.ps1),
> inspect it, and run it locally.

## Install with Cargo

If you already have the [Rust toolchain](https://rustup.rs/):

```sh
cargo install --git https://github.com/congiuluc/token-saver
```

This compiles and installs both binaries into `~/.cargo/bin`.

## Build from source

```sh
git clone https://github.com/congiuluc/token-saver
cd token-saver
cargo build --release
```

The optimized binaries are produced at:

```text
target/release/token-saver(.exe)   # main binary
target/release/tks(.exe)          # short alias
```

The `.exe` suffix is present on Windows only. Copy them to a directory on your
`PATH` to finish.

## The `tks` alias

Every install method places **two** binaries on your `PATH`:

- `token-saver` — the full command name.
- `tks` — a short alias with identical behavior.

`tks git status` is exactly the same as `token-saver git status`. The alias
exists purely to save keystrokes in interactive shells and AI agent prompts.

## Verify

```sh
token-saver --help
tks --version
```

## Supported release archives

| Platform        | Architecture | Release archive                            |
| --------------- | ------------ | ------------------------------------------ |
| Linux           | x86_64       | `token-saver-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux           | arm64        | `token-saver-aarch64-unknown-linux-gnu.tar.gz` |
| macOS           | x86_64       | `token-saver-x86_64-apple-darwin.tar.gz`       |
| macOS           | arm64        | `token-saver-aarch64-apple-darwin.tar.gz`      |
| Windows         | x86_64       | `token-saver-x86_64-pc-windows-msvc.zip`       |
| Windows         | arm64        | `token-saver-aarch64-pc-windows-msvc.zip`      |

Every archive ships with a matching `.sha256` checksum file.
