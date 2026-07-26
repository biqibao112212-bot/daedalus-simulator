@echo off
setlocal
chcp 65001 >nul
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0install-windows.ps1" %*
set "SETUP_EXIT=%ERRORLEVEL%"
if not "%SETUP_EXIT%"=="0" (
  echo.
  echo Installation failed. See README_ZH.md for prerequisites and troubleshooting.
  pause
  exit /b %SETUP_EXIT%
)
echo.
echo Installation completed. Shortcuts were added to the Start menu.
pause
exit /b 0
