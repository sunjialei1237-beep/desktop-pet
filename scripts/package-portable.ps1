# 便携版（免安装）打包脚本：zip 打包 desktop-pet.exe + 使用说明。
# 用法：powershell -ExecutionPolicy Bypass -File scripts\package-portable.ps1
# 输出：D:\cargo-target\desktop-pet\release\bundle\Liri-<version>-x64-portable.zip
# 注意：本文件必须保持 UTF-8 带 BOM（Windows PowerShell 5.1 按 GBK 解析 .ps1）。
$ErrorActionPreference = "Stop"

$exe = "D:\cargo-target\desktop-pet\release\desktop-pet.exe"
$bundleDir = "D:\cargo-target\desktop-pet\release\bundle"
if (-not (Test-Path $exe)) { throw "release exe not found: $exe" }

$version = "0.1.0"
$zipName = "Liri-$version-x64-portable.zip"
$zipPath = Join-Path $bundleDir $zipName

# 临时目录组装
$tmp = Join-Path $env:TEMP ("liri-portable-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
$inner = Join-Path $tmp "Liri"
New-Item -ItemType Directory -Path $inner | Out-Null
Copy-Item $exe (Join-Path $inner "Liri.exe")

$readme = @(
"Liri · 璃 —— 免安装便携版",
"==============================",
"",
"使用方法：",
"1. 把整个 Liri 文件夹放到你喜欢的位置（比如 D:\Liri）。",
"2. 双击 Liri.exe 启动（首次启动会弹出配置向导：粘贴 API Key，",
"   可选下载记忆模型，即可开始使用）。",
"3. 可选：右键 Liri.exe → 发送到 → 桌面快捷方式，方便每天打开。",
"",
"说明：",
"- 数据与配置保存在 %APPDATA%\DesktopPet\，删掉便携版文件夹不会删除你的记忆。",
"- 卸载 = 删除整个文件夹即可，与已安装版互不影响。"
)
$readme | Out-File -FilePath (Join-Path $inner "使用说明.txt") -Encoding utf8

# zip（.NET 兼容中文文件名）
Compress-Archive -Path (Join-Path $inner "*") -DestinationPath $zipPath -CompressionLevel Optimal -Force
Remove-Item -Recurse -Force $tmp

Write-Host "PORTABLE_ZIP=$zipPath"
Get-Item $zipPath | Select-Object Name, Length