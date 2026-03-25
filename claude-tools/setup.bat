@echo off
setlocal enabledelayedexpansion

echo.
echo  ================================================================
echo   Claude Code Tools Setup
echo  ================================================================
echo.

:: Check for Node.js
where node >nul 2>&1
if %errorlevel% neq 0 (
    echo [ERROR] Node.js is required. Install from https://nodejs.org/
    pause
    exit /b 1
)

echo [1/4] Installing mgrep...
call npm install -g @mixedbread/mgrep
if %errorlevel% neq 0 (
    echo [ERROR] Failed to install mgrep
    pause
    exit /b 1
)
echo [OK] mgrep installed

echo.
echo [2/4] Authenticating mgrep...
echo Please complete the login in your browser...
call mgrep login
if %errorlevel% neq 0 (
    echo [WARN] Login skipped or failed - you can run 'mgrep login' later
)

echo.
echo [3/4] Installing Claude Code integration...
call mgrep install-claude-code
if %errorlevel% neq 0 (
    echo [WARN] Claude Code integration skipped
)

echo.
echo [4/4] Copying configuration...
set "SCRIPT_DIR=%~dp0"
set "REPO_ROOT=%SCRIPT_DIR%.."
copy "%SCRIPT_DIR%.mgreprc.yaml" "%REPO_ROOT%\.mgreprc.yaml" >nul
echo [OK] Configuration copied to repository root

echo.
echo  ================================================================
echo   Setup Complete!
echo  ================================================================
echo.
echo To start indexing this repository, run:
echo   mgrep watch %REPO_ROOT%
echo.
echo Then you can search with:
echo   mgrep "your natural language query"
echo.
pause
