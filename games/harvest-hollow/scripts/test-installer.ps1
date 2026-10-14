$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$game = Split-Path (Split-Path $PSCommandPath -Parent) -Parent
$installer = Join-Path $game 'installers/harvest-hollow-setup-windows-x64.exe'
function Assert($value, $message) { if (!$value) { throw $message } }
Assert ([bool]$env:RUNNER_TEMP) 'Use a clean Windows CI test account with RUNNER_TEMP set'
$install = Join-Path $env:RUNNER_TEMP 'harvest-hollow-installation-check'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'BlueEngine Games/Harvest Hollow.lnk'
$desktop = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Harvest Hollow.lnk'
Assert (!(Test-Path $install)) 'Installation test directory already exists'
Assert (!(Test-Path $shortcut)) 'Existing Start Menu shortcut present; use a clean test account'
Assert (!(Test-Path $desktop)) 'Existing desktop shortcut present; use a clean test account'
$arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', '/TASKS=desktopicon', ("/DIR=`"$install`""))
$p = Start-Process $installer -ArgumentList $arguments -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Installer failed'
$installedExe = Join-Path $install 'harvest-hollow.exe'
Assert (Test-Path $installedExe) 'Installed executable missing'
Assert (Test-Path $shortcut) 'Start Menu shortcut missing'
Assert (Test-Path $desktop) 'Requested desktop shortcut missing'
$shell = New-Object -ComObject WScript.Shell
foreach ($path in @($shortcut, $desktop)) {
    $link = $shell.CreateShortcut($path)
    Assert ($link.TargetPath -eq $installedExe) 'Shortcut target differs from installed game'
    Assert ($link.WorkingDirectory -eq $install) 'Shortcut working directory differs'
}
$stamp = Get-Content (Join-Path $game 'dist/ship.json') -Raw | ConvertFrom-Json
foreach ($file in $stamp.file_sha256.PSObject.Properties) {
    $installed = Join-Path $install $file.Name
    Assert (Test-Path $installed -PathType Leaf) "Installed payload missing: $($file.Name)"
    $hash = (Get-FileHash $installed -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert ($hash -eq $file.Value) "Installer changed verified payload: $($file.Name)"
}
$banks = @(Get-ChildItem (Join-Path $install 'assets/audio') -Filter 'bank.json' -Recurse -File)
Assert ($banks.Count -ge 4) 'Installer omitted soundtrack banks'
Assert (@(Get-ChildItem (Join-Path $install 'assets/audio') -Filter '*.wav' -Recurse -File).Count -gt 0) 'Installer omitted rendered music'
Assert (@(Get-ChildItem $install -Recurse -Filter '*.wasm').Count -eq 0) 'Installer contains WASM'

# Exercise retention at the engine-owned game storage paths. Never overwrite player data.
$data = Join-Path $env:LOCALAPPDATA 'BlueEngine'
New-Item $data -ItemType Directory -Force | Out-Null
$fixtures = @(
    (Join-Path $data 'blueengine_harvest-hollow_v1_progress'),
    (Join-Path $data 'blueengine_harvest-hollow_v1_quick'),
    (Join-Path $data 'blueengine_harvest-hollow_v1_settings')
)
foreach ($path in $fixtures) {
    Assert (!(Test-Path $path)) 'Existing player data present; use a clean Windows test account'
}
foreach ($path in $fixtures) {
    Set-Content $path 'harvest-hollow-installer-retention-fixture'
}
$retained = @{}
foreach ($path in $fixtures) {
    $retained[$path] = (Get-FileHash $path -Algorithm SHA256).Hash
}
$p = Start-Process $installer -ArgumentList $arguments -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Upgrade failed'
foreach ($path in $fixtures) {
    Assert ((Get-FileHash $path -Algorithm SHA256).Hash -eq $retained[$path]) 'Upgrade changed player save or settings data'
}
foreach ($file in $stamp.file_sha256.PSObject.Properties) {
    $hash = (Get-FileHash (Join-Path $install $file.Name) -Algorithm SHA256).Hash.ToLowerInvariant()
    Assert ($hash -eq $file.Value) "Upgrade changed verified payload: $($file.Name)"
}
$p = Start-Process (Join-Path $install 'unins000.exe') -ArgumentList @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART') -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Uninstaller failed'
Assert (!(Test-Path $installedExe)) 'Uninstaller retained executable'
Assert (!(Test-Path $shortcut)) 'Uninstaller retained Start Menu shortcut'
Assert (!(Test-Path $desktop)) 'Uninstaller retained desktop shortcut'
foreach ($path in $fixtures) {
    Assert ((Get-FileHash $path -Algorithm SHA256).Hash -eq $retained[$path]) 'Uninstaller changed player save or settings data'
}
Remove-Item -LiteralPath $fixtures
Write-Output 'Installer payload, four soundtrack banks, both shortcuts, upgrade and save-preserving uninstall passed.'
