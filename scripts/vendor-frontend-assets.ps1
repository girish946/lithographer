# Download third-party frontend assets used by src/index.html for offline bundling.
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Vendor = Join-Path $Root 'src\vendor'
$FaBase = 'https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.6.0'
$TailwindVersion = '3.4.17'

$dirs = @(
    (Join-Path $Vendor 'fontawesome\css'),
    (Join-Path $Vendor 'fontawesome\webfonts'),
    (Join-Path $Vendor 'fonts')
)
foreach ($dir in $dirs) {
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
}

function Download-File {
    param(
        [Parameter(Mandatory = $true)][string]$Uri,
        [Parameter(Mandatory = $true)][string]$OutFile
    )
    Invoke-WebRequest -Uri $Uri -OutFile $OutFile -UseBasicParsing
}

Write-Host "Downloading Tailwind CSS browser build v$TailwindVersion..."
Download-File `
    -Uri "https://cdn.tailwindcss.com/$TailwindVersion" `
    -OutFile (Join-Path $Vendor "tailwindcss-$TailwindVersion.js")

Write-Host 'Downloading Font Awesome 6.6.0...'
Download-File `
    -Uri "$FaBase/css/all.min.css" `
    -OutFile (Join-Path $Vendor 'fontawesome\css\all.min.css')

foreach ($font in @('fa-solid-900', 'fa-regular-400', 'fa-brands-400')) {
    Download-File `
        -Uri "$FaBase/webfonts/$font.woff2" `
        -OutFile (Join-Path $Vendor "fontawesome\webfonts\$font.woff2")
}

Write-Host 'Downloading Inter and Space Grotesk (latin subsets)...'
Download-File `
    -Uri 'https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7.woff2' `
    -OutFile (Join-Path $Vendor 'fonts\inter-latin.woff2')
Download-File `
    -Uri 'https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPbF4Cw.woff2' `
    -OutFile (Join-Path $Vendor 'fonts\space-grotesk-latin.woff2')

$fontsCss = @'
/* Bundled locally from Google Fonts (latin subsets). */
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 400;
  font-display: swap;
  src: url('./inter-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 500;
  font-display: swap;
  src: url('./inter-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
@font-face {
  font-family: 'Inter';
  font-style: normal;
  font-weight: 600;
  font-display: swap;
  src: url('./inter-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
@font-face {
  font-family: 'Space Grotesk';
  font-style: normal;
  font-weight: 500;
  font-display: swap;
  src: url('./space-grotesk-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
@font-face {
  font-family: 'Space Grotesk';
  font-style: normal;
  font-weight: 600;
  font-display: swap;
  src: url('./space-grotesk-latin.woff2') format('woff2');
  unicode-range: U+0000-00FF, U+0131, U+0152-0153, U+02BB-02BC, U+02C6, U+02DA, U+02DC, U+0304, U+0308, U+0329, U+2000-206F, U+20AC, U+2122, U+2191, U+2193, U+2212, U+2215, U+FEFF, U+FFFD;
}
'@

$fontsCssPath = Join-Path $Vendor 'fonts\fonts.css'
[System.IO.File]::WriteAllText($fontsCssPath, $fontsCss, [System.Text.UTF8Encoding]::new($false))

Write-Host 'Vendor assets ready under src/vendor/'
Get-ChildItem -LiteralPath $Vendor -Recurse -File | ForEach-Object { $_.FullName }