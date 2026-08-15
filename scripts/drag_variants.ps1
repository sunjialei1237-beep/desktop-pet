# Variant A: fast fling up + release WHILE MOVING (cursor flies past her head).
# Variant B: release, wait 450ms (she's mid-fall), re-grab, drag, release.
param([int]$Mode = 0)  # 0=both, 1=A only, 2=B only
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
function BodyCenter { $rr = Get-Rect; $cx = [int]$rr.L + 300; $cy = [int]$rr.T + 750; return @($cx, $cy) }

if ($Mode -ne 2) {
  Start-Sleep -Milliseconds 300
  $c = BodyCenter
  [Win32]::SetCursorPos([int]$c[0], [int]$c[1]) | Out-Null
  Start-Sleep -Milliseconds 120
  Sample "A-pre"
  [Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 50
  for ($i = 1; $i -le 14; $i++) {   # fast fling: 14 x 18px up, 12ms apart
    [Win32]::SetCursorPos([int]$c[0], ([int]$c[1] - 18 * $i)) | Out-Null
    Start-Sleep -Milliseconds 12
  }
  [Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # release mid-motion
  Sample "A-released"
  $e = $sw.ElapsedMilliseconds + 2500
  while ($sw.ElapsedMilliseconds -lt $e) { Sample "A-post"; Start-Sleep -Milliseconds 100 }
}

if ($Mode -ne 1) {
  Start-Sleep -Milliseconds 300
  $c = BodyCenter
  [Win32]::SetCursorPos([int]$c[0], [int]$c[1]) | Out-Null
  Start-Sleep -Milliseconds 120
  Sample "B-pre"
  [Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 50
  for ($i = 1; $i -le 18; $i++) {
    [Win32]::SetCursorPos(([int]$c[0] - 10 * $i), ([int]$c[1] - 8 * $i)) | Out-Null
    Start-Sleep -Milliseconds 15
  }
  [Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Sample "B-released1"
  Start-Sleep -Milliseconds 450                    # she's mid-fall now
  Sample "B-midfall"
  $c2 = BodyCenter
  [Win32]::SetCursorPos([int]$c2[0], [int]$c2[1]) | Out-Null
  Start-Sleep -Milliseconds 80
  [Win32]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 50
  for ($i = 1; $i -le 16; $i++) {                  # re-drag further up-left
    [Win32]::SetCursorPos(([int]$c2[0] - 8 * $i), ([int]$c2[1] - 6 * $i)) | Out-Null
    Start-Sleep -Milliseconds 15
  }
  Sample "B-redragged"
  [Win32]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  Sample "B-released2"
  $e = $sw.ElapsedMilliseconds + 3000
  while ($sw.ElapsedMilliseconds -lt $e) { Sample "B-post"; Start-Sleep -Milliseconds 100 }
}
$out | ForEach-Object { Write-Output $_ }
