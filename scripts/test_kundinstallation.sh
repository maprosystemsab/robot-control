#!/bin/bash
# test_kundinstallation.sh — körs på DATORN. Provinstallerar som en ny kund i en
# tom Ubuntu-behållare (Podman): git clone från GitHub, install_client.sh och
# install_dator.sh, som dokument/installationsguide_kund.md steg 2, 4 och 8. Datorn
# påverkas inte; behållaren tas bort efteråt.
#
#   sudo apt install -y podman        # en gång
#   bash scripts/test_kundinstallation.sh            # Ubuntu 22.04
#   bash scripts/test_kundinstallation.sh 24.04      # annan version
#
# Tar 20–40 minuter. Testar det som finns på GitHub, så pusha först.
# Testar inte fönster och dosa (behållaren har ingen skärm).
set -euo pipefail
VER="${1:-22.04}"
NAMN="kundtest-${VER//./}"
LOGG="kundtest-${VER}.log"
SKRIPT="$(mktemp)"
trap 'rm -f "$SKRIPT"; podman rm -f "$NAMN" >/dev/null 2>&1 || true' EXIT
sed -n '/^# ---- körs i behållaren ----$/,$p' "$0" | tail -n +2 > "$SKRIPT"
echo "Provinstallerar på Ubuntu $VER, logg: $LOGG"
podman run --name "$NAMN" --network host -v "$SKRIPT:/kundtest.sh:ro,Z" \
  "docker.io/library/ubuntu:$VER" bash /kundtest.sh > "$LOGG" 2>&1 || true
grep -E "########|STEG|Ubuntu:| is /|not found|Kunde inte|KLART" "$LOGG"
if grep -q "STEG2 OK" "$LOGG" && grep -q "STEG4 OK" "$LOGG" && grep -q "STEG8 OK" "$LOGG"; then
  echo "=== ALLT OK på Ubuntu $VER ==="
else
  echo "=== NÅGOT GICK FEL — se $LOGG ==="; exit 1
fi
exit 0
# ---- körs i behållaren ----
# Kör i en tom Ubuntu-behållare (som root). Gör som en ny kunddator:
# en vanlig användare med sudo följer dokument/installationsguide_kund.md ord för ord.
set -u
export DEBIAN_FRONTEND=noninteractive TZ=Europe/Stockholm

steg() { echo; echo "######## $* ########"; }

steg "0. Förbered: sådant som redan finns på en vanlig Ubuntu-skrivbordsdator"
apt-get update -qq
apt-get install -y -qq sudo lsb-release ca-certificates tzdata >/dev/null
useradd -m -s /bin/bash kund
echo "kund ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/kund
echo "Ubuntu: $(lsb_release -ds)"

som_kund() { su - kund -c "export DEBIAN_FRONTEND=noninteractive TZ=Europe/Stockholm; $*"; }

steg "Steg 2: sudo apt install -y git + git clone robot-control"
som_kund 'sudo apt install -y git 2>&1 | tail -3 && git clone -q https://github.com/maprosystemsab/robot-control.git ~/robot-control && ls ~/robot-control | tr "\n" " "' \
  && echo "STEG2 OK" || echo "STEG2 FEL"

steg "Steg 4: bash scripts/install_client.sh"
som_kund 'cd ~/robot-control && bash scripts/install_client.sh' > /tmp/install_client.log 2>&1
rc=$?; tail -25 /tmp/install_client.log
[ $rc -eq 0 ] && echo "STEG4 OK" || echo "STEG4 FEL (exit $rc)"

steg "Steg 8: git clone rise_sdvp + sudo bash install_dator.sh (svar: y)"
som_kund 'git clone -q https://github.com/maprosystemsab/rise_sdvp.git ~/rise_sdvp && cd ~/rise_sdvp && echo y | sudo bash install_dator.sh' > /tmp/install_dator.log 2>&1
rc=$?; tail -25 /tmp/install_dator.log
[ $rc -eq 0 ] && echo "STEG8 OK" || echo "STEG8 FEL (exit $rc)"

steg "Kontroll: genvägarna i en ny inloggning"
som_kund 'bash -lc "type Robotstyrning robotstyrning RControlStation; ls -la \$(readlink -f \$(command -v Robotstyrning)) \$(readlink -f \$(command -v RControlStation))"'

steg "Kontroll: paket som inte gick att installera (install_dator.sh)"
grep -E "Kunde inte installera|FEL\]" /tmp/install_dator.log || echo "inga"
echo "KLART"
