@echo off
rem Double-click me. I just run diagnose_llm.ps1 with script execution allowed.
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0diagnose_llm.ps1"
echo.
pause
