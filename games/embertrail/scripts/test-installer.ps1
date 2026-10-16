$ErrorActionPreference = 'Stop'
$game = Split-Path (Split-Path $PSCommandPath -Parent) -Parent
$installer = Join-Path $game 'installers/embertrail-setup-windows-x64.exe'
$install = Join-Path $env:RUNNER_TEMP 'embertrail-installation-check'
function Assert($value, $message) { if (!$value) { throw $message } }
$p = Start-Process $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',("/DIR=`"$install`"")) -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Installer failed'
Assert (Test-Path (Join-Path $install 'embertrail.exe')) 'Installed executable missing'
Assert (Test-Path (Join-Path $install 'assets/audio/score/tracks.json')) 'Installed soundtrack catalogue missing'
Assert (Test-Path (Join-Path $install 'assets/fonts/DejaVu-LICENSE.txt')) 'Installed font license missing'
$catalogue = Get-Content (Join-Path $install 'assets/audio/score/tracks.json') -Raw | ConvertFrom-Json
Assert ($catalogue.Count -gt 0) 'Installed soundtrack catalogue is empty'
foreach ($track in $catalogue) {
    Assert (Test-Path (Join-Path $install ("assets/audio/score/{0}/bank.json" -f $track.id))) 'Installed track bank missing'
    Assert (Test-Path (Join-Path $install ("assets/audio/score/{0}/song.wav" -f $track.id))) 'Installed track recording missing'
}
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'BlueEngine Games/Embertrail.lnk'
Assert (Test-Path $shortcut) 'Start Menu shortcut missing'
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
Assert ($link.TargetPath -eq (Join-Path $install 'embertrail.exe')) 'Shortcut target differs from installed game'
Assert ($link.WorkingDirectory -eq $install) 'Shortcut working directory differs'
$data = Join-Path $env:LOCALAPPDATA 'BlueEngine'
New-Item $data -ItemType Directory -Force | Out-Null
$save = Join-Path $data 'blueengine_embertrail_v1_quick'
$progress = Join-Path $data 'blueengine_embertrail_v1_progress'
$settings = Join-Path $data 'blueengine_embertrail_v1_settings'
# Refuse to overwrite any existing player files when this verifier is run outside CI.
Assert (!(Test-Path $save)) 'Existing save data present; use a clean Windows test account'
Assert (!(Test-Path $progress)) 'Existing campaign progress present; use a clean Windows test account'
Assert (!(Test-Path $settings)) 'Existing settings present; use a clean Windows test account'
# Test retention at the runtime's actual save/settings paths without launching over fixture data.
Set-Content $save 'installer-retention-fixture'
Set-Content $progress 'installer-campaign-retention-fixture'
Set-Content $settings '{"sound":true,"music":true}'
$saveHash = (Get-FileHash $save -Algorithm SHA256).Hash
$progressHash = (Get-FileHash $progress -Algorithm SHA256).Hash
$settingsHash = (Get-FileHash $settings -Algorithm SHA256).Hash
$expectedExe = (Get-FileHash (Join-Path $game 'dist/embertrail.exe') -Algorithm SHA256).Hash
Assert ((Get-FileHash (Join-Path $install 'embertrail.exe') -Algorithm SHA256).Hash -eq $expectedExe) 'Installer changed the verified executable'
Assert (!(Get-ChildItem $install -Recurse -Filter '*.wasm')) 'Installer contains WASM'
$distRoot = (Resolve-Path (Join-Path $game 'dist')).Path.TrimEnd('\')
foreach ($file in Get-ChildItem (Join-Path $distRoot 'assets') -File -Recurse) {
    $relative = $file.FullName.Substring($distRoot.Length + 1)
    $installed = Join-Path $install $relative
    Assert (Test-Path $installed) ("Installed asset missing: {0}" -f $relative)
    Assert ((Get-FileHash $installed -Algorithm SHA256).Hash -eq (Get-FileHash $file.FullName -Algorithm SHA256).Hash) ("Installer changed asset: {0}" -f $relative)
}
$p = Start-Process $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',("/DIR=`"$install`"")) -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Upgrade failed'
Assert ((Get-FileHash $save -Algorithm SHA256).Hash -eq $saveHash) 'Upgrade changed save data'
Assert ((Get-FileHash $progress -Algorithm SHA256).Hash -eq $progressHash) 'Upgrade changed campaign progress'
Assert ((Get-FileHash $settings -Algorithm SHA256).Hash -eq $settingsHash) 'Upgrade changed settings'
$p = Start-Process (Join-Path $install 'unins000.exe') -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Uninstaller failed'
Assert (!(Test-Path (Join-Path $install 'embertrail.exe'))) 'Uninstaller retained executable'
Assert ((Get-FileHash $save -Algorithm SHA256).Hash -eq $saveHash) 'Uninstaller changed save data'
Assert ((Get-FileHash $progress -Algorithm SHA256).Hash -eq $progressHash) 'Uninstaller changed campaign progress'
Assert ((Get-FileHash $settings -Algorithm SHA256).Hash -eq $settingsHash) 'Uninstaller changed settings'
Assert (!(Test-Path $shortcut)) 'Uninstaller retained shortcut'
Write-Output 'Installer, upgrade, shortcut and save-preserving uninstall passed.'
