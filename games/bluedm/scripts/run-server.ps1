param(
    [string]$Listen = "0.0.0.0:4100",
    [string]$Key = "bluedm-family"
)
$ErrorActionPreference = "Stop"
Set-Location (Split-Path -Parent $PSScriptRoot)
cargo run --bin bluedm-server -- --listen $Listen --key $Key
