# Full drag regression suite for the OS top-clamp snap-undo fix.
# Fresh pet required. Tests:
#   T1  off-screen up-drag 600px -> OS snaps to 0 -> restore to release point (-348)
#   T2  click-through press + cursor move + release (NO real drag) -> nothing happens
#   T3  on-screen up-drag 100px -> stays at release point
#   T4  on-screen down-drag 300px -> stays at release point
# IMPORTANT: SetWindowPos resets MUST pass SWP_NOSIZE (0x0001) or the window
# shrinks to 0x0 (this accidentally broke a previous run).
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W32reg {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
}
"@
[W32reg]::SetProcessDPIAware() | Out-Null
$pet = Get-Process desktop-pet | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $pet) { Write-Output "NO PET PROCESS"; exit 1 }
$h = $pet.MainWindowHandle
function GetRect { $r = New-Object W32reg+RECT; [W32reg]::GetWindowRect($h, [ref]$r) | Out-Null; return $r }
function DragRel {
  param([int]$dx, [int]$dy)
  $r0 = GetRect
  $gx = $r0.L + 300; $gy = $r0.T + 750
  [W32reg]::SetCursorPos($gx, $gy) | Out-Null
  Start-Sleep -Milliseconds 200
  [W32reg]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  $steps = [Math]::Max([Math]::Abs($dx), [Math]::Abs($dy))
  for ($i = 1; $i -le $steps; $i++) {
    $nx = $gx + [int]([double]$dx * $i / $steps)
    $ny = $gy + [int]([double]$dy * $i / $steps)
    [W32reg]::SetCursorPos($nx, $ny) | Out-Null
    Start-Sleep -Milliseconds 15
  }
  [W32reg]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 500
}
function ResetPos {
  param([int]$y)
  # SWP_NOSIZE (0x0001) | SWP_NOZORDER (0x0004) — never pass 0x0004 alone
  [W32reg]::SetWindowPos($h, [IntPtr]::Zero, 1931, $y, 0, 0, 0x0001 -bor 0x0004) | Out-Null
  Start-Sleep -Seconds 2
}
# --- T1: off-screen up-drag ---
$r = GetRect; Write-Output "T1 pre: L=$($r.L) T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T) (expect 600x1140)"
DragRel -dx 0 -dy -600
$r = GetRect; Write-Output "T1 post: L=$($r.L) T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T) (expect T=-348, full size)"
Start-Sleep -Milliseconds 1200
$r = GetRect; Write-Output "T1 hold: T=$($r.T) (expect -348)"
# --- T2: click-through press + move, no real drag ---
Start-Sleep -Milliseconds 2500  # let the arm clear wasDragged
$r0 = GetRect
[W32reg]::SetCursorPos($r0.L + 300, $r0.T + 100) | Out-Null
Start-Sleep -Milliseconds 250
[W32reg]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
for ($i = 1; $i -le 15; $i++) { [W32reg]::SetCursorPos($r0.L + 300, $r0.T + 100 - 20 * $i) | Out-Null; Start-Sleep -Milliseconds 30 }
[W32reg]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 900
$r = GetRect; Write-Output "T2 post: T=$($r.T) (expect -348, NO restore)"
# --- T3: on-screen up-drag ---
ResetPos 228
$r = GetRect; Write-Output "T3 pre: T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T)"
DragRel -dx 0 -dy -100
$r = GetRect; Write-Output "T3 post: T=$($r.T) (expect 128)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T3 hold: T=$($r.T) (expect 128)"
# --- T4: on-screen down-drag ---
DragRel -dx 0 -dy 300
$r = GetRect; Write-Output "T4 post: T=$($r.T) (expect 428)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T4 hold: T=$($r.T) (expect 428)"
Write-Output "SUITE DONE"
