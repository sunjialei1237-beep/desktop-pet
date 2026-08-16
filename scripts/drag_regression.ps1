# Full drag regression suite for the manual-drag + screen-wall fix.
# 2026-08-16 (续⁴²): walls hug the VISUAL body (no padding) — head can touch
# the screen TOP (window top goes off-screen, T<0) and the feet the taskbar.
# Fresh pet required. Tests:
#   T1  up-drag 600px -> top wall T=-360 (HEAD at screen y=0; no OS snap)
#   T2  click-through press + cursor move + release (NO real drag) -> nothing happens
#   T3  on-screen up-drag 100px -> stays at release point
#   T4  on-screen down-drag 300px -> stays at release point
#   T5  left-drag 2100px -> left wall L=-186 (visual left edge at screen left)
#   T6  right-drag 2300px -> right wall L=2146 (visual right edge at screen right)
#   T7  down-drag 900px -> bottom wall T=378 (FEET on taskbar top 1368)
# IMPORTANT: SetWindowPos resets MUST pass SWP_NOSIZE (0x0001) or the window
# shrinks to 0x0 (this accidentally broke a previous run).
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W32reg2 {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
}
"@
[W32reg2]::SetProcessDPIAware() | Out-Null
$pet = Get-Process desktop-pet | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $pet) { Write-Output "NO PET PROCESS"; exit 1 }
$h = $pet.MainWindowHandle
function GetRect { $r = New-Object W32reg2+RECT; [W32reg2]::GetWindowRect($h, [ref]$r) | Out-Null; return $r }
function DragRel {
  param([int]$dx, [int]$dy, [int]$grabY = 750)
  $r0 = GetRect
  $gx = $r0.L + 300; $gy = $r0.T + $grabY
  [W32reg2]::SetCursorPos($gx, $gy) | Out-Null
  Start-Sleep -Milliseconds 200
  [W32reg2]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  $steps = [Math]::Max([Math]::Abs($dx), [Math]::Abs($dy))
  for ($i = 1; $i -le $steps; $i++) {
    $nx = $gx + [int]([double]$dx * $i / $steps)
    $ny = $gy + [int]([double]$dy * $i / $steps)
    [W32reg2]::SetCursorPos($nx, $ny) | Out-Null
    Start-Sleep -Milliseconds 15
  }
  [W32reg2]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 500
}
function ResetPos {
  param([int]$x, [int]$y)
  # SWP_NOSIZE (0x0001) | SWP_NOZORDER (0x0004) — never pass 0x0004 alone
  [W32reg2]::SetWindowPos($h, [IntPtr]::Zero, $x, $y, 0, 0, 0x0001 -bor 0x0004) | Out-Null
  Start-Sleep -Seconds 2
}
# --- T1: up-drag -> top wall ---
$r = GetRect; Write-Output "T1 pre: L=$($r.L) T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T) (expect 600x1140)"
DragRel -dx 0 -dy -600
$r = GetRect; Write-Output "T1 post: T=$($r.T) (expect -360, head at screen top)"
Start-Sleep -Milliseconds 1200
$r = GetRect; Write-Output "T1 hold: T=$($r.T) (expect -360, no snap to 0)"
# --- T2: click-through press + move, no real drag ---
Start-Sleep -Milliseconds 2500  # let the arm clear wasDragged
$r0 = GetRect
[W32reg2]::SetCursorPos($r0.L + 300, $r0.T + 100) | Out-Null
Start-Sleep -Milliseconds 250
[W32reg2]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
for ($i = 1; $i -le 15; $i++) { [W32reg2]::SetCursorPos($r0.L + 300, $r0.T + 100 - 20 * $i) | Out-Null; Start-Sleep -Milliseconds 30 }
[W32reg2]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 900
$r = GetRect; Write-Output "T2 post: T=$($r.T) (expect -360, NO restore/move)"
# --- T3: on-screen up-drag ---
ResetPos 1931 228
DragRel -dx 0 -dy -100
$r = GetRect; Write-Output "T3 post: T=$($r.T) (expect ~137, stays at release point)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T3 hold: T=$($r.T) (expect ~137)"
# --- T4: on-screen down-drag ---
DragRel -dx 0 -dy 300
$r = GetRect; Write-Output "T4 post: T=$($r.T) (expect ~437, stays at release point)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T4 hold: T=$($r.T) (expect ~437)"
# --- T5: left-drag -> left wall ---
ResetPos 1931 228
DragRel -dx -2100 -dy 0
$r = GetRect; Write-Output "T5 post: L=$($r.L) T=$($r.T) (expect L=-186, left wall)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T5 hold: L=$($r.L) (expect -186)"
# --- T6: right-drag -> right wall ---
DragRel -dx 2300 -dy 0
$r = GetRect; Write-Output "T6 post: L=$($r.L) (expect 2146, right wall)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T6 hold: L=$($r.L) (expect 2146)"
# --- T7: down-drag -> bottom wall (floorY) ---
DragRel -dx 0 -dy 900
$r = GetRect; Write-Output "T7 post: T=$($r.T) (expect 378, feet on taskbar)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "T7 hold: T=$($r.T) (expect 378)"
Write-Output "SUITE DONE"
