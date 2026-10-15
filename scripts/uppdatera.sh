#!/usr/bin/env bash
# uppdatera.sh — VÅRT skript för allt: datorn (scripts/uppdatera_dator.sh) och sedan
# robotens Pi och styrkortet. Kunder kör bara uppdatera_dator.sh; robotarna och
# styrkorten uppdaterar vi på distans med det här skriptet.
#
#   bash scripts/uppdatera.sh                                   datorn + roboten
#   PI=anvandare@192.168.200.x bash scripts/uppdatera.sh        annan robot (sparas)
#
# Datorn först: RControlStation måste känna till styrkortets nya firmwareversion
# innan kortet flashas, annars kopplar den ner direkt. Roboten: koden (bara filerna i
# git) skickas över och scripts/uppdatera_pi.sh körs där: Car_Client, robotd och
# styrkortets firmware (bara om versionen är ny). Robotens inställningar, WireGuard-
# nycklar och styrkortets inställningar rörs inte.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
GREEN='\033[1;32m'; YELLOW='\033[1;33m'; RED='\033[1;31m'; NC='\033[0m'
steg() { echo -e "\n${GREEN}##### $* #####${NC}"; }
KONF="$HOME/.config/robot-control"
RESULTAT=()

EFTER_PULL=0
for a in "$@"; do
  case "$a" in
    --efter-pull) EFTER_PULL=1 ;;
    --robot) ;;   # gammalt val: roboten ingår alltid här
    *) echo -e "${RED}Okänt val: $a${NC}"; exit 1 ;;
  esac
done

# GitHub-kontot bytte namn 2026-10-06 (lordajz-cmyk -> maprosystemsab). GitHub skickar
# vidare från det gamla namnet, men byt adressen så att det fungerar även om det upphör.
ny_adress() {
  local url
  url="$(git -C "$1" remote get-url origin 2>/dev/null)" || return 0
  case "$url" in *lordajz-cmyk*) git -C "$1" remote set-url origin "${url//lordajz-cmyk/maprosystemsab}" ;; esac
}

if [ "$EFTER_PULL" -eq 0 ]; then
  if pgrep -x RControlStation >/dev/null || pgrep -x robotstyrning >/dev/null; then
    echo -e "${RED}Stäng Robotstyrning och RControlStation först, och kör sedan samma kommando igen.${NC}"
    exit 1
  fi
  steg "Hämtar senaste versionen (git pull)"
  ny_adress .
  if ! git pull --ff-only; then
    echo -e "${RED}git pull gick inte. Har filer i ~/robot-control ändrats för hand?${NC}"
    exit 1
  fi
  exec bash scripts/uppdatera.sh --efter-pull
fi

# --- 1. Datorn ------------------------------------------------------------------------
if bash scripts/uppdatera_dator.sh --efter-pull; then
  RESULTAT+=("Datorn: uppdaterad")
else
  RESULTAT+=("Datorn: FEL, se ovan")
fi

# --- 2. Robotens Pi och styrkortet ------------------------------------------------------
steg "Roboten (Pi:n och styrkortet)"
PI="${PI:-$(cat "$KONF/pi" 2>/dev/null || echo robant@192.168.200.10)}"
echo "Robot: $PI"
# En och samma ssh-anslutning för allt, så att lösenordet bara behövs en gång.
SOCK="$(mktemp -u /tmp/uppdatera_ssh.XXXXXX)"
# accept-new: första gången godkänns robotens nyckel utan yes/no-fråga (en ändrad
# nyckel stoppas fortfarande).
SSH=(ssh -o ConnectTimeout=15 -o StrictHostKeyChecking=accept-new -o ControlMaster=auto -o ControlPath="$SOCK" -o ControlPersist=600)
if ! "${SSH[@]}" -o ServerAliveInterval=15 "$PI" true; then
  echo -e "${RED}Når inte roboten ($PI). Är den påslagen och WireGuard uppe?${NC}"
  RESULTAT+=("Roboten: FEL, gick inte att nå")
else
  mkdir -p "$KONF" && echo "$PI" > "$KONF/pi"
  echo "--- Skickar koden till roboten ---"
  # Bara filerna i git: inga byggda filer (en Car_Client eller firmware byggd på
  # den här datorn fungerar inte på Pi:n) och inga lokala/interna filer.
  SKICKAT=0
  if git ls-files -z | rsync -a --from0 --files-from=- -e "${SSH[*]}" ./ "$PI:robot-control/"; then
    SKICKAT=1
  fi
  # Styrkortets firmware, Car_Client och verktygen: det senaste från rise_sdvp (som
  # uppdatera_dator.sh nyss hämtat), inte kopian i robot-control som kan ligga efter.
  RISE="$(cat "$KONF/rise_sdvp" 2>/dev/null)"
  if [ "$SKICKAT" -eq 1 ] && [ -n "$RISE" ] && [ -d "$RISE/Embedded/RC_Controller" ]; then
    if [ -n "$(git -C "$RISE" status --porcelain --untracked-files=no -- Embedded/RC_Controller Linux/Car_Client Linux/tools)" ]; then
      echo -e "${YELLOW}rise_sdvp ($RISE) har ändrade filer som inte är incheckade — skickar robot-controls kopia i stället.${NC}"
      RESULTAT+=("Firmware/Car_Client: robot-controls kopia (rise_sdvp hade ej incheckade ändringar)")
    elif (cd "$RISE" && git ls-files -z -- Embedded/RC_Controller Linux/Car_Client Linux/tools \
          | rsync -a --from0 --files-from=- -e "${SSH[*]}" ./ "$PI:robot-control/rise_sdvp/"); then
      echo "Firmware och Car_Client från rise_sdvp $(git -C "$RISE" log -1 --format='%h (%cd)' --date=short)."
    else
      SKICKAT=0
    fi
  elif [ "$SKICKAT" -eq 1 ]; then
    echo -e "${YELLOW}Hittar ingen rise_sdvp-klon (RControlStation) — skickar robot-controls kopia av firmwaren.${NC}"
  fi
  if [ "$SKICKAT" -eq 1 ]; then
    if "${SSH[@]}" -t "$PI" "cd robot-control && bash scripts/uppdatera_pi.sh"; then
      RESULTAT+=("Roboten: Car_Client, robotd och styrkortet uppdaterade")
    else
      RESULTAT+=("Roboten: FEL, se ovan")
    fi
  else
    RESULTAT+=("Roboten: FEL när koden skulle skickas")
  fi
  "${SSH[@]}" -O exit "$PI" 2>/dev/null
fi

steg "Sammanfattning"
ALLT_OK=1
for r in "${RESULTAT[@]}"; do
  if [[ "$r" == *FEL* ]]; then echo -e "${RED}  $r${NC}"; ALLT_OK=0; else echo -e "  $r"; fi
done
if [ "$ALLT_OK" -eq 1 ]; then
  echo -e "\n${GREEN}Allt är uppdaterat: datorn, roboten och styrkortet.${NC}"
else
  echo -e "\n${YELLOW}Något blev inte klart. Kör samma kommando igen; det som redan är klart går fort.${NC}"
  exit 1
fi
