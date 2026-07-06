#!/usr/bin/env bash
# Build a portable .tar.gz of the release binary plus desktop integration files.
# Usage: packaging/linux/build-tarball.sh [version]
set -euo pipefail

VERSION="${1:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}"
ARCH="$(uname -m)"
NAME="NeonPrime-${VERSION}-linux-${ARCH}"
STAGE="target/pkg/${NAME}"

echo ">> Building release binaries (GUI + TUI)"
cargo build --release --bin neonprime-linux --bin neonprime-tui

echo ">> Staging ${STAGE}"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"
cp target/release/neonprime-linux "${STAGE}/neonprime"
cp target/release/neonprime-tui "${STAGE}/neonprime-tui"
cp packaging/linux/neonprime.desktop "${STAGE}/"
cp assets/app-icon.svg "${STAGE}/neonprime.svg"
cp README.md LICENSE "${STAGE}/" 2>/dev/null || true

echo ">> Packing"
tar -C target/pkg -czf "${NAME}.tar.gz" "${NAME}"
echo ">> Wrote ${NAME}.tar.gz"
