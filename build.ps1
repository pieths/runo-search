# runo-search/build.ps1

$ErrorActionPreference = "Stop"

# ============================================================================
# Configuration
# ============================================================================

$rustToolchain     = "stable"
$rustupInitUrl     = "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe"
# NOTE: Update this hash when changing the rustup version.
# Get the current hash from: https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe.sha256
$expectedHash      = "88d8258dcf6ae4f7a80c7d1088e1f36fa7025a1cfd1343731b4ee6f385121fc0"

$projectRoot       = $PSScriptRoot
$rustLocalDir      = Join-Path $projectRoot "rust_local"
$rustupHome        = Join-Path $rustLocalDir "rustup"
$cargoHome         = Join-Path $rustLocalDir "cargo"
$cargoBin          = Join-Path $cargoHome "bin"
$cargoExe          = Join-Path $cargoBin "cargo.exe"
$rustupInitExe     = Join-Path $projectRoot "rustup-init.exe"

$nodeLocalDir      = Join-Path $projectRoot "node_local"
$nodeExe           = Join-Path $nodeLocalDir "node.exe"
$npmCmd            = Join-Path $nodeLocalDir "npm.cmd"

# ============================================================================
# Header
# ============================================================================

Write-Host ""
Write-Host "================================================" -ForegroundColor Cyan
Write-Host "  runo-search Build Script" -ForegroundColor Cyan
Write-Host "================================================" -ForegroundColor Cyan
Write-Host ""

# ============================================================================
# Step 1: Install local Node.js (if not present)
# ============================================================================

$nodeScript = Join-Path $PSScriptRoot "scripts\download_node.ps1"
& $nodeScript
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Failed to download Node.js!" -ForegroundColor Red
    exit 1
}

# Add local Node.js to PATH for this session
if ($env:Path -notlike "*$nodeLocalDir*") {
    $env:Path = "$nodeLocalDir;$env:Path"
}

Write-Host ""

# ============================================================================
# Step 2: Install local Rust toolchain (if not present)
# ============================================================================

# Set environment for local install — scoped to this process only
$env:RUSTUP_HOME = $rustupHome
$env:CARGO_HOME  = $cargoHome

if (Test-Path $cargoExe) {
    $rustcExe = Join-Path $cargoBin "rustc.exe"
    $currentVersion = & $rustcExe --version 2>$null
    Write-Host "Rust already installed locally: $currentVersion" -ForegroundColor Green
} else {
    Write-Host "Installing Rust toolchain locally to rust_local/..." -ForegroundColor Yellow

    # Download rustup-init.exe
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    try {
        Invoke-WebRequest -Uri $rustupInitUrl -OutFile $rustupInitExe -UseBasicParsing
        Write-Host "Downloaded rustup-init.exe" -ForegroundColor Green
    } catch {
        Write-Host "ERROR: Failed to download rustup-init.exe!" -ForegroundColor Red
        Write-Host $_.Exception.Message -ForegroundColor Red
        exit 1
    }

    # Verify SHA256 checksum
    Write-Host "Verifying SHA256 checksum..." -ForegroundColor Yellow
    $actualHash = (Get-FileHash -Path $rustupInitExe -Algorithm SHA256).Hash.ToLower()
    if ($actualHash -ne $expectedHash) {
        Write-Host "ERROR: Checksum verification failed!" -ForegroundColor Red
        Write-Host "  Expected: $expectedHash" -ForegroundColor Red
        Write-Host "  Actual:   $actualHash" -ForegroundColor Red
        Remove-Item $rustupInitExe -Force
        exit 1
    }
    Write-Host "Checksum verified" -ForegroundColor Green

    # Install Rust locally
    # -y              : non-interactive
    # --default-toolchain stable : install stable Rust
    # --no-modify-path : do NOT modify system PATH
    & $rustupInitExe -y --default-toolchain $rustToolchain --no-modify-path
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Rust installation failed!" -ForegroundColor Red
        Remove-Item $rustupInitExe -Force
        exit 1
    }

    # Clean up installer
    Remove-Item $rustupInitExe -Force

    $rustcExe = Join-Path $cargoBin "rustc.exe"
    $installedVersion = & $rustcExe --version 2>$null
    Write-Host "Rust installed: $installedVersion" -ForegroundColor Green
}

# Add local cargo/bin to PATH for this session
if ($env:Path -notlike "*$cargoBin*") {
    $env:Path = "$cargoBin;$env:Path"
}

Write-Host ""

# ============================================================================
# Step 3: Install npm dependencies (if needed)
# ============================================================================

$nodeModulesDir = Join-Path $projectRoot "node_modules"
if (!(Test-Path $nodeModulesDir)) {
    Write-Host "Installing npm dependencies..." -ForegroundColor Yellow
    & $npmCmd install
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: npm install failed!" -ForegroundColor Red
        exit 1
    }
    Write-Host "npm dependencies installed" -ForegroundColor Green
} else {
    Write-Host "npm dependencies already present" -ForegroundColor Green
}

Write-Host ""

# ============================================================================
# Step 4: Build the native addon
# ============================================================================

Write-Host "Building native addon (release)..." -ForegroundColor Yellow
& $npmCmd run build
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host ""

# ============================================================================
# Step 5: Package distribution zip
# ============================================================================

Write-Host "Packaging distribution zip..." -ForegroundColor Yellow

$distDir = Join-Path $projectRoot "dist"
$zipName = "runo-search.zip"
$zipPath = Join-Path $distDir $zipName

# Clean previous dist
if (Test-Path $distDir) {
    Remove-Item $distDir -Recurse -Force
}
New-Item -ItemType Directory -Path $distDir -Force | Out-Null

# Collect the 4 files needed by vsc-toolbox
$filesToPackage = @(
    (Join-Path $projectRoot "runo-search.win32-x64-msvc.node"),
    (Join-Path $projectRoot "index.js"),
    (Join-Path $projectRoot "index.d.ts"),
    (Join-Path $projectRoot "LICENSE")
)

foreach ($f in $filesToPackage) {
    if (!(Test-Path $f)) {
        Write-Host "ERROR: Required file not found: $f" -ForegroundColor Red
        exit 1
    }
    Copy-Item $f $distDir
}

# Create zip
Compress-Archive -Path (Join-Path $distDir "*") -DestinationPath $zipPath -Force
Write-Host "Distribution zip created: dist\$zipName" -ForegroundColor Green

Write-Host ""
Write-Host "================================================" -ForegroundColor Green
Write-Host "  Build Complete!" -ForegroundColor Green
Write-Host "================================================" -ForegroundColor Green
Write-Host ""
Write-Host "Output: dist\$zipName" -ForegroundColor Cyan
Write-Host "  - runo-search.win32-x64-msvc.node" -ForegroundColor Cyan
Write-Host "  - index.js" -ForegroundColor Cyan
Write-Host "  - index.d.ts" -ForegroundColor Cyan
Write-Host "  - LICENSE" -ForegroundColor Cyan
Write-Host ""
