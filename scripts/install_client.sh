#!/usr/bin/env bash
# install_client.sh — installerar SentinAnt Lite (klienten, hette Robotstyrning till
# 2026-11-27) och mock-robotd på en Ubuntu-dator. Körs på datorn man styr roboten
# från, som vanlig användare (skriptet frågar själv efter sudo för systempaketen):
#
#   bash scripts/install_client.sh
#
# Vad den gör:
#   1. Installerar systempaket klienten behöver (fönster, dosa, videoavkodning)
#   2. Installerar Rust via rustup om det saknas
#   3. Bygger klienten (paketet robotstyrning) och mock-robotd (release)
#   4. Kopierar det färdiga programmet till ~/.local/share/sentinant-lite/sentinant-lite.
#      Där äger användaren filen, så programmets egen uppdatering (knappen längst ned
#      på startskärmen) kan byta den utan lösenord. Efter den här installationen
#      behövs alltså inget terminalkommando för att uppdatera programmet.
#   5. Kommandon i ~/.local/bin: sentinant-lite, och de gamla namnen robotstyrning och
#      Robotstyrning (alla tre startar samma program), mock-robotd, och ~/.local/bin i PATH
#   6. Programmenyn: "SentinAnt Lite" med ikon. Den gamla genvägen "Robotstyrning" tas
#      bort (och byts i Favoriter och på skrivbordet, om den låg där).
#
# Inställningar och sparade robotar (~/.config/robotstyrning, ~/.config/robot-control)
# rörs inte. Säkert att köra igen efter git pull; då byggs den nya versionen. Har
# programmet under tiden uppdaterat sig självt till något nyare får det ligga kvar.
# Se dokument/installationsguide.md.

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
# Prov-miljövariabeln för byggtiden får aldrig följa med in i en installation: versionen
# ska vara commitens tid, samma som den publicerade versionen av samma commit har.
unset SENTINANT_LITE_BYGGE
cargo build --release -p robotstyrning -p mock-robotd

REPO="$(pwd)"
BYGGT="$REPO/target/release/robotstyrning"
# Versionen ska vara commitens tid, samma som scripts/sentinant_lite_version.sh räknar fram.
# Byggskriptet (client/build.rs) räknar bara om den när git-mappen det senast såg ändras, så
# den kan vara inaktuell om robot-control-mappen har flyttats eller kopierats: räkna då om.
VANTAD="$(bash scripts/sentinant_lite_version.sh)"
if [ "$("$BYGGT" --version)" != "$VANTAD" ]; then
  echo "Programmet uppger version $("$BYGGT" --version), väntat $VANTAD: bygger om."
  touch client/build.rs
  cargo build --release -p robotstyrning -p mock-robotd
fi
if [ "$("$BYGGT" --version)" != "$VANTAD" ]; then
  echo "FEL: det byggda programmet uppger version $("$BYGGT" --version), väntat $VANTAD. Ingenting har installerats." >&2
  exit 1
fi
# Ett provbygge (godkänner provnyckeln, som vem som helst kan signera med) installeras aldrig.
if grep -a -q "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL" "$BYGGT"; then
  echo "FEL: $BYGGT är ett PROVBYGGE (byggt med provnyckeln) och installeras inte." >&2
  echo "Bygg om utan --features provnyckel (cargo build --release -p robotstyrning -p mock-robotd)." >&2
  exit 1
fi

echo "--- Installerar programmet i ~/.local/share/sentinant-lite ---"
# En kopia i en mapp som användaren äger, inte en länk in i target/: ett program som
# ligger i cargos byggmapp räknas som utvecklingsversion och får ingen uppdateringsknapp.
# Härifrån byter programmet ut sig självt när man trycker "Uppdatera nu".
MAPP="$HOME/.local/share/sentinant-lite"
PROGRAM="$MAPP/sentinant-lite"
mkdir -p "$MAPP"
NY="$("$BYGGT" --version)"
HAR=""
if [ -x "$PROGRAM" ]; then HAR="$("$PROGRAM" --version 2>/dev/null || true)"; fi
if [ -n "$HAR" ] && [ "$HAR" != "$NY" ] && [ "$(printf '%s\n%s\n' "$HAR" "$NY" | sort -V | tail -n 1)" = "$HAR" ]; then
  # Aldrig bakåt: programmet har uppdaterat sig självt (knappen) till något nyare än källkoden här.
  echo "Det installerade programmet ($HAR) är nyare än det som byggdes ($NY) och får ligga kvar."
else
  # Skrivs bredvid och byts i ett steg, så att ett program som står öppet inte störs.
  install -m 755 "$BYGGT" "$PROGRAM.installeras"
  strip "$PROGRAM.installeras" 2>/dev/null || true
  mv -f "$PROGRAM.installeras" "$PROGRAM"
  echo "SentinAnt Lite $NY${HAR:+ (förut $HAR)}"
fi
install -m 644 client/assets/ikon.png "$MAPP/sentinant-lite.png"

echo "--- Kommandon i ~/.local/bin (sentinant-lite, robotstyrning, Robotstyrning) ---"
# De gamla namnen finns kvar som kommandon, så att den som är van att skriva
# Robotstyrning i en terminal får samma program.
mkdir -p "$HOME/.local/bin"
for namn in sentinant-lite robotstyrning Robotstyrning; do
  ln -sfn "$PROGRAM" "$HOME/.local/bin/$namn"
done
ln -sfn "$REPO/target/release/mock-robotd" "$HOME/.local/bin/mock-robotd"

# ~/.local/bin måste finnas i PATH. Ubuntu lägger till den vid inloggning om
# mappen finns, men inte alltid i redan öppna terminaler — säkra med .bashrc.
if ! grep -q 'robot-control: ~/.local/bin' "$HOME/.bashrc" 2>/dev/null; then
  cat >> "$HOME/.bashrc" <<'RC'

# robot-control: ~/.local/bin i PATH (sentinant-lite, mock-robotd)
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac
RC
  echo "Lade till ~/.local/bin i PATH via ~/.bashrc (gäller nya terminaler)."
fi

echo "--- Genväg i programmenyn: SentinAnt Lite ---"
DESKTOP_DIR="$HOME/.local/share/applications"
GENVAG="$DESKTOP_DIR/sentinant-lite.desktop"
mkdir -p "$DESKTOP_DIR"
cat > "$GENVAG" <<EOF
[Desktop Entry]
Type=Application
Name=SentinAnt Lite
Comment=Styrprogram för roboten
Exec="$PROGRAM"
Icon=$MAPP/sentinant-lite.png
StartupWMClass=sentinant-lite
Terminal=false
Categories=Utility;
EOF

# Den gamla genvägen (Robotstyrning) ersätts, så att det inte ligger två i menyn.
if [ -e "$DESKTOP_DIR/robotstyrning.desktop" ]; then
  rm -f "$DESKTOP_DIR/robotstyrning.desktop"
  echo "Tog bort den gamla genvägen Robotstyrning ur programmenyn."
fi
# Låg den gamla genvägen i Favoriter (raden till vänster på skärmen) byts den där också.
if command -v gsettings >/dev/null 2>&1; then
  FAV="$(gsettings get org.gnome.shell favorite-apps 2>/dev/null || true)"
  case "$FAV" in
    *sentinant-lite.desktop*) ;;
    *robotstyrning.desktop*)
      if gsettings set org.gnome.shell favorite-apps "${FAV//robotstyrning.desktop/sentinant-lite.desktop}" 2>/dev/null; then
        echo "Bytte Robotstyrning mot SentinAnt Lite i Favoriter."
      fi ;;
  esac
fi
# Detsamma på skrivbordet, för den som lagt en genväg där.
SKRIVBORD="$(xdg-user-dir DESKTOP 2>/dev/null || true)"
if [ -n "$SKRIVBORD" ] && [ "$SKRIVBORD" != "$HOME" ] && [ -e "$SKRIVBORD/robotstyrning.desktop" ]; then
  install -m 755 "$GENVAG" "$SKRIVBORD/sentinant-lite.desktop"
  gio set "$SKRIVBORD/sentinant-lite.desktop" metadata::trusted true 2>/dev/null || true
  rm -f "$SKRIVBORD/robotstyrning.desktop"
  echo "Bytte genvägen Robotstyrning på skrivbordet mot SentinAnt Lite."
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR" 2>/dev/null || true
fi

cat <<'EOF'

=== Klart ===

Installerat:
  SentinAnt Lite      (klienten, hette förut Robotstyrning. Finns i programmenyn; i en
                       terminal: sentinant-lite, eller som förut Robotstyrning)
  mock-robotd         (låtsas-roboten, för lokal test utan hårdvara)

Nya versioner av SentinAnt Lite installeras från programmet: knappen längst ned på
startskärmen blir grön ("Uppdatera nu") när det finns en uppdatering. VPN:et
(WireGuard) ska vara igång.

Snabbtest utan någon hårdvara alls (två terminaler):
  Terminal 1: mock-robotd --bind 0.0.0.0:9000
  Terminal 2: sentinant-lite
  (skriv "127.0.0.1" på anslutningsskärmen)

Över er faktiska WireGuard-tunnel: skriv robotens VPN-IP istället
(t.ex. "192.168.200.8") när robotd/mock-robotd kör där.

Se README.md i projektet för fullständiga instruktioner.
EOF
