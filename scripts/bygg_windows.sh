#!/usr/bin/env bash
# bygg_windows.sh — bygger SentinAnt Lite för Windows (64-bitars) på den här Linux-datorn:
# programfilen och installationsfilen kunden dubbelklickar på.
#
#   bash scripts/bygg_windows.sh
#
# Hamnar i paket/:
#   windows/sentinant-lite.exe                     programmet (det uppdateringen byter ut)
#   SentinAnt-Lite-installation-<version>.exe      installationsfilen (per användare, ingen admin)
#
# Kräver (en gång):
#   rustup target add x86_64-pc-windows-gnu
#   sudo apt install g++-mingw-w64-x86-64-posix mingw-w64-x86-64-dev nsis nasm
# Samma bygge som SentinAnts (autorobot, scripts/bygg_windows.sh). Cargos byggmapp för
# Windows (target/x86_64-pc-windows-gnu, drygt 1 GB) går att ta bort efteråt.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

MAL=x86_64-pc-windows-gnu
for verktyg in x86_64-w64-mingw32-g++ x86_64-w64-mingw32-windres makensis; do
  command -v "$verktyg" >/dev/null || { echo "Saknas: $verktyg. Kör: sudo apt install g++-mingw-w64-x86-64-posix mingw-w64-x86-64-dev nsis nasm" >&2; exit 1; }
done
rustup target list --installed | grep -qx "$MAL" || { echo "Saknas: rustup target add $MAL" >&2; exit 1; }

# Prov-miljövariabeln för byggtiden får aldrig följa med in i en installationsfil.
unset SENTINANT_LITE_BYGGE
VERSION="$(bash scripts/sentinant_lite_version.sh)"
VER3="${VERSION%%+*}"
UT="paket"
mkdir -p "$UT/windows"

# openh264 är C++: C++-biblioteket ska byggas in i programfilen, annars vill programmet ha
# libstdc++-6.dll bredvid sig. cc får länka det statiskt, och rustc får veta var det ligger.
export CXXSTDLIB_x86_64_pc_windows_gnu="static=stdc++"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-L native=$(dirname "$(x86_64-w64-mingw32-g++ -print-file-name=libstdc++.a)")"

echo "--- SentinAnt Lite $VERSION för Windows ---"
# (Länkarens varning "corrupt .drectve at end of def file" är ofarlig: äldre binutils.)
cargo build --release -p robotstyrning --target "$MAL" 2>&1 | tail -1
BYGGT="target/$MAL/release/robotstyrning.exe"
[ -f "$BYGGT" ] || { echo "FEL: bygget gav ingen programfil ($BYGGT). Kör cargo build --release -p robotstyrning --target $MAL och läs felet." >&2; exit 1; }
# Ett provbygge (godkänner provnyckeln, som vem som helst kan signera med) delas aldrig ut.
if grep -a -q "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL" "$BYGGT"; then
  echo "FEL: $BYGGT är ett PROVBYGGE (byggt med provnyckeln) och paketeras inte." >&2
  exit 1
fi
PROGRAM="$UT/windows/sentinant-lite.exe"
cp "$BYGGT" "$PROGRAM"
x86_64-w64-mingw32-strip "$PROGRAM"

# Programmet ska klara sig med det som finns i Windows: inga DLL-filer från mingw.
if x86_64-w64-mingw32-objdump -p "$PROGRAM" | grep -i "DLL Name" | grep -qi "libstdc++\|libgcc\|libwinpthread"; then
  echo "FEL: programmet behöver DLL-filer från mingw (se CXXSTDLIB ovan):" >&2
  x86_64-w64-mingw32-objdump -p "$PROGRAM" | grep -i "DLL Name" | grep -i "libstdc++\|libgcc\|libwinpthread" >&2
  exit 1
fi
# Versionen ska stå i programfilen (den går inte att fråga här).
grep -q -a -F "$VERSION" "$PROGRAM" || { echo "FEL: $PROGRAM innehåller inte versionen $VERSION" >&2; exit 1; }

INSTALLATION="$UT/SentinAnt-Lite-installation-${VERSION/+/-}.exe"
makensis -V2 -INPUTCHARSET UTF8 \
  -DVERSION="$VERSION" -DVERSION4="$VER3.0" \
  -DPROGRAM="$PWD/$PROGRAM" -DUTFIL="$PWD/$INSTALLATION" \
  -DIKON="$PWD/client/assets/ikon.ico" -DTREDJEPART="$PWD/TREDJEPART.md" \
  client/windows/installation.nsi
echo "  $PROGRAM ($(du -h "$PROGRAM" | cut -f1))"
echo "  $INSTALLATION ($(du -h "$INSTALLATION" | cut -f1))"
echo "Klart. Filerna ligger i $UT/ (läggs inte i git)."
