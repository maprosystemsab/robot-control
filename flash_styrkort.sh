#!/bin/bash

# ==============================================================================
# ⚡ flash_styrkort.sh: Kompilera och programmera Carcontroller-kortet (STM32)
# ==============================================================================
# Bygger styrsystemets firmware (rise_sdvp / RC_Controller), flashar det till
# STM32-kortet via ST-LINK V2 och kan köra ett live-diagnostiktest.
#
# Fungerar oavsett var skriptet ligger (rise_sdvp-mappen, robot-control-repot
# eller någon annanstans): firmware-källan hittas automatiskt, se hitta_fw_dir.
#
# Användning:
#   ./flash_styrkort.sh                      interaktivt, som förut
#   ./flash_styrkort.sh --maskin drangen     hoppa över maskinfrågan
#   ./flash_styrkort.sh --maskin rovmcu --profil robant
#                                            MP101/ROV_MCU-kortet (Upwis) med en maskins
#                                            inställningar (robant, mactrac eller drangen)
#   ./flash_styrkort.sh --maskin rovmcu --profil robant --nodstopp-di1
#                                            MP101 med nödstoppet på DI1 (NC-knapp, se ROVMCU.md)
#   ./flash_styrkort.sh --bara-bygg          bygg och kontrollera, RÖR INGEN HÅRDVARA
#   ./flash_styrkort.sh --patch FIL.patch    bygg i en TILLFÄLLIG KOPIA med patchen
#                                            applicerad; din firmware-mapp lämnas orörd
#   ./flash_styrkort.sh --fw-dir MAPP        peka ut RC_Controller-mappen själv
#                                            (eller sätt miljövariabeln RC_FW_DIR)
#   ./flash_styrkort.sh --ja                 hoppa över FLASHA-bekräftelsen (icke-interaktivt)
#
# Flashningen kräver att du skriver ordet FLASHA, om du inte kör med --ja
# (eller sätter miljövariabeln RC_FLASH_JA=1). Bygget kan köras utan att någon
# hårdvara är inkopplad.
# ==============================================================================

GREEN='\e[32m'
RED='\e[31m'
YELLOW='\e[33m'
BLUE='\e[34m'
BOLD='\e[1m'
NC='\e[0m' # No Color

# Ta reda på den faktiska användaren och var skriptet ligger
REAL_USER=${SUDO_USER:-$USER}
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

FW_NAME=""
PROFIL=""
PATCH_FILE=""
BUILD_ONLY=0
FW_DIR_ARG="${RC_FW_DIR:-}"
AUTO_JA="${RC_FLASH_JA:-0}"

usage() {
  sed -n '3,25p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
  case "$1" in
    --maskin)     FW_NAME="$2"; shift 2 ;;
    --profil|--profile) PROFIL="$2"; shift 2 ;;
    --patch)      PATCH_FILE="$2"; shift 2 ;;
    --bara-bygg)  BUILD_ONLY=1; shift ;;
    --fw-dir)     FW_DIR_ARG="$2"; shift 2 ;;
    --ja)         AUTO_JA=1; shift ;;
    --nodstopp-di1) NODSTOPP_DI1=1; shift ;;
    -h|--hjalp|--help) usage; exit 0 ;;
    *) echo -e "${RED}Okänt argument: $1${NC}"; usage; exit 1 ;;
  esac
done

# ------------------------------------------------------------------------------
# Hitta firmware-källan (Embedded/RC_Controller). Första träffen med en Makefile
# vinner. Ordningen: --fw-dir/RC_FW_DIR, sedan vanliga platser relativt skriptet,
# sedan RControlStation-installationen i hemkatalogen.
# ------------------------------------------------------------------------------
hitta_fw_dir() {
  local c
  local hem
  hem=$(getent passwd "$REAL_USER" | cut -d: -f6)
  for c in \
    "$FW_DIR_ARG" \
    "$DIR/Embedded/RC_Controller" \
    "$DIR/rise_sdvp/Embedded/RC_Controller" \
    "$DIR/../rise_sdvp/Embedded/RC_Controller" \
    "$hem/RControllStation/rise_sdvp/Embedded/RC_Controller" \
    "$hem/RControlStation/rise_sdvp/Embedded/RC_Controller"; do
    if [ -n "$c" ] && [ -f "$c/Makefile" ] && [ -f "$c/commands.c" ]; then
      ( cd "$c" && pwd )
      return 0
    fi
  done
  return 1
}

echo -e "${BLUE}${BOLD}======================================================================${NC}"
echo -e "${BLUE}${BOLD}   ⚡  FLASHNING AV CARCONTROLLER-KORTET (STM32-MIKRODATORN)   ⚡${NC}"
echo -e "${BLUE}${BOLD}======================================================================${NC}"

FW_SRC=$(hitta_fw_dir)
if [ -z "$FW_SRC" ]; then
  echo -e "${RED}❌ Hittade inte firmware-källan (Embedded/RC_Controller).${NC}"
  echo -e "Ange var den ligger: ${BOLD}$0 --fw-dir /sökväg/till/Embedded/RC_Controller${NC}"
  exit 1
fi
echo -e "Firmware-källa: ${BOLD}$FW_SRC${NC}"

# Kontrollera om utvecklingsverktygen för ARM och OpenOCD finns installerade.
# OpenOCD behövs bara om vi ska flasha.
NEED_TOOLS="arm-none-eabi-gcc"
[ "$BUILD_ONLY" -eq 0 ] && NEED_TOOLS="$NEED_TOOLS openocd"
MISSING=""
for t in $NEED_TOOLS; do
  command -v "$t" &> /dev/null || MISSING="$MISSING $t"
done
if [ -n "$MISSING" ]; then
  echo -e "${YELLOW}⚠️ Verktyg saknas på den här datorn:${BOLD}$MISSING${NC}"
  if [ "$AUTO_JA" = "1" ]; then
    ANS="j"
  else
    read -p "Installera build-essential, openocd och gcc-arm-none-eabi med apt (kräver sudo)? [j/N] " ANS
  fi
  if [[ "$ANS" =~ ^[JjYy]$ ]]; then
    sudo apt update && sudo apt install -y build-essential openocd gcc-arm-none-eabi
  else
    echo -e "${RED}Avbryter — verktygen behövs.${NC}"
    exit 1
  fi
fi

# ------------------------------------------------------------------------------
# 🛠️ STEG 1: Välj maskintyp och kompilera firmware
# ------------------------------------------------------------------------------
if [ -z "$FW_NAME" ]; then
  echo -e "\n${YELLOW}${BOLD}[Steg 1/3] Välj vilken maskin du vill bygga för:${NC}"
  echo -e " 1) ${BOLD}Drängen${NC}"
  echo -e " 2) ${BOLD}Mactrac${NC}"
  echo -e " 3) ${BOLD}RobAnt${NC} (VESC 28/36/76)"
  echo -e " 4) ${BOLD}ROV_MCU / MP101${NC} (Upwis nya kort; flashas helst med flash_styrkort_rovmcu.sh)"
  read -p "Välj maskin (1, 2, 3 eller 4): " M_CHOICE
  case "$M_CHOICE" in
    1) FW_NAME="drangen" ;;
    2) FW_NAME="mactrac" ;;
    3) FW_NAME="robant" ;;
    4) FW_NAME="rovmcu"
       read -p "Vilken maskin sitter kortet i? 1) Drängen 2) Mactrac 3) RobAnt [3]: " P_CHOICE
       case "${P_CHOICE:-3}" in 1) PROFIL="drangen" ;; 2) PROFIL="mactrac" ;; *) PROFIL="robant" ;; esac ;;
    *) echo -e "${RED}Ogiltigt val! Avbryter.${NC}"; exit 1 ;;
  esac
else
  echo -e "\n${YELLOW}${BOLD}[Steg 1/3] Maskin: $FW_NAME${NC}"
  case "$FW_NAME" in
    drangen|mactrac|robant|rovmcu) ;;
    *) echo -e "${RED}Okänd maskin '$FW_NAME' (drangen, mactrac, robant eller rovmcu). Avbryter.${NC}"; exit 1 ;;
  esac
fi

# Med --patch byggs en tillfällig kopia (patchen är skriven med sökvägar som
# Embedded/RC_Controller/..., så kopian lägger firmware-mappen där). Annars byggs
# källmappen på plats, precis som förut.
TMP_ROOT=""
cleanup() { [ -n "$TMP_ROOT" ] && rm -rf "$TMP_ROOT"; }
trap cleanup EXIT

if [ -n "$PATCH_FILE" ]; then
  if [ ! -f "$PATCH_FILE" ]; then
    echo -e "${RED}❌ Hittar inte patchfilen: $PATCH_FILE${NC}"
    exit 1
  fi
  PATCH_FILE=$(cd "$(dirname "$PATCH_FILE")" && pwd)/$(basename "$PATCH_FILE")
  TMP_ROOT=$(mktemp -d)
  mkdir -p "$TMP_ROOT/Embedded"
  cp -r "$FW_SRC" "$TMP_ROOT/Embedded/RC_Controller"
  rm -rf "$TMP_ROOT/Embedded/RC_Controller/build" "$TMP_ROOT/Embedded/RC_Controller/.dep"
  if ! ( cd "$TMP_ROOT" && patch -p1 --dry-run < "$PATCH_FILE" > /dev/null ); then
    echo -e "${RED}❌ Patchen passar inte mot den här firmware-versionen (kanske redan applicerad?).${NC}"
    exit 1
  fi
  ( cd "$TMP_ROOT" && patch -p1 < "$PATCH_FILE" )
  FW_DIR="$TMP_ROOT/Embedded/RC_Controller"
  echo -e "${GREEN}Patch applicerad på en tillfällig kopia — din firmware-mapp är orörd.${NC}"
else
  FW_DIR="$FW_SRC"
fi

# MP101/ROV_MCU byggs som en vanlig maskin med BOARD=mp101 (Upwis, rise_sdvp PR #1).
# Äldre firmwarekällor saknar BOARD och har i stället målet "rovmcu".
MAKE_TARGET="$FW_NAME"
MAKE_BOARD=""
if [ -n "$PROFIL" ] && [ "$FW_NAME" != rovmcu ]; then
  echo -e "${RED}--profil gäller bara --maskin rovmcu.${NC}"; exit 1
fi
if [ -n "${NODSTOPP_DI1:-}" ] && [ "$FW_NAME" != rovmcu ]; then
  echo -e "${RED}--nodstopp-di1 gäller bara --maskin rovmcu.${NC}"; exit 1
fi
if [ "$FW_NAME" = rovmcu ]; then
  case "${PROFIL:=robant}" in drangen|mactrac|robant) ;; *) echo -e "${RED}Okänd profil '$PROFIL' (robant, mactrac eller drangen).${NC}"; exit 1 ;; esac
  if grep -q '^BOARD ?=' "$FW_DIR/Makefile"; then
    MAKE_TARGET="$PROFIL"
    MAKE_BOARD="BOARD=mp101${NODSTOPP_DI1:+ NODSTOPP=di1}"
  elif [ -n "${NODSTOPP_DI1:-}" ]; then
    echo -e "${RED}Firmwarekällan saknar BOARD=mp101 och kan inte bygga nödstoppet på DI1.${NC}"; exit 1
  fi
fi

echo -e "\nKompilerar firmware för ${BOLD}$FW_NAME${NC}${MAKE_BOARD:+ (${PROFIL}, $MAKE_BOARD)}..."

# Bygg källkoden (körs som den vanliga användaren för att undvika root-ägda filer)
if [ "$EUID" -eq 0 ]; then
  sudo -u "$REAL_USER" bash -c "cd '$FW_DIR' && make clean && make -j\$(nproc) $MAKE_TARGET $MAKE_BOARD"
else
  ( cd "$FW_DIR" && make clean && make -j"$(nproc)" "$MAKE_TARGET" $MAKE_BOARD )
fi
BUILD_RC=$?

BIN="$FW_DIR/build/fw_${MAKE_TARGET}.bin"
if [ $BUILD_RC -eq 0 ] && [ -f "$BIN" ]; then
  echo -e "${GREEN}✅ Styrkortets firmware ($FW_NAME) kompilerad framgångsrikt!${NC}"
  echo -e "   Fil:    $BIN"
  echo -e "   Storlek: $(stat -c %s "$BIN") byte   sha256: $(sha256sum "$BIN" | cut -c1-16)…"
else
  echo -e "${RED}❌ Kompileringsfel för styrkortets firmware. Kontrollera loggarna ovan.${NC}"
  exit 1
fi

# Med --patch försvinner den tillfälliga kopian när skriptet slutar, så spara
# binären bredvid skriptet så att den går att flasha/granska senare.
if [ -n "$TMP_ROOT" ]; then
  mkdir -p "$DIR/fw_out"
  cp "$BIN" "$DIR/fw_out/fw_${FW_NAME}_patchad.bin"
  cp "${BIN%.bin}.elf" "$DIR/fw_out/fw_${FW_NAME}_patchad.elf"
  BIN="$DIR/fw_out/fw_${FW_NAME}_patchad.bin"
  echo -e "   Sparad kopia: $BIN"
fi

if [ "$BUILD_ONLY" -eq 1 ]; then
  echo -e "\n${GREEN}${BOLD}Klart (--bara-bygg): ingen hårdvara har rörts, ingenting flashades.${NC}"
  exit 0
fi

# ------------------------------------------------------------------------------
# ⚡ STEG 2: Flasha styrkortet med ST-LINK V2
# ------------------------------------------------------------------------------
echo -e "\n${YELLOW}${BOLD}[Steg 2/3] Förbered programmering via ST-LINK V2...${NC}"

# På robotens Pi ligger Car_Client och kortet i drift. En flashning startar om
# kortet, så varna tydligt innan något görs.
CAR_CLIENT_RUNNING=0
if pgrep -x Car_Client > /dev/null 2>&1; then
  CAR_CLIENT_RUNNING=1
  echo -e "${RED}${BOLD}⚠️  Car_Client körs på den här datorn.${NC} Flashningen startar om styrkortet"
  echo -e "   och avbryter allt som styr roboten just nu. Försäkra dig om att roboten står stilla,"
  echo -e "   och på en hydraulisk maskin (MacTrac) att motorn är avstängd."
  echo -e "   Koppla från RControlStation/robotstyrning först, så kan skriptet läsa av kortet efteråt."
fi

echo -e "${BLUE}${BOLD}Instruktioner för hårdvarukoppling:${NC}"
echo -e "1. Anslut din ${BOLD}ST-LINK V2 USB-sticka${NC} till datorn."
echo -e "2. Koppla SWD-kablarna till Carcontroller-kortets SWD-pinnar:"
echo -e "   - ${BOLD}SWCLK${NC} -> SWCLK"
echo -e "   - ${BOLD}SWDIO${NC} -> SWDIO"
echo -e "   - ${BOLD}GND${NC}   -> GND"
echo -e "   - ${BOLD}3.3V${NC}  -> 3.3V (Strömsätter styrkortet direkt från ST-LINK!)"
echo ""

echo -e "Firmware som flashas: ${BOLD}$BIN${NC}"
if [ "$AUTO_JA" = "1" ]; then
  echo -e "${YELLOW}--ja angiven: hoppar över FLASHA-bekräftelsen och flashar direkt.${NC}"
else
  read -p "Skriv ordet FLASHA för att programmera kortet (allt annat avbryter): " CONFIRM
  if [ "$CONFIRM" != "FLASHA" ]; then
    echo -e "${YELLOW}Avbrutet — ingenting flashades.${NC}"
    exit 0
  fi
fi

# Kontrollera om ST-LINK syns på USB-bussen
if ! lsusb | grep -qi "st-link"; then
  echo -e "${YELLOW}⚠️ Kunde inte hitta någon ST-LINK V2 ansluten via USB. Kontrollera kontakten.${NC}"
  read -p "Vill du försöka flasha ändå? Tryck [ENTER] för att köra OpenOCD (Ctrl+C avbryter)." RetryTrigger
fi

echo -e "\n${BOLD}Startar flashning via OpenOCD...${NC}"

# Flasha ELF-filen, inte .bin: .bin täcker hela 0x08000000–slutet, inklusive
# EEPROM-emuleringen på 0x08004000–0x0800BFFF (se eeprom.h), så varje flashning
# nollställde robotens sparade inställningar (aktuatorer, ID m.m.). ELF-filen
# skriver bara sektorerna som programmet faktiskt ligger i och låter EEPROM vara.
ELF="${BIN%.bin}.elf"
if [ -f "$ELF" ]; then
  PROG_CMD="program $ELF verify reset exit"
else
  echo -e "${YELLOW}⚠️ Hittade ingen ELF-fil ($ELF) — flashar .bin, vilket RADERAR sparade inställningar på styrkortet.${NC}"
  PROG_CMD="program $BIN verify reset exit 0x08000000"
fi
if [ "$EUID" -ne 0 ]; then
  sudo openocd -f board/stm32f4discovery.cfg -c "reset_config trst_only combined" -c "$PROG_CMD"
else
  openocd -f board/stm32f4discovery.cfg -c "reset_config trst_only combined" -c "$PROG_CMD"
fi

if [ $? -eq 0 ]; then
  echo -e "\n${GREEN}✅ Flashningen slutförd och verifierad framgångsrikt!${NC}"
else
  echo -e "\n${RED}❌ Flashningen misslyckades!${NC}"
  echo -e "Koppla ur och sätt i ST-LINK:en igen, kontrollera att kablarna"
  echo -e "sitter stabilt på styrkortets SWD-pinnar och testa igen."
  exit 1
fi

# ------------------------------------------------------------------------------
# 🔬 STEG 3: Kontroll efter flashningen
# ------------------------------------------------------------------------------
# På en robot/maskin (Car_Client kör): läs styrkortets inställningar via Car_Client,
# samma som Read i RControlStation. Bara läsning. Svarar kortet kör den nya firmwaren.
if [ "$CAR_CLIENT_RUNNING" -eq 1 ]; then
  echo -e "\n${YELLOW}${BOLD}[Steg 3/3] Läser av styrkortet via Car_Client...${NC}"
  LAS=""
  for c in "$(dirname "$(dirname "$FW_SRC")")/Linux/tools/las_styrkort.py" "$DIR/Linux/tools/las_styrkort.py"; do
    [ -f "$c" ] && LAS="$c" && break
  done
  sleep 8   # kortet startar om och Car_Client öppnar porten igen
  if [ -n "$LAS" ] && command -v python3 >/dev/null 2>&1; then
    if (cd /tmp && python3 "$LAS" 127.0.0.1 | grep -q "^STYRKORT"); then
      echo -e "${GREEN}✅ Styrkortet svarar efter flashningen.${NC}"
      TW="$(dirname "$LAS")/testa_write.py"
      if [ -f "$TW" ]; then
        echo -e "   Testa att Write fungerar (skriver tillbaka kortets egna värden, ändrar inget):"
        echo -e "   ${BOLD}python3 $TW 3${NC}"
      fi
    else
      echo -e "${YELLOW}⚠️ Kunde inte läsa styrkortet. Är RControlStation ansluten? Koppla från den och kör:${NC}"
      echo -e "   ${BOLD}python3 $LAS${NC}"
    fi
  else
    echo -e "${YELLOW}Läsverktyget (Linux/tools/las_styrkort.py) saknas här. Kontrollera med Read i RControlStation.${NC}"
  fi
  RUN_DIAG="n"
elif [ "$AUTO_JA" = "1" ]; then
  RUN_DIAG="n"
  echo -e "\n${YELLOW}${BOLD}[Steg 3/3] --ja angiven: hoppar över det interaktiva skrivbordstestet.${NC}"
else
  echo -e "\n${YELLOW}${BOLD}[Steg 3/3] Kortet på skrivbordet: vill du köra ett test via USB nu? (y/N)${NC}"
  read -p "Köra diagnostiktest? (y/N): " RUN_DIAG
fi

if [[ "$RUN_DIAG" =~ ^[Yy]$ ]]; then
  echo -e "\n${BLUE}${BOLD}Instruktioner för skrivbordstest:${NC}"
  echo -e "1. Behåll din ${BOLD}ST-LINK V2${NC} inkopplad (så att kortet får ström)."
  echo -e "2. Anslut ${BOLD}TVÅ Micro-USB-kablar${NC} mellan din dator och de två portarna på Carcontroller-kortet."
  echo ""
  read -p "Tryck på [ENTER] när kablarna är anslutna så startar vi testet!" ConnectTrigger

  echo -e "\n${BLUE}Söker efter serieportar från styrkortet...${NC}"
  sleep 2 # Vänta lite så att USB-portarna hinner registreras av Linux

  PORT0="/dev/ttyACM0"
  PORT1="/dev/ttyACM1"

  # Kontrollera om USB-portarna har skapats i systemet
  if [ -c "$PORT0" ] && [ -c "$PORT1" ]; then
    echo -e "${GREEN}✅ Framgång! Hittade båda USB-portarna ($PORT0 och $PORT1) på datorn!${NC}"
    echo -e "Detta bevisar att båda delarna av styrkortet är igång och pratar med datorn."
  elif [ -c "$PORT0" ] || [ -c "$PORT1" ]; then
    echo -e "${YELLOW}⚠️ Delvis framgång! Hittade bara en av portarna. Kontrollera att BÅDA USB-sladdarna är i.${NC}"
  else
    echo -e "${RED}❌ Testet misslyckades! Kunde inte hitta några USB-portar från styrkortet.${NC}"
    echo -e "Kontrollera att USB-sladdarna sitter i ordentligt och att kortet har ström."
  fi

  # Läsning direkt från portarna stjäl data från Car_Client, så hoppa över den
  # när Car_Client kör (då äger den portarna).
  GPS_PORT=""
  if [ "$CAR_CLIENT_RUNNING" -eq 1 ]; then
    echo -e "\n${YELLOW}Hoppar över GPS-läsningen: Car_Client kör och äger portarna.${NC}"
  else
    # Testa om vi kan detektera GPS-dataströmmen live på någon av portarna
    for port in "$PORT0" "$PORT1"; do
      if [ -c "$port" ]; then
        # Läs 5 rader under max 1.5 sekunder och sök efter NMEA-kod ($G)
        if timeout 1.5 head -n 5 "$port" 2>/dev/null | grep -q "\$G"; then
          GPS_PORT="$port"
          break
        fi
      fi
    done

    if [ -n "$GPS_PORT" ]; then
      echo -e "\n${GREEN}✅ DETEKTERADE GPS-DATASTRÖM LIVE PÅ PORT: $GPS_PORT!${NC}"
      echo -e "Här är råa, nytagna GPS-rader från ditt inbyggda u-blox chip:"
      echo -e "${BLUE}----------------------------------------------------------------------${NC}"
      # Visa rader som innehåller GPS GGA eller liknande
      timeout 1 head -n 3 "$GPS_PORT" | grep "\$G"
      echo -e "${BLUE}----------------------------------------------------------------------${NC}"
      echo -e "Detta bekräftar att u-blox-mottagaren fungerar utmärkt och skickar data!"
    else
      echo -e "\n${YELLOW}⚠️ Kunde inte automatiskt läsa någon GPS-dataström på USB-portarna.${NC}"
      echo -e "Detta beror oftast på att u-blox-enheten är tyst eller att behörigheterna spökar."
    fi
  fi
fi

echo -e "\n${GREEN}${BOLD}======================================================================${NC}"
echo -e "${GREEN}${BOLD}🎉 FLASHNINGEN ÄR KLAR! 🎉${NC}"
echo -e "${GREEN}${BOLD}======================================================================${NC}"
echo -e "Firmware (${BOLD}$FW_NAME${NC}) har laddats upp till Carcontroller-kortet."
echo -e "======================================================================"
