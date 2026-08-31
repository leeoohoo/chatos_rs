@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-client.ps1" %*
if errorlevel 1 (
  echo.
  echo ChatOS Windows installation failed. Review the error above.
  pause
  exit /b 1
)
endlocal
