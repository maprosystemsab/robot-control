#!/usr/bin/env bash
# uppdatera_dator.sh — uppdaterar programmen på DATORN: Robotstyrning och
# RControlStation. Inget annat: roboten och dess styrkort rörs inte (dem uppdaterar
# vi på distans). Inga frågor utom datorns lösenord.
#
#   cd ~/robot-control && git pull && bash scripts/uppdatera_dator.sh
#
#   1. git pull (robot-control), och skriptet startar om sig självt med den nya versionen
#   2. Robotstyrning (scripts/install_client.sh)
#   3. RControlStation, om den finns: git pull i rise_sdvp och ombyggnad
#      (databasen med dosans inställningar behålls)
#
# Stäng Robotstyrning och RControlStation innan.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
GREEN='\033[1;32m'; YELLOW='\033[1;33m'; RED='\033[1;31m'; NC='\033[0m'
steg() { echo -e "\n${GREEN}##### $* #####${NC}"; }
RESULTAT=()

EFTER_PULL=0
for a in "$@"; do
  case "$a" in
    --efter-pull) EFTER_PULL=1 ;;
    *) echo -e "${RED}Okänt val: $a${NC}"; exit 1 ;;
  esac
done

if pgrep -x RControlStation >/dev/null || pgrep -x robotstyrning >/dev/null; then
  echo -e "${RED}Stäng Robotstyrning och RControlStation först, och kör sedan samma kommando igen.${NC}"
  exit 1
fi

# GitHub-kontot bytte namn 2026-10-06 (lordajz-cmyk -> maprosystemsab). GitHub skickar
# vidare från det gamla namnet, men byt adressen så att det fungerar även om det upphör.
ny_adress() {
  local url
  url="$(git -C "$1" remote get-url origin 2>/dev/null)" || return 0
  case "$url" in *lordajz-cmyk*) git -C "$1" remote set-url origin "${url//lordajz-cmyk/maprosystemsab}" ;; esac
}

if [ "$EFTER_PULL" -eq 0 ]; then
  steg "1/3 Hämtar senaste versionen (git pull)"
  ny_adress .
  if ! git pull --ff-only; then
    echo -e "${RED}git pull gick inte. Har filer i ~/robot-control ändrats för hand?${NC}"
    echo "Visa: git status    Ångra ändringarna: git checkout -- .    Kör sedan igen."
    exit 1
  fi
  exec bash scripts/uppdatera_dator.sh --efter-pull
fi

# --- 2. Robotstyrning --------------------------------------------------------------
steg "2/3 Robotstyrning"
if bash scripts/install_client.sh; then
  RESULTAT+=("Robotstyrning: uppdaterad")
else
  RESULTAT+=("Robotstyrning: FEL, se ovan")
fi

# --- 3. RControlStation --------------------------------------------------------------
steg "3/3 RControlStation"
RCS_REPO=""
# 1. Där programmet faktiskt ligger: install_dator.sh bygger i den klonade mappen
#    (var den än ligger) och länkar /usr/local/bin/RControlStation dit.
for l in "$(command -v RControlStation 2>/dev/null)" /usr/local/bin/RControlStation "$HOME/.local/bin/RControlStation"; do
  [ -n "$l" ] && [ -e "$l" ] || continue
  d="$(readlink -f "$l")"
  while [ -n "$d" ] && [ "$d" != "/" ]; do
    if [ -d "$d/Linux/RControlStation" ] && [ -d "$d/.git" ]; then RCS_REPO="$d"; break 2; fi
    d="$(dirname "$d")"
  done
done
# 2. Vanliga platser, och annars en rise_sdvp-klon någonstans i hemmappen.
if [ -z "$RCS_REPO" ]; then
  for c in "$HOME/rise_sdvp" "$HOME/RControllStation/rise_sdvp" "$HOME/RControlStation/rise_sdvp" \
           $(find "$HOME" -maxdepth 6 -type d -path "*/Linux/RControlStation" -not -path "*/.*" 2>/dev/null | sed 's|/Linux/RControlStation$||'); do
    if [ -d "$c/Linux/RControlStation" ] && [ -d "$c/.git" ]; then RCS_REPO="$c"; break; fi
  done
fi
if [ -z "$RCS_REPO" ]; then
  if command -v RControlStation >/dev/null 2>&1; then
    echo -e "${YELLOW}RControlStation finns ($(readlink -f "$(command -v RControlStation)")), men inte i en git-klon av rise_sdvp — kan inte uppdatera den.${NC}"
    echo "Installera om enligt steg 8 i guiden (git clone … ~/rise_sdvp, sudo bash install_dator.sh)."
    RESULTAT+=("RControlStation: FEL, hittar ingen git-klon att uppdatera")
  else
    echo "RControlStation är inte installerad (steg 8 i guiden) — hoppar över."
    RESULTAT+=("RControlStation: inte installerad")
  fi
elif ! { ny_adress "$RCS_REPO"; git -C "$RCS_REPO" pull --ff-only; }; then
  echo -e "${RED}git pull i $RCS_REPO gick inte (ändrade filer?).${NC}"
  RESULTAT+=("RControlStation: FEL vid git pull")
else
  # uppdatera.sh skickar styrkortets firmware och Car_Client från den här klonen till roboten.
  mkdir -p "$HOME/.config/robot-control" && echo "$RCS_REPO" > "$HOME/.config/robot-control/rise_sdvp"
  STATION="$RCS_REPO/Linux/RControlStation"
  BYGG="$STATION/build/cmake_linux"
  # Bygg vidare i den befintliga byggmappen: databasen (dosa-bindningar) bredvid
  # programmet ligger kvar. Finns ingen byggmapp: fullständigt bygge.
  if [ -f "$BYGG/CMakeCache.txt" ]; then
    LOGG="$(mktemp /tmp/rcontrolstation_bygge.XXXXXX.log)"
    cmake --build "$BYGG" -j"$(nproc)" > "$LOGG" 2>&1
    OK=$?
    grep -E "error|Built target RControlStation$" "$LOGG" | tail -5
    [ "$OK" -eq 0 ] && rm -f "$LOGG" || echo "Hela byggloggen: $LOGG"
  else
    (cd "$STATION" && ./build_cmake_linux.sh release)
    OK=$?
  fi
  if [ "$OK" -eq 0 ] && [ -x "$BYGG/build/lin/RControlStation" ]; then
    RESULTAT+=("RControlStation: uppdaterad")
  else
    RESULTAT+=("RControlStation: FEL vid bygget, se ovan")
  fi
fi

steg "Sammanfattning (datorn)"
ALLT_OK=1
for r in "${RESULTAT[@]}"; do
  if [[ "$r" == *FEL* ]]; then echo -e "${RED}  $r${NC}"; ALLT_OK=0; else echo -e "  $r"; fi
done
if [ "$ALLT_OK" -eq 1 ]; then
  echo -e "\n${GREEN}Datorn är uppdaterad. Starta Robotstyrning eller RControlStation som vanligt.${NC}"
else
  echo -e "\n${YELLOW}Något blev inte klart. Kör samma kommando igen; det som redan är klart går fort.${NC}"
  exit 1
fi
