# Creates a BlueEngine Desktop shortcut with the official project icon.
# Build first:  cargo build --release --bin blueengine-sandbox      Then:  powershell -File scripts\make_shortcut.ps1
$repo = Split-Path -Parent $PSScriptRoot
$exe = Join-Path $repo "target\release\blueengine-sandbox.exe"
if (-not (Test-Path $exe)) {
    $exe = Join-Path $repo "bin\blueengine-sandbox.exe"
}
if (-not (Test-Path $exe)) {
    throw "Build first: cargo build --release --bin blueengine-sandbox ($exe is missing)"
}

$desktop = [Environment]::GetFolderPath("Desktop")
$lnk = Join-Path $desktop "BlueEngineSandbox.lnk"

$ws = New-Object -ComObject WScript.Shell
$s = $ws.CreateShortcut($lnk)
$s.TargetPath = $exe
$s.Arguments = ""  # Bare launch opens the sandbox workbench
$s.WorkingDirectory = $repo
$s.IconLocation = (Join-Path $repo "assets\branding\blueengine.ico") + ",0"
$s.Description = "BlueEngine Sandbox - maps, asset inspection, characters and creative authoring"
$s.Save()

Write-Output "Created shortcut: $lnk"
Write-Output "Target: $exe"
Write-Output "Icon: $($s.IconLocation)"
