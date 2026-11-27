#!/usr/bin/env bash
# jamfor_robotuppdatering.sh — den gemensamma craten robotuppdatering/ ska vara ordagrant
# lika i autorobot (SentinAnt, originalet) och robot-control (kopian), liksom klientens
# firmware.rs, scripts/satt_kortvariant.sh och scripts/uppdatering_manifest.py (verktyget
# som skriver och signerar uppdateringslistan; publicera_lite.sh använder kopian här).
# Skriptet säger till om något skiljer.
#
#   bash scripts/jamfor_robotuppdatering.sh [AUTOROBOT] [ROBOT-CONTROL]
#   bash scripts/jamfor_robotuppdatering.sh --kopiera    kopierar originalet till robot-control
#
# Standardmappar: ~/SentinAnt/program/autorobot och ~/Robotstyrning/program/robot-control.
set -euo pipefail
KOPIERA=0; [ "${1:-}" = "--kopiera" ] && { KOPIERA=1; shift; }
A="${1:-$HOME/SentinAnt/program/autorobot}"; R="${2:-$HOME/Robotstyrning/program/robot-control}"
[ -d "$A/robotuppdatering" ] || { echo "Hittar inte $A/robotuppdatering" >&2; exit 1; }
[ -d "$R/robotd" ] || { echo "Hittar inte robot-control i $R" >&2; exit 1; }
if [ "$KOPIERA" = 1 ]; then
  rsync -a --delete "$A/robotuppdatering/" "$R/robotuppdatering/"
  cp "$A/client/src/firmware.rs" "$R/client/src/firmware.rs"
  cp "$A/scripts/uppdatering_manifest.py" "$R/scripts/uppdatering_manifest.py"
  echo "Kopierat från $A till $R."
fi
FEL=0
diff -rq "$A/robotuppdatering" "$R/robotuppdatering" || FEL=1
diff -q "$A/client/src/firmware.rs" "$R/client/src/firmware.rs" || FEL=1
diff -q "$A/scripts/uppdatering_manifest.py" "$R/scripts/uppdatering_manifest.py" || FEL=1
# satt_kortvariant.sh skiljer bara på raden som säger var den andra kopian finns.
diff <(grep -v "Samma skript finns" "$A/scripts/satt_kortvariant.sh") <(grep -v "Samma skript finns" "$R/scripts/satt_kortvariant.sh") >/dev/null || { echo "scripts/satt_kortvariant.sh skiljer"; FEL=1; }
if [ "$FEL" = 0 ]; then echo "Kopiorna är lika."; else echo "KOPIORNA SKILJER SIG: rätta i autorobot (originalet) och kör med --kopiera." >&2; exit 1; fi
