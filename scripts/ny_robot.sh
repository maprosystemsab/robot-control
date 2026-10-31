#!/usr/bin/env bash
# ny_robot.sh — körs på DATORN. Gör en robots Pi klar: frågar efter namn,
# WireGuard-IP m.m., skickar över koden och installerar det som behövs:
#   1. WireGuard (valfritt)              wireguard/wireguard.sh
#   2. Grundsystemet (valfritt):         rise_sdvp/install_car_client.sh
#      Car_Client, Swepos-RTK, udev-regler, flashverktyg
#   3. robotd som tjänst                 scripts/install_pi.sh + uppdatera_robotd.sh
#
#   bash scripts/ny_robot.sh
#
# Förutsättning: Raspberry Pi OS med SSH påslaget. Se dokument/installationsguide.md.
# Går att köra om; befintlig config skrivs bara över om du svarar ja (den
# gamla sparas som .bak).
#
# Senare uppdateringar av robotd: bash scripts/skicka_robotd.sh (med PI=...).

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

GREEN='\033[1;32m'; YELLOW='\033[1;33m'; RED='\033[1;31m'; NC='\033[0m'

fraga() { # fraga "Text" standardvärde -> svar i $SVAR
  local text="$1" std="${2:-}"
  if [ -n "$std" ]; then
    read -r -p "$text [$std]: " SVAR
    SVAR="${SVAR:-$std}"
  else
    SVAR=""
    while [ -z "$SVAR" ]; do read -r -p "$text: " SVAR; done
  fi
}

echo -e "${GREEN}=== Ny robot: installera robotd ===${NC}"
echo

fraga "Robotens namn (t.ex. drangen, robant)"
NAMN="$(echo "$SVAR" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9_-')"
[ -n "$NAMN" ] || { echo -e "${RED}Ogiltigt namn.${NC}"; exit 1; }

while true; do
  fraga "Robotens WireGuard-IP (192.168.200.x)"
  IP="$SVAR"
  [[ "$IP" =~ ^[0-9]{1,3}(\.[0-9]{1,3}){3}$ ]] && break
  echo -e "${RED}Det ser inte ut som en IP-adress, försök igen.${NC}"
done

fraga "Adress att nå Pi:n på just nu (ny Pi utan WireGuard: t.ex. raspberrypi.local eller LAN-IP)" "$IP"
NU_ADR="$SVAR"
fraga "Användarnamn på Pi:n" "$NAMN"
ANV="$SVAR"
fraga "Bil-ID i Car_Client (--setid)" "4"
CAR_ID="$SVAR"
fraga "VESC-ID på roboten, kommaseparerade" "28,36,76"
VESC_IDS="$(echo "$SVAR" | tr -d ' ')"
fraga "Max vid fullt spakutslag (som Max i RControlStation)" "0.45"
DRIVE_MAX="$SVAR"
fraga "Är en nödstoppsknapp (NC) inkopplad på GPIO27 och GND? (j/n)" "n"
NODSTOPP="$SVAR"
fraga "Slå på I2C för OLED-skärmen? Kräver omstart av Pi:n senare (j/n)" "n"
I2C="$SVAR"
fraga "Installera WireGuard på Pi:n nu? (j/n)" "n"
WG="$SVAR"
fraga "Installera grundsystemet (Car_Client, Swepos-RTK, udev)? Behövs på en ny Pi (j/n)" "j"
GRUND="$SVAR"

PI="${ANV}@${NU_ADR}"
echo
echo -e "${GREEN}--- Sammanfattning ---${NC}"
echo "  Robot:      $NAMN"
echo "  Pi:         $PI"
echo "  robotd:     lyssnar på ${IP}:9000 (video ${IP}:9001)"
echo "  Bil-ID:     $CAR_ID"
echo "  VESC:       $VESC_IDS"
echo "  Max:        $DRIVE_MAX"
echo "  I2C/OLED:   $I2C"
echo "  Nödstopp:   $NODSTOPP (GPIO27)"
echo "  WireGuard:  $WG     Grundsystem: $GRUND"
echo "  Startar automatiskt vid uppstart, tar Car_Client bara medan någon kör"
echo "  med robotstyrning (RControlStation fungerar som vanligt annars)."
echo
read -r -p "Skriv ja för att installera: " OK
[ "$OK" = "ja" ] || { echo "Avbrutet, inget gjort."; exit 0; }

echo
echo -e "${GREEN}--- Kontrollerar anslutningen till $PI ---${NC}"
if ! ssh -o BatchMode=yes -o ConnectTimeout=8 "$PI" true 2>/dev/null; then
  echo -e "${YELLOW}Kan inte logga in utan lösenord.${NC}"
  read -r -p "Kopiera din SSH-nyckel till Pi:n (slipper lösenord för SSH framöver)? [J/n] " K
  if [ "${K:-j}" != "n" ]; then
    ssh-copy-id "$PI"
  fi
  ssh -o ConnectTimeout=8 "$PI" true || { echo -e "${RED}Når inte $PI — är WireGuard uppe på båda?${NC}"; exit 1; }
fi

if [ "$GRUND" = "j" ] && ssh "$PI" 'systemctl is-active --quiet car_client.service'; then
  echo -e "${YELLOW}Car_Client kör redan på Pi:n. Grundsystemet installeras om från robot-control"
  echo -e "(Car_Client byggs från rise_sdvp/ här och car_client.service pekar dit efteråt).${NC}"
  read -r -p "Fortsätta med grundsystemet ändå? [j/N] " G
  [ "${G:-n}" = "j" ] || GRUND="n"
fi
if [ "$GRUND" != "j" ] && ! ssh "$PI" 'systemctl is-active --quiet car_client.service'; then
  echo -e "${YELLOW}Varning: car_client.service kör inte på Pi:n. robotd installeras ändå, men kan"
  echo -e "inte köra roboten förrän grundsystemet är installerat (kör om och svara j).${NC}"
fi

SKRIV_OVER=0
if ssh "$PI" 'test -f /etc/robotd/config.json'; then
  echo -e "${YELLOW}Pi:n har redan en /etc/robotd/config.json:${NC}"
  ssh "$PI" 'cat /etc/robotd/config.json'
  echo
  read -r -p "Ersätt den med värdena ovan? Den gamla sparas som .bak. [j/N] " E
  [ "${E:-n}" = "j" ] && SKRIV_OVER=1
fi

echo -e "${GREEN}--- Kopierar projektet ---${NC}"
rsync -a --exclude target --exclude .git --exclude osm_tiles --include '/rise_sdvp/Linux/Car_Client/res/**' --exclude '*.png' ./ "${PI}:robot-control/"

NODSTOPP_GPIO=""
[ "$NODSTOPP" = "j" ] && NODSTOPP_GPIO=27

SKIPPA_I2C=1
[ "$I2C" = "j" ] && SKIPPA_I2C=0

if [ "$WG" = "j" ]; then
  echo -e "${GREEN}--- WireGuard (svara på frågorna; sista siffran i ${IP} är VPN-adressen) ---${NC}"
  ssh -t "$PI" "cd robot-control/wireguard && sudo ./wireguard.sh"
  echo -e "${YELLOW}Kom ihåg: lägg till Pi:ns publika nyckel på VPN-servern (wireguard_admin.sh),"
  echo -e "se wireguard/Wireguard_Guide.md.${NC}"
fi

if [ "$GRUND" = "j" ]; then
  echo -e "${GREEN}--- Grundsystemet: Car_Client, Swepos-RTK, udev (frågar efter Swepos-konto) ---${NC}"
  ssh -t "$PI" "cd robot-control/rise_sdvp && sudo CAR_ID='$CAR_ID' NODSTOPP_GPIO='$NODSTOPP_GPIO' ./install_car_client.sh"
fi

echo -e "${GREEN}--- Installerar robotd (frågar efter sudo-lösenordet) ---${NC}"
ssh -t "$PI" "cd robot-control && \
  ROBOT_ID='$NAMN' BIND_ADDR='${IP}:9000' CAR_ID='$CAR_ID' VESC_IDS='$VESC_IDS' \
  DRIVE_MAX='$DRIVE_MAX' SKRIV_OVER_CONFIG='$SKRIV_OVER' SKIPPA_I2C='$SKIPPA_I2C' \
  bash scripts/install_pi.sh && bash scripts/uppdatera_robotd.sh --aktivera"

echo
if ! grep -qE "^[0-9.]+\s+.*\b${NAMN}\b" /etc/hosts; then
  read -r -p "Lägg till '$NAMN' i datorns /etc/hosts, så kan du skriva namnet i robotstyrning? [J/n] " H
  if [ "${H:-j}" != "n" ]; then
    echo "$IP $NAMN" | sudo tee -a /etc/hosts > /dev/null
    echo "Tillagt: $IP $NAMN"
  fi
fi

# Robotens adress till scripts/uppdatera.sh (datorn uppdaterar sedan den här roboten).
mkdir -p "$HOME/.config/robot-control" && echo "${ANV}@${IP}" > "$HOME/.config/robot-control/pi"

echo
echo -e "${GREEN}=== Klart: $NAMN ===${NC}"
echo "Anslut med robotstyrning till: $NAMN  (eller $IP)"
echo "Logg på roboten:    ssh ${ANV}@${IP} journalctl -u robotd -f"
echo "Uppdatera allt:     bash scripts/uppdatera.sh   (dator, robot och styrkort; kunder: uppdatera_dator.sh)"
echo "Flasha styrkortet:  på Pi:n: cd robot-control && ./flash_styrkort.sh"
