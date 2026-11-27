#!/usr/bin/env bash
# sentinant_lite_version.sh — skriver SentinAnt Lites version för trädet man står i, räknad
# på samma sätt som client/build.rs: paketets version ur Cargo.toml + tiden för senaste
# commit i UTC (ÅÅÅÅMMDDTTMM), t.ex. 0.1.0+202611270737.
#
#   bash scripts/sentinant_lite_version.sh
#
# Används av bygg_windows.sh och publicera_lite.sh (Windows-programmet går inte att fråga
# på en Linux-dator; skripten kontrollerar att versionen står i programfilen).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
VER="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
BYGGE="${SENTINANT_LITE_BYGGE:-$(TZ=UTC git log -1 --format=%cd --date=format-local:%Y%m%d%H%M 2>/dev/null || echo 0)}"
if [ "$BYGGE" = 0 ] || [ -z "$BYGGE" ]; then echo "$VER"; else echo "$VER+$BYGGE"; fi
