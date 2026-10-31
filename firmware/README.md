# Firmware-patchar för CarController (rise_sdvp / RC_Controller)

Patcharna ändrar **inte** din firmware-mapp av sig själva. De är skrivna mot
`~/RControlStation/program/rise_sdvp/Embedded/RC_Controller` och kompilerades i en kopia
2026-09-21 (`make`, arm-none-eabi-gcc → `rc_controller.bin`, inga varningar från
den tillagda koden). **Ingen har flashats på riktiga kortet.**

## 0001-cmd-get-vesc-status.patch

Nytt kommando `CMD_GET_VESC_STATUS = 140`. Kortet svarar med de VESC som det hört
CAN-status från (id, ålder i ms, rpm, ström, duty). Ändrar inget och skickar
ingenting på CAN-bussen. Behövs för att `robotd` ska kunna skanna efter VESC:er
på riktigt (Car_Client släpper igenom kommandot och svaret utan ändring).

Svarsformat: `[bil-ID][140]` + för varje VESC 11 byte:
`id u8 | ålder_ms u16 | rpm i32 | ström*10 i16 | duty*1000 i16` (big-endian).

Förutsättning: VESC-erna måste ha "Send CAN Status" påslaget (VESC Tool →
App Settings → General → CAN status message mode), annars hör kortet inget.

### Använda

```bash
cd ~/RControlStation/program/rise_sdvp
patch -p1 --dry-run < ~/Robotstyrning/program/robot-control/firmware/0001-cmd-get-vesc-status.patch   # testa först
patch -p1 < ~/Robotstyrning/program/robot-control/firmware/0001-cmd-get-vesc-status.patch
cd Embedded/RC_Controller && make          # bygg
# flasha med ert vanliga flöde (flash_styrkort.sh) när det passar
```

Ångra: `patch -R -p1 < …/0001-cmd-get-vesc-status.patch`.
