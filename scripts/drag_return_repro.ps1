# Reproduce "release-after-drag returns to original position": simple A->B
# drag (no pause), release, then sample 3.5s. If the window teleports back
# toward the pre-drag coords after release, it's reproduced.
param(
  [int]$GrabDx = 300,   # physical offset from window left (body center @150%)
  [int]$GrabDy = 750,   # physical offset from window top
  [int]$Regrab = 0      # 1 = also test quick re-grab mid-fall variant
)
$ErrorActionPreference = "Stop"
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint data, UIntPtr extra);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L; public int T; public int R; public int B; }
}
"@
[Win32]::SetProcessDPIAware() | Out-Null
$p = Get-Process desktop-pet -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$script:h = $p.MainWindowHandle
function Get-Rect { $r = New-Object Win32+RECT; [Win32]::GetWindowRect($script:h, [ref]$r) | Out-Null; $r }
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$out = New-Object System.Collections.Generic.List[string]
function Sample([string]$tag) { $r = Get-Rect; $out.Add(("{0},{1},{2},{3}" -f $tag, [int]$sw.ElapsedMilliseconds, $r.L, $r.T)) }

Start-Sleep -Milliseconds 300
$r0 = Get-Rect
$gx = $r0.L + $GrabDx; $gy = $r0.T + $GrabDy
[Win32]::SetCursorPos($gx, $gy) | Out-Null
Start-Sleep -Milliseconds 120
Sample "pre"

[Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 60
for ($i = 1; $i -le 24; $i++) {
  [Win32]::SetCursorPos(($gx + 8 * $i), ($gy - 6 * $i)) | Out-Null
  Start-Sleep -Milliseconds 18
}
Sample "dragged"
[Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Sample "released"

if ($Regrab -eq 1) {
  # variant: let the fall start, then re-grab mid-fall and drag further
  Start-Sleep -Milliseconds 450
  Sample "falling"
  $rf = Get-Rect
  $fx = $rf.L + $GrabDx; $fy = $rf.T + $GrabDy
  [Win32]::SetCursorPos($fx, $fy) | Out-Null
  Start-Sleep -Milliseconds 60
  [Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 50
  for ($i = 1; $i -le 18; $i++) {
    [Win32]::SetCursorPos(($fx + 8 * $i), ($fy - 4 * $i)) | Out-Null
    Start-Sleep -Milliseconds 18
  }
  Sample "redragged"
  [Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Sample "re-released"
}

$end = $sw.ElapsedMilliseconds + 3500
while ($sw.ElapsedMilliseconds -lt $end) {
  Sample "post"
  Start-Sleep -Milliseconds 100
}
$out | ForEach-Object { Write-Output $_ }
