@echo off
cd /d "%~dp0"
if not exist "target\release\be2.exe" (
  echo Build first with: cargo build --locked --release --bin be2
  pause
  exit /b 1
)
"target\release\be2.exe" --game "assets\games\three-switches\game.json"
