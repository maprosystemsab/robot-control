#!/usr/bin/env bash
# install_client.sh — installerar Robotstyrning (klienten) och mock-robotd på
# en Ubuntu-dator. Körs på datorn man styr roboten från, som vanlig användare
# (skriptet frågar själv efter sudo för systempaketen):
#
#   bash scripts/install_client.sh
#
# Vad den gör:
#   1. Installerar systempaket klienten behöver (fönster, dosa, videoavkodning)
#   2. Installerar Rust via rustup om det saknas
#   3. Bygger robotstyrning och mock-robotd (release)
#   4. Genvägar i ~/.local/bin: robotstyrning, Robotstyrning, mock-robotd
#      (länkar till det byggda programmet) och ~/.local/bin i PATH
#   5. Programmenyn: "Robotstyrning"
#
# Säkert att köra igen efter git pull; då byggs den nya versionen och
# genvägarna pekar redan rätt. Se dokument/installationsguide.md.

set -euo pipefail

echo "=== robot-control: installation på styrdatorn (Ubuntu) ==="

if [ ! -f "Cargo.toml" ] || [ ! -d "client" ]; then
  echo "Kör det här skriptet från roten av robot-control-projektet (där Cargo.toml ligger)." >&2
  exit 1
fi

echo "--- Installerar systempaket ---"
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libssl-dev \
  libx11-dev \
  libxkbcommon-dev \
  libxkbcommon-x11-0 \
  libwayland-dev \
  libgl1-mesa-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libudev-dev \
  gstreamer1.0-plugins-base \
  gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad \
  libgstreamer1.0-dev \
  libgstreamer-plugins-base1.0-dev \
  curl \
  git

echo "--- Kontrollerar Rust ---"
if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust hittades inte, installerar via rustup..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
else
  echo "Rust finns redan ($(cargo --version))."
fi

echo "--- Bygger klienten och mock-robotd (release) ---"
cargo build --release -p robotstyrning -p mock-robotd

echo "--- Genvägar i ~/.local/bin (robotstyrning / Robotstyrning) ---"
# Länkar, inte kopior (samma som RControlStation): efter en ny `cargo build
# --release` eller `git pull` + omkörning av det här skriptet gäller den nya
# versionen direkt, i alla terminaler.
REPO="$(pwd)"
mkdir -p "$HOME/.local/bin"
for namn in robotstyrning Robotstyrning; do
  ln -sfn "$REPO/target/release/robotstyrning" "$HOME/.local/bin/$namn"
done
ln -sfn "$REPO/target/release/mock-robotd" "$HOME/.local/bin/mock-robotd"

# ~/.local/bin måste finnas i PATH. Ubuntu lägger till den vid inloggning om
# mappen finns, men inte alltid i redan öppna terminaler — säkra med .bashrc.
if ! grep -q 'robot-control: ~/.local/bin' "$HOME/.bashrc" 2>/dev/null; then
  cat >> "$HOME/.bashrc" <<'RC'

# robot-control: ~/.local/bin i PATH (robotstyrning, mock-robotd)
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac
RC
  echo "Lade till ~/.local/bin i PATH via ~/.bashrc (gäller nya terminaler)."
fi

echo "--- Skapar skrivbordsgenväg ---"
DESKTOP_DIR="$HOME/.local/share/applications"
mkdir -p "$DESKTOP_DIR"
cat > "$DESKTOP_DIR/robotstyrning.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Robotstyrning
Comment=Styrprogram för roboten
Exec=$HOME/.local/bin/robotstyrning
Terminal=false
Categories=Utility;
EOF

cat <<'EOF'

=== Klart ===

Installerat:
  robotstyrning       (klienten — skriv robotstyrning eller Robotstyrning i valfri terminal)
  mock-robotd         (låtsas-roboten, för lokal test utan hårdvara)

Snabbtest utan någon hårdvara alls (två terminaler):
  Terminal 1: mock-robotd --bind 0.0.0.0:9000
  Terminal 2: robotstyrning
  (skriv "127.0.0.1" på anslutningsskärmen)

Över er faktiska WireGuard-tunnel: skriv robotens VPN-IP istället
(t.ex. "192.168.200.8") när robotd/mock-robotd kör där.

Se README.md i projektet för fullständiga instruktioner.
EOF
