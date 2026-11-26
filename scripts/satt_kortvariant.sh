#!/usr/bin/env bash
# satt_kortvariant.sh — skriver robotens kortvariant i robotd:s inställningar, så att
# styrkortets firmware går att uppdatera från SentinAnt och Robotstyrning (Inställningar →
# Robotens firmware). Körs PÅ roboten. Samma skript finns i SentinAnts skriptmapp.
#
#   sudo bash scripts/satt_kortvariant.sh mp101-robant-nodstopp-lagesknapp
#   sudo bash scripts/satt_kortvariant.sh --visa        visar vad som står nu
#   sudo bash scripts/satt_kortvariant.sh --ta-bort     roboten kan inte uppdateras från apparna
#
# Varianten = <gamla|mp101>-<robant|mactrac|drangen>[-nodstopp][-lagesknapp]:
#   gamla / mp101   det äldre styrkortet (flashas med ST-Link) eller MP101 (via CM5:ans ben)
#   profilen        maskinen firmwaren är byggd för
#   -nodstopp       MP101 med nödstoppet på styrkortets DI1 (firmware byggd med NODSTOPP=di1)
#   -lagesknapp     MP101 med lägesknappen på DI2 (LAGESKNAPP=di2)
# Den ska beskriva den firmware roboten HAR. Den gissas aldrig: fel variant kan ge en robot
# utan fungerande nödstopp. Skriptet jämför med det som går att se på roboten (CM5 eller
# inte, märkfilerna /etc/robotd/nodstopp-di1 och lagesknapp-di2, estop_board i SentinAnts
# inställningar) och vägrar om det säger emot. Inställningsfilen sparas först som .fore-variant.
# robotd läser varianten vid start: starta om den tjänst som kör när roboten står still
# och ingen är ansluten.
set -euo pipefail
FILER=(/etc/robotd/config.json /etc/sentinant/config.json)
VISA=0; VARIANT=""
case "${1:-}" in
  --visa) VISA=1 ;;
  --ta-bort) VARIANT="" ;;
  "") sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; exit 1 ;;
  *) VARIANT="$1"
     [[ "$VARIANT" =~ ^(gamla-(robant|mactrac|drangen)|mp101-(robant|mactrac|drangen)(-nodstopp)?(-lagesknapp)?)$ ]] || { echo "Ingen giltig variant: $VARIANT" >&2; exit 1; } ;;
esac
FINNS=()
for f in "${FILER[@]}"; do [ -f "$f" ] && FINNS+=("$f"); done
[ ${#FINNS[@]} -gt 0 ] || { echo "Hittar ingen inställningsfil (${FILER[*]})." >&2; exit 1; }
las() { python3 -c 'import json,sys; print((json.load(open(sys.argv[1])).get("firmware") or {}).get("variant") or "(ingen)")' "$1"; }
if [ "$VISA" = 1 ]; then
  for f in "${FINNS[@]}"; do echo "$f: $(las "$f")"; done
  exit 0
fi
if [ -n "$VARIANT" ]; then
  CM5=0; tr -d '\0' < /proc/device-tree/model 2>/dev/null | grep -q "Compute Module 5" && CM5=1
  fel() { echo "FEL: $*" >&2; echo "Varianten skrevs inte." >&2; exit 1; }
  case "$VARIANT" in
    mp101-*) [ "$CM5" = 1 ] || fel "varianten säger MP101, men den här datorn är ingen CM5." ;;
    gamla-*) [ "$CM5" = 0 ] || fel "varianten säger det äldre styrkortet, men den här datorn är en CM5 (MP101)." ;;
  esac
  if [ -f /etc/robotd/config.json ]; then
    for par in "nodstopp:/etc/robotd/nodstopp-di1" "lagesknapp:/etc/robotd/lagesknapp-di2"; do
      DEL="${par%%:*}"; MARKE="${par#*:}"
      if [ -e "$MARKE" ] && [[ "$VARIANT" != *-$DEL* ]]; then fel "$MARKE finns (firmwaren byggs med $DEL), men varianten saknar -$DEL."; fi
      if [ ! -e "$MARKE" ] && [[ "$VARIANT" == *-$DEL* ]]; then fel "varianten har -$DEL, men $MARKE finns inte (uppdatera_pi.sh bygger då utan $DEL)."; fi
    done
  fi
  if [ -f /etc/sentinant/config.json ] || { [ -f /etc/robotd/config.json ] && grep -q '"estop_board"' /etc/robotd/config.json; }; then
    for f in "${FINNS[@]}"; do
      BOARD="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("estop_board"))' "$f")"
      if [ "$BOARD" = True ] && [[ "$VARIANT" != *-nodstopp* ]]; then fel "$f har estop_board (nödstoppet sitter på styrkortet), men varianten saknar -nodstopp."; fi
    done
  fi
fi
for f in "${FINNS[@]}"; do
  cp -p "$f" "$f.fore-variant"
  python3 - "$f" "$VARIANT" <<'PY'
import json, os, sys
fil, variant = sys.argv[1], sys.argv[2] or None
cfg = json.load(open(fil))
fw = cfg.get("firmware") if isinstance(cfg.get("firmware"), dict) else {}
fw["variant"] = variant
cfg["firmware"] = fw
tmp = fil + ".ny"
with open(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, os.stat(fil).st_mode & 0o777), "w") as f:
    json.dump(cfg, f, indent=2, ensure_ascii=False)
    f.write("\n")
os.replace(tmp, fil)
PY
  echo "$f: kortvariant $(las "$f") (förra filen: $f.fore-variant)"
done
echo "Starta om robotd (den tjänst som kör) när roboten står still och ingen är ansluten, så gäller varianten."
