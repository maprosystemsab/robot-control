#!/usr/bin/env bash
# wg-vakt.sh — installerar en vakt som håller WireGuard-tunneln (wg0) vid liv när
# serverns publika adress byts (kontorets 5G-router får ny IP vid omstart; duckdns
# följer efter inom 5 min, men wg-quick slår bara upp namnet när tunneln startar).
#
#   sudo bash wireguard/wg-vakt.sh            installera (robot eller dator)
#   sudo bash wireguard/wg-vakt.sh --ta-bort  ta bort
#
# Vakten körs varje minut: har servern inte svarat på 150 s läses Endpoint om från
# /etc/wireguard/wg0.conf och sätts på nytt med `wg set`, som slår upp namnet igen.
# Inget startas om; videon och styrningen tar vid så fort handskakningen är tillbaka.
# Logg: journalctl -t wg-vakt
set -euo pipefail
[ "$(id -u)" = 0 ] || { echo "Kör med sudo"; exit 1; }

if [ "${1:-}" = "--ta-bort" ]; then
    systemctl disable --now wg-vakt.timer 2>/dev/null || true
    rm -f /etc/systemd/system/wg-vakt.timer /etc/systemd/system/wg-vakt.service /usr/local/sbin/wg-vakt
    systemctl daemon-reload
    echo "wg-vakt borttagen."
    exit 0
fi

cat > /usr/local/sbin/wg-vakt <<'EOF'
#!/usr/bin/env bash
# Slår upp serverns adress igen när wg0 har tappat kontakten (se wireguard/wg-vakt.sh).
IF=wg0
CONF=/etc/wireguard/$IF.conf
GRANS=150
wg show "$IF" >/dev/null 2>&1 || exit 0
nu=$(date +%s)
# Peer-block i konfigurationen: PublicKey + Endpoint.
awk '/^\[Peer\]/{p=1; k=""; e=""} p&&/^PublicKey/{sub(/^[^=]*= */,""); k=$0} p&&/^Endpoint/{sub(/^[^=]*= */,""); e=$0; if(k!="") print k, e}' "$CONF" |
while read -r nyckel endpoint; do
    senast=$(wg show "$IF" latest-handshakes | awk -v k="$nyckel" '$1==k{print $2}')
    [ -n "$senast" ] || continue
    if [ "$senast" = 0 ] || [ $((nu - senast)) -gt $GRANS ]; then
        if wg set "$IF" peer "$nyckel" endpoint "$endpoint" 2>/dev/null; then
            logger -t wg-vakt "ingen handskakning på $((senast ? nu - senast : -1)) s: $endpoint uppslagen igen ($(wg show "$IF" endpoints | awk -v k="$nyckel" '$1==k{print $2}'))"
        else
            logger -t wg-vakt "kunde inte slå upp $endpoint (inget nät?)"
        fi
    fi
done
EOF
chmod 755 /usr/local/sbin/wg-vakt

cat > /etc/systemd/system/wg-vakt.service <<'EOF'
[Unit]
Description=WireGuard-vakt: slå upp serverns adress igen om tunneln tappat kontakten
After=wg-quick@wg0.service
[Service]
Type=oneshot
ExecStart=/usr/local/sbin/wg-vakt
EOF
cat > /etc/systemd/system/wg-vakt.timer <<'EOF'
[Unit]
Description=WireGuard-vakt varje minut
[Timer]
OnBootSec=2min
OnUnitActiveSec=1min
[Install]
WantedBy=timers.target
EOF
systemctl daemon-reload
systemctl enable --now wg-vakt.timer >/dev/null 2>&1
echo "wg-vakt installerad (varje minut, logg: journalctl -t wg-vakt)."
