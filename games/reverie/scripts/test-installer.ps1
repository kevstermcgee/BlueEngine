$ErrorActionPreference = 'Stop'
$game = Split-Path (Split-Path $PSCommandPath -Parent) -Parent
$installer = Join-Path $game 'installers/reverie-setup-windows-x64.exe'
$install = Join-Path $env:RUNNER_TEMP 'reverie-installation-check'
function Assert($value, $message) { if (!$value) { throw $message } }
$p = Start-Process $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',("/DIR=`"$install`"")) -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Installer failed'
Assert (Test-Path (Join-Path $install 'reverie.exe')) 'Installed executable missing'
Assert (Test-Path (Join-Path $install 'assets/audio/wonder/bank.json')) 'Installed score missing'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'BlueEngine Games/Reverie.lnk'
Assert (Test-Path $shortcut) 'Start Menu shortcut missing'
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($shortcut)
Assert ($link.TargetPath -eq (Join-Path $install 'reverie.exe')) 'Shortcut target differs from installed game'
Assert ($link.WorkingDirectory -eq $install) 'Shortcut working directory differs'
New-Item (Join-Path $install 'saves') -ItemType Directory -Force | Out-Null
Set-Content (Join-Path $install 'saves/player-note.txt') 'preserve my museum'
Set-Content (Join-Path $install 'settings.json') '{}'
$p = Start-Process $installer -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',("/DIR=`"$install`"")) -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Upgrade failed'
Assert (Test-Path (Join-Path $install 'saves/player-note.txt')) 'Upgrade removed player data'
$p = Start-Process (Join-Path $install 'unins000.exe') -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -Wait -PassThru
Assert ($p.ExitCode -eq 0) 'Uninstaller failed'
Assert (!(Test-Path (Join-Path $install 'reverie.exe'))) 'Uninstaller retained executable'
Assert (Test-Path (Join-Path $install 'saves/player-note.txt')) 'Uninstaller removed player data'
Assert (!(Test-Path $shortcut)) 'Uninstaller retained shortcut'
Write-Output 'Installer, upgrade, shortcut and save-preserving uninstall passed.'
