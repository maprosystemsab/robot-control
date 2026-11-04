#!/bin/bash

# ==============================================================================
# 🛡️ wireguard.sh: Installera och förbered WireGuard VPN för roboten och PC:n
# ==============================================================================
# Detta skript installerar WireGuard-paketen på Linux-systemet (Pi eller PC)
# och sätter upp en helt färdig och fungerande wg0-anslutning med rätt nycklar,
# IP-adresser och automatiska inställningar baserade på maskinens roll.
# ==============================================================================

GREEN='\e[32m'
RED='\e[31m'
YELLOW='\e[33m'
BLUE='\e[34m'
BOLD='\e[1m'
NC='\e[0m' # No Color

# Säkerställ att skriptet körs som root (sudo) för att kunna installera paket och konfigurera nätverk
if [ "$EUID" -ne 0 ]; then
  echo -e "${RED}${BOLD}Fel:${NC} Detta skript måste köras med sudo! Kör: ${BOLD}sudo ./wireguard.sh${NC}"
  exit 1
fi

echo -e "${BLUE}${BOLD}======================================================================${NC}"
echo -e "${BLUE}${BOLD}   🛡️  INSTALLATION & AUTOMATISK KONFIGURATION AV WIREGUARD VPN  🛡️${NC}"
echo -e "${BLUE}${BOLD}======================================================================${NC}"
echo -e "Detta skript installerar WireGuard och nätverksverktyg för säker fjärrstyrning,\n"
echo -e "samt skapar en färdig konfigurationsfil för tunneln direkt!\n"

# ------------------------------------------------------------------------------
# 📦 Installera paket
# ------------------------------------------------------------------------------
echo -e "${YELLOW}${BOLD}Installerar WireGuard och nätverksverktyg...${NC}"
# Vänta upp till 5 min om apt är upptaget, och städa upp en avbruten installation.
APT="apt-get -y -o DPkg::Lock::Timeout=300"
dpkg --configure -a
$APT update

# Inte resolvconf: vår wg0.conf har ingen DNS-rad, och på Raspberry Pi OS Trixie
# tar resolvconf över /etc/resolv.conf och tömmer den (ingen DNS alls).
PACKAGES=(
    wireguard
    wireguard-tools
)

FAILED_PKGS=()
for pkg in "${PACKAGES[@]}"; do
  if dpkg -s "$pkg" &>/dev/null; then
    echo -e "  [${GREEN}Redan installerad${NC}] $pkg"
    continue
  fi
  
  if APT_OUT=$($APT install "$pkg" 2>&1); then
    echo -e "  [${GREEN}OK${NC}] Installerad: $pkg"
  else
    echo -e "  [${RED}FEL${NC}] Kunde inte installera: $pkg"
    echo "$APT_OUT" | grep -E "^(E|Error|W):" | tail -3 | sed 's/^/        /'
    FAILED_PKGS+=("$pkg")
  fi
done

if [ ${#FAILED_PKGS[@]} -eq 0 ]; then
  echo -e "\n${GREEN}✅ Alla WireGuard-paket installerades framgångsrikt!${NC}"
else
  echo -e "\n${YELLOW}⚠️ Vissa paket kunde inte installeras direkt via apt: ${FAILED_PKGS[*]}${NC}"
  echo -e "Försök installera dem manuellt om VPN inte går igång."
fi

# ------------------------------------------------------------------------------
# 🔑 Generera publika och privata nycklar
# ------------------------------------------------------------------------------
echo -e "\n${YELLOW}${BOLD}Förbereder WireGuard-nycklar...${NC}"
WG_DIR="/etc/wireguard"
mkdir -p "$WG_DIR"
chmod 700 "$WG_DIR"

if [ ! -f "$WG_DIR/private.key" ]; then
  echo -e "Genererar ett nytt par med kryptonycklar för denna maskin..."
  wg genkey | tee "$WG_DIR/private.key" | wg pubkey > "$WG_DIR/public.key"
  chmod 600 "$WG_DIR/private.key" "$WG_DIR/public.key"
  echo -e "${GREEN}✅ Nya nycklar har genererats i $WG_DIR/${NC}"
else
  echo -e "🔑 Befintliga nycklar hittades i $WG_DIR/."
fi

PRIV_KEY=$(cat "$WG_DIR/private.key" 2>/dev/null)
PUB_KEY=$(cat "$WG_DIR/public.key" 2>/dev/null)

echo -e "\n🔑 ${BOLD}Denna maskins publika nyckel (Public Key):${NC}"
echo -e "  ${BLUE}${BOLD}$PUB_KEY${NC}"

# ------------------------------------------------------------------------------
# 🛠️ Automatisk konfigurering av wg0.conf
# ------------------------------------------------------------------------------
echo -e "\n${YELLOW}${BOLD}Konfigurerar VPN-tunneln (wg0.conf)...${NC}"
echo -e "Detta skript konfigurerar denna maskin som en VPN-klient."

read -p "1. Ange önskat namn för den här enheten (t.ex. Robot_Pi eller Gunnars_PC) [Standard: Robot_Pi]: " ROLE_NAME
ROLE_NAME=${ROLE_NAME:-"Robot_Pi"}

echo -e "2. Ange VPN-IP-adressen för den här enheten (i nätverket 192.168.200.X):"
read -p "   Ange bara sista siffran X (t.ex. 3 för laptop, 8 för robot) [Standard: 8]: " IP_LAST
IP_LAST=${IP_LAST:-"8"}
CLIENT_IP="192.168.200.$IP_LAST"

echo -e "\n   👉 Konfigurerar enheten som: ${BOLD}$ROLE_NAME${NC}"
echo -e "   👉 Tilldelad IP-adress:       ${GREEN}$CLIENT_IP${NC}"

SERVER_IP="maprosystems.duckdns.org"
SERVER_PORT="51820"
SERVER_PUB_KEY="OggFztlNOfnjKxsk6astR/+hHCKIAvxsisl/o4RS1RM="

echo -e "\nServerinställningar:"
echo -e "   Standard Server-IP:  ${BOLD}$SERVER_IP${NC}"
echo -e "   Standard Port:       ${BOLD}$SERVER_PORT${NC}"
read -p "Vill du anpassa serverns IP, port eller Public Key? (y/N): " CHANGE_SRV

if [[ "$CHANGE_SRV" =~ ^[YyJj] ]]; then
  read -p "  Ange serverns IP/Domän: " CHOSEN_SERVER_IP
  SERVER_IP=${CHOSEN_SERVER_IP:-$SERVER_IP}
  read -p "  Ange serverns port [Standard: 51820]: " CHOSEN_SERVER_PORT
  SERVER_PORT=${CHOSEN_SERVER_PORT:-$SERVER_PORT}
  read -p "  Ange serverns Public Key: " CHOSEN_SERVER_PUB_KEY
  SERVER_PUB_KEY=${CHOSEN_SERVER_PUB_KEY:-$SERVER_PUB_KEY}
fi

if [ -n "$CLIENT_IP" ]; then
  # Skapa wg0.conf
  WG_CONF="$WG_DIR/wg0.conf"
  
  echo -e "\nGenererar $WG_CONF för ${BOLD}$ROLE_NAME${NC} (${GREEN}$CLIENT_IP${NC})..."
  
  cat <<EOF > "$WG_CONF"
# ==============================================================================
# WireGuard Client Configuration for $ROLE_NAME
# Generated automatically by wireguard.sh on $(date)
# ==============================================================================

[Interface]
# Denna maskins publika nyckel: $PUB_KEY
PrivateKey = $PRIV_KEY
Address = $CLIENT_IP/24
# 1280: 4G-nät släpper inte alltid igenom WireGuards standard 1420 (stora paket
# försvann, TCP stannade medan ping gick; MacBot 2026-09-30).
MTU = 1280

[Peer]
PublicKey = $SERVER_PUB_KEY
Endpoint = $SERVER_IP:$SERVER_PORT
AllowedIPs = 192.168.200.0/24
PersistentKeepalive = 25
EOF

  chmod 600 "$WG_CONF"
  echo -e "${GREEN}✅ Konfigurationsfilen sparad framgångsrikt i $WG_CONF!${NC}"
  
  # Fråga om de vill aktivera tjänsten på en gång
  echo -e "\n${YELLOW}${BOLD}Vill du aktivera och starta WireGuard-tunneln nu?${NC}"
  echo -e "Detta kommer att göra att anslutningen startar automatiskt vid boot."
  read -p "Aktivera nu? (j/n): " START_NOW
  if [[ "$START_NOW" =~ ^[JjYy] ]]; then
    echo -e "Aktiverar systemd-tjänsten..."
    systemctl enable wg-quick@wg0.service &>/dev/null
    echo -e "Startar WireGuard wg0..."
    systemctl restart wg-quick@wg0.service &>/dev/null
    echo -e "${GREEN}✅ WireGuard-tjänsten är startad och aktiverad på den här maskinen!${NC}"
    # Vakten slår upp serverns adress igen om kontorets publika IP byts (2026-11-04).
    bash "$(dirname "${BASH_SOURCE[0]}")/wg-vakt.sh" || echo -e "${YELLOW}wg-vakt kunde inte installeras${NC}"
  else
    echo -e "${YELLOW}WireGuard startades inte. Du kan starta manuellt senare med: sudo wg-quick up wg0${NC}"
  fi

  # Visa instruktion för servern
  echo -e "\n${BLUE}${BOLD}======================================================================${NC}"
  echo -e "${BLUE}${BOLD}👉 DETTA MÅSTE DU GÖRA PÅ SERVERN (${SERVER_IP}) 👈${NC}"
  echo -e "${BLUE}${BOLD}======================================================================${NC}"
  echo -e "För att servern ska acceptera din maskin måste du registrera den som peer."
  echo -e "Lägg till följande rader i serverns konfiguration (${BOLD}/etc/wireguard/wg0.conf${NC}):\n"
  echo -e "${GREEN}[Peer]"
  echo -e "PublicKey = $PUB_KEY"
  echo -e "AllowedIPs = $CLIENT_IP/32${NC}\n"
  echo -e "Kom ihåg att köra ${BOLD}sudo systemctl restart wg-quick@wg0${NC} på servern efter ändringen!"
  echo -e "======================================================================"
fi

# ------------------------------------------------------------------------------
# ℹ️ Allmän status och kommandon
# ------------------------------------------------------------------------------
echo -e "\n${GREEN}${BOLD}======================================================================${NC}"
echo -e "${GREEN}${BOLD}🎉 WIREGUARD INSTÄLLNINGAR KLARA! 🎉${NC}"
echo -e "${GREEN}${BOLD}======================================================================${NC}"
echo -e "Användbara kommandon för WireGuard:"
echo -e "  - Kontrollera live-status:              ${BOLD}sudo wg show${NC}"
echo -e "  - Starta tunnel:                        ${BOLD}sudo wg-quick up wg0${NC}"
echo -e "  - Stoppa tunnel:                        ${BOLD}sudo wg-quick down wg0${NC}"
echo -e "  - Kontrollera anslutning till server:   ${BOLD}ping -c 3 192.168.200.1${NC}"
echo -e "======================================================================"
