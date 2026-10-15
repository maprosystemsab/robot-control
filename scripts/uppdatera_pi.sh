#!/usr/bin/env bash
# uppdatera_pi.sh — körs PÅ robotens Pi, normalt av scripts/uppdatera.sh från datorn
# (som först har skickat över den nya koden). Inga frågor utom sudo-lösenordet:
#   0. Läser av roboten: maskin, kort, firmware, styrkortets inställningar,
#      motorlista och vilka VESC som svarar. Sparas i ~/uppdatering_backup/.
#   1. Car_Client byggs om och startas om
#   2. robotd byggs om och startas om (scripts/uppdatera_robotd.sh), och
#      statusskärmen om den är installerad (~/.local/bin/statusskarm)
#   3. Styrkortet flashas om dess firmwareversion skiljer sig från källans
#      (ST-Link ska sitta i). Inställningarna i kortet behålls (ELF-flashning).
#   4. Kontroll: inställningar, motorlista och VESC jämförs med steg 0. Har något
#      ändrats skrivs de sparade inställningarna tillbaka.
#
#   bash scripts/uppdatera_pi.sh
#   MASKIN=mactrac bash scripts/uppdatera_pi.sh   annan firmware än robant
#   FLASHA_ALLTID=1 bash scripts/uppdatera_pi.sh  flasha även om versionen är samma
#
# Flashar inte om någon är ansluten till Car_Client (RControlStation eller
# robotstyrning), eftersom roboten då kan vara i drift. Kör om när den är fri.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
ROOT="$PWD"
GREEN='\033[1;32m'; YELLOW='\033[1;33m'; RED='\033[1;31m'; NC='\033[0m'
steg() { echo -e "\n${GREEN}=== $* ===${NC}"; }
FEL=0
fel() { echo -e "${RED}FEL: $*${NC}"; FEL=1; }

MASKIN="${MASKIN:-$(cat "$HOME/.config/robot-control/maskin" 2>/dev/null || echo robant)}"
# Kortet: nya ROV_MCU (MP101, Upwis) sitter ihop med en CM5; annars det gamla styrkortet.
KORTTYP="gamla"
if tr -d '\0' < /proc/device-tree/model 2>/dev/null | grep -qi "Compute Module 5"; then
  KORTTYP="rovmcu"
fi
BACKUP_DIR="$HOME/uppdatering_backup"
BACKUP="$BACKUP_DIR/styrkort_$(date +%Y-%m-%d_%H-%M-%S).json"
SAMMANFATTNING=()
CC_DIR="$ROOT/rise_sdvp/Linux/Car_Client"
FW_DIR="$ROOT/rise_sdvp/Embedded/RC_Controller"
TOOLS="$ROOT/rise_sdvp/Linux/tools"

echo "sudo behövs för att starta om tjänsterna och flasha (frågar en gång):"
sudo -v || { echo "Avbryter: sudo krävs."; exit 1; }
# Håll sudo vid liv medan bygget pågår (cargo kan ta länge på en Pi).
( while true; do sudo -n true; sleep 50; done ) 2>/dev/null &
SUDO_KEEP=$!
trap 'kill $SUDO_KEEP 2>/dev/null' EXIT

# --- 0. Läs av roboten ------------------------------------------------------------
steg "0/4 Läser av roboten"
echo "Maskin: $MASKIN ($([ "$KORTTYP" = rovmcu ] && echo "ROV_MCU-kortet" || echo "styrkort med ST-Link"))"
mkdir -p "$BACKUP_DIR"
HAR_BACKUP=0
if python3 "$TOOLS/styrkort_backup.py" spara "$BACKUP"; then
  HAR_BACKUP=1
  echo "Backup: $BACKUP"
else
  echo -e "${YELLOW}Kunde inte läsa styrkortet före uppdateringen (ansluten klient, eller kortet svarar inte).${NC}"
  echo -e "${YELLOW}Styrkortet flashas inte utan backup. Car_Client och robotd uppdateras ändå.${NC}"
fi

# --- 1. Car_Client --------------------------------------------------------------
steg "1/4 Car_Client"
if [ -f "$CC_DIR/Car_Client.pro" ]; then
  QMAKE=qmake; command -v qmake6 >/dev/null && QMAKE=qmake6
  # Aldrig en binär från en annan dator (hände på MacBot 2026-09-28): stämmer inte
  # arkitekturen tas den bort, så länkas den om här.
  case "$(uname -m)" in
    aarch64) ARK="aarch64" ;;
    armv7l|armv6l) ARK="ARM," ;;
    x86_64) ARK="x86-64" ;;
    *) ARK="" ;;
  esac
  if [ -n "$ARK" ] && [ -f "$CC_DIR/Car_Client" ] && ! file "$CC_DIR/Car_Client" | grep -q "$ARK"; then
    echo "Car_Client är byggd för en annan dator — tar bort den och bygger om."
    rm -f "$CC_DIR/Car_Client"
  fi
  if (cd "$CC_DIR" && $QMAKE >/dev/null && make -j"$(nproc)" 2>&1 | tail -3); then
    if systemctl list-unit-files car_client.service >/dev/null 2>&1 && systemctl is-enabled --quiet car_client.service; then
      sudo systemctl restart car_client.service && echo "car_client.service omstartad."
    fi
  else
    fel "Car_Client gick inte att bygga (se ovan)."
  fi
else
  echo "Ingen Car_Client här — hoppar över."
fi

# --- 2. robotd ------------------------------------------------------------------
steg "2/4 robotd"
if [ -d "$ROOT/robotd" ]; then
  bash scripts/uppdatera_robotd.sh || fel "robotd gick inte att uppdatera."
else
  echo "Ingen robotd här — hoppar över."
fi

# Statusskärmen (touchskärmen i skåpet), bara om den är installerad på den här Pi:n.
SKARM_SRC="$ROOT/rise_sdvp/Linux/StatusSkarm"
if [ -x "$HOME/.local/bin/statusskarm" ] && [ -f "$SKARM_SRC/Cargo.toml" ]; then
  echo "--- Statusskärmen ---"
  source "$HOME/.cargo/env" 2>/dev/null || true
  if (cd "$SKARM_SRC" && cargo build --release 2>&1 | tail -2); then
    # Byt filen med mv (den körs just nu), och låt statusskarm-loop starta om den.
    cp "$SKARM_SRC/target/release/statusskarm" "$HOME/.local/bin/statusskarm.ny" \
      && mv -f "$HOME/.local/bin/statusskarm.ny" "$HOME/.local/bin/statusskarm" \
      && { pkill -x statusskarm 2>/dev/null; echo "Statusskärmen uppdaterad (startar om av sig själv)."; }
  else
    fel "Statusskärmen gick inte att bygga (se ovan)."
  fi
fi

# --- 3. Styrkortets firmware ----------------------------------------------------
steg "3/4 Styrkortet (firmware $MASKIN)"
KALLA=$(python3 "$TOOLS/fw_version.py" --kalla "$FW_DIR" 2>/dev/null)
KORT=""
if [ ! -f "$TOOLS/fw_version.py" ] || [ -z "$KALLA" ]; then
  fel "Verktyget $TOOLS/fw_version.py saknas eller firmware-källan hittas inte. Flashar inte."
else
  echo "Väntar på att Car_Client ska vara igång..."
  for _ in 1 2 3 4 5 6 7 8; do
    KORT=$(python3 "$TOOLS/fw_version.py" 2>/dev/null) && break
    sleep 5
  done
fi
if [ -z "$KALLA" ]; then
  :
elif [ -z "$KORT" ]; then
  if ss -tn state established '( sport = :8300 )' 2>/dev/null | grep -q ":8300"; then
    fel "Någon är ansluten till Car_Client (RControlStation/robotstyrning), så kortet flashas inte nu. Koppla från och kör uppdateringen igen."
  else
    fel "Styrkortet svarar inte. Flashar inte. Kontrollera strömmen och USB-kabeln."
  fi
elif [ "$KORT" = "$KALLA" ] && [ "${FLASHA_ALLTID:-0}" != "1" ]; then
  echo "Styrkortet har redan firmware $KORT — ingen flashning behövs."
  SAMMANFATTNING+=("Styrkortet: redan firmware $KORT")
elif [ "$HAR_BACKUP" -ne 1 ] && [ "${FLASHA_UTAN_BACKUP:-0}" != "1" ]; then
  fel "Styrkortet har $KORT, ny version är $KALLA, men flashas inte utan backup (se steg 0). Kör igen."
else
  echo "Styrkortet har $KORT, ny version är $KALLA — flashar (inställningarna behålls)."
  if ! lsusb | grep -qi "st-link"; then
    fel "Ingen ST-Link hittades på USB. Flashar inte."
  elif if [ "$KORTTYP" = rovmcu ]; then
         RC_FW_DIR="$FW_DIR" bash "$ROOT/flash_styrkort.sh" --maskin rovmcu --profil "$MASKIN" --fw-dir "$FW_DIR" --ja
       else
         RC_FW_DIR="$FW_DIR" bash "$ROOT/flash_styrkort.sh" --maskin "$MASKIN" --fw-dir "$FW_DIR" --ja
       fi; then
    sleep 5
    NY=""
    for _ in 1 2 3 4 5 6; do NY=$(python3 "$TOOLS/fw_version.py" 2>/dev/null) && break; sleep 5; done
    if [ "$NY" = "$KALLA" ]; then
      echo -e "${GREEN}Styrkortet kör nu firmware $NY.${NC}"
      SAMMANFATTNING+=("Styrkortet: flashat $KORT -> $NY")
      FLASHAT=1
    else
      fel "Efter flashningen svarar kortet med '${NY:-inget svar}' (väntat $KALLA)."
    fi
  else
    fel "Flashningen misslyckades (se ovan). Kortet har kvar sin gamla firmware om openocd inte hann skriva."
  fi
fi

# --- 4. Kontroll ----------------------------------------------------------------
steg "4/4 Kontroll av styrkortet"
if [ "$HAR_BACKUP" -eq 1 ]; then
  python3 "$TOOLS/styrkort_backup.py" aterstall "$BACKUP"
  case $? in
    0) SAMMANFATTNING+=("Inställningar, motorlista och VESC: som före") ;;
    2) fel "Styrkortet svarar inte efter uppdateringen. Kontrollera strömmen och USB-kabeln." ;;
    *) fel "Styrkortet skiljer sig från före uppdateringen (se ovan). Backup: $BACKUP" ;;
  esac
else
  echo "Ingen backup från steg 0 — hoppar över jämförelsen."
fi
if [ "${FLASHAT:-0}" = "1" ] && grep -q "ANGLE_SENSOR_PA3" "$FW_DIR/conf_general.h" 2>/dev/null && [ "$KORTTYP" != rovmcu ]; then
  SAMMANFATTNING+=("Gör själv: kontrollera styrningen (kör 'vinkel' i RControlStation, och kalibrera vid behov)")
fi

echo
for r in "${SAMMANFATTNING[@]}"; do echo "  $r"; done
if [ "$FEL" -eq 0 ]; then
  echo -e "${GREEN}Pi:n och styrkortet är uppdaterade.${NC}"
else
  echo -e "${RED}Uppdateringen av Pi:n blev inte helt klar — se FEL ovan.${NC}"
  exit 1
fi
