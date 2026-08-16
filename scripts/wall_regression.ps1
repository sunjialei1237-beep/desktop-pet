# Wall regression for the visual-body screen walls (2026-08-16 续⁴²):
# head can touch the screen TOP (window top goes off-screen, T<0) and the
# feet the taskbar TOP. Tests:
#   W1  up-drag   -> top wall T=-360 physical (head at screen y=0), holds (no snap)
#   W2  down-drag -> bottom wall T=378 physical (feet on taskbar top 1368)
#   W3  left-drag -> left wall L=-186 (visual left edge at screen x=0)
#   W4  right-drag-> right wall L=2146 (visual right edge at screen x=2560)
#   W5  mid-screen release -> stays exactly at release point
# SetWindowPos resets MUST pass SWP_NOSIZE (0x0001) | SWP_NOZORDER (0x0004).
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W32reg3 {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint data, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
}
"@
[W32reg3]::SetProcessDPIAware() | Out-Null
$pet = Get-Process desktop-pet | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $pet) { Write-Output "NO PET PROCESS"; exit 1 }
$h = $pet.MainWindowHandle
function GetRect { $r = New-Object W32reg3+RECT; [W32reg3]::GetWindowRect($h, [ref]$r) | Out-Null; return $r }
function DragRel {
  param([int]$dx, [int]$dy, [int]$grabY = 750)
  $r0 = GetRect
  $gx = $r0.L + 300; $gy = $r0.T + $grabY
  if ($gy -lt 10) { $gy = 10 }  # keep the grab point on-screen when parked high
  [W32reg3]::SetCursorPos($gx, $gy) | Out-Null
  Start-Sleep -Milliseconds 200
  [W32reg3]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  $steps = [Math]::Max([Math]::Abs($dx), [Math]::Abs($dy))
  for ($i = 1; $i -le $steps; $i++) {
    $nx = $gx + [int]([double]$dx * $i / $steps)
    $ny = $gy + [int]([double]$dy * $i / $steps)
    if ($ny -lt 0) { $ny = 0 }  # physical cursor can't leave the screen
    [W32reg3]::SetCursorPos($nx, $ny) | Out-Null
    Start-Sleep -Milliseconds 15
  }
  [W32reg3]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 500
}
function ResetPos {
  param([int]$x, [int]$y)
  [W32reg3]::SetWindowPos($h, [IntPtr]::Zero, $x, $y, 0, 0, 0x0001 -bor 0x0004) | Out-Null
  Start-Sleep -Seconds 2
}
$r = GetRect; Write-Output "start: L=$($r.L) T=$($r.T) W=$($r.R-$r.L) H=$($r.B-$r.T) (expect 600x1140)"
# --- W1: up-drag -> top wall (HEAD at screen top) ---
ResetPos 1931 228
DragRel -dx 0 -dy -700
$r = GetRect; Write-Output "W1 post: T=$($r.T) (expect -360, head at screen top)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "W1 hold: T=$($r.T) (expect -360, NO OS snap to 0)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "W1 hold2: T=$($r.T) (expect -360)"
# --- W2: down-drag -> bottom wall (FEET on taskbar top) ---
DragRel -dx 0 -dy 1200
$r = GetRect; Write-Output "W2 post: T=$($r.T) B=$($r.B) (expect T=378; feet=378+990=1368=taskbar top)"
Start-Sleep -Milliseconds 1200
$r = GetRect; Write-Output "W2 hold: T=$($r.T) (expect 378)"
# --- W3: left-drag -> left wall ---
ResetPos 1931 228
DragRel -dx -2200 -dy 0
$r = GetRect; Write-Output "W3 post: L=$($r.L) (expect -186, visual left edge at screen 0)"
Start-Sleep -Milliseconds 1200
$r = GetRect; Write-Output "W3 hold: L=$($r.L) (expect -186)"
# --- W4: right-drag -> right wall ---
DragRel -dx 2400 -dy 0
$r = GetRect; Write-Output "W4 post: L=$($r.L) (expect 2146, visual right edge at screen 2560)"
Start-Sleep -Milliseconds 1200
$r = GetRect; Write-Output "W4 hold: L=$($r.L) (expect 2146)"
# --- W5: mid-screen release stays ---
ResetPos 1931 228
DragRel -dx 0 -dy -100
$r = GetRect; Write-Output "W5 post: T=$($r.T) (expect ~137 = 228-91ish release point, stays)"
Start-Sleep -Milliseconds 1500
$r = GetRect; Write-Output "W5 hold: T=$($r.T) (expect ~137)"
Write-Output "SUITE DONE"
