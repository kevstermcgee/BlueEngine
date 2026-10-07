# Compatibility entry for installed BEA shortcuts. The separate map GUI is retired.
param([Parameter(ValueFromRemainingArguments=$true)][string[]] $LaunchArguments)
& (Join-Path $PSScriptRoot 'launch_bea.bat') @LaunchArguments
exit $LASTEXITCODE
