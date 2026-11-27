#!/usr/bin/env bash
# bygg_paket.sh — bygger installationspaketet (.deb) för SentinAnt Lite, att lämna till en
# ny kunddator (Ubuntu 22.04 eller nyare). Paketet innehåller bara det färdiga programmet,
# ingen källkod.
#
#   bash scripts/bygg_paket.sh
#
# Hamnar i paket/:
#   sentinant-lite_<version>_amd64.deb      sudo apt install ./sentinant-lite_….deb
#
# Paketet lägger programmet i /usr/bin/sentinant-lite, med genväg och ikon i programmenyn.
# /usr/bin kräver lösenord, så programmets egen uppdatering (knappen på startskärmen)
# lägger nya versioner i ~/.local/share/sentinant-lite/program/, och programmet i /usr/bin
# lämnar över till den kopian vid start (client/src/uppdatering.rs). Inställningar och
# sparade robotar (~/.config/robotstyrning) rörs inte av paketet.
#
# Versionen är programmets egen (tiden för senaste commit, se client/build.rs): samma som
# uppdateringsknappen jämför med.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

ARCH="$(dpkg --print-architecture)"
UT="paket"
mkdir -p "$UT"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Prov-miljövariabeln för byggtiden får aldrig följa med in i ett paket.
unset SENTINANT_LITE_BYGGE
cargo build --release -p robotstyrning 2>&1 | tail -1
PROGRAM="target/release/robotstyrning"
# Ett provbygge (godkänner provnyckeln, som vem som helst kan signera med) paketeras aldrig.
if grep -a -q "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL" "$PROGRAM"; then
  echo "FEL: $PROGRAM är ett PROVBYGGE (byggt med provnyckeln) och paketeras inte." >&2
  exit 1
fi
VERSION="$("$PROGRAM" --version)"
[ "$VERSION" = "$(bash scripts/sentinant_lite_version.sh)" ] || { echo "FEL: programmet uppger '$VERSION', väntat $(bash scripts/sentinant_lite_version.sh). Kör: touch client/build.rs och försök igen (byggskriptets version är inaktuell)." >&2; exit 1; }
echo "--- SentinAnt Lite $VERSION ($ARCH) ---"

R="$TMP/sentinant-lite"
install -D -m 755 "$PROGRAM" "$R/usr/bin/sentinant-lite"
strip "$R/usr/bin/sentinant-lite"
install -D -m 644 /dev/stdin "$R/usr/share/applications/sentinant-lite.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=SentinAnt Lite
Comment=Styrprogram för roboten
Exec=/usr/bin/sentinant-lite
Icon=sentinant-lite
StartupWMClass=sentinant-lite
Terminal=false
Categories=Utility;
DESKTOP
install -D -m 644 client/assets/ikon.png "$R/usr/share/icons/hicolor/256x256/apps/sentinant-lite.png"
install -D -m 644 /dev/stdin "$R/usr/share/doc/sentinant-lite/copyright" <<COPYRIGHT
SentinAnt Lite $VERSION
Upphovsrätt © $(date +%Y) Mapro Systems AB.

Biblioteken med öppen källkod som är inbyggda i programmet, och deras licenser,
står i TREDJEPART.md i samma mapp.
COPYRIGHT
install -m 644 TREDJEPART.md "$R/usr/share/doc/sentinant-lite/TREDJEPART.md"
mkdir -p "$R/DEBIAN"
cat > "$R/DEBIAN/control" <<CONTROL
Package: sentinant-lite
Version: $VERSION
Architecture: $ARCH
Maintainer: Mapro Systems AB
Section: misc
Priority: optional
Installed-Size: $(du -sk "$R/usr" | cut -f1)
Depends: libc6, libstdc++6, libgcc-s1, libudev1, libgl1, libx11-6, libxcursor1, libxrandr2, libxi6, libxkbcommon-x11-0, libwayland-client0
Description: SentinAnt Lite - styr roboten med handkontroll
 Kamerabild, karta, status och inställningar för roboten, över WireGuard-tunneln.
 Hette tidigare Robotstyrning. Nya versioner installeras från programmet: knappen
 längst ned på startskärmen blir grön när det finns en uppdatering.
CONTROL
FIL="$UT/sentinant-lite_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$R" "$FIL" >/dev/null
echo "  $FIL ($(du -h "$FIL" | cut -f1))"
echo "Klart. Paketet ligger i $UT/ (läggs inte i git)."
