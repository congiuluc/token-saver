<#
.SYNOPSIS
    token-saver installer for Windows.

.DESCRIPTION
    Downloads the latest (or a pinned) prebuilt release archive from GitHub,
    verifies its SHA-256 checksum, and installs the token-saver and ts binaries
    into a directory on your PATH.

.PARAMETER Version
    Release tag to install (for example v0.1.0). Defaults to the latest release.

.PARAMETER BinDir
    Install directory. Defaults to %LOCALAPPDATA%\Programs\token-saver.

.EXAMPLE
    irm https://raw.githubusercontent.com/congiuluc/token-saver/main/install.ps1 | iex

.EXAMPLE
    .\install.ps1 -Version v0.1.0
#>
[CmdletBinding()]
param(
    [string]$Version = $env:TOKEN_SAVER_VERSION,
    [string]$BinDir = $env:TOKEN_SAVER_BIN_DIR
)

$ErrorActionPreference = "Stop"
$repo = "congiuluc/token-saver"

if ([string]::IsNullOrWhiteSpace($Version)) { $Version = "latest" }
if ([string]::IsNullOrWhiteSpace($BinDir)) {
    $BinDir = Join-Path $env:LOCALAPPDATA "Programs\token-saver"
}

function Install-FromSource {
    param(
        [string]$Tag
    )

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw "release archive unavailable and cargo is not installed; install Rust or publish a GitHub release."
    }
    if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
        throw "release archive unavailable and git is not installed; install Git or publish a GitHub release."
    }

    $sourceRoot = Join-Path $tmp "cargo-root"
    $cargoArgs = @(
        "install"
        "--locked"
        "--force"
        "--root"
        $sourceRoot
        "--git"
        "https://github.com/$repo"
    )
    if ($Tag -ne "latest") {
        $cargoArgs += @("--tag", $Tag)
    }

    Write-Warning "Release archive unavailable; building token-saver from source with cargo."
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) {
        throw "cargo install failed with exit code $LASTEXITCODE."
    }

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item (Join-Path $sourceRoot "bin\token-saver.exe") (Join-Path $BinDir "token-saver.exe") -Force
    Copy-Item (Join-Path $sourceRoot "bin\ts.exe") (Join-Path $BinDir "ts.exe") -Force

    Write-Host "Installed token-saver.exe and ts.exe to $BinDir"

    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$BinDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$BinDir", "User")
        Write-Host "Added $BinDir to your user PATH. Restart your terminal to use it."
    }

    Write-Host ""
    Write-Host 'Run "token-saver --help" to get started.'
}

# Detect CPU architecture.
$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    "AMD64" { "x86_64" }
    "ARM64" { "aarch64" }
    default { throw "Unsupported architecture: $($env:PROCESSOR_ARCHITECTURE)" }
}

$target = "$arch-pc-windows-msvc"
$asset = "token-saver-$target.zip"

$base = if ($Version -eq "latest") {
    "https://github.com/$repo/releases/latest/download"
}
else {
    "https://github.com/$repo/releases/download/$Version"
}

$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("token-saver-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

try {
    $zipPath = Join-Path $tmp $asset
    Write-Host "Downloading $asset ..."
    try {
        Invoke-WebRequest -Uri "$base/$asset" -OutFile $zipPath -UseBasicParsing
    }
    catch {
        Install-FromSource -Tag $Version
        return
    }

    # Verify checksum when the .sha256 file is published.
    try {
        $shaPath = "$zipPath.sha256"
        Invoke-WebRequest -Uri "$base/$asset.sha256" -OutFile $shaPath -UseBasicParsing
        $expected = ((Get-Content $shaPath -Raw).Trim() -split '\s+')[0].ToLower()
        $actual = (Get-FileHash $zipPath -Algorithm SHA256).Hash.ToLower()
        if ($expected -ne $actual) {
            throw "Checksum verification failed (expected $expected, got $actual)."
        }
        Write-Host "Checksum verified."
    }
    catch [System.Net.WebException] {
        Write-Warning "Checksum file not available; skipping verification."
    }

    Expand-Archive -Path $zipPath -DestinationPath $tmp -Force

    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item (Join-Path $tmp "token-saver-$target\token-saver.exe") (Join-Path $BinDir "token-saver.exe") -Force
    Copy-Item (Join-Path $tmp "token-saver-$target\ts.exe") (Join-Path $BinDir "ts.exe") -Force

    Write-Host "Installed token-saver.exe and ts.exe to $BinDir"

    # Add the install directory to the user PATH if it is not already present.
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -notlike "*$BinDir*") {
        [Environment]::SetEnvironmentVariable("Path", "$userPath;$BinDir", "User")
        Write-Host "Added $BinDir to your user PATH. Restart your terminal to use it."
    }

    Write-Host ""
    Write-Host 'Run "token-saver --help" to get started.'
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
