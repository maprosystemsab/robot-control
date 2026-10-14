#!/usr/bin/env bash
# install_pi.sh — installerar robotd/robotctl på en Raspberry Pi (Pi OS).
#
# Körs PÅ Pi:n själv, från roten av det klonade/kopierade projektet. Två
# vanliga arbetsflöden:
#
#   A) Om projektet ligger i ett git-repo ni redan klonat på Pi:n:
#        cd robot-control && git pull
#        bash scripts/install_pi.sh
#
#   B) Utan git — kopiera dit projektet från er utvecklingsdator, t.ex.
#      med scripts/deploy_to_pi.sh, eller `scp -r`/`rsync` manuellt, och
#      kör sedan samma kommando som ovan.
#
# Skriptet bryr sig inte om HUR filerna kom dit — det enda kravet är att
# ni står i projektets rot (där Cargo.toml ligger) när ni kör det. Säkert
# att köra om flera gånger (t.ex. efter varje `git pull`) — bygger om,
# skriver inte över en redan existerande config.json.
# Vad den gör:
#   1. Installerar systempaket robotd behöver bygga mot (CAN, I2C/GPIO, TLS)
#   2. Installerar Rust via rustup om det saknas
#   3. Bygger robotd + robotctl (release-läge)
#   4. Lägger binärerna i /usr/local/bin
#   5. Skapar /etc/robotd/config.json med förvalda värden om den inte redan finns
#   6. Installerar (men aktiverar INTE) en systemd-tjänst för robotd — se
#      kommentar i slutet för varför den lämnas avstängd tills vidare
#
# OBS: Det här bygger den riktiga robotd, inte mock-robotd. Se
# PROJECT_SPEC.md §13 för aktuell status på hur långt CAN/VESC-kopplingen
# kommit — kör robotd manuellt (`robotd`) och titta i loggen innan ni
# aktiverar den som en tjänst som startar automatiskt.

set -euo pipefail

echo "=== robot-control: installation på Raspberry Pi ==="

if [ ! -f "Cargo.toml" ] || [ ! -d "robotd" ]; then
  echo "Kör det här skriptet från roten av robot-control-projektet (där Cargo.toml ligger)." >&2
  exit 1
fi

echo "--- Installerar systempaket ---"
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  libudev-dev \
  can-utils \
  i2c-tools \
  ffmpeg \
  v4l-utils \
  git \
  curl

echo "--- Aktiverar I2C (för OLED-displayen) om det inte redan är på ---"
if [ "${SKIPPA_I2C:-0}" = "1" ]; then
  echo "Hoppar över I2C (SKIPPA_I2C=1)."
elif command -v raspi-config >/dev/null 2>&1; then
  sudo raspi-config nonint do_i2c 0 || echo "Kunde inte aktivera I2C automatiskt — gör det manuellt via raspi-config > Interface Options > I2C."
else
  echo "raspi-config hittades inte — kontrollera manuellt att I2C är påslaget (/boot/config.txt: dtparam=i2c_arm=on)."
fi

echo "--- Kontrollerar Rust ---"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust hittades inte, installerar via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "Rust finns redan ($(cargo --version))."
fi

echo "--- Bygger robotd + robotctl (release, kan ta flera minuter på en Pi) ---"
cargo build --release -p robotd -p robotctl

echo "--- Installerar binärer till /usr/local/bin ---"
sudo install -m 755 target/release/robotd /usr/local/bin/robotd
sudo install -m 755 target/release/robotctl /usr/local/bin/robotctl

echo "--- Skapar konfiguration om den saknas ---"
sudo mkdir -p /etc/robotd
if [ -f /etc/robotd/config.json ] && [ "${SKRIV_OVER_CONFIG:-0}" = "1" ]; then
  BACKUP="/etc/robotd/config.json.bak_$(date +%Y%m%d_%H%M%S)"
  sudo cp /etc/robotd/config.json "$BACKUP"
  sudo rm /etc/robotd/config.json
  echo "Gammal config sparad som $BACKUP, skriver en ny."
fi
if [ ! -f /etc/robotd/config.json ]; then
  ROBOT_ID="${ROBOT_ID:-robot-1}"
  # Car_Client:s --setid (första byten i varje paket), VESC-ID:n på roboten
  # och Max vid fullt spakutslag (som i RControlStation).
  CAR_ID="${CAR_ID:-4}"
  VESC_IDS="${VESC_IDS:-28,36,76}"
  DRIVE_MAX="${DRIVE_MAX:-0.45}"
  # bind_addr: robotd lyssnar direkt över er WireGuard-VPN (se
  # PROJECT_SPEC.md §14) — ingen central reläserver. Sätt detta till
  # robotens fasta VPN-IP (t.ex. "192.168.200.8:9000", se ert
  # Wireguard_Guide.md för vilken IP just den här maskinen har), eller
  # "0.0.0.0:9000" + en brandväggsregel (ufw) som bara släpper in på wg0.
  BIND_ADDR="${BIND_ADDR:-0.0.0.0:9000}"
  # usb_device_path: enhetsfilen för CarController-kortet, används bara av
  # OLED-displayens "USB: OK/SAKNAS"-koll (se PROJECT_SPEC.md §11/§13 för
  # vilken av /dev/vehicle eller /dev/car som gäller hos er — kontrollera
  # med `ls -la /dev/vehicle /dev/car` på Pi:n om ni är osäkra).
  USB_DEVICE_PATH="${USB_DEVICE_PATH:-/dev/vehicle}"
  # Pi 5 saknar hårdvarukodaren h264_v4l2m2m (videon startar aldrig), så där
  # kodas videon i programvara (libx264). Pi 4 behåller hårdvarukodaren.
  if grep -q "Raspberry Pi 5" /proc/device-tree/model 2>/dev/null; then
    VIDEO_ENCODER="${VIDEO_ENCODER:-libx264}"
  else
    VIDEO_ENCODER="${VIDEO_ENCODER:-h264_v4l2m2m}"
  fi
  sudo tee /etc/robotd/config.json > /dev/null <<EOF
{
  "robot_id": "${ROBOT_ID}",
  "bind_addr": "${BIND_ADDR}",
  "vesc_profile": { "roller": {} },
  "watchdog_soft_ms": 150,
  "watchdog_hard_ms": 400,
  "lighting_gpio_pin": 17,
  "car_client_addr": "127.0.0.1:8300",
  "car_client_id": ${CAR_ID},
  "known_vesc_ids": [${VESC_IDS}],
  "usb_device_path": "${USB_DEVICE_PATH}",
  "drive": { "speed_max": ${DRIVE_MAX}, "steering_max": ${DRIVE_MAX} },
  "video": { "encoder": "${VIDEO_ENCODER}" }
}
EOF
  echo "Skapade /etc/robotd/config.json (robot_id=${ROBOT_ID}, bind_addr=${BIND_ADDR}, bil-ID=${CAR_ID}, VESC=${VESC_IDS}, max=${DRIVE_MAX}, video=${VIDEO_ENCODER})."
  echo "Om ni kör WireGuard: sätt bind_addr till robotens wg0-IP + port,"
  echo "t.ex. 192.168.200.8:9000, så robotd bara syns på VPN:et."
  echo "OBS: koden tolererar numera saknade/nya fält i config.json (fylls"
  echo "från standardvärden), så den här filen går bra att uppdatera för"
  echo "hand senare om fler fält tillkommer."
else
  echo "/etc/robotd/config.json finns redan, rör den inte."
fi

echo "--- Installerar (men aktiverar inte) systemd-tjänst ---"
sudo tee /etc/systemd/system/robotd.service > /dev/null <<'EOF'
[Unit]
Description=robotd - robotens styrtjänst
# Inte After=car_client.service: den tjänsten har After=multi-user.target, och
# robotd (WantedBy=multi-user.target) efter den ger en ordningscirkel där systemd
# stryker robotds start vid uppstart (hänt på RobAnt 2026-09-23). robotd behöver
# inte vänta: den ansluter till Car_Client först när en klient kopplar upp sig.
After=network-online.target
Wants=network-online.target

[Service]
# Ingen fast startfördröjning behövs: robotd väntar själv på WireGuard-adressen
# och ansluter till Car_Client först när en klient kopplar upp sig (och släpper
# den när klienten går, så RControlStation fungerar som vanligt däremellan).
# Uppdatera/slå på med scripts/skicka_robotd.sh från datorn.
ExecStart=/usr/local/bin/robotd
Restart=always
RestartSec=2
User=root
# robotd behöver GPIO/I2C/CAN-åtkomst, körs som root för enkelhets skull i
# det här skedet. Går att strama åt (t.ex. lägga användaren i grupperna
# gpio/i2c/dialout) när allt är verifierat att fungera.

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl daemon-reload

echo "--- Efterinstallationskontroll ---"
echo "(Samma sorts kontroller som OLED-displayen gör löpande, se PROJECT_SPEC.md §1/§16 —"
echo " men här får du svaret direkt i terminalen innan ni kopplar in allt.)"
echo ""

if [ -f /sys/class/net/wg0/operstate ] && [ "$(cat /sys/class/net/wg0/operstate)" = "up" ]; then
  echo "[OK]      WireGuard (wg0) är uppe."
else
  echo "[SAKNAS]  WireGuard (wg0) syns inte som uppe. Om ni inte redan kört"
  echo "          era egna wireguard.sh/wireguard_admin.sh-skript för den här"
  echo "          maskinen: gör det nu, annars kommer klienten inte att nå"
  echo "          robotd oavsett hur rätt allt annat är."
fi

if [ -e /dev/i2c-1 ]; then
  echo "[OK]      /dev/i2c-1 finns (OLED-displayen bör kunna initieras)."
else
  echo "[SAKNAS]  /dev/i2c-1 finns inte än. Om I2C precis aktiverades av det"
  echo "          här skriptet (raspi-config) kan en omstart krävas:"
  echo "          sudo reboot"
fi

USB_CHECK_PATH="${USB_DEVICE_PATH:-/dev/vehicle}"
if [ -e "${USB_CHECK_PATH}" ]; then
  echo "[OK]      ${USB_CHECK_PATH} finns (CarController-kortet ser ut att vara inkopplat)."
else
  echo "[SAKNAS]  ${USB_CHECK_PATH} finns inte. Normalt om CarController-kortet"
  echo "          inte är inkopplat än — annars: kolla \`ls -la /dev/vehicle"
  echo "          /dev/car\` för att se vilket namn som faktiskt dyker upp,"
  echo "          och justera usb_device_path i /etc/robotd/config.json."
fi

if systemctl is-active --quiet car_client.service 2>/dev/null; then
  echo "[OK]      car_client.service kör (startar Car_Client i en screen-session)."
elif command -v screen >/dev/null 2>&1 && screen -list 2>/dev/null | grep -q "\.car\b"; then
  echo "[OK]      En screen-session vid namn 'car' kör (Car_Client, men utan"
  echo "          car_client.service — kördes den manuellt istället?)."
else
  echo "[INFO]    Varken car_client.service eller en 'car'-screen-session"
  echo "          hittades. Normalt om ni inte kört er andra installation"
  echo "          (maprosystemsab/rise_sdvp: install_pi.sh/install_allt.sh) än."
fi

if systemctl is-active --quiet car_rtk.service 2>/dev/null; then
  echo "[OK]      car_rtk.service kör (Swepos RTK-korrektioner)."
else
  echo "[INFO]    car_rtk.service kör inte/hittades inte. Bra att veta: RTK-"
  echo "          precision kräver ett Swepos-konto — utan det får ni bara"
  echo "          vanlig GNSS-noggrannhet (~1-2m), inte cm-precision."
fi

echo ""

cat <<'EOF'

=== Klart ===

robotd och robotctl är installerade i /usr/local/bin.
En systemd-tjänst (robotd.service) är installerad men INTE aktiverad —
medvetet, se PROJECT_SPEC.md §13: CAN/VESC-kopplingen genom Car_Client är
inte helt verifierad än. Testa manuellt först:

    robotd

Titta i loggen. robotd lyssnar nu direkt på adressen i config.json
("bind_addr") — om ni redan har WireGuard uppe (wireguard.sh) räcker det
att klienten ansluter till robotens VPN-IP, ingen ytterligare parkoppling
behövs (se PROJECT_SPEC.md §14).

När ni är nöjda och vill att den ska starta automatiskt:

    sudo systemctl enable --now robotd

EOF
