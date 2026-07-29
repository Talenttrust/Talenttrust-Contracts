@echo off
echo ============================================================
echo Visual Studio Build Tools Installer
echo ============================================================
echo.
echo This script will install the minimal C++ build tools needed
echo for Rust MSVC toolchain compilation.
echo.
echo Installation size: ~6 GB
echo Estimated time: 10-20 minutes depending on your connection
echo.
pause

echo.
echo Downloading Visual Studio Build Tools...
powershell -Command "Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile '%TEMP%\vs_BuildTools.exe'"

if errorlevel 1 (
    echo Failed to download installer!
    pause
    exit /b 1
)

echo.
echo Starting installation...
echo The installer GUI will open. Please select:
echo 1. "Desktop development with C++"
echo 2. Click "Install"
echo.
echo Or use the automated silent installation by uncommenting the line below:
rem %TEMP%\vs_BuildTools.exe --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended --passive --wait

start "" /wait "%TEMP%\vs_BuildTools.exe"

echo.
echo ============================================================
echo Installation complete!
echo ============================================================
echo.
echo Please close this window and restart your PowerShell/Terminal
echo Then run: cargo test
echo.
pause
