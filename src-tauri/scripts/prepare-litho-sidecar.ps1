# Build the litho CLI and copy it into src-tauri/binaries/ with the target-triple
# suffix (and .exe on Windows) required by Tauri externalBin sidecars.
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$LithoDir = (Resolve-Path (Join-Path $Root '..\litho')).Path
$BinDir = Join-Path $Root 'src-tauri\binaries'

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

function Ensure-StaticLibLzma {
    $vcpkgRoot = 'C:\vcpkg'
    $vcpkgExe = Join-Path $vcpkgRoot 'vcpkg.exe'
    if (-not (Test-Path -LiteralPath $vcpkgExe)) {
        Write-Error @"
vcpkg not found at $vcpkgExe.
Install vcpkg (or use GitHub Actions windows-latest) and run:
  vcpkg install liblzma:x64-windows-static-md
"@
    }
    $env:VCPKG_ROOT = $vcpkgRoot
    if (Test-Path Env:VCPKGRS_DYNAMIC) {
        Remove-Item Env:VCPKGRS_DYNAMIC
    }
    Write-Host 'Ensuring static liblzma (x64-windows-static-md) is installed...'
    & $vcpkgExe install liblzma:x64-windows-static-md
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Ensure-StaticLibLzma

Write-Host 'Building litho CLI (release, real-io)...'
Push-Location $LithoDir
try {
    cargo build --release --no-default-features --features real-io --bin litho
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

$Triple = (rustc --print host-tuple).Trim()
$Source = Join-Path $LithoDir 'target\release\litho.exe'
$Dest = Join-Path $BinDir "litho-$Triple.exe"

if (-not (Test-Path -LiteralPath $Source)) {
    Write-Error "litho binary not found at $Source"
}

Copy-Item -LiteralPath $Source -Destination $Dest -Force

# Release Windows builds strip Rust symbol names; sanity-check CLI strings instead.
$bytes = [System.IO.File]::ReadAllBytes($Dest)
$text = [System.Text.Encoding]::ASCII.GetString($bytes)
foreach ($marker in @('gui', 'flash', 'clone')) {
    if ($text -notmatch $marker) {
        Write-Error "litho sidecar failed sanity check (missing '$marker'; stale or wrong binary?)"
    }
}

Write-Host "Sidecar ready: $Dest"