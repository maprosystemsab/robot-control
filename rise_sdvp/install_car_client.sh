#!/bin/bash

# ==============================================================================
# 🍓 install_car_client.sh: grundsystemet på robotens Raspberry Pi
# ==============================================================================
# Kopia av install_pi.sh från maprosystemsab/rise_sdvp (commit 1b55ca6), så att
# robot-control går att installera utan att klona rise_sdvp. Körs PÅ Pi:n:
#     cd robot-control/rise_sdvp && sudo CAR_ID=4 ./install_car_client.sh
# (eller via scripts/ny_robot.sh från datorn). CAR_ID = Car_Client --setid.
# NODSTOPP_GPIO=27: nödstoppsknapp (NC-brytare mellan GPIO27 och GND) är inkopplad.
# Car_Client spärrar då all körning när kretsen bryts. Utelämna om ingen knapp finns
# (en okopplad pinne läses som intryckt nödstopp).
# ==============================================================================
# Detta skript sätter upp alla paket, udev-regler, Swepos, bygger Car_Client
# och ställer in boot-autostart på din Raspberry Pi.
# Det har även avancerad felrapportering för saknade eller trasiga paket.
# ==============================================================================

GREEN='\e[32m'
RED='\e[31m'
YELLOW='\e[33m'
BLUE='\e[34m'
BOLD='\e[1m'
NC='\e[0m' # No Color

# Säkerställ att skriptet körs som root (sudo)
if [ "$EUID" -ne 0 ]; then
  echo -e "${RED}${BOLD}Fel:${NC} Detta skript måste köras med sudo! Kör: ${BOLD}sudo ./install_pi.sh${NC}"
  exit 1
fi

REAL_USER=${SUDO_USER:-$USER}
REAL_HOME=$(getent passwd "$REAL_USER" | cut -d: -f6)
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo -e "${BLUE}${BOLD}======================================================================${NC}"
echo -e "${BLUE}${BOLD}   🍓  KONFIGURATION AV ENBART RASPBERRY PI-SYSTEMET  🍓${NC}"
echo -e "${BLUE}${BOLD}======================================================================${NC}"
echo -e "Detta skript sätter upp alla programvaru-tjänster på din Raspberry Pi."
echo -e "Faktisk användare: ${BOLD}$REAL_USER${NC}"
echo -e "Installationsmapp: ${BOLD}$DIR${NC}\n"

# ------------------------------------------------------------------------------
# 📦 STEG 1: Paketinstallation med avancerad felrapportering
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}[Steg 1/5] Installerar Linux-paket...${NC}"
# Vänta upp till 5 min om apt är upptaget (t.ex. automatiska uppdateringar efter
# uppstart), och städa upp en tidigare avbruten installation (strömavbrott o.d.).
APT="apt-get -y -o DPkg::Lock::Timeout=300"
dpkg --configure -a
$APT update

# Listan på alla baspaket som behövs på Pi:n
PACKAGES=(
    build-essential
    openocd
    gcc-arm-none-eabi
    rtklib
    screen
    git
    udev
    usbutils
    wireguard
    wireguard-tools
)

# Intelligent detektering av Qt-version (Prioriterar Qt6 enligt Benjamins instruktioner)
if apt-cache show qt6-base-dev &>/dev/null; then
  echo -e "${GREEN}✅ Detekterade Qt6 i paketförråden! Använder Benjamins referenspaket.${NC}"
  PACKAGES+=(qt6-base-dev qt6-declarative-dev qt6-quick3d-dev qt6-serialport-dev)
else
  echo -e "${YELLOW}⚠️ Hittade inte Qt6. Faller tillbaka på Qt5-paket...${NC}"
  PACKAGES+=(qt5-qmake qtbase5-dev libqt5serialport5-dev)
fi

# Försök ladda ner och installera alla på en gång först
echo -e "Försöker installera alla paket på en gång..."
if $APT install "${PACKAGES[@]}" &>/dev/null; then
  echo -e "${GREEN}✅ Alla Linux-paket installerades framgångsrikt!${NC}\n"
else
  echo -e "${YELLOW}⚠️ Något paket gick inte att installera på en gång. Testar individuellt för att hitta felet...${NC}"
  FAILED_PKGS=()
  
  for pkg in "${PACKAGES[@]}"; do
    # Kontrollera om paketet redan är installerat
    if dpkg -s "$pkg" &>/dev/null; then
      continue
    fi
    
    # Försök installera paketet individuellt
    if APT_OUT=$($APT install "$pkg" 2>&1); then
      echo -e "  [${GREEN}OK${NC}] Installerad: $pkg"
    else
      echo -e "  [${RED}FEL${NC}] Kunde inte installera: $pkg"
      echo "$APT_OUT" | grep -E "^(E|Error|W):" | tail -3 | sed 's/^/        /'
      FAILED_PKGS+=("$pkg")
    fi
  done

  # Slutrapport för paketen på Pi:n
  if [ ${#FAILED_PKGS[@]} -eq 0 ]; then
    echo -e "${GREEN}✅ Alla tillgängliga paket installerades framgångsrikt!${NC}\n"
  else
    echo -e "\n${YELLOW}${BOLD}⚠️  INSTALLATIONS-SAMMANFATTNING AV PAKET:${NC}"
    echo -e "${RED}Följande paket kunde inte installeras på din Raspberry Pi:${NC}"
    for fpkg in "${FAILED_PKGS[@]}"; do
      echo -e "  - ${BOLD}$fpkg${NC}"
    done
    echo -e "\n${YELLOW}Tips: Du kan behöva kontrollera din internetanslutning eller köra 'sudo apt update'.${NC}"
    echo -e "Installationen fortsätter, men bygget av Car_Client kan påverkas om viktiga paket saknas.\n"
  fi
fi

# ------------------------------------------------------------------------------
# 🌐 STEG 2: Installera USB udev-regler
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}[Steg 2/5] Installerar USB-regler (udev)...${NC}"
UDEV_DIR="/etc/udev/rules.d"

if [ -f "$DIR/Linux/PI/udev/10-rise_sdvp.rules" ]; then
  cp "$DIR/Linux/PI/udev/10-rise_sdvp.rules" "$UDEV_DIR/"
  cp "$DIR/Linux/PI/udev/49-stlinkv2.rules" "$UDEV_DIR/"
  udevadm control --reload-rules && udevadm trigger
  echo -e "${GREEN}✅ USB-regler installerade! (/dev/car och /dev/ublox är aktiva)${NC}\n"
else
  echo -e "${YELLOW}⚠️ Hittade inte udev-mallarna. Skapar dem manuellt...${NC}"
  echo 'KERNEL=="ttyACM*", ATTRS{idVendor}=="1546", ATTRS{idProduct}=="01a8", SYMLINK+="ublox rtk"' > "$UDEV_DIR/10-rise_sdvp.rules"
  echo 'KERNEL=="ttyACM*", ATTRS{idVendor}=="1546", ATTRS{idProduct}=="01a9", SYMLINK+="ublox rtk"' >> "$UDEV_DIR/10-rise_sdvp.rules"
  echo 'KERNEL=="ttyACM*", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="5740", SYMLINK+="car vehicle"' >> "$UDEV_DIR/10-rise_sdvp.rules"
  echo 'SUBSYSTEMS=="usb", ATTRS{idVendor}=="0483", ATTRS{idProduct}=="3748", MODE:="0666", SYMLINK+="stlinkv2_%n"' > "$UDEV_DIR/49-stlinkv2.rules"
  udevadm control --reload-rules && udevadm trigger
  echo -e "${GREEN}✅ Manuella USB-regler skapade och laddade!${NC}\n"
fi

# MP101 (CM5): Car_Client skriver lägesknappens läge till /run/lageknapp (firmware LAGESKNAPP=di2).
# /run är bara skrivbar för root, så filen skapas vid varje start med Car_Client-användaren som ägare.
if tr -d '\0' < /proc/device-tree/model 2>/dev/null | grep -qi "Compute Module 5"; then
  echo "f /run/lageknapp 0664 $REAL_USER $REAL_USER -" > /etc/tmpfiles.d/lageknapp.conf
  systemd-tmpfiles --create /etc/tmpfiles.d/lageknapp.conf
  echo -e "${GREEN}✅ /run/lageknapp för lägesknappen (MP101).${NC}\n"
fi

# ------------------------------------------------------------------------------
# 📡 STEG 3: Konfigurera Swepos RTK-tjänst (str2str)
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}[Steg 3/5] Konfigurerar Swepos RTK-korrektioner...${NC}"

# Sök efter existerande Swepos-konfiguration för smarta standardval
DEFAULT_USER=""
DEFAULT_PASS=""
DEFAULT_LAT="60.063221"
DEFAULT_LON="18.078982"

if [ -f "/etc/systemd/system/car_rtk.service" ]; then
  EXISTING_EXEC=$(grep "ExecStart" /etc/systemd/system/car_rtk.service)
  # Extrahera användarnamn och lösenord
  if [[ $EXISTING_EXEC =~ ntrip://([^:]+):([^@]+)@ ]]; then
    DEFAULT_USER="${BASH_REMATCH[1]}"
    DEFAULT_PASS="${BASH_REMATCH[2]}"
  fi
  # Extrahera latitud och longitud
  if [[ $EXISTING_EXEC =~ -p[[:space:]]+([0-9.-]+)[[:space:]]+([0-9.-]+) ]]; then
    DEFAULT_LAT="${BASH_REMATCH[1]}"
    DEFAULT_LON="${BASH_REMATCH[2]}"
  fi
fi

if [ -n "$DEFAULT_USER" ]; then
  echo -e "${GREEN}Hittade existerande Swepos-inställningar! Tryck Enter för att behålla dem.${NC}"
  read -p "Mata in ditt Swepos-användarnamn [$DEFAULT_USER]: " SWEPOS_USER
  SWEPOS_USER=${SWEPOS_USER:-$DEFAULT_USER}

  read -s -p "Mata in ditt Swepos-lösenord [använd sparad]: " SWEPOS_PASS
  echo ""
  SWEPOS_PASS=${SWEPOS_PASS:-$DEFAULT_PASS}
else
  echo -e "Swepos behövs bara för RTK (cm-noggrann GPS till karta/autopilot i RControlStation)."
  echo -e "robotstyrning/robotd använder inte GPS. ${BOLD}Tryck Enter för att hoppa över RTK.${NC}"
  read -p "Mata in ditt Swepos-användarnamn (tomt = hoppa över): " SWEPOS_USER
  if [ -n "$SWEPOS_USER" ]; then
    read -s -p "Mata in ditt Swepos-lösenord: " SWEPOS_PASS
    echo ""
  fi
fi

if [ -z "$SWEPOS_USER" ]; then
  echo -e "${YELLOW}Hoppar över Swepos/RTK (car_rtk.service skapas inte). Kör skriptet igen för att lägga till det senare.${NC}\n"
else

read -p "Mata in din basstations Latitud [$DEFAULT_LAT]: " SWEPOS_LAT
SWEPOS_LAT=${SWEPOS_LAT:-$DEFAULT_LAT}

read -p "Mata in din basstations Longitud [$DEFAULT_LON]: " SWEPOS_LON
SWEPOS_LON=${SWEPOS_LON:-$DEFAULT_LON}

SERVICE_FILE="/etc/systemd/system/car_rtk.service"
cat <<EOF > "$SERVICE_FILE"
[Unit]
Description=RTCM Forwarder str2str för Swepos
After=network.target

[Service]
ExecStart=/usr/bin/str2str \\
  -in ntrip://$SWEPOS_USER:$SWEPOS_PASS@nrtk-swepos.lm.se:80/RTCM3_GNSS \\
  -out tcpsvr://:1234 \\
  -msg "1005,1074,1084,1094,1230" \\
  -p $SWEPOS_LAT $SWEPOS_LON 17 \\
  -n 1
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable car_rtk.service
systemctl restart car_rtk.service

echo -e "${GREEN}✅ Swepos RTK-tjänst konfigurerad och startad!${NC}\n"
fi

# ------------------------------------------------------------------------------
# 💻 STEG 4: Kompilera Car_Client
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}[Steg 4/5] Kompilerar Car_Client på din Raspberry Pi...${NC}"

# Sök dynamiskt efter Car_Client-mappen
CLIENT_DIR=""
if [ -d "$DIR/Linux/Car_Client" ]; then
  CLIENT_DIR="$DIR/Linux/Car_Client"
elif [ -d "$DIR/rise_sdvp/Linux/Car_Client" ]; then
  CLIENT_DIR="$DIR/rise_sdvp/Linux/Car_Client"
fi

if [ -n "$CLIENT_DIR" ] && [ -d "$CLIENT_DIR" ]; then
  # Bestäm qmake-kommando baserat på installerad Qt-version
  QMAKE_CMD="qmake"
  if command -v qmake6 &>/dev/null; then
    QMAKE_CMD="qmake6"
  fi
  
  echo -e "Bygger projektet med kommando: ${BOLD}$QMAKE_CMD${NC}"
  sudo -u "$REAL_USER" bash -c "cd '$CLIENT_DIR' && $QMAKE_CMD && make clean && make -j\$(nproc)"
  if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Car_Client kompilerad utan fel!${NC}\n"
  else
    echo -e "${RED}❌ Kompileringsfel i Car_Client. Kontrollera felmeddelandena ovan.${NC}"
    exit 1
  fi
else
  echo -e "${RED}❌ Hittade inte Car_Client-mappen på $CLIENT_DIR!${NC}"
  exit 1
fi

# ------------------------------------------------------------------------------
# 🔄 STEG 5: Konfigurera Autostart av Car_Client vid boot
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}[Steg 5/5] Konfigurerar automatisk uppstart vid boot...${NC}"

START_SCRIPT="$REAL_HOME/start_car.sh"
# Fördröjningen ligger INUTI screen så att skriptet returnerar direkt. En blockerande
# "sleep 90" före screen fick systemd (Type=forking, standardtimeout 90 s) att ge upp
# och markera car_client.service som misslyckad vid uppstart.
# Car_Client körs i en slinga: avslutas eller kraschar den startas den om efter 3 s, så
# att roboten aldrig blir stående utan den (hände på RobAnt 3 2026-10-01 när "Shut down
# Raspberry Pi" i RControlStation misslyckades och Car_Client ändå avslutades).
cat <<EOF > "$START_SCRIPT"
#!/bin/bash
# Startar Car_Client i en bakgrunds-screen (namn "car") med en initial fördröjning inuti
# screen (icke-blockerande för systemd). Avslutas Car_Client startas den om efter 3 s.
screen -S car -d -m bash -c "sleep 15; cd '$CLIENT_DIR'; while true; do ./Car_Client -p /dev/vehicle --useudp --logusb --usetcp --tcprtcmserver 8200 --tcpubxserver 8210 --setid ${CAR_ID:-4}${NODSTOPP_GPIO:+ --nodstopp-gpio $NODSTOPP_GPIO}; echo \"\\\$(date +%T) Car_Client avslutades (kod \\\$?), startar om om 3 s\" | tee -a '$REAL_HOME/car_client_omstarter.log'; sleep 3; done"
echo "Car_Client startades i en screen-session med namnet 'car'."
echo "För att ansluta live, kör: screen -r car"
EOF

# "Reboot/Shut down Raspberry Pi" i RControlStation kör "sudo reboot" / "sudo shutdown"
# från Car_Client: tillåt just de två utan lösenord, annars misslyckas de.
SUDOERS_FIL="/etc/sudoers.d/${REAL_USER}-avstangning"
echo "$REAL_USER ALL=(root) NOPASSWD: /usr/sbin/reboot, /usr/sbin/shutdown, /sbin/reboot, /sbin/shutdown" > /tmp/avstangning.sudoers
if visudo -c -f /tmp/avstangning.sudoers >/dev/null; then
  install -m 440 /tmp/avstangning.sudoers "$SUDOERS_FIL"
fi
rm -f /tmp/avstangning.sudoers

chown "$REAL_USER:$REAL_USER" "$START_SCRIPT"
chmod +x "$START_SCRIPT"

CLIENT_SERVICE="/etc/systemd/system/car_client.service"
cat <<EOF > "$CLIENT_SERVICE"
[Unit]
Description=Car_Client Autostart i Screen
After=network.target multi-user.target car_rtk.service

[Service]
Type=forking
User=$REAL_USER
ExecStart=$START_SCRIPT
Restart=on-failure
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable car_client.service
systemctl start car_client.service

echo -e "${GREEN}✅ Raspberry Pi 4 konfigurerad framgångsrikt! Autostart är aktiverad!${NC}"
echo -e "Koppla nu in dina två USB-kablar och njut av robotdriften."
echo -e "Live-konsolen för Car_Client finns tillgänglig via: ${BOLD}screen -r car${NC}\n"
echo -e "${YELLOW}${BOLD}⚡ Viktigt om styrkortets ström:${NC}"
echo -e "  Styrkortet MÅSTE matas via strömplinten (GND / 7–60 V), t.ex. från 48 V-batteriet."
echo -e "  Enbart USB/ST-Link räcker för processorn men INTE för CAN-kretsen – då når inga"
echo -e "  kommandon motorstyrningarna (VESC). Ingen bygel på stiften EN_BAT_VIN (stänger av kortet)."
echo -e "  RControlStation ska visa ungefär batterispänningen, inte ~2–3 V.\n"
