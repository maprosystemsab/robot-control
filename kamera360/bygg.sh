#!/usr/bin/env bash
# Bygger insta360-brygga mot Insta360 Camera SDK och installerar den.
#
#   bash kamera360/bygg.sh [SDK-KATALOG]
#
# SDK-KATALOG är den uppackade CameraSDK-katalogen (med include/ och lib/).
# Standard: väljer x86_64- eller aarch64-varianten under ~/Hämtningar/insta360-sdk.
# SDK:t är Insta360:s (licensierat) och ligger INTE i repot.
#
# Installerar:
#   /usr/local/bin/insta360-brygga
#   /usr/local/lib/libCameraSDK.so
#   /etc/udev/rules.d/99-insta360.rules  (kameran läsbar utan root)
set -euo pipefail

HAR="$(cd "$(dirname "$0")" && pwd)"
SDK="${1:-}"
if [[ -z "$SDK" ]]; then
    case "$(uname -m)" in
        x86_64)  SDK=$(ls -d ~/Hämtningar/insta360-sdk/x86/CameraSDK-* 2>/dev/null | head -1) ;;
        aarch64) SDK=$(ls -d ~/Hämtningar/insta360-sdk/arm/CameraSDK-* 2>/dev/null | head -1) ;;
    esac
fi
if [[ -z "$SDK" || ! -f "$SDK/lib/libCameraSDK.so" ]]; then
    echo "Hittar inte Camera SDK (ange katalogen med include/ och lib/ som argument)." >&2
    exit 1
fi
echo "SDK: $SDK"

UT="$HAR/bygg"
mkdir -p "$UT"
g++ -std=c++17 -O2 -Wall -o "$UT/insta360-brygga" "$HAR/insta360_brygga.cpp" \
    -I"$SDK/include" -L"$SDK/lib" -lCameraSDK -lpthread \
    -Wl,-rpath,/usr/local/lib
echo "Byggd: $UT/insta360-brygga"

sudo install -m 755 "$UT/insta360-brygga" /usr/local/bin/insta360-brygga
sudo install -m 644 "$SDK/lib/libCameraSDK.so" /usr/local/lib/libCameraSDK.so
sudo ldconfig
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="2e1a", MODE="0666"' | sudo tee /etc/udev/rules.d/99-insta360.rules >/dev/null
sudo udevadm control --reload-rules
echo "Installerad: /usr/local/bin/insta360-brygga (dra ur och sätt i kameran om den redan satt i)"
