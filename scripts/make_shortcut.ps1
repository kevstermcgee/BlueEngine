# Creates a Desktop shortcut that launches the Blue Test Lab in Blue Engine 2.
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
$lnk = Join-Path $desktop "Blue Test Lab.lnk"

$ws = New-Object -ComObject WScript.Shell
$s = $ws.CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = ""  # Bare launch opens Blue Test Lab by default
$s.WorkingDirectory = $repo
$s.IconLocation = (Join-Path $PSScriptRoot "test_lab.ico") + ",0"
$s.Description = "Blue Engine 2 - Test Lab (WASD walk, mouse look, E pick up props, click attack, wheel switches weapon)"
$s.Save()

Write-Output "Created shortcut: $lnk"
Write-Output "Target: $exe"
Write-Output "Icon: $($s.IconLocation)"
