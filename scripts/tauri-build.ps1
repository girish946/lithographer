# Prepare litho sidecar, run Tauri CLI, then Linux-only post-steps (none on Windows).
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$TauriArgs
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$PrepareScript = Join-Path $RepoRoot 'src-tauri\scripts\prepare-litho-sidecar.ps1'

& $PrepareScript
if ($LASTEXITCODE -ne 0) {
    exit $LASTEXITCODE
}

Push-Location $RepoRoot
try {
    if ($TauriArgs.Count -eq 0) {
        npm exec tauri
    } else {
        npm exec -- tauri @TauriArgs
    }
    exit $LASTEXITCODE
} finally {
    Pop-Location
}