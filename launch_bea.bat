@echo off
setlocal
cd /d "%~dp0"
rem Compatibility: bare BEA opens the supported workbench. Old stock-client args are preserved.
if not "%~1"=="" goto stock
if exist "blueengine-sandbox.exe" (
    "blueengine-sandbox.exe"
    exit /b
)
if exist "bin\blueengine-sandbox.exe" (
    "bin\blueengine-sandbox.exe"
    exit /b
)
cargo run --profile fast --bin blueengine-sandbox --
exit /b
:stock
if exist "be2.exe" (
    "be2.exe" %*
    exit /b
)
if exist "bin\BE2.exe" (
    "bin\BE2.exe" %*
    exit /b
)
if exist "target\release\be2.exe" (
    "target\release\be2.exe" %*
    exit /b
)
cargo run --profile fast --bin be2 -- %*
