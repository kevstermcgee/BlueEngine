param([string]$cmd = "help", [Parameter(ValueFromRemainingArguments=$true)][string[]]$CheckArgs)
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

switch ($cmd) {
    "web" {
        $sub = if ($CheckArgs.Count) { $CheckArgs[0] } else { "build" }
        $rest = if ($CheckArgs.Count -gt 1) { $CheckArgs[1..($CheckArgs.Count-1)] } else { @() }
        python "$Root/../../../tools/be2.py" web $sub $Root @rest
        exit $LASTEXITCODE
    }
    "publish" {
        python "$Root/../../../tools/be2.py" web publish $Root @CheckArgs
        exit $LASTEXITCODE
    }
    "check" {
        python scripts/check.py @CheckArgs
        exit $LASTEXITCODE
    }
    "build-all" {
        cargo build --release
        exit $LASTEXITCODE
    }
    "dev" {
        python scripts/dev.py @CheckArgs
        exit $LASTEXITCODE
    }
    "play" {
        python scripts/dev.py --release
        exit $LASTEXITCODE
    }
    "package" {
        python scripts/ship.py package @CheckArgs
        exit $LASTEXITCODE
    }
    "shortcut" {
        python scripts/ship.py shortcut @CheckArgs
        exit $LASTEXITCODE
    }
    "ship" {
        python scripts/ship.py ship @CheckArgs
        exit $LASTEXITCODE
    }
    default {
        Write-Host "Usage: .\scripts\blue.ps1 {check|build-all|dev|play|package|shortcut|ship}"
    }
}
