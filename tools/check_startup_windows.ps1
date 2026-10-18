param(
  [Parameter(Mandatory = $true)][string]$Executable,
  [Parameter(Mandatory = $true)][string]$ExpectedTitle,
  [Parameter(Mandatory = $true)][string]$OutputDirectory,
  [string]$WorkingDirectory = '',
  [string]$OpenGLDirectory = '',
  [ValidateSet('F', 'F11')][string]$ToggleKey = 'F11',
  [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
if (-not ('BlueEngineStartupWindow' -as [type])) {
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class BlueEngineStartupWindow {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct MonitorInfo {
    public int Size; public Rect Monitor; public Rect Work; public uint Flags;
  }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
  [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] public static extern IntPtr GetWindowStyle(IntPtr window, int index);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr window, uint flags);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool GetMonitorInfo(IntPtr monitor, ref MonitorInfo info);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr window, int command);
  [DllImport("user32.dll")] public static extern void keybd_event(byte key, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)] public static extern bool SetDllDirectory(string path);
}
'@
}
[void][BlueEngineStartupWindow]::SetProcessDPIAware()
$Executable = (Resolve-Path -LiteralPath $Executable).Path
if (-not $WorkingDirectory) { $WorkingDirectory = Split-Path -Parent $Executable }
$WorkingDirectory = (Resolve-Path -LiteralPath $WorkingDirectory).Path
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
[void][IO.Directory]::CreateDirectory($OutputDirectory)
$playerDirectory = Join-Path $OutputDirectory 'isolated-player'
[void][IO.Directory]::CreateDirectory($playerDirectory)
$script:runningGame = $null
if ($OpenGLDirectory) {
  $OpenGLDirectory = (Resolve-Path -LiteralPath $OpenGLDirectory).Path
  if (-not [BlueEngineStartupWindow]::SetDllDirectory($OpenGLDirectory)) { throw 'Cannot select the explicitly requested CI OpenGL directory.' }
}

function Get-WindowBounds([IntPtr]$Window) {
  $bounds = [BlueEngineStartupWindow+Rect]::new()
  if (-not [BlueEngineStartupWindow]::GetWindowRect($Window, [ref]$bounds)) { throw 'Cannot read game window bounds.' }
  return $bounds
}

function Test-Fullscreen([IntPtr]$Window) {
  $bounds = Get-WindowBounds $Window
  $info = [BlueEngineStartupWindow+MonitorInfo]::new()
  $info.Size = [Runtime.InteropServices.Marshal]::SizeOf($info)
  $monitor = [BlueEngineStartupWindow]::MonitorFromWindow($Window, 2)
  if (-not [BlueEngineStartupWindow]::GetMonitorInfo($monitor, [ref]$info)) { throw 'Cannot read monitor bounds.' }
  # Miniquad can retain monitor-sized outer bounds when restoring a normal window.
  # Captions and resize borders distinguish that window from borderless fullscreen.
  $style = [BlueEngineStartupWindow]::GetWindowStyle($Window, -16).ToInt64()
  return ($style -band 0x00c40000) -eq 0 -and
    [Math]::Abs($bounds.Left - $info.Monitor.Left) -le 2 -and
    [Math]::Abs($bounds.Top - $info.Monitor.Top) -le 2 -and
    [Math]::Abs($bounds.Right - $info.Monitor.Right) -le 2 -and
    [Math]::Abs($bounds.Bottom - $info.Monitor.Bottom) -le 2
}

function Wait-WindowState([IntPtr]$Window, [bool]$Fullscreen) {
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  do {
    $script:runningGame.Refresh()
    if ($script:runningGame.HasExited) { throw 'Game exited during startup verification.' }
    if ((Test-Fullscreen $Window) -eq $Fullscreen) { return }
    Start-Sleep -Milliseconds 100
  } while ([DateTime]::UtcNow -lt $deadline)
  throw "Game window did not become fullscreen=$Fullscreen."
}

function Save-Window([IntPtr]$Window, [string]$Name, [switch]$StudioCard, [switch]$GameFrame) {
  $bounds = Get-WindowBounds $Window
  $width = $bounds.Right - $bounds.Left
  $height = $bounds.Bottom - $bounds.Top
  if ($width -lt 200 -or $height -lt 150) { throw 'Game window is too small to inspect.' }
  $bitmap = [Drawing.Bitmap]::new($width, $height)
  $graphics = [Drawing.Graphics]::FromImage($bitmap)
  try {
    $graphics.CopyFromScreen($bounds.Left, $bounds.Top, 0, 0, $bitmap.Size)
    $path = Join-Path $OutputDirectory "$Name.png"
    $bitmap.Save($path, [Drawing.Imaging.ImageFormat]::Png)
    if ($StudioCard) {
      # Inspect the rendered title area, not an embedded string or capture fixture.
      # The retained PNG is also reviewed visually before publication.
      $scale = [Math]::Min($width / 900.0, $height / 600.0)
      $lightPixels = 0
      $y0 = [Math]::Max(0, [int]($height * 0.43 + 10 * $scale))
      $y1 = [Math]::Min($height, [int]($height * 0.43 + 80 * $scale))
      $x0 = [Math]::Max(0, [int]($width / 2 - 230 * $scale))
      $x1 = [Math]::Min($width, [int]($width / 2 + 230 * $scale))
      for ($y = $y0; $y -lt $y1; $y += 3) {
        for ($x = $x0; $x -lt $x1; $x += 3) {
          $pixel = $bitmap.GetPixel($x, $y)
          if ($pixel.R -gt 170 -and $pixel.G -gt 180 -and $pixel.B -gt 190) { $lightPixels++ }
        }
      }
      if ($lightPixels -lt 30) { throw "Studio title was not visible in $Name.png (light title samples: $lightPixels)." }
    } elseif ($GameFrame) {
      # Ignore title bars/taskbars: they can be visible while the client is still black.
      $visiblePixels = 0
      for ($y = [int]($height * 0.12); $y -lt [int]($height * 0.90); $y += 12) {
        for ($x = [int]($width * 0.08); $x -lt [int]($width * 0.92); $x += 12) {
          $pixel = $bitmap.GetPixel($x, $y)
          if ($pixel.R -gt 32 -or $pixel.G -gt 32 -or $pixel.B -gt 32) { $visiblePixels++ }
        }
      }
      if ($visiblePixels -lt 30) { throw "Game/title content was still black in $Name.png." }
    }
    return @{ screenshot = "$Name.png"; width = $width; height = $height; left = $bounds.Left; top = $bounds.Top }
  } finally {
    $graphics.Dispose()
    $bitmap.Dispose()
  }
}

function Start-Game([bool]$Windowed) {
  $info = [Diagnostics.ProcessStartInfo]::new()
  $info.FileName = $Executable
  $info.WorkingDirectory = $WorkingDirectory
  $info.UseShellExecute = $false
  $info.Environment['BLUEENGINE_DATA_DIR'] = Join-Path $playerDirectory $(if ($Windowed) { 'windowed' } else { 'fullscreen' })
  $info.Environment['LOCALAPPDATA'] = Join-Path $playerDirectory 'LocalAppData'
  $info.Environment['APPDATA'] = Join-Path $playerDirectory 'AppData'
  if ($OpenGLDirectory) {
    $info.Environment['GALLIUM_DRIVER'] = 'llvmpipe'
    $info.Environment['LIBGL_ALWAYS_SOFTWARE'] = '1'
  }
  if ($Windowed) { $info.ArgumentList.Add('--windowed') }
  $script:runningGame = [Diagnostics.Process]::Start($info)
  $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
  do {
    $script:runningGame.Refresh()
    if ($script:runningGame.HasExited) { throw "Game exited before creating a window (exit $($script:runningGame.ExitCode))." }
    $window = $script:runningGame.MainWindowHandle
    if ($window -ne [IntPtr]::Zero -and [BlueEngineStartupWindow]::IsWindowVisible($window)) {
      if ($script:runningGame.MainWindowTitle -ne $ExpectedTitle) { throw "Unexpected title: $($script:runningGame.MainWindowTitle)" }
      [void][BlueEngineStartupWindow]::ShowWindow($window, 5)
      [void][BlueEngineStartupWindow]::SetForegroundWindow($window)
      return $window
    }
    Start-Sleep -Milliseconds 25
  } while ([DateTime]::UtcNow -lt $deadline)
  throw 'Game did not create a visible window.'
}

function Stop-Game {
  if ($script:runningGame) {
    $script:runningGame.Refresh()
    if (-not $script:runningGame.HasExited) {
      [void]$script:runningGame.CloseMainWindow()
      if (-not $script:runningGame.WaitForExit(1500)) {
        $script:runningGame.Kill($true)
        [void]$script:runningGame.WaitForExit(5000)
      }
    }
    $script:runningGame.Dispose()
    $script:runningGame = $null
  }
}

function Press-Toggle([IntPtr]$Window) {
  [void][BlueEngineStartupWindow]::SetForegroundWindow($Window)
  if ([BlueEngineStartupWindow]::GetForegroundWindow() -ne $Window) { throw 'Cannot focus the game for the fullscreen control check.' }
  $key = if ($ToggleKey -eq 'F11') { [byte]0x7a } else { [byte]0x46 }
  $scan = [byte][BlueEngineStartupWindow]::MapVirtualKey($key, 0)
  try {
    [BlueEngineStartupWindow]::keybd_event($key, $scan, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 100
  } finally {
    [BlueEngineStartupWindow]::keybd_event($key, $scan, 2, [UIntPtr]::Zero)
  }
}

try {
  $window = Start-Game $false
  Wait-WindowState $window $true
  Start-Sleep -Milliseconds 650
  $intro = Save-Window $window 'fullscreen-studio-card' -StudioCard
  Start-Sleep -Milliseconds 4500
  Wait-WindowState $window $true
  $game = Save-Window $window 'fullscreen-game' -GameFrame
  if ((Get-FileHash (Join-Path $OutputDirectory $intro.screenshot)).Hash -eq
      (Get-FileHash (Join-Path $OutputDirectory $game.screenshot)).Hash) { throw 'Studio card did not advance to the game.' }
  Press-Toggle $window
  Wait-WindowState $window $false
  $toggled = Save-Window $window 'first-toggle-windowed'
  Press-Toggle $window
  Wait-WindowState $window $true
  Stop-Game

  $window = Start-Game $true
  Wait-WindowState $window $false
  Start-Sleep -Milliseconds 650
  $windowed = Save-Window $window 'windowed-studio-card' -StudioCard
  Start-Sleep -Milliseconds 4500
  Wait-WindowState $window $false
  $windowedGame = Save-Window $window 'windowed-game' -GameFrame
  Stop-Game

  $proof = @{
    schema_version = 1; title = $ExpectedTitle; executable_sha256 = (Get-FileHash $Executable -Algorithm SHA256).Hash.ToLowerInvariant()
    normal_fullscreen = $true; studio_card = $intro; game_after_intro = $game
    toggle_key = $ToggleKey; first_toggle_windowed = $toggled; second_toggle_fullscreen = $true
    explicit_windowed_studio_card = $windowed; explicit_windowed_game = $windowedGame
    isolated_player_data = $true
  }
  $proof | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 (Join-Path $OutputDirectory 'startup-proof.json')
  Write-Host "Verified ${ExpectedTitle}: real studio card, fullscreen default, first $ToggleKey toggle, windowed override and isolated player data."
} finally {
  Stop-Game
  if ($OpenGLDirectory) { [void][BlueEngineStartupWindow]::SetDllDirectory($null) }
}
