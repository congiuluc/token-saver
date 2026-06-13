# Installation

TokenSaver is cross-platform and runs on **Windows, Linux, and macOS** for both
`x86_64` and `arm64` (aarch64) CPUs. Choose whichever method fits your workflow.

## Prebuilt binaries (recommended)

Each tagged release publishes prebuilt archives for every supported platform on
the [GitHub Releases page](https://github.com/congiuluc/TokenSaver/releases). The
install scripts below pick the correct archive for your OS and architecture,
verify its SHA-256 checksum, and place the `tokensaver` and `ts` binaries on your
`PATH`.

### Linux and macOS

```sh
curl -fsSL https://raw.githubusercontent.com/congiuluc/TokenSaver/main/install.sh | sh
```

The binaries are installed to `~/.local/bin` by default. Override the location
or version with environment variables:

```sh
TOKENSAVER_BIN_DIR=/usr/local/bin TOKENSAVER_VERSION=v0.1.0 \
  curl -fsSL https://raw.githubusercontent.com/congiuluc/TokenSaver/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/congiuluc/TokenSaver/main/install.ps1 | iex
```

The binaries are installed to `%LOCALAPPDATA%\Programs\tokensaver` and that
directory is added to your user `PATH`. Restart your terminal afterwards. To pin
a version:

```powershell
$env:TOKENSAVER_VERSION = "v0.1.0"; irm https://raw.githubusercontent.com/congiuluc/TokenSaver/main/install.ps1 | iex
```

> Want to review the script first? Download
> [`install.sh`](https://github.com/congiuluc/TokenSaver/blob/main/install.sh) or
> [`install.ps1`](https://github.com/congiuluc/TokenSaver/blob/main/install.ps1),
> inspect it, and run it locally.

## Install with Cargo

If you already have the [Rust toolchain](https://rustup.rs/):

```sh
cargo install --git https://github.com/congiuluc/TokenSaver
```

This compiles and installs both binaries into `~/.cargo/bin`.

## Build from source

```sh
git clone https://github.com/congiuluc/TokenSaver
cd TokenSaver
cargo build --release
```

The optimized binaries are produced at:

```text
target/release/tokensaver(.exe)   # main binary
target/release/ts(.exe)           # short alias
```

The `.exe` suffix is present on Windows only. Copy them to a directory on your
`PATH` to finish.

## Verify

```sh
tokensaver --help
```

## Supported release archives

| Platform        | Architecture | Release archive                            |
| --------------- | ------------ | ------------------------------------------ |
| Linux           | x86_64       | `tokensaver-x86_64-unknown-linux-gnu.tar.gz`  |
| Linux           | arm64        | `tokensaver-aarch64-unknown-linux-gnu.tar.gz` |
| macOS           | x86_64       | `tokensaver-x86_64-apple-darwin.tar.gz`       |
| macOS           | arm64        | `tokensaver-aarch64-apple-darwin.tar.gz`      |
| Windows         | x86_64       | `tokensaver-x86_64-pc-windows-msvc.zip`       |
| Windows         | arm64        | `tokensaver-aarch64-pc-windows-msvc.zip`      |

Every archive ships with a matching `.sha256` checksum file.
