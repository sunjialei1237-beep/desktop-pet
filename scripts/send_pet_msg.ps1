# dev 实跑验证辅助：向正在运行的桌宠发送一条消息（UI 自动化）。
# 用法：先把消息文本写入 Windows 剪贴板（中文经 base64 避免编码损坏），再运行本脚本：
#   powershell -NoProfile -Command "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('<base64>')))"
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\send_pet_msg.ps1
# 原理：Alt+Space 全局快捷键唤出输入框并聚焦 -> Ctrl+V 粘贴 -> Enter 发送。
# 发送后从 dev-run.log 看链路日志，回复内容查 DB conversations 表（气泡可能已消失）。
Add-Type -AssemblyName System.Windows.Forms
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("% ")
Start-Sleep -Milliseconds 1200
[System.Windows.Forms.SendKeys]::SendWait("^v")
Start-Sleep -Milliseconds 800
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Write-Output "message sent"
