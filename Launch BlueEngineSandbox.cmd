@echo off
cd /d "%~dp0"
if exist target\release\blueengine-sandbox.exe (
  start "" "target\release\blueengine-sandbox.exe"
) else (
  echo Build first: cargo build --release --locked --bin blueengine-sandbox --bin be2
  pause
)
