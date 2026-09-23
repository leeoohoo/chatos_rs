@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0..\clients\windows\scripts\package-client.ps1" -Install %*
if errorlevel 1 (
  echo.
  echo ChatOS Windows packaging or installation failed. Review the error above.
  exit /b 1
)
echo.
echo ChatOS Windows was packaged and installed successfully.
endlocal
