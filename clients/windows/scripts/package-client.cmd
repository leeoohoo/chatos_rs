@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0package-client.ps1" %*
if errorlevel 1 (
  echo.
  echo ChatOS Windows packaging failed. Review the error above.
  pause
  exit /b 1
)
echo.
echo ChatOS Windows EXE installer created successfully.
pause
endlocal
