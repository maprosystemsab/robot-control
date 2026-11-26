#!/usr/bin/env bash
# uppdatera_robotd.sh — bygger och installerar robotd på Pi:n och (om)startar
# robotd.service. Körs PÅ Pi:n från projektets rot, normalt via
# scripts/skicka_robotd.sh från datorn.
#
#   bash scripts/uppdatera_robotd.sh             bygg, installera, starta om tjänsten om den är på
#   bash scripts/uppdatera_robotd.sh --aktivera  dessutom: starta robotd automatiskt vid uppstart
#   bash scripts/uppdatera_robotd.sh --stang-av  stoppa tjänsten och stäng av autostart
#
# Rör inte Car_Client, I2C eller något annat. robotd tar Car_Client bara medan
# någon är ansluten med robotstyrning; annars fungerar RControlStation som vanligt.

set -euo pipefail

MODE="uppdatera"
case "${1:-}" in
  --aktivera) MODE="aktivera" ;;
  --stang-av) MODE="stang-av" ;;
  "") ;;
  *) echo "Okänt val: $1 (använd --aktivera eller --stang-av)" >&2; exit 1 ;;
esac

if [ "$MODE" = "stang-av" ]; then
  sudo systemctl disable --now robotd.service
  echo "robotd.service stoppad och startar inte längre automatiskt."
  exit 0
fi

if [ ! -f Cargo.toml ] || [ ! -d robotd ]; then
  echo "Kör från robot-control-projektets rot." >&2
  exit 1
fi

# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || true

echo "--- Bygger robotd ---"
cargo build --release -p robotd 2>&1 | tail -2

# Ett provbygge (godkänner provsignerade uppdateringslistor, se robotuppdatering/Cargo.toml)
# får aldrig installeras på en robot. Filen startas inte: bara märket i den letas upp.
if grep -a -q "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL" target/release/robotd; then
  echo "FEL: target/release/robotd är ett PROVBYGGE (byggt med provnyckeln) och installeras inte." >&2
  exit 1
fi
echo "--- Installerar /usr/local/bin/robotd ---"
sudo install -m 755 target/release/robotd /usr/local/bin/robotd

echo "--- Skriver robotd.service ---"
sudo tee /etc/systemd/system/robotd.service > /dev/null <<'EOF'
[Unit]
Description=robotd - robotens styrtjänst (robotstyrning)
# Inte After=car_client.service: den tjänsten har After=multi-user.target, och
# robotd (WantedBy=multi-user.target) efter den ger en ordningscirkel där systemd
# stryker robotds start vid uppstart (hänt på RobAnt 2026-09-23). robotd behöver
# inte vänta: den ansluter till Car_Client först när en klient kopplar upp sig.
After=network-online.target
Wants=network-online.target

[Service]
# Ingen fast startfördröjning behövs: robotd väntar själv på WireGuard-adressen
# och ansluter till Car_Client först när en klient kopplar upp sig.
ExecStart=/usr/local/bin/robotd
Restart=always
RestartSec=2
User=root

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl daemon-reload

# Manuellt startade testinstanser skulle ta port 9000 före tjänsten.
if pgrep -x robotd >/dev/null && ! systemctl is-active --quiet robotd.service; then
  echo "--- Stänger manuellt startade robotd (testkörningar) ---"
  sudo pkill -INT -x robotd || true
  sleep 1
fi

if [ "$MODE" = "aktivera" ]; then
  sudo systemctl enable robotd.service
fi

if systemctl is-enabled --quiet robotd.service; then
  sudo systemctl restart robotd.service
  sleep 2
  systemctl --no-pager --lines=8 status robotd.service || true
  echo
  echo "robotd kör som tjänst och startar automatiskt vid uppstart."
  echo "Logg:        journalctl -u robotd -f"
  echo "Stäng av:    bash scripts/uppdatera_robotd.sh --stang-av"
else
  echo "robotd är installerad men startar inte automatiskt (kör med --aktivera för det)."
fi
