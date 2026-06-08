# Coco — 一键打包脚本 (Windows)
# 用法: powershell -ExecutionPolicy Bypass -File build\build.ps1

$ErrorActionPreference = "Stop"

$ROOT = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$FRONTEND = Join-Path $ROOT "frontend"
$TAURI_APP = Join-Path $ROOT "crates\app"
$BUILD_OUT = Join-Path $FRONTEND "build\installers"

Write-Host "=== Coco Build ===" -ForegroundColor Cyan
Write-Host "Frontend: $FRONTEND"
Write-Host "Repo:     $ROOT"
Write-Host ""

# Step 1: 构建前端 (VITE_API_MODE=tauri 使用真实后端)
Write-Host "[1/3] Building frontend..." -ForegroundColor Yellow
Push-Location $FRONTEND
$env:VITE_API_MODE = "tauri"
npm run build
if ($LASTEXITCODE -ne 0) { throw "Frontend build failed" }
Pop-Location
Write-Host "[1/3] Frontend build OK" -ForegroundColor Green
Write-Host ""

# Step 2: 构建 Tauri 安装包
Write-Host "[2/3] Building Tauri app (release)..." -ForegroundColor Yellow
Push-Location $TAURI_APP
cargo tauri build
if ($LASTEXITCODE -ne 0) { throw "Tauri build failed" }
Pop-Location
Write-Host "[2/3] Tauri build OK" -ForegroundColor Green
Write-Host ""

# Step 3: 复制产物到 build/installers/
Write-Host "[3/3] Copying installers..." -ForegroundColor Yellow
if (!(Test-Path $BUILD_OUT)) { New-Item -ItemType Directory -Path $BUILD_OUT | Out-Null }

$bundlePath = Join-Path $ROOT "target\release\bundle"

# MSI
$msiFiles = Get-ChildItem -Path (Join-Path $bundlePath "msi") -Filter "*.msi" -ErrorAction SilentlyContinue
foreach ($f in $msiFiles) {
    Copy-Item $f.FullName -Destination $BUILD_OUT -Force
    Write-Host "  Copied: $($f.Name)"
}

# NSIS
$nsisFiles = Get-ChildItem -Path (Join-Path $bundlePath "nsis") -Filter "*.exe" -ErrorAction SilentlyContinue
foreach ($f in $nsisFiles) {
    Copy-Item $f.FullName -Destination $BUILD_OUT -Force
    Write-Host "  Copied: $($f.Name)"
}

Write-Host ""
Write-Host "=== Build Complete ===" -ForegroundColor Green
Write-Host "Installers at: $BUILD_OUT"
Write-Host ""
Get-ChildItem $BUILD_OUT | Format-Table Name, Length -AutoSize
