# ROV_MCU (Upwis MP101_323): stiftkarta och firmware

Kortet (95 × 160 mm) ersätter sladdhärvan: Raspberry Pi CM5, STM32F415VGT, två
u-blox ZED-F9P, BMI270 + IIS2MDC, USB-hubb, 2 × CAN, 4 DI/O. Stiftkartan är
avläst ur `MP101_323_sch.pdf` (2026-09-30) och **inte provad på hårdvara än**.

Bygg och flasha på kortets CM5: `./flash_styrkort_rovmcu.sh` (i rise_sdvp-roten).
Firmware: `make -j4 robant BOARD=mp101` väljer MP101_323/F415-kortdefinitionen
med **RobAnt-profilen** (VESC 28/36/76). Samma kortval används med de befintliga
målen `mactrac` och `drangen`. Utan `BOARD=mp101` används legacy-kortet.
Flashskriptet väljer RobAnt som standard; `--profile mactrac` eller
`--profile drangen` väljer en annan maskinprofil.

Alla kort och profiler delar `build/`. **Kör `make clean` före byte av kort
eller profil**, exempelvis `make clean && make -j4 robant BOARD=mp101`.
Bygg en profil åt gången. Flashskripten kör redan `make clean` före bygget.

## STM32F415VGT

| Funktion | Ben | Signal i schemat | I firmwaren |
|---|---|---|---|
| USB mot CM5 (via hubb) | PA11/PA12 | MCU_USB_DM/DP | som F9-kortet |
| SWD | PA13/PA14 | FMU_SWDIO/SWCLK | flashning |
| GPS 1 (U10) seriell | PC6 TX / PC7 RX | MCU_TX_F9P_RX / MCU_RX_F9P_TX | USART6, ublox.c |
| GPS 1 reset / timepulse | PC9 / PC8 | FMU_CH7_F9P_RESET_N / FMU_CH6_TIMEPULSE | ublox.c (PC9) |
| GPS 2 (U7) seriell | PA9 / PA10 | USART1_TX/RX | används inte än (byglar väljer CM5/STM) |
| GPS 2 reset / timepulse | PA15 / PA8 | FMU_CH77_F9P_RESET_N / FMU_CH66_TIMEPULSE | PA15 hög i `rovmcu_board_init()` |
| CAN1 | PB8 RX / PB9 TX | CAN1_RX/TX | comm_can.c (som F9-kortet) |
| CAN2 | PB12 RX / PB13 TX | CAN2_RX/TX | används inte än |
| CAN1/CAN2 tystläge (TCAN1057A S) | PB15 / PB14 | SILENT_CAN / SILENT_CAN2 | CAN1 **låg** (aktiv), oanvänd CAN2 **hög** (tyst) |
| IMU BMI270 + kompass IIS2MDC | PB10 SCL / PB11 SDA | I2C2_*_GPS2_MAG_LED | hårdvaru-I2C2, 100 kHz, `imu/bmi270_wrapper.c` (endast BMI270 används) |
| IMU-avbrott / kompassavbrott | PB5 / PE6 | FMU_CH5_IMU_INT2 / IIS2MDC_IRQ | används inte |
| I2C1 (extern) | PB6 / PB7 | I2C1_SCL/SDA | används inte |
| DI1 (J18/J27) | PB0 | SERV_0 | PWM, TIM3_CH3; med `NODSTOPP=di1` **nödstoppsingång** (se nedan) |
| DI2 (J23/J32) | PB1 | SERV_1 | PWM, TIM3_CH4 |
| DI3 (J24/J37) | PA2 | SERV_2 | PWM, TIM9_CH1 |
| DI4 (J26/J42) | PA3 | SERV_3 | **hjulpulsingång**, TIM2_CH4; R127 pull-up |
| Riktning DI1-4 (74AXP1T45, hög = STM32 → kontakt) | PE4 / PB3 / PB4 / PD7 | DIR_U34 / U38 / U39 / U40 | **hög/hög/hög/låg** (ut/ut/ut/in) |
| Lysdioder | PC10 / PC11 | LED0 / LED1 | **grön / röd** |
| Spänningsmätning | PC1 | PWR_5VDIV | 20k/10k, ×3; **reglerad 5 V**, inte batteri |
| Analoga in J30 | PA4 / PA5 / PA6 | ADC11 / ADC12 / ADC13 | MCU ADC-kanaler 4/5/6; **PA4 (J30 stift 2) = vinkelgivaren** (RobAnt, `ANGLE_SENSOR_J30`, delare 20k/10k) |
| Reserverade GPS-hjulpulsutgångar | PD1 / PD0 | WHEEL_1 / WHEEL_2 | reserverade för F9R |
| Telemetri/radio | PC12/PD2 (UART5), PD3-6 (USART2), PA0/PA1 (UART4) | TEL2 / TEL3 / UART4 | används inte |
| Debug-seriell | PD8 / PD9 | USART3_TX/RX_DEBUG | används inte |
| Summer | PD14 | FMU_BUZZER | används inte |
| Extern avbrottsingång | PE15 | EXT_INT | används inte (ID-switchar av på F9-kortet) |
| Reset / BOOT0 | NRST / BOOT0 | FMU_RST / FMU_BOOT_0 | styrs av CM5 |

## CM5 (40-stiftsbenen, BCM-numrering)

| CM5 | Signal | Till |
|---|---|---|
| GPIO11 | FMU_SWCLK (R52) | STM32 PA14 |
| GPIO8 | FMU_SWDIO (R39 0R; R37 till GPIO25 ej monterad) | STM32 PA13 |
| GPIO17 | FMU_RST (R49) | STM32 NRST |
| GPIO16 | FMU_BOOT_0 (R24) | STM32 BOOT0 |
| GPIO6 | FMU_NRST (R103) | se sida 2 |

Rekommenderat i `/boot/firmware/config.txt`: `gpio=17=op,dh` och `gpio=16=op,dl`
(Pi:ns standardneddragning på GPIO17 mot STM32:ans svaga uppdragning på NRST kan
annars ge en odefinierad resetnivå).

## Kvar

- Prova allt på riktigt kort: USB, CAN (VESC/ADDIO), GPS 1, BMI270, DI1-4, lysdioder.
- Vinkelgivaren på J30 stift 2 (signal) och stift 5 (GND): byggd 2026-10-22, inte provad med givare.
- BMI270:ns axlar och kortets riktning på maskinen (`BOARD_YAW_ROT`).
- Kompass IIS2MDC (drivrutin saknas; kompassen används inte i dag).
- Två antenner: RXD2/TXD2 på F9P:erna är inte inkopplade, så RTCM från GPS 1 till
  GPS 2 går via CM5 (`str2str` USB → USB); kursen (RELPOSNED) ska sedan in i firmwaren.
- CAN2, GPS 2 via USART1, hjulpulser på PD0/PD1.

## Nödstopp på DI1 (`NODSTOPP=di1`)

`make clean && make -j4 robant BOARD=mp101 NODSTOPP=di1` bygger in `nodstopp_di.c`: styrkortet läser
nödstoppet själv, oberoende av Pi:n. Utan flaggan är firmwaren byte för byte som förut.

- Koppling: NC-knapp mellan DI1 stift 2 (signal) och stift 3 (GND). DI1 saknar pull-up
  (R130 1 kΩ är "no mount"): montera R130, eller sätt 1–4,7 kΩ mellan stift 1 (+5 V) och stift 2
  i knappens plintar. Sluten = låg = OK; intryckt eller kabelbrott = hög = stopp.
- U34 vänds till ingång (DIR_U34 låg) och PB0 används inte som PWM. Bygget stoppas för maskiner
  som styr med PWM-servo på PB0 (`SERVO_VESC_ID < 0`).
- Som Car_Clients GPIO-nödstopp: 40 ms stoppar (autopilot av, fart 0, hydraulik stopp, körkommandon
  spärras); utdragen i 0,5 s ger spärrat läge som släpps av förarens stoppkommando; läget skickas som
  `[id][143][läge]` (0 ok, 1 intryckt, 2 spärrad) så RControlStation visar det. Använd inte Car_Clients
  `--nodstopp-gpio` samtidigt.

