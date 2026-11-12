#!/bin/bash
# ==============================================================================
# ⚡ flash_styrkort_rovmcu.sh: bygg och flasha STM32:an på ROV_MCU (Upwis MP101_323)
# ==============================================================================
# Körs PÅ kortets egen Raspberry Pi CM5:
#
#   cd ~/rise_sdvp && ./flash_styrkort_rovmcu.sh            bygg, fråga FLASHA, flasha
#   ./flash_styrkort_rovmcu.sh --ja                          utan FLASHA-frågan
#   ./flash_styrkort_rovmcu.sh --bara-bygg                   bygg bara, rör ingen hårdvara
#   ./flash_styrkort_rovmcu.sh --st-link                     tvinga ST-Link
#   ./flash_styrkort_rovmcu.sh --cm5                         tvinga CM5:ans SWD-ben
#   --nodstopp-di1 --lagesknapp-di2                          som mk1 (.16): nödstopp DI1, lägesknapp DI2
#   (glöms de byggs firmware utan dem: nödstoppet går inte att läsa, mk1 2026-11-12)
#
# Firmware: make robant BOARD=mp101 (eller mactrac/drangen via --profile).
# Se Embedded/RC_Controller/ROVMCU.md. ELF-filen flashas, så
# styrkortets inställningar (EEPROM) behålls.
#
# Två sätt att nå STM32:an:
#   - ST-Link V2 på USB (som på MacBot), om en sådan sitter i.
#   - CM5:ans egna ben, inbyggt på kortet: GPIO11 = SWCLK, GPIO8 = SWDIO (via R39),
#     GPIO17 = NRST, GPIO16 = BOOT0. openocd med linuxgpiod (fungerar på Pi 5/CM5).
# ==============================================================================
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
FW_DIR="$DIR/Embedded/RC_Controller"

RED='\e[31m'; GREEN='\e[32m'; YELLOW='\e[33m'; BOLD='\e[1m'; NC='\e[0m'

SATT=""; JA=0; BARA_BYGG=0; PROFILE=robant; FW_VAL=()
ORIGINAL_ARGS=("$@")
while [ "$#" -gt 0 ]; do
  case "$1" in
    --st-link) SATT="stlink"; shift ;;
    --cm5) SATT="cm5"; shift ;;
    --ja) JA=1; shift ;;
    --bara-bygg) BARA_BYGG=1; shift ;;
    --nodstopp-di1) FW_VAL+=(--nodstopp-di1); shift ;;
    --lagesknapp-di2) FW_VAL+=(--lagesknapp-di2); shift ;;
    --profile) [ "$#" -ge 2 ] || exit 1; PROFILE="$2"; shift 2 ;;
    -h|--hjalp|--help)
      sed -n '3,22p' "$0" | sed 's/^# \{0,1\}//'
      echo "  --profile robant|mactrac|drangen (default: robant)"; exit 0 ;;
    *) echo -e "${RED}Okänt argument: $1${NC}"; exit 1 ;;
  esac
done
case "$PROFILE" in robant|mactrac|drangen) ;; *) echo "Unknown profile: $PROFILE" >&2; exit 1 ;; esac
ELF="$FW_DIR/build/fw_${PROFILE}.elf"

# CM5-benen (BCM-numrering)
SWCLK=11; SWDIO=8; NRST=17; BOOT0=16

# --- 1. Bygg (via flash_styrkort.sh, som flashar ELF och kan ST-Link) --------------
if [ -z "$SATT" ] && lsusb 2>/dev/null | grep -qi "st-link"; then
  SATT="stlink"
fi
if [ "$SATT" = "stlink" ]; then
  echo -e "${BOLD}ST-Link hittad: flashar med flash_styrkort.sh.${NC}"
  ARGS=(--maskin rovmcu --fw-dir "$FW_DIR" --profile "$PROFILE" "${FW_VAL[@]}")
  [ "$JA" -eq 1 ] && ARGS+=(--ja)
  [ "$BARA_BYGG" -eq 1 ] && ARGS+=(--bara-bygg)
  exec "$DIR/flash_styrkort.sh" "${ARGS[@]}"
fi

if [ "$BARA_BYGG" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
  exec sudo bash "$0" --cm5 "${ORIGINAL_ARGS[@]}"
fi

echo -e "${BOLD}Bygger firmware för ROV_MCU...${NC}"
BUILD_LOG=$(mktemp /tmp/rovmcu_bygg.XXXXXX.log) || exit 1
if ! bash "$DIR/flash_styrkort.sh" --maskin rovmcu --profile "$PROFILE" --fw-dir "$FW_DIR" "${FW_VAL[@]}" --bara-bygg >"$BUILD_LOG" 2>&1; then
  tail -20 "$BUILD_LOG"
  echo -e "${RED}❌ Bygget misslyckades (hela loggen: $BUILD_LOG).${NC}"
  exit 1
fi
rm -f "$BUILD_LOG"
[ -s "$ELF" ] || { echo "Missing firmware ELF: $ELF" >&2; exit 1; }
echo -e "${GREEN}✅ Byggd: $ELF${NC}"
[ "$BARA_BYGG" -eq 1 ] && { echo "Klart (--bara-bygg): ingen hårdvara har rörts."; exit 0; }

# --- 2. Flasha via CM5:ans SWD-ben ---------------------------------------------------
for TOOL in openocd gpiodetect gpioinfo gpioset pinctrl; do
  command -v "$TOOL" >/dev/null || { echo "Missing $TOOL; run install_pi.sh --board mp101 first." >&2; exit 1; }
done
gpioset --help | grep -q -- '--consumer' || { echo "CM5 flashing requires libgpiod v2." >&2; exit 1; }
# GPIO-kretsen för 40-stiftsbenen heter "pinctrl-rp1" på Pi 5/CM5 (gpiochip0 eller
# gpiochip4 beroende på kärnversion).
CHIP=$(gpiodetect | sed -n 's/^gpiochip\([0-9][0-9]*\) \[pinctrl-rp1\].*/\1/p')
[[ "$CHIP" =~ ^[0-9]+$ ]] || { echo "Cannot identify the CM5 pinctrl-rp1 GPIO chip." >&2; exit 1; }

if pgrep -x Car_Client >/dev/null 2>&1; then
  echo -e "${RED}${BOLD}⚠️  Car_Client körs.${NC} Flashningen startar om styrkortet: maskinen ska stå still"
  echo "   (motorn av på hydrauliska maskiner) och RControlStation/robotstyrning vara frånkopplade."
fi
if [ "$JA" -ne 1 ]; then
  read -p "Skriv ordet FLASHA för att programmera STM32:an via CM5 (allt annat avbryter): " SVAR
  [ "$SVAR" = "FLASHA" ] || { echo "Avbrutet — ingenting flashades."; exit 130; }
fi

CFG=$(mktemp /tmp/rovmcu_openocd.XXXXXX.cfg) || exit 1
GPIO_LOG=$(mktemp /tmp/rovmcu_gpio.XXXXXX.log) || { rm -f "$CFG"; exit 1; }
BOOT_PID=""
RESTORE_GPIO=0
cleanup_gpio() {
  local result=$?
  trap - EXIT
  if [ -n "$BOOT_PID" ]; then
    kill "$BOOT_PID" 2>/dev/null || true
    wait "$BOOT_PID" 2>/dev/null || true
  fi
  if [ "$RESTORE_GPIO" -eq 1 ]; then
    pinctrl set "$BOOT0" op dl || result=1
    pinctrl set "$NRST" op dh || result=1
  fi
  rm -f "$CFG" "$GPIO_LOG"
  exit "$result"
}
trap cleanup_gpio EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cat > "$CFG" <<EOF
adapter driver linuxgpiod
adapter gpio swclk $SWCLK -chip $CHIP
adapter gpio swdio $SWDIO -chip $CHIP
adapter gpio srst $NRST -chip $CHIP
transport select swd
adapter speed 1000
reset_config srst_only srst_push_pull
source [find target/stm32f4x.cfg]
EOF

# BOOT0 låg (vanlig start från flash) medan vi flashar.
gpioset --chip "gpiochip$CHIP" --consumer mp101-boot0 "$BOOT0=0" >"$GPIO_LOG" 2>&1 &
BOOT_PID=$!
BOOT_READY=0
for ATTEMPT in {1..20}; do
  if ! kill -0 "$BOOT_PID" 2>/dev/null; then break; fi
  if gpioinfo --chip "gpiochip$CHIP" "$BOOT0" 2>/dev/null | grep -q '"mp101-boot0"'; then
    BOOT_READY=1
    break
  fi
  sleep 0.05
done
if [ "$BOOT_READY" -ne 1 ]; then
  cat "$GPIO_LOG" >&2
  echo "Could not hold STM32 BOOT0 low; flashing aborted." >&2
  exit 1
fi
RESTORE_GPIO=1

echo -e "\n${BOLD}Flashar via CM5 (gpiochip$CHIP: SWCLK=$SWCLK SWDIO=$SWDIO NRST=$NRST)...${NC}"
if openocd -f "$CFG" -c "program {$ELF} verify reset exit"; then
  echo -e "${GREEN}✅ Flashningen slutförd och verifierad.${NC}"
  RES=0
else
  echo -e "${RED}❌ Flashningen misslyckades.${NC} Kontrollera att R39 (SWDIO) är monterad och att"
  echo "   STM32:an har ström. Pröva annars med ST-Link (--st-link)."
  RES=1
fi
# STM32:ans reset (GPIO17) och BOOT0 (GPIO16) ska ha fasta nivåer när ingen flashar,
# annars kan Pi:ns standardneddragning på GPIO17 hålla STM32:an i reset.
kill "$BOOT_PID" 2>/dev/null || true
wait "$BOOT_PID" 2>/dev/null || true
BOOT_PID=""
pinctrl set "$BOOT0" op dl || exit 1
pinctrl set "$NRST" op dh || exit 1
RESTORE_GPIO=0

if ! grep -qsE "^gpio=17=op,dh" /boot/firmware/config.txt; then
  echo -e "\n${YELLOW}Tips:${NC} lägg till i /boot/firmware/config.txt (och starta om CM5 en gång):"
  echo "   gpio=17=op,dh   # STM32 NRST hög (kör)"
  echo "   gpio=16=op,dl   # STM32 BOOT0 låg (starta från flash)"
fi

# Kontroll: läs kortets firmwareversion via Car_Client om den kör.
if [ "$RES" -eq 0 ] && pgrep -x Car_Client >/dev/null 2>&1 && [ -f "$DIR/Linux/tools/fw_version.py" ]; then
  sleep 8
  echo "Firmware på kortet: $(cd /tmp && python3 "$DIR/Linux/tools/fw_version.py" 2>/dev/null || echo 'svarar inte (RControlStation ansluten?)')"
fi
exit $RES
