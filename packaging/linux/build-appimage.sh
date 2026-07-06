#!/usr/bin/env bash
# Build a NeonPrime AppImage. Needs: cargo, wget/curl, and (optionally)
# rsvg-convert or ImageMagick to rasterize the icon. Downloads appimagetool on
# demand. Usage: packaging/linux/build-appimage.sh [version]
set -euo pipefail

VERSION="${1:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"
ARCH="$(uname -m)"
APPDIR="target/AppDir"

echo ">> Building release binary"
cargo build --release --bin neonprime-linux

echo ">> Assembling ${APPDIR}"
rm -rf "${APPDIR}"
mkdir -p "${APPDIR}/usr/bin" \
         "${APPDIR}/usr/share/applications" \
         "${APPDIR}/usr/share/icons/hicolor/scalable/apps" \
         "${APPDIR}/usr/share/icons/hicolor/256x256/apps"

install -m755 target/release/neonprime-linux "${APPDIR}/usr/bin/neonprime"
install -m755 packaging/linux/AppRun    "${APPDIR}/AppRun"
install -m644 packaging/linux/neonprime.desktop "${APPDIR}/neonprime.desktop"
install -m644 packaging/linux/neonprime.desktop "${APPDIR}/usr/share/applications/neonprime.desktop"
install -m644 assets/app-icon.svg "${APPDIR}/usr/share/icons/hicolor/scalable/apps/neonprime.svg"

# Icon: AppImage prefers a PNG top-level .DirIcon. Rasterize if we can, else SVG.
if command -v rsvg-convert >/dev/null 2>&1; then
  rsvg-convert -w 256 -h 256 assets/app-icon.svg \
    -o "${APPDIR}/usr/share/icons/hicolor/256x256/apps/neonprime.png"
  cp "${APPDIR}/usr/share/icons/hicolor/256x256/apps/neonprime.png" "${APPDIR}/neonprime.png"
  cp "${APPDIR}/neonprime.png" "${APPDIR}/.DirIcon"
elif command -v convert >/dev/null 2>&1; then
  convert -background none -resize 256x256 assets/app-icon.svg \
    "${APPDIR}/usr/share/icons/hicolor/256x256/apps/neonprime.png"
  cp "${APPDIR}/usr/share/icons/hicolor/256x256/apps/neonprime.png" "${APPDIR}/neonprime.png"
  cp "${APPDIR}/neonprime.png" "${APPDIR}/.DirIcon"
else
  echo "!! No SVG rasterizer found; using SVG icon (some launchers want PNG)"
  cp assets/app-icon.svg "${APPDIR}/neonprime.svg"
fi

# Fetch appimagetool if not already present.
TOOL="target/appimagetool-${ARCH}.AppImage"
if [ ! -x "${TOOL}" ]; then
  echo ">> Downloading appimagetool"
  URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-${ARCH}.AppImage"
  if command -v wget >/dev/null 2>&1; then wget -qO "${TOOL}" "${URL}"; else curl -fsSL -o "${TOOL}" "${URL}"; fi
  chmod +x "${TOOL}"
fi

OUT="NeonPrime-${VERSION}-${ARCH}.AppImage"
echo ">> Packing ${OUT}"
# --appimage-extract-and-run lets it work inside containers without FUSE.
ARCH="${ARCH}" "${TOOL}" --appimage-extract-and-run "${APPDIR}" "${OUT}"
echo ">> Wrote ${OUT}"
