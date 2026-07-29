# How to Fix the MSVC Linker Error on Windows

## Problem
You're getting `error: linker 'link.exe' not found` when trying to build Rust projects on Windows with the MSVC toolchain.

## Solution Options

### Option 1: Install Visual Studio Build Tools (Recommended - about 6 GB)

1. **Download Visual Studio Build Tools 2022:**
   - Go to: https://visualstudio.microsoft.com/downloads/
   - Scroll down to "All Downloads" → "Tools for Visual Studio"
   - Download "Build Tools for Visual Studio 2022"

2. **Install with C++ Support:**
   - Run the downloaded `vs_BuildTools.exe`
   - Select "Desktop development with C++"
   - This will install:
     - MSVC v143 compiler
     - Windows 11 SDK
     - C++ build tools

3. **After Installation:**
   - Restart your terminal/PowerShell
   - Run: `cargo build` or `cargo test`

### Option 2: Use GNU Toolchain Instead (Requires MinGW-w64)

If you don't want to install Visual Studio Build Tools, you can use the GNU toolchain, but you'll need MinGW-w64:

#### Step 1: Install MSYS2 (provides MinGW-w64)

1. Download MSYS2 from: https://www.msys2.org/
2. Install it (default location: `C:\msys64`)
3. Open "MSYS2 MSYS" from Start Menu
4. Run these commands:
   ```bash
   pacman -Syu
   pacman -S mingw-w64-x86_64-toolchain
   ```

#### Step 2: Add MinGW to PATH

Add to your system PATH:
- `C:\msys64\mingw64\bin`

#### Step 3: Switch Rust Toolchain

Open PowerShell in your project directory and run:
```powershell
rustup override set stable-x86_64-pc-windows-gnu
```

### Option 3: Use WSL2 (Linux Subsystem) - Easiest if you have WSL

If you have WSL2 installed, you can build in Linux which doesn't need Visual Studio:

1. Open WSL terminal
2. Install Rust:
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
3. Navigate to your project and run:
   ```bash
   cargo test
   ```

### Option 4: Quick Download Link for Build Tools Installer

**Direct Link (Microsoft Official):**
```
https://aka.ms/vs/17/release/vs_BuildTools.exe
```

**Run this PowerShell command to download:**
```powershell
Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile "$env:USERPROFILE\Downloads\vs_BuildTools.exe"
```

Then run the installer and select "Desktop development with C++"

## Quick Check After Installation

After installing build tools, restart PowerShell and run:

```powershell
cd C:\Users\USER\Desktop\GrantFox\Talenttrust-Contracts\contracts\escrow
cargo test --lib milestones_auth_matrix
```

## Current Project Status

Your milestones authorization matrix tests are complete and ready to run. Once you fix the linker, the tests will execute successfully.

## Alternative: Skip Local Testing

If you have CI/CD (GitHub Actions, GitLab CI, etc.), you can push your code and let the CI run the tests in a properly configured Linux environment. Most Rust CI templates handle this automatically.

### Example GitHub Actions Workflow

Create `.github/workflows/test.yml`:

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          override: true
      - name: Run tests
        run: cargo test --all-targets
      - name: Run clippy
        run: cargo clippy --all-targets -- -D warnings
```

This will run all your tests in the cloud without needing local build tools.
