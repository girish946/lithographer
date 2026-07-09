# Regenerate src-tauri/icons/* from the same logo assets used in the HTML UI.
$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Source = Join-Path $RepoRoot 'src\assets\dark-logo.jpg'
$OutDir = Join-Path $RepoRoot 'src-tauri\icons'

if (-not (Test-Path -LiteralPath $Source)) {
    Write-Error "Icon source not found: $Source"
}

Push-Location $RepoRoot
try {
    Write-Host "Generating Tauri icons from $Source ..."
    npm exec -- tauri icon $Source -o src-tauri/icons
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
} finally {
    Pop-Location
}

Write-Host "Icons written to $OutDir"