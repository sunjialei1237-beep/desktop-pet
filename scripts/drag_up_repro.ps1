# Slow human-like UP drag from the pet's body, release, then rest. Grab point
# defaults to body center of a window at the bottom-right corner.
param([int]$GrabX = 2231, [int]$GrabY = 978, [int]$UpPx = 200)
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
$gx = $r0.L + ($GrabX - 1931); $gy = $r0.T + ($GrabY - 228)  # body-center offset
[Win32]::SetCursorPos($gx, $gy) | Out-Null
Start-Sleep -Milliseconds 150
Sample "pre"
[Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
for ($i = 1; $i -le 25; $i++) {   # slow: 25 steps x 8px up, 40ms apart = 1s
  [Win32]::SetCursorPos($gx, ($gy - [int]($UpPx * $i / 25))) | Out-Null
  Start-Sleep -Milliseconds 40
}
Sample "dragged"
[Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Sample "released"
$end = $sw.ElapsedMilliseconds + 4000
while ($sw.ElapsedMilliseconds -lt $end) { Sample "post"; Start-Sleep -Milliseconds 120 }
$out | ForEach-Object { Write-Output $_ }
