#!/usr/bin/env bash
# publicera_lite.sh — bygger SentinAnt Lite för Linux och Windows och lägger en färdig,
# signerad uppdatering i en lokal mapp. Mappen kopieras sedan för hand till
# uppdateringsservern (se README.md, "Publicera en ny version"). När den ligger där blir
# knappen på startskärmen grön hos alla, och ett tryck på "Uppdatera nu" installerar den
# nya versionen.
#
#   bash scripts/publicera_lite.sh [--nyheter "Kort text som visas vid knappen"]
#                                  [--ut MAPP] [--nyckel FIL] [--giltig-dagar N]
#                                  [--tillat-andrat] [--utan-server] [--utan-windows]
#
# Mappen (standard paket/uppdatering/) innehåller efteråt:
#   uppdatering.json                                   listan, signerad
#   sentinant-lite-<version>-linux-x86_64              programmet för Linux (det knappen hämtar)
#   sentinant-lite-<version>-windows-x86_64.exe        programmet för Windows (det knappen hämtar)
#   SentinAnt-Lite-installation.exe                    installationsfilen för nya Windows-datorer
#   sentinant-lite_amd64.deb                           paketet för nya Linux-datorer
# och dessutom filerna för de andra posterna i listan (SentinAnt, styrkortets firmware).
#
# Listan är gemensam med SentinAnt och styrkortets firmware: SentinAnt Lite har posten
# "sentinant-lite". Skriptet hämtar först serverns lista (bara läsning), kontrollerar dess
# signatur och utgår från den, så att de andra posterna ligger kvar orörda och löpnumret
# blir högre än serverns. Filerna som de andra posterna nämner hämtas till mappen, så att
# den kan läggas ut som en helhet.
#
# Skriptet rör inte servern och skickar ingenting. Den privata nyckeln ligger i
# ~/Claude/hemligt/sentinant-uppdateringsnyckel-privat.pem (samma som SentinAnt, aldrig i
# git). Versionen är tiden för senaste commit, så allt ska vara committat innan.
#
#   --utan-server    hämta inte serverns lista först (bara för prov utan nät)
#   --utan-windows   bygg bara Linux (bara för prov; en riktig publicering har båda)
#   --tillat-andrat  bygg fast det finns ocommittade ändringar (bara för prov)
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

UT="paket/uppdatering"; NYHETER=""; NYCKEL=(); ANDRAT=0; GILTIG=0; WINDOWS=1
SERVER=(--utga-fran "http://192.168.200.1:8088/sentinant/")
while [ $# -gt 0 ]; do
  case "$1" in
    --ut) UT="$2"; shift 2 ;;
    --nyheter) NYHETER="$2"; shift 2 ;;
    --nyckel) NYCKEL+=(--nyckel "$2"); shift 2 ;;
    --giltig-dagar) GILTIG="$2"; shift 2 ;;
    --tillat-andrat) ANDRAT=1; shift ;;
    --utan-server) SERVER=(); shift ;;
    --utan-windows) WINDOWS=0; shift ;;
    -h|--hjalp) sed -n '2,33p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Okänt val: $1" >&2; exit 1 ;;
  esac
done

# Versionen räknas ur senaste commit: ocommittade ändringar skulle ge ett program som
# inte motsvarar sin version. Prov-miljövariabeln för byggtiden får aldrig följa med.
unset SENTINANT_LITE_BYGGE
if [ "$ANDRAT" -eq 0 ] && [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "Det finns ändringar som inte är committade:" >&2
  git status --short --untracked-files=no >&2
  echo "Committa först (eller --tillat-andrat för ett prov)." >&2
  exit 1
fi
VERSION="$(bash scripts/sentinant_lite_version.sh)"
NAMN="${VERSION/+/-}"
echo "##### SentinAnt Lite $VERSION (gren $(git rev-parse --abbrev-ref HEAD), commit $(git rev-parse --short HEAD)) #####"

echo "--- Linux ---"
cargo build --release -p robotstyrning 2>&1 | tail -1
PROGRAM="target/release/robotstyrning"
# Ett provbygge (godkänner provnyckeln, som vem som helst kan signera med) publiceras aldrig.
if grep -a -q "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL" "$PROGRAM"; then
  echo "FEL: $PROGRAM är ett PROVBYGGE (byggt med provnyckeln) och publiceras inte." >&2
  exit 1
fi
if [ "$("$PROGRAM" --version)" != "$VERSION" ]; then
  # Byggskriptet räknar bara om versionen när git-mappen det senast såg ändras. Har samma
  # byggmapp använts från en annan kopia av repot kan den vara inaktuell: räkna om en gång.
  echo "Programmet uppger '$("$PROGRAM" --version)', väntat $VERSION: bygger om med omräknad version."
  touch client/build.rs
  cargo build --release -p robotstyrning 2>&1 | tail -1
fi
[ "$("$PROGRAM" --version)" = "$VERSION" ] || { echo "FEL: Linux-programmet uppger '$("$PROGRAM" --version)', väntat $VERSION" >&2; exit 1; }
bash scripts/bygg_paket.sh
if [ "$WINDOWS" -eq 1 ]; then bash scripts/bygg_windows.sh; fi

mkdir -p "$UT"
LINUX="sentinant-lite-$NAMN-linux-x86_64"
FILER=(--fil "linux-x86_64=$LINUX")
install -m 755 "$PROGRAM" "$UT/$LINUX"
strip "$UT/$LINUX"
install -m 644 "paket/sentinant-lite_${VERSION}_$(dpkg --print-architecture).deb" "$UT/sentinant-lite_amd64.deb"
if [ "$WINDOWS" -eq 1 ]; then
  WIN="sentinant-lite-$NAMN-windows-x86_64.exe"
  FILER+=(--fil "windows-x86_64=$WIN")
  install -m 644 paket/windows/sentinant-lite.exe "$UT/$WIN"
  install -m 644 "paket/SentinAnt-Lite-installation-$NAMN.exe" "$UT/SentinAnt-Lite-installation.exe"
else
  echo "OBS: utan Windows. Den här mappen är ett prov och ska inte läggas ut."
fi

echo "--- Lista och signatur ---"
python3 scripts/uppdatering_manifest.py --mapp "$UT" --paket sentinant-lite --version "$VERSION" --nyheter "$NYHETER" \
  --giltig-dagar "$GILTIG" "${FILER[@]}" "${NYCKEL[@]}" "${SERVER[@]}"

# Sista kontrollen görs av programmet självt, med nyckeln som är inbyggd i det: godkänner
# det inte mappen gör ingen kunds program det heller. Alla poster kontrolleras, också
# SentinAnts och firmwarens.
echo "--- Kontroll med programmets inbyggda nyckel ---"
"$PROGRAM" --kontrollera-uppdatering "$UT" || { echo "FEL: programmet godkänner inte $UT. Lägg INTE ut den." >&2; exit 1; }

echo
echo "Klart: $UT/ innehåller SentinAnt Lite $VERSION."
echo "Ingenting har skickats. Lägg ut mappen på uppdateringsservern enligt README.md"
echo "(programfilerna först, uppdatering.json sist)."
