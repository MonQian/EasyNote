@echo off
setlocal
cd /d "%~dp0"
if exist "%USERPROFILE%\.cargo\bin\cargo.exe" set "PATH=%USERPROFILE%\.cargo\bin;%PATH%"
if exist "release\EasyNote_1.0.0.exe" (
    start "" "release\EasyNote_1.0.0.exe"
    exit /b 0
)
where cargo >nul 2>nul
if errorlevel 1 (
    echo Rust toolchain is required. Install Rust from https://rustup.rs/ and try again.
    pause
    exit /b 1
)
where npm >nul 2>nul
if errorlevel 1 (
    echo Node.js and npm are required.
    pause
    exit /b 1
)
if not exist node_modules call npm install
call npm run tauri dev
if errorlevel 1 pause
