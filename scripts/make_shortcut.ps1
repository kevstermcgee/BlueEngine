# Creates a BlueEngine Desktop shortcut with the official project icon.
# Build first:  cargo build --release --bin be2      Then:  powershell -File scripts\make_shortcut.ps1
$repo = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repo "target\release\be2.exe"
if (-not (Test-Path $exe)) {
    $exe = Join-Path $repo "bin\BEA.exe"
}
if (-not (Test-Path $exe)) {
    throw "Build first: cargo build --release --bin be2 ($exe is missing)"
}

$desktop = [Environment]::GetFolderPath("Desktop")
$lnk = Join-Path $desktop "BlueEngine.lnk"

$ws = New-Object -ComObject WScript.Shell
$s = $ws.CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = ""  # Bare launch opens Blue Test Lab by default
$s.WorkingDirectory = $repo
$s.IconLocation = (Join-Path $repo "assets\branding\blueengine.ico") + ",0"
$s.Description = "BlueEngine - Test Lab (WASD walk, mouse look, E pick up props, click attack, wheel switches weapon)"
$s.Save()

Write-Output "Created shortcut: $lnk"
Write-Output "Target: $exe"
Write-Output "Icon: $($s.IconLocation)"
