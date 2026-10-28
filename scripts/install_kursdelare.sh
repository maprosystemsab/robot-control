#!/usr/bin/env bash
# install_kursdelare.sh — kurs med två antenner på MP101 (scripts/kursdelare.py). Körs PÅ
# roboten (CM5), från robot-control:s rot. Se kursdelare.py för hur det fungerar.
#
#   bash scripts/install_kursdelare.sh [kursoffset]   installera/uppdatera (standard −91,
#                                                     uppmätt på mk1 2026-10-28)
#   bash scripts/install_kursdelare.sh --ta-bort      ta bort, GPS 1 blir /dev/ublox igen
#
# Kursdelaren äger GPS 1 och GPS 2 och gör /dev/ublox och /dev/rtk till en delad port.
# car_client.service och sentinant-robotd.service startar efter den.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

REGEL=/etc/udev/rules.d/12-kursdelare.rules
ENHET=/etc/systemd/system/kursdelare.service
DROPIN=(car_client sentinant-robotd)

if ! tr -d '\0' < /proc/device-tree/model 2>/dev/null | grep -qi "Compute Module 5"; then
  echo "Bara för MP101 (CM5)." >&2
  exit 1
fi

if [ "${1:-}" = "--ta-bort" ]; then
  sudo systemctl disable --now kursdelare.service 2>/dev/null || true
  sudo rm -f "$REGEL" "$ENHET" /usr/local/bin/kursdelare.py
  for t in "${DROPIN[@]}"; do sudo rm -f "/etc/systemd/system/$t.service.d/kursdelare.conf"; done
  sudo systemctl daemon-reload
  sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=tty
  echo "Kursdelaren borttagen. Starta om car_client (eller sentinant-robotd) så tar den /dev/ublox direkt."
  exit 0
fi

OFFSET="${1:--91}"
echo "--- kursdelaren (kursoffset $OFFSET°) ---"
sudo install -m 755 scripts/kursdelare.py /usr/local/bin/kursdelare.py
if [ ! -f /etc/kursdelare.json ] || [ -n "${1:-}" ]; then
  echo "{\"kurs_offset_deg\": $OFFSET, \"min_langd_m\": 0.5, \"max_langd_m\": 1.5}" | sudo tee /etc/kursdelare.json > /dev/null
fi

# GPS 1 och GPS 2 får egna namn; /dev/ublox och /dev/rtk skapas av kursdelaren.
# SYMLINK= (inte +=) ersätter ublox/rtk/ublox2 från 10-/11-reglerna.
sudo tee "$REGEL" > /dev/null <<'EOF'
# Kursdelaren (robot-control scripts/kursdelare.py) äger GPS-portarna på MP101.
KERNEL=="ttyACM*", SUBSYSTEMS=="usb", KERNELS=="1-1.2", ATTRS{idVendor}=="1546", SYMLINK="ublox-gps1", ENV{ID_MM_DEVICE_IGNORE}="1"
KERNEL=="ttyACM*", SUBSYSTEMS=="usb", KERNELS=="1-1.3", ATTRS{idVendor}=="1546", SYMLINK="ublox-gps2", ENV{ID_MM_DEVICE_IGNORE}="1"
EOF

sudo tee "$ENHET" > /dev/null <<'EOF'
[Unit]
Description=Kursdelaren: kurs med två antenner (MP101) och delad GPS 1
After=systemd-udev-settle.service
Before=car_client.service sentinant-robotd.service

[Service]
ExecStart=/usr/bin/python3 /usr/local/bin/kursdelare.py
Restart=always
RestartSec=2
User=root

[Install]
WantedBy=multi-user.target
EOF

for t in "${DROPIN[@]}"; do
  sudo mkdir -p "/etc/systemd/system/$t.service.d"
  printf '[Unit]\nAfter=kursdelare.service\nWants=kursdelare.service\n' | sudo tee "/etc/systemd/system/$t.service.d/kursdelare.conf" > /dev/null
done

sudo systemctl daemon-reload
sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=tty
sleep 2
sudo systemctl enable kursdelare.service
sudo systemctl restart kursdelare.service
sleep 3
ls -l /dev/ublox /dev/rtk /dev/ublox-gps1 /dev/ublox-gps2 2>&1 | sed 's/^/  /'
echo "Starta om den som använder GPS 1 (car_client eller sentinant-robotd), så tar den den delade porten."
echo "Logg: journalctl -u kursdelare -f    Kursen: cat /run/kurs"
