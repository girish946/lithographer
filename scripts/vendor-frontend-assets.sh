#!/usr/bin/env bash
# Download third-party frontend assets used by src/index.html for offline bundling.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VENDOR="$ROOT/src/vendor"
FA_BASE="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.6.0"
TAILWIND_VERSION="3.4.17"

mkdir -p \
  "$VENDOR/fontawesome/css" \
  "$VENDOR/fontawesome/webfonts" \
  "$VENDOR/fonts"

echo "Downloading Tailwind CSS browser build v${TAILWIND_VERSION}..."
curl -fsSL "https://cdn.tailwindcss.com/${TAILWIND_VERSION}" \
  -o "$VENDOR/tailwindcss-${TAILWIND_VERSION}.js"

echo "Downloading Font Awesome ${FA_BASE##*/}..."
curl -fsSL "${FA_BASE}/css/all.min.css" -o "$VENDOR/fontawesome/css/all.min.css"
for font in fa-solid-900 fa-regular-400 fa-brands-400; do
  curl -fsSL "${FA_BASE}/webfonts/${font}.woff2" \
    -o "$VENDOR/fontawesome/webfonts/${font}.woff2"
done

echo "Downloading Inter and Space Grotesk (latin subsets)..."
curl -fsSL \
  "https://fonts.gstatic.com/s/inter/v20/UcC73FwrK3iLTeHuS_nVMrMxCp50SjIa1ZL7.woff2" \
  -o "$VENDOR/fonts/inter-latin.woff2"
curl -fsSL \
  "https://fonts.gstatic.com/s/spacegrotesk/v22/V8mDoQDjQSkFtoMM3T6r8E7mPbF4Cw.woff2" \
  -o "$VENDOR/fonts/space-grotesk-latin.woff2"

cat > "$VENDOR/fonts/fonts.css" <<'EOF'
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
EOF

echo "Vendor assets ready under src/vendor/"
ls -lh "$VENDOR/tailwindcss-${TAILWIND_VERSION}.js" \
  "$VENDOR/fontawesome/css/all.min.css" \
  "$VENDOR/fontawesome/webfonts/"*.woff2 \
  "$VENDOR/fonts/"*.woff2 \
  "$VENDOR/fonts/fonts.css"