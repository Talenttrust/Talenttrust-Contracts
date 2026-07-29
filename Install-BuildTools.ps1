<#
.SYNOPSIS
    Automated installer for Visual Studio Build Tools (C++ support)

.DESCRIPTION
    This script automatically downloads and installs the minimal
    Visual Studio Build Tools needed for Rust MSVC toolchain.

.NOTES
    - Requires Administrator privileges
    - Downloads ~6 GB
    - Installation takes 10-20 minutes
#>

# Check for admin privileges
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host "================================================================" -ForegroundColor Yellow
    Write-Host "This script requires Administrator privileges!" -ForegroundColor Yellow
    Write-Host "================================================================" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Please right-click this script and select 'Run as Administrator'" -ForegroundColor Cyan
    Write-Host "Or run from an elevated PowerShell prompt:" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  PowerShell -ExecutionPolicy Bypass -File Install-BuildTools.ps1" -ForegroundColor White
    Write-Host ""
    pause
    exit 1
}

Write-Host "================================================================" -ForegroundColor Green
Write-Host "Visual Studio Build Tools Installer" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
Write-Host ""
Write-Host "This will install:" -ForegroundColor Cyan
Write-Host "  - MSVC C++ compiler and linker" -ForegroundColor White
Write-Host "  - Windows SDK" -ForegroundColor White
Write-Host "  - C++ build tools" -ForegroundColor White
Write-Host ""
Write-Host "Download size: ~500 MB" -ForegroundColor Yellow
Write-Host "Installation size: ~6 GB" -ForegroundColor Yellow
Write-Host "Estimated time: 10-20 minutes" -ForegroundColor Yellow
Write-Host ""

$response = Read-Host "Do you want to continue? (Y/N)"
if ($response -ne 'Y' -and $response -ne 'y') {
    Write-Host "Installation cancelled." -ForegroundColor Red
    exit 0
}

# Download installer
$installerPath = "$env:TEMP\vs_BuildTools.exe"
Write-Host ""
Write-Host "Step 1: Downloading Visual Studio Build Tools..." -ForegroundColor Cyan

try {
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile $installerPath -ErrorAction Stop
    $ProgressPreference = 'Continue'
    Write-Host "  ✓ Download complete" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Download failed: $_" -ForegroundColor Red
    pause
    exit 1
}

# Run installer
Write-Host ""
Write-Host "Step 2: Installing build tools..." -ForegroundColor Cyan
Write-Host "  This may take 10-20 minutes depending on your system." -ForegroundColor Yellow
Write-Host ""

try {
    $arguments = @(
        "--add", "Microsoft.VisualStudio.Workload.VCTools",
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
        "--add", "Microsoft.VisualStudio.Component.Windows11SDK.22621",
        "--includeRecommended",
        "--quiet",
        "--wait",
        "--norestart"
    )
    
    $process = Start-Process -FilePath $installerPath -ArgumentList $arguments -Wait -PassThru -NoNewWindow
    
    if ($process.ExitCode -eq 0 -or $process.ExitCode -eq 3010) {
        Write-Host ""
        Write-Host "================================================================" -ForegroundColor Green
        Write-Host "  ✓ Installation completed successfully!" -ForegroundColor Green
        Write-Host "================================================================" -ForegroundColor Green
        Write-Host ""
        
        if ($process.ExitCode -eq 3010) {
            Write-Host "NOTE: A restart may be required for all changes to take effect." -ForegroundColor Yellow
            Write-Host ""
        }
        
        Write-Host "Next steps:" -ForegroundColor Cyan
        Write-Host "  1. Close and reopen your PowerShell terminal" -ForegroundColor White
        Write-Host "  2. Navigate to your project:" -ForegroundColor White
        Write-Host "       cd contracts\escrow" -ForegroundColor Gray
        Write-Host "  3. Run your tests:" -ForegroundColor White
        Write-Host "       cargo test --lib milestones_auth_matrix" -ForegroundColor Gray
        Write-Host ""
        
    } else {
        Write-Host ""
        Write-Host "  ✗ Installation failed with exit code: $($process.ExitCode)" -ForegroundColor Red
        Write-Host ""
        Write-Host "Please try manual installation:" -ForegroundColor Yellow
        Write-Host "  1. Go to: https://visualstudio.microsoft.com/downloads/" -ForegroundColor White
        Write-Host "  2. Download 'Build Tools for Visual Studio 2022'" -ForegroundColor White
        Write-Host "  3. Run installer and select 'Desktop development with C++'" -ForegroundColor White
        Write-Host ""
        pause
        exit 1
    }
    
} catch {
    Write-Host ""
    Write-Host "  ✗ Installation error: $_" -ForegroundColor Red
    pause
    exit 1
}

# Verify installation
Write-Host "Step 3: Verifying installation..." -ForegroundColor Cyan

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
    $buildToolsPath = & $vswhere -latest -products Microsoft.VisualStudio.Product.BuildTools -property installationPath
    
    if ($buildToolsPath) {
        Write-Host "  ✓ Build Tools found at: $buildToolsPath" -ForegroundColor Green
        
        # Check for link.exe
        $vcToolsPath = Get-ChildItem -Path "$buildToolsPath\VC\Tools\MSVC" -Directory | Select-Object -First 1
        if ($vcToolsPath) {
            $linkExe = Get-ChildItem -Path $vcToolsPath.FullName -Recurse -Filter "link.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($linkExe) {
                Write-Host "  ✓ MSVC linker (link.exe) found" -ForegroundColor Green
            }
        }
    }
}

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host "Setup complete! You're ready to build Rust projects." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green
Write-Host ""
pause
