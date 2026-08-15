# Drag-shake reproduction harness (2026-08-15).
# Simulates the exact user gesture that triggers the fall-vs-OS-drag fight:
# press on the pet body -> drag up-right -> PAUSE ~800ms while still holding
# (mid-drag pause; old code mistook this for a release and armed the fall) ->
# resume moving (old code: rAF setPosition fought the OS drag = violent shake).
# Samples the window rect throughout; prints CSV "phase,ms,L,T" (physical px).
#
# The process declares itself DPI-aware FIRST so every coordinate below is
# physical screen px, matching the app's own click-through math (an unaware
# process gets virtualized coords: GetWindowRect returned logical values on a
# 150% monitor, which made the first attempts click outside the window).
#
# Usage: powershell -File scripts\drag_shake_test.ps1 -GrabX 2231 -GrabY 978 [-PauseMs 800]
param(
  [int]$GrabX = 2231,     # physical screen px on the pet body (from cdp_probe)
  [int]$GrabY = 978,
  [int]$PauseMs = 800
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
if (-not $p) { throw "no desktop-pet window found" }
$script:h = $p.MainWindowHandle
function Get-Rect {
  $r = New-Object Win32+RECT
  [Win32]::GetWindowRect($script:h, [ref]$r) | Out-Null
  return $r
}
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$samples = New-Object System.Collections.Generic.List[string]
function Sample([string]$phase) {
  $r = Get-Rect
  $samples.Add(("{0},{1},{2},{3}" -f $phase, [int]$sw.ElapsedMilliseconds, $r.L, $r.T))
}

# Stabilize before grabbing.
Start-Sleep -Milliseconds 400
$r0 = Get-Rect
$gx = $GrabX; $gy = $GrabY
[Win32]::SetCursorPos($gx, $gy) | Out-Null
Start-Sleep -Milliseconds 150
Sample "pre"
Write-Output ("# window=" + $r0.L + "," + $r0.T + " grab=" + $gx + "," + $gy) | Out-Null

# Press.
[Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)   # LEFTDOWN
Start-Sleep -Milliseconds 80

# Phase 1: drag up-right (26 steps x 10px, 20ms apart ~ 520ms).
for ($i = 1; $i -le 26; $i++) {
  [Win32]::SetCursorPos(($gx + 10 * $i), ($gy - 8 * $i)) | Out-Null
  Start-Sleep -Milliseconds 20
}
Sample "drag1"

# Phase 2: mid-drag PAUSE, button still held.
$pauseEnd = $sw.ElapsedMilliseconds + $PauseMs
while ($sw.ElapsedMilliseconds -lt $pauseEnd) {
  Sample "pause"
  Start-Sleep -Milliseconds 120
}

# Phase 3: resume moving (12 steps x 10px right / 4px down, 30ms cadence).
for ($i = 1; $i -le 12; $i++) {
  [Win32]::SetCursorPos(($gx + 260 + 10 * $i), ($gy - 208 + 4 * $i)) | Out-Null
  Start-Sleep -Milliseconds 10
  Sample "resume"
  Start-Sleep -Milliseconds 20
}

# Release.
[Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)   # LEFTUP
Sample "released"

# Phase 4: post-release fall watch (~2.5s).
$end = $sw.ElapsedMilliseconds + 2500
while ($sw.ElapsedMilliseconds -lt $end) {
  Sample "post"
  Start-Sleep -Milliseconds 60
}

$samples | ForEach-Object { Write-Output $_ }
