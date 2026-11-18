param (
    [switch]$SkipInstaller
)

Write-Host "Building IAMMUSIC executable with Rust (Cargo)..." -ForegroundColor Cyan

# Build Rust release binary
Push-Location rust_app
cargo build --release -j 2
if ($LASTEXITCODE -ne 0) {
    Write-Host "Cargo build failed." -ForegroundColor Red
    Pop-Location
    exit 1
}
Pop-Location

# Ensure any running instances are closed before replacing files
Get-Process -Name "*iammusic*", "*IAMMUSIC*" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 300

# Ensure dist directory
New-Item -ItemType Directory -Path "dist" -Force | Out-Null
Copy-Item -Path "rust_app\target\release\iammusic.exe" -Destination "dist\IAMMUSIC.exe" -Force
Copy-Item -Path "rust_app\target\release\iammusic.exe" -Destination "IAMMUSIC.exe" -Force
Copy-Item -Path "icon.ico" -Destination "dist\icon.ico" -Force

Write-Host "Executable built successfully in .\dist\IAMMUSIC.exe" -ForegroundColor Green

# Copy to Releases folder
New-Item -ItemType Directory -Path "Releases" -Force | Out-Null
Copy-Item -Path "dist\IAMMUSIC.exe" -Destination "Releases\IAMMUSIC.exe" -Force
Write-Host "Copied to Releases\IAMMUSIC.exe" -ForegroundColor Green

if ($SkipInstaller) {
    exit 0
}

# Check for Inno Setup Compiler
$isccPaths = @(
    "C:\Program Files (x86)\Inno Setup 6\ISCC.exe",
    "C:\Program Files\Inno Setup 6\ISCC.exe",
    "${env:LOCALAPPDATA}\Programs\Inno Setup 6\ISCC.exe"
)

$isccPath = $null
foreach ($p in $isccPaths) {
    if (Test-Path $p) {
        $isccPath = $p
        break
    }
}

if (-Not $isccPath) {
    Write-Host "Inno Setup Compiler not found. Attempting to install via winget..." -ForegroundColor Yellow
    winget install --id JRSoftware.InnoSetup -e --accept-package-agreements --accept-source-agreements
    
    # Re-check after install
    foreach ($p in $isccPaths) {
        if (Test-Path $p) {
            $isccPath = $p
            break
        }
    }
    
    if (-Not $isccPath) {
        Write-Host "Inno Setup not found after install. Please restart your terminal and run this script again." -ForegroundColor Red
        exit 1
    }
}

Write-Host "Building Installer..." -ForegroundColor Cyan
& $isccPath "installer.iss"

if ($LASTEXITCODE -eq 0) {
    Write-Host "Installer built successfully! Check the Output/ folder." -ForegroundColor Green
    Copy-Item -Path "Output\IAMMUSIC_Installer.exe" -Destination "Releases\IAMMUSIC_Installer.exe" -Force
    Write-Host "Copied installer to Releases\IAMMUSIC_Installer.exe" -ForegroundColor Green
} else {
    Write-Host "Installer build failed." -ForegroundColor Red
}
