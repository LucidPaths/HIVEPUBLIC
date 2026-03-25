# HIVE Desktop Application - One-Click Launcher (PowerShell)
# Right-click -> Run with PowerShell, OR double-click START_HIVE.bat

$ErrorActionPreference = "Stop"
$Host.UI.RawUI.WindowTitle = "HIVE - Starting..."

Write-Host ""
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host "  HIVE - Hierarchical Intelligence with Virtualized Execution" -ForegroundColor Cyan
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host ""

# Set paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DesktopDir = Join-Path $ScriptDir "HIVE\desktop"
$TauriDir = Join-Path $DesktopDir "src-tauri"
$ExePath = Join-Path $TauriDir "target\release\HIVE.exe"

# Check if already built
if (Test-Path $ExePath) {
    Write-Host "[OK] HIVE.exe found - launching..." -ForegroundColor Green
    Start-Process $ExePath
    exit 0
}

Write-Host "[INFO] HIVE.exe not found - need to build first" -ForegroundColor Yellow
Write-Host ""

# Function to check if command exists
function Test-Command($cmd) {
    try {
        Get-Command $cmd -ErrorAction Stop | Out-Null
        return $true
    } catch {
        return $false
    }
}

# Check Node.js
Write-Host "[1/5] Checking Node.js..." -ForegroundColor White
if (-not (Test-Command "node")) {
    Write-Host "[MISSING] Node.js not found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Opening Node.js download page..." -ForegroundColor Yellow
    Write-Host "Please install Node.js LTS, then run this script again." -ForegroundColor Yellow
    Start-Process "https://nodejs.org/"
    Read-Host "Press Enter after installing Node.js"
    exit 1
}
$nodeVer = node --version
Write-Host "[OK] Node.js $nodeVer" -ForegroundColor Green

# Check npm
Write-Host "[2/5] Checking npm..." -ForegroundColor White
if (-not (Test-Command "npm")) {
    Write-Host "[MISSING] npm not found" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}
$npmVer = npm --version
Write-Host "[OK] npm $npmVer" -ForegroundColor Green

# Check Rust
Write-Host "[3/5] Checking Rust..." -ForegroundColor White
if (-not (Test-Command "rustc")) {
    Write-Host "[MISSING] Rust not found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Opening Rust installer page..." -ForegroundColor Yellow
    Write-Host "Please install Rust, RESTART your computer, then run this script again." -ForegroundColor Yellow
    Start-Process "https://rustup.rs/"
    Read-Host "Press Enter to exit"
    exit 1
}
$rustVer = rustc --version
Write-Host "[OK] $rustVer" -ForegroundColor Green

# Install npm dependencies
Write-Host "[4/5] Installing dependencies..." -ForegroundColor White
Set-Location $DesktopDir
if (-not (Test-Path "node_modules")) {
    Write-Host "Installing npm packages (this may take a few minutes)..." -ForegroundColor Yellow
    npm install
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[ERROR] npm install failed" -ForegroundColor Red
        Read-Host "Press Enter to exit"
        exit 1
    }
}
Write-Host "[OK] Dependencies installed" -ForegroundColor Green

# Build Tauri app
Write-Host "[5/5] Building HIVE (this may take 5-10 minutes on first run)..." -ForegroundColor White
Write-Host ""
npm run tauri build
if ($LASTEXITCODE -ne 0) {
    Write-Host ""
    Write-Host "[ERROR] Build failed. Check the error messages above." -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

# Launch the app
Write-Host ""
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host "  BUILD COMPLETE - Launching HIVE..." -ForegroundColor Cyan
Write-Host " ================================================================" -ForegroundColor Cyan
Write-Host ""

if (Test-Path $ExePath) {
    Start-Process $ExePath
    Write-Host "[OK] HIVE is now running!" -ForegroundColor Green
} else {
    Write-Host "[ERROR] Build completed but HIVE.exe not found" -ForegroundColor Red
    Write-Host "Expected location: $ExePath" -ForegroundColor Red
    Read-Host "Press Enter to exit"
    exit 1
}

Write-Host ""
Write-Host "You can close this window." -ForegroundColor Gray
Start-Sleep -Seconds 5
