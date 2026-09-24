@echo off
setlocal
cd /d "%~dp0"
echo ========================================================
echo   Launching Blue Engine Antigravity (BEA)...
echo ========================================================
if "%~1"=="" (
    start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0BEA_Launcher.ps1"
    exit /b 0
)

if exist "bin\BEA.exe" (
    start "" "bin\BEA.exe" %*
) else if exist "target\release\be2.exe" (
    start "" "target\release\be2.exe" %*
) else (
    cargo run --release --bin be2 -- %*
)
