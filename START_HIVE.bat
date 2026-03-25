@echo off
setlocal enabledelayedexpansion
title HIVE - Local AI Runtime
color 0A

:: Keep window open on error
if "%1"=="" (
    cmd /k "%~f0" run
    exit /b
)

echo.
echo  ================================================================
echo   HIVE - Standalone Local AI Runtime
echo  ================================================================
echo.

:: Set paths
set "SCRIPT_DIR=%~dp0"
set "DESKTOP_DIR=%SCRIPT_DIR%HIVE\desktop"
set "EXE_PATH=%DESKTOP_DIR%\src-tauri\target\release\HIVE.exe"

:: Check if already built - just launch it
if exist "%EXE_PATH%" (
    echo  [OK] HIVE.exe found - launching...
    start "" "%EXE_PATH%"
    exit /b 0
)

:: ================================================================
:: FULL DEPENDENCY CHECK
:: ================================================================
echo  Checking ALL build prerequisites...
echo.

set "MISSING="
set "HAS_MSVC=0"
set "HAS_MINGW=0"
set "HAS_WSL=0"

:: ----------------------------------------
:: 1. Node.js
:: ----------------------------------------
set "NODE_OK=0"
where node >nul 2>&1
if !errorlevel! equ 0 (
    set "NODE_OK=1"
    for /f "tokens=*" %%i in ('node --version 2^>nul') do echo  [OK] Node.js %%i
)
if "!NODE_OK!"=="0" (
    echo  [X] Node.js - NOT FOUND
    set "MISSING=!MISSING! nodejs"
)

:: ----------------------------------------
:: 2. Rust
:: ----------------------------------------
set "RUST_OK=0"
where rustc >nul 2>&1
if !errorlevel! equ 0 (
    set "RUST_OK=1"
    for /f "tokens=*" %%i in ('rustc --version 2^>nul') do echo  [OK] %%i
)
if "!RUST_OK!"=="0" (
    echo  [X] Rust - NOT FOUND
    set "MISSING=!MISSING! rust"
)

:: ----------------------------------------
:: 3. C++ Toolchain (MSVC or MinGW)
:: ----------------------------------------
where cl >nul 2>&1
if !errorlevel! equ 0 (
    set "HAS_MSVC=1"
    echo  [OK] MSVC cl.exe found
)

where gcc >nul 2>&1
if !errorlevel! equ 0 (
    where dlltool >nul 2>&1
    if !errorlevel! equ 0 (
        set "HAS_MINGW=1"
        echo  [OK] MinGW gcc + dlltool found
    )
)

if "!HAS_MSVC!"=="0" (
    if "!HAS_MINGW!"=="0" (
        echo  [X] C++ Toolchain - NOT FOUND
        set "MISSING=!MISSING! mingw"
    )
)

:: ----------------------------------------
:: 4. WebView2
:: ----------------------------------------
set "WV2_OK=0"
reg query "HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" >nul 2>&1
if !errorlevel! equ 0 set "WV2_OK=1"
reg query "HKEY_CURRENT_USER\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}" >nul 2>&1
if !errorlevel! equ 0 set "WV2_OK=1"

if "!WV2_OK!"=="1" (
    echo  [OK] WebView2 Runtime
) else (
    echo  [X] WebView2 - NOT FOUND
    set "MISSING=!MISSING! webview2"
)

:: ----------------------------------------
:: 5. WSL2
:: ----------------------------------------
echo.
echo  Checking runtime environment...
echo.

wsl --status >nul 2>&1
if !errorlevel! equ 0 (
    set "HAS_WSL=1"
    echo  [OK] WSL2 enabled

    :: Check ROCm
    wsl -e test -d /opt/rocm 2>nul
    if !errorlevel! equ 0 (
        for /f "tokens=*" %%v in ('wsl -e cat /opt/rocm/.info/version 2^>nul') do echo  [OK] ROCm %%v
    ) else (
        echo  [!] ROCm not found in WSL
    )

    :: Check llama-server in WSL
    wsl -e bash -c "test -f /usr/local/bin/llama-server" 2>nul
    if !errorlevel! equ 0 (
        echo  [OK] llama-server in WSL
    ) else (
        wsl -e bash -c "test -f ~/llama.cpp/build/bin/llama-server || test -f ~/llama.cpp/llama-server" 2>nul
        if !errorlevel! equ 0 (
            echo  [OK] llama-server found in WSL home
        ) else (
            echo  [!] llama-server not found in WSL
        )
    )
) else (
    echo  [!] WSL2 not enabled - needed for AMD GPUs
)

:: Check Windows llama-server (check same paths as Rust code)
set "LLAMA_SERVER_FOUND=0"
set "LLAMA_SERVER_PATHS="

:: Path 1: Next to HIVE.exe
if exist "%DESKTOP_DIR%\src-tauri\target\release\llama-server.exe" (
    set "LLAMA_SERVER_FOUND=1"
    echo  [OK] llama-server.exe found ^(next to HIVE.exe^)
)
:: Path 2: bin folder next to HIVE.exe
if exist "%DESKTOP_DIR%\src-tauri\target\release\bin\llama-server.exe" (
    set "LLAMA_SERVER_FOUND=1"
    echo  [OK] llama-server.exe found ^(bin folder^)
)
:: Path 3: %LocalAppData%\HIVE\bin
if exist "%LOCALAPPDATA%\HIVE\bin\llama-server.exe" (
    set "LLAMA_SERVER_FOUND=1"
    echo  [OK] llama-server.exe found ^(AppData^)
)

if "!LLAMA_SERVER_FOUND!"=="0" (
    echo  [!] llama-server.exe not found for Windows
    echo      Expected in one of:
    echo        - "!DESKTOP_DIR!\src-tauri\target\release\"
    echo        - "!DESKTOP_DIR!\src-tauri\target\release\bin\"
    echo        - "%LOCALAPPDATA%\HIVE\bin\"
)

:: ================================================================
:: SUMMARY
:: ================================================================
echo.
echo  ================================================================

if "!MISSING!"=="" (
    echo   All build prerequisites found!
    echo  ================================================================
    goto :build
)

echo   Missing:!MISSING!
echo  ================================================================
echo.

:: ----------------------------------------
:: AUTO-INSTALL
:: ----------------------------------------
where winget >nul 2>&1
if !errorlevel! neq 0 (
    echo  winget not available. Install manually:
    echo.
    echo !MISSING! | findstr /c:"nodejs" >nul
    if !errorlevel! equ 0 echo   - Node.js: https://nodejs.org/
    echo !MISSING! | findstr /c:"rust" >nul
    if !errorlevel! equ 0 echo   - Rust: https://rustup.rs/
    echo !MISSING! | findstr /c:"mingw" >nul
    if !errorlevel! equ 0 echo   - MSYS2: https://www.msys2.org/
    echo !MISSING! | findstr /c:"webview2" >nul
    if !errorlevel! equ 0 echo   - WebView2: https://developer.microsoft.com/microsoft-edge/webview2/
    echo.
    pause
    exit /b 1
)

echo  Installing missing dependencies...
echo.

:: Install Node
echo !MISSING! | findstr /c:"nodejs" >nul
if !errorlevel! equ 0 (
    echo  [*] Installing Node.js...
    winget install OpenJS.NodeJS.LTS --silent --accept-package-agreements --accept-source-agreements
)

:: Install Rust
echo !MISSING! | findstr /c:"rust" >nul
if !errorlevel! equ 0 (
    echo  [*] Installing Rust...
    winget install Rustlang.Rustup --silent --accept-package-agreements --accept-source-agreements
)

:: Install MinGW
echo !MISSING! | findstr /c:"mingw" >nul
if !errorlevel! equ 0 (
    echo  [*] Installing MSYS2...
    winget install MSYS2.MSYS2 --silent --accept-package-agreements --accept-source-agreements
    timeout /t 5 /nobreak >nul

    if exist "C:\msys64\usr\bin\bash.exe" (
        echo  [*] Installing MinGW toolchain...
        C:\msys64\usr\bin\bash.exe -lc "pacman -Sy --noconfirm mingw-w64-x86_64-toolchain"

        echo  [*] Adding MinGW to PATH...
        setx PATH "C:\msys64\mingw64\bin;%PATH%" >nul 2>&1
        set "PATH=C:\msys64\mingw64\bin;!PATH!"

        echo  [*] Setting Rust to GNU toolchain...
        rustup default stable-x86_64-pc-windows-gnu >nul 2>&1
    )
)

:: Install WebView2
echo !MISSING! | findstr /c:"webview2" >nul
if !errorlevel! equ 0 (
    echo  [*] Installing WebView2...
    winget install Microsoft.EdgeWebView2Runtime --silent --accept-package-agreements --accept-source-agreements
)

echo.
echo  ================================================================
echo   Done. Close this window and run START_HIVE.bat again.
echo  ================================================================
pause
exit /b 0

:: ================================================================
:: BUILD
:: ================================================================
:build
echo.
echo  Building HIVE (first build takes ~5 min)...
echo.

cd /d "%DESKTOP_DIR%"

if not exist "node_modules" (
    echo  [*] npm install...
    call npm install
)

:: Set toolchain
if "!HAS_MINGW!"=="1" (
    rustup default stable-x86_64-pc-windows-gnu >nul 2>&1
    echo  [*] Using GNU toolchain
) else (
    rustup default stable-x86_64-pc-windows-msvc >nul 2>&1
    echo  [*] Using MSVC toolchain
)

echo  [*] Building...
call npm run tauri build

if !errorlevel! neq 0 (
    echo.
    echo  BUILD FAILED - see error above
    pause
    exit /b 1
)

echo.
echo  ================================================================
echo   BUILD COMPLETE!
echo  ================================================================

if exist "%EXE_PATH%" (
    echo  Launching HIVE...
    start "" "%EXE_PATH%"
)

pause
exit /b 0
