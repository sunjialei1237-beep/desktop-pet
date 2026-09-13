# 桌宠 LLM 连接诊断脚本
# 读 %APPDATA%\DesktopPet\config.toml 的 [llm] 段，向配置的服务器发一条最小测试消息，
# 按 HTTP 状态码输出人话结论。发给朋友时请和 桌宠诊断.bat 放在同一文件夹。
# 注意：本文件必须保持 UTF-8 with BOM（PS5.1 GBK 解析中文会炸，见 HANDOFF 续⁵⁴ 坑位）。

$cfgPath = Join-Path $env:APPDATA 'DesktopPet\config.toml'

Write-Host '== 桌宠连接诊断 =='
if (-not (Test-Path -LiteralPath $cfgPath)) {
    Write-Host "[X] 找不到配置文件：$cfgPath"
    Write-Host '    说明桌宠还没在这台电脑上完成过首次配置（或安装不完整）。'
    exit 1
}

# 只解析 [llm] 段的三个键；[[llm_profiles]] / [llm.gate] 等其他段一律忽略
$sec = ''
$vals = @{}
foreach ($line in Get-Content -LiteralPath $cfgPath) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $sec = $Matches[1]; continue }
    if ($sec -eq 'llm' -and $line -match '^\s*(base_url|api_key|main_model)\s*=\s*"([^"]*)"') {
        $vals[$Matches[1]] = $Matches[2]
    }
}

$base  = $vals['base_url']
$key   = $vals['api_key']
$model = $vals['main_model']
if (-not $model) { $model = 'deepseek-v4-pro' }  # config.rs 的出厂默认

Write-Host "配置文件：$cfgPath"
if ($key) {
    $head = $key.Substring(0, [Math]::Min(6, $key.Length))
    $tail = $key.Substring([Math]::Max(0, $key.Length - 4))
    Write-Host "api_key   = $head****$tail（已打码）"
} else {
    Write-Host 'api_key   = （空！）'
}
Write-Host "base_url  = $(if ($base) { $base } else { '（空！）' })"
Write-Host "main_model= $model"

if (-not $base -or -not $key) {
    Write-Host '[X] [llm] 段缺 base_url 或 api_key —— 首次配置向导可能没有保存成功。'
    Write-Host '    处理：打开桌宠 -> 设置 -> 重新填 API Key 并保存。'
    exit 1
}

$endpoint = $base.TrimEnd('/')
if ($endpoint -notmatch 'chat/completions$') { $endpoint = "$endpoint/chat/completions" }
Write-Host "测试端点  = $endpoint"
Write-Host '正在发送最小测试请求（最多等 30 秒）...'

# JSON 写临时文件再 -d @file：PS5.1 直接传内联 JSON 会把双引号拆坏
$body = @{ model = $model; messages = @(@{ role = 'user'; content = 'hi' }); max_tokens = 8 } |
    ConvertTo-Json -Depth 4
$bodyFile = [IO.Path]::GetTempFileName()
[IO.File]::WriteAllText($bodyFile, $body)
$respFile = [IO.Path]::GetTempFileName()

$code = & curl.exe -sS -m 30 --connect-timeout 10 -o $respFile -w '%{http_code}' `
    $endpoint -H "Authorization: Bearer $key" -H 'Content-Type: application/json' `
    -d "@$bodyFile" 2>$null
if (-not $code) { $code = '000' }

$resp = (Get-Content -LiteralPath $respFile -Raw) -replace '\s+', ' '
Remove-Item $bodyFile, $respFile -ErrorAction SilentlyContinue
if ($resp -and $resp.Length -gt 400) { $resp = $resp.Substring(0, 400) + '...' }

Write-Host "HTTP 状态 = $code"
Write-Host "返回内容  = $resp"
Write-Host ''
switch ($code) {
    '200' { Write-Host '[OK] API 正常返回 —— key、余额、网络都没问题，问题出在桌宠 App 本身。请把本窗口全部内容截图发给开发者。' }
    '401' { Write-Host '[X] 401 = API Key 无效。最常见：key 复制时缺字符/带空格，或 key 和 base_url 不是同一家（如 DeepSeek 地址配了别家的 key）。处理：桌宠 -> 设置 -> 重填正确的 key。' }
    '402' { Write-Host '[X] 402 = 账户余额不足。处理：去对应平台充值后重试。' }
    '403' { Write-Host '[X] 403 = key 无权访问该模型（或地区受限）。把本窗口截图发给开发者。' }
    '404' { Write-Host '[X] 404 = 地址或模型名不对。检查 base_url 是否多了/少了 /v1、main_model 拼写。把截图发给开发者。' }
    '429' { Write-Host '[X] 429 = 触发限流，或账户欠费（智谱会返回 429+code 1113）。等几分钟重试；仍不行去平台查余额。' }
    '000' { Write-Host '[X] 连不上服务器 = 本机网络问题（DNS/代理/防火墙）。换网络或关代理后重试。' }
    default { Write-Host "[?] 未预期状态 $code。把本窗口全部内容截图发给开发者。" }
}

# ============ 第二部分：向量模型下载诊断 ============
Write-Host ''
Write-Host '== 向量模型下载源诊断 =='

# 模型目录：%APPDATA%\DesktopPet\models\<model_name>（config [embedding] 段，默认 bge-m3）
$sec2 = ''
$emb = @{}
foreach ($line in Get-Content -LiteralPath $cfgPath) {
    if ($line -match '^\s*\[(.+)\]\s*$') { $sec2 = $Matches[1]; continue }
    if ($sec2 -eq 'embedding' -and $line -match '^\s*model_name\s*=\s*"([^"]*)"') {
        $emb['model_name'] = $Matches[1]
    }
}
$modelName = $emb['model_name']; if (-not $modelName) { $modelName = 'bge-m3' }
$modelDir = Join-Path (Split-Path $cfgPath -Parent) "models\$modelName"

$root = [IO.Path]::GetPathRoot($cfgPath)
try {
    $freeGB = [Math]::Round((New-Object System.IO.DriveInfo($root)).AvailableFreeSpace / 1GB, 1)
    $warn = if ($freeGB -lt 1) { '  [!] 剩余空间不足！' } else { '' }
    Write-Host "磁盘空间  ：$root 剩余 $freeGB GB（模型约需 0.6 GB）$warn"
} catch {}

if (Test-Path -LiteralPath $modelDir) {
    Write-Host "模型目录  ：$modelDir"
    $files = Get-ChildItem -LiteralPath $modelDir -File -ErrorAction SilentlyContinue
    if ($files) {
        foreach ($f in $files) {
            Write-Host ("  {0}  {1:N1} MB" -f $f.Name, ($f.Length / 1MB))
        }
    } else {
        Write-Host '  （目录为空——下载从未真正开始）'
    }
    if ($files | Where-Object { $_.Name -like '*.download_tmp' }) {
        Write-Host '[i] 存在 .download_tmp 临时文件：过 1 分钟再跑一次本诊断看它的大小——增大=还在下（只是慢），不变=卡死了。'
    }
} else {
    Write-Host "模型目录  ：$modelDir（尚未创建——下载从未真正开始）"
}

# 两个下载源（主模型走 hf-mirror.com，onnxruntime.dll 走 github.com——后者是国内网络常见卡点）
$sources = @(
    @{ name = '模型文件源 hf-mirror.com'; url = 'https://hf-mirror.com/Xenova/bge-m3/resolve/main/config.json' },
    @{ name = '运行库源 github.com';      url = 'https://github.com/microsoft/onnxruntime/releases/download/v1.20.1/onnxruntime-win-x64-1.20.1.zip' }
)
foreach ($s in $sources) {
    $r = & curl.exe -sS -I --max-time 15 -o NUL -w '%{http_code}|%{time_total}' $s.url 2>$null
    if (-not $r) { $r = '000|0' }
    $parts = $r -split '\|'
    if ($parts[0] -eq '000') {
        Write-Host ("[X] {0}：15 秒内连不上——下载卡住就是这里的原因。" -f $s.name)
    } else {
        Write-Host ("[OK] {0}：可达（HTTP {1}，用时 {2} 秒）" -f $s.name, $parts[0], $parts[1])
    }
}

Write-Host ''
Write-Host '提示：向导的模型下载页有「跳过」按钮——跳过不影响聊天，只影响长期记忆的质量。'
Write-Host '      另外下载不支持断点续传、总超时 10 分钟：网络慢时 570MB 下不完会整体失败重来。'
Write-Host '      建议：先跳过、把聊天连接问题查清楚，之后在设置里再试下载。'

