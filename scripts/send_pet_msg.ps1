# Dev-run helper: send one message to the running desktop pet (UI automation).
# Usage: put the message into the Windows clipboard FIRST (Chinese text must go
# through base64 to survive bash->powershell quoting), then run this script:
#   powershell -NoProfile -Command "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('<base64>')))"
#   powershell -NoProfile -ExecutionPolicy Bypass -File scripts\send_pet_msg.ps1
# How: Alt+Space global shortcut summons + focuses the pet input -> Ctrl+V -> Enter.
# NOTE: keep this file ASCII-only; PowerShell 5.1 misreads BOM-less UTF-8 comments.
# Verify the chain in dev-run.log; the reply text lives in the DB conversations
# table (the bubble may already be gone by the time you look).
Add-Type -AssemblyName System.Windows.Forms
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait("% ")
Start-Sleep -Milliseconds 1200
[System.Windows.Forms.SendKeys]::SendWait("^v")
Start-Sleep -Milliseconds 800
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Write-Output "message sent"
