# ==============================================================================
# AnyPS5 - Automated Self-Hosted GitHub Actions Runner Setup (Windows x64)
# ==============================================================================
# This script sets up an optimized, dedicated GitHub Actions runner for:
# Repository: boykopovar/AnyPS5
# Labels: self-hosted, windows, x64, anyps5-builder
# ==============================================================================

[CmdletBinding()]
param(
    [string]$Repo = "boykopovar/AnyPS5",
    [string]$RunnerToken = "",
    [string]$RunnerName = "$($env:COMPUTERNAME)-anyps5-worker",
    [string]$RunnerDir = "$PSScriptRoot\actions-runner",
    [switch]$InstallAsService,
    [switch]$Ephemeral
)

$ErrorActionPreference = "Stop"

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " 🎮 AnyPS5 Community Builder - Self-Hosted Runner Setup" -ForegroundColor White
Write-Host " Target Repository: $Repo" -ForegroundColor Yellow
Write-Host " Target Labels: self-hosted, windows, x64, anyps5-builder" -ForegroundColor Green
Write-Host "==========================================================" -ForegroundColor Cyan

# 1. Prerequisite Toolchain Checks
Write-Host "`n[1/5] Checking local C++ & Build Prerequisites..." -ForegroundColor Cyan

$missingTools = @()

# Check Git
if (Get-Command git -ErrorAction SilentlyContinue) {
    Write-Host "  [OK] Git: $(git --version)" -ForegroundColor Green
} else {
    $missingTools += "git"
    Write-Host "  [FAIL] Git is not installed or not in PATH!" -ForegroundColor Red
}

# Check CMake
if (Get-Command cmake -ErrorAction SilentlyContinue) {
    $cmakeVer = (cmake --version)[0]
    Write-Host "  [OK] CMake: $cmakeVer" -ForegroundColor Green
} else {
    $missingTools += "cmake"
    Write-Host "  [WARN] CMake not found. Required for building AnyPS5." -ForegroundColor Yellow
}

# Check Ninja
if (Get-Command ninja -ErrorAction SilentlyContinue) {
    Write-Host "  [OK] Ninja: $(ninja --version)" -ForegroundColor Green
} else {
    Write-Host "  [WARN] Ninja not found (will fallback to MSBuild/NMake if available)." -ForegroundColor Yellow
}

# Check Clang or MSVC
$hasClang = [bool](Get-Command clang++ -ErrorAction SilentlyContinue)
$hasCl = [bool](Get-Command cl -ErrorAction SilentlyContinue)
if ($hasClang) {
    Write-Host "  [OK] Clang: $(clang++ --version | Select-Object -First 1)" -ForegroundColor Green
}
if ($hasCl) {
    Write-Host "  [OK] MSVC C++ Compiler detected." -ForegroundColor Green
}
if (-not $hasClang -and -not $hasCl) {
    Write-Host "  [WARN] Neither Clang++ nor MSVC 'cl.exe' was found in active PATH." -ForegroundColor Yellow
    Write-Host "         Make sure Visual Studio C++ build tools or LLVM is installed." -ForegroundColor DarkGray
}

# Check Vulkan SDK
if ($env:VULKAN_SDK) {
    Write-Host "  [OK] Vulkan SDK: $env:VULKAN_SDK" -ForegroundColor Green
} else {
    Write-Host "  [INFO] VULKAN_SDK environment variable not set (may be needed for GPU/Graphics builds)." -ForegroundColor DarkGray
}

# Check if runner is already configured with valid .credentials
$isAlreadyRegistered = (Test-Path "$RunnerDir\.credentials") -and (Test-Path "$RunnerDir\run.cmd")
if ($isAlreadyRegistered) {
    Write-Host "`n[+] Found existing runner credentials in $RunnerDir\.credentials!" -ForegroundColor Green
    Write-Host "    Bypassing token registration. Starting AnyPS5 runner directly..." -ForegroundColor Cyan
    Push-Location $RunnerDir
    try {
        if ($InstallAsService) {
            & .\svc.cmd start
        } else {
            & .\run.cmd
        }
    } finally {
        Pop-Location
    }
    exit 0
}

# 2. Token Retrieval
Write-Host "`n[2/5] Obtaining GitHub Runner Registration Token..." -ForegroundColor Cyan

if (-not $RunnerToken) {
    # Check if GitHub CLI is installed and authenticated
    if (Get-Command gh -ErrorAction SilentlyContinue) {
        Write-Host "  Checking GitHub CLI (gh) authentication..." -ForegroundColor DarkGray
        try {
            $tokenOutput = gh api --method POST -H "Accept: application/vnd.github+json" "/repos/$Repo/actions/runners/registration-token" 2>$null | ConvertFrom-Json
            if ($tokenOutput -and $tokenOutput.token) {
                $RunnerToken = $tokenOutput.token
                Write-Host "  [SUCCESS] Acquired fresh registration token via GitHub CLI!" -ForegroundColor Green
            }
        } catch {
            Write-Host "  Could not auto-generate token via gh CLI." -ForegroundColor DarkGray
        }
    }
}

if (-not $RunnerToken) {
    Write-Host "`n  ------------------------------------------------------------------" -ForegroundColor Yellow
    Write-Host "  No registration token provided. To get one:" -ForegroundColor White
    Write-Host "  1. Open: https://github.com/$Repo/settings/actions/runners/new" -ForegroundColor Cyan
    Write-Host "  2. Or have a maintainer run: python runner/anyps5_runner_manager.py --get-token" -ForegroundColor Cyan
    Write-Host "  ------------------------------------------------------------------" -ForegroundColor Yellow
    $RunnerToken = Read-Host "`n  Please enter your GitHub Runner Registration Token"
}

if (-not $RunnerToken) {
    Write-Error "Runner registration token is required. Aborting setup."
    exit 1
}

# 3. Create Runner Directory & Download Official GitHub Actions Runner
Write-Host "`n[3/5] Downloading latest GitHub Actions Runner for Windows x64..." -ForegroundColor Cyan

if (-not (Test-Path $RunnerDir)) {
    New-Item -ItemType Directory -Path $RunnerDir -Force | Out-Null
}

$repoUrl = "https://github.com/$Repo"
$latestVersion = "2.322.0" # Stable known runner version
$runnerArchive = "$RunnerDir\actions-runner-win-x64.zip"

if (-not (Test-Path "$RunnerDir\config.cmd")) {
    $downloadUrl = "https://github.com/actions/runner/releases/download/v$latestVersion/actions-runner-win-x64-$latestVersion.zip"
    Write-Host "  Downloading from: $downloadUrl" -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $downloadUrl -OutFile $runnerArchive -UseBasicParsing
    
    Write-Host "  Extracting archive..." -ForegroundColor DarkGray
    Expand-Archive -Path $runnerArchive -DestinationPath $RunnerDir -Force
    Remove-Item $runnerArchive -Force
    Write-Host "  [OK] Runner extracted to: $RunnerDir" -ForegroundColor Green
} else {
    Write-Host "  [OK] Existing runner binaries found in $RunnerDir" -ForegroundColor Green
}

# 4. Configure Runner
Write-Host "`n[4/5] Registering Runner with GitHub Actions..." -ForegroundColor Cyan

Push-Location $RunnerDir
try {
    $configArgs = @(
        "--url", $repoUrl,
        "--token", $RunnerToken,
        "--name", $RunnerName,
        "--labels", "windows,x64,anyps5-builder",
        "--work", "_work",
        "--unattended",
        "--replace"
    )

    if ($Ephemeral) {
        $configArgs += "--ephemeral"
        Write-Host "  [MODE] Ephemeral enabled (runner will process 1 build job and automatically deregister)." -ForegroundColor Yellow
    }

    & .\config.cmd @configArgs

    if ($LASTEXITCODE -eq 0) {
        Write-Host "  [SUCCESS] Runner registered successfully!" -ForegroundColor Green
    } else {
        Write-Host "  [WARN] config.cmd exited with code $LASTEXITCODE. Check if runner was already configured." -ForegroundColor Yellow
    }

    # 5. Execution or Service Setup
    Write-Host "`n[5/5] Ready to launch worker!" -ForegroundColor Cyan

    if ($InstallAsService) {
        Write-Host "  Installing runner as a Windows Service..." -ForegroundColor Cyan
        & .\svc.cmd install
        & .\svc.cmd start
        Write-Host "  [SUCCESS] Runner service started in background! It will automatically build AnyPS5 PRs." -ForegroundColor Green
    } else {
        Write-Host "  ================================================================" -ForegroundColor Green
        Write-Host "  Runner is ready! Starting interactive worker..." -ForegroundColor White
        Write-Host "  Press Ctrl+C at any time to stop lending your CPU." -ForegroundColor Yellow
        Write-Host "  ================================================================" -ForegroundColor Green
        & .\run.cmd
    }
} finally {
    Pop-Location
}
