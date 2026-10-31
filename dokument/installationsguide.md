# Installationsguide: robotstyrning

Så installerar du hela systemet: styrdatorn, en ny robot och styrkortet.
Allt som behövs finns i det här repot. Du behöver **inte** klona rise_sdvp
(de delar som behövs ligger i `rise_sdvp/`, se [rise_sdvp/README.md](rise_sdvp/README.md)).

```
 Styrdatorn                         Roboten (Raspberry Pi 4)
┌──────────────┐   WireGuard   ┌──────────┐  TCP 8300  ┌────────────┐  USB  ┌───────────┐  CAN  ┌──────┐
│ robotstyrning│──────────────▶│  robotd  │───────────▶│ Car_Client │──────▶│ styrkortet│──────▶│ VESC │
│  + PS4-dosa  │ 192.168.200.x │ :9000/9001│           └────────────┘       └───────────┘       └──────┘
└──────────────┘               └──────────┘
```

- **robotstyrning** (klienten) på din dator: kamerabild, status, dosa.
- **robotd** på Pi:n: tar emot styrningen, watchdog (stopp efter 0,4 s utan
  kommandon), mjuka ramper, skickar vidare till Car_Client.
- **Car_Client** på Pi:n: pratar USB med styrkortet. Tar bara **en** klient åt
  gången. robotd ansluter bara medan någon kör med robotstyrning, så
  RControlStation fungerar som vanligt resten av tiden.
- **Styrkortet** skickar vidare till VESC:erna på CAN-bussen.

I guiden står det var varje kommando körs:
**Datorn** = en terminal på din dator (`mapro@…`),
**Pi:n** = en terminal inloggad på roboten (`ssh användare@ip`).

---

## 1. Hårdvara, kontrollera först

- **Styrkortet måste matas via strömplinten** (GND / 7–60 V, t.ex. 48 V-batteriet).
  Bara USB/ST-Link räcker för processorn men **inte för CAN-kretsen**. Då når
  inga kommandon VESC:erna. Ingen bygel på `EN_BAT_VIN` (J6), den stänger av kortet.
- Styrkortet med USB till Pi:n (blir `/dev/vehicle`), u-blox-GPS med USB (`/dev/ublox`).
- ST-Link mellan Pi:n och styrkortets SWD om du vill kunna flasha på distans.
- VESC:erna ska ha **CAN status** påslaget (VESC Tool → App Settings → General →
  CAN status message mode), annars ser robotd dem inte. Anteckna deras CAN-ID
  (RobAnt: 28 vänster, 36 höger, 76 styrning).
- Kamera (Logitech C922) på `/dev/video0`.

## 2. Styrdatorn (en gång)

**Datorn:**
```bash
git clone git@github.com:maprosystemsab/robot-control.git
cd robot-control
bash scripts/install_client.sh        # bygger robotstyrning + genväg i terminalen och programmenyn
sudo ./wireguard/wireguard.sh         # datorn på VPN:et, se wireguard/Wireguard_Guide.md
```

Sedan startar du programmet genom att skriva `Robotstyrning` (eller `robotstyrning`)
i valfri terminal, eller från programmenyn. Genvägen pekar på det senast byggda
programmet, så efter `git pull` + `install_client.sh` gäller den nya versionen direkt.

VPN-servern sätts upp en gång med `wireguard/wireguard_admin.sh`, se
[wireguard/Wireguard_Server_Guide.md](wireguard/Wireguard_Server_Guide.md).
Varje ny enhet (dator eller robot) ska läggas till som peer på servern.

## 3. Ny robot

### 3.1 SD-kortet
Skriv **Raspberry Pi OS (64-bit)** med Raspberry Pi Imager. Under inställningarna:
sätt användarnamn och lösenord (t.ex. robotens namn), slå på **SSH** och ställ in wifi
om Pi:n inte sitter på kabel. Starta Pi:n och kontrollera att du når den:

**Datorn:** `ssh användare@raspberrypi.local` (eller Pi:ns IP i ditt nät).

### 3.2 Installera allt med ett skript
**Datorn**, i robot-control:
```bash
bash scripts/ny_robot.sh
```
Skriptet frågar efter följande (tryck Enter för standardvärdet):

| Fråga | Exempel / standard |
|---|---|
| Robotens namn | `drangen` |
| Robotens WireGuard-IP | `192.168.200.12` (ledig adress på VPN:et) |
| Adress att nå Pi:n på just nu | `raspberrypi.local` för en ny Pi, annars WireGuard-IP:n |
| Användarnamn på Pi:n | samma som namnet |
| Bil-ID i Car_Client | `4` |
| VESC-ID | `28,36,76` |
| Max vid fullt spakutslag | `0.45` |
| I2C för OLED-skärmen | `n` (kräver omstart av Pi:n) |
| Installera WireGuard | `j` på en ny Pi |
| Installera grundsystemet | `j` på en ny Pi (Car_Client, Swepos-RTK, udev, flashverktyg) |

Sedan visas en sammanfattning, och inget görs förrän du skriver `ja`. Därefter:
1. Erbjuder att kopiera din SSH-nyckel (då slipper du lösenordet för SSH).
2. Kopierar robot-control till `~/robot-control` på Pi:n.
3. **WireGuard:** frågar namn och sista siffran i IP:n. Lägg sedan till Pi:ns
   publika nyckel på VPN-servern.
4. **Grundsystemet:** bygger Car_Client (tar några minuter) och frågar efter
   Swepos-konto för RTK. **Swepos är valfritt.** robotstyrning använder inte GPS,
   så tryck bara Enter för att hoppa över RTK. Det behövs bara för
   centimeterexakt position i RControlStation.
5. **robotd:** byggs, installeras och startas som tjänst som startar vid uppstart.
6. Erbjuder att lägga robotens namn i datorns `/etc/hosts`, så att du kan skriva
   namnet i robotstyrning i stället för IP:n.

sudo-lösenordet på Pi:n efterfrågas några gånger.

### 3.3 Styrkortets firmware
**Pi:n:**
```bash
cd ~/robot-control
./flash_styrkort.sh                  # välj maskin i menyn (RobAnt = VESC 28/36/76)
```
Skriptet bygger firmware från `rise_sdvp/Embedded/RC_Controller` och flashar ELF-filen.
Styrkortets sparade inställningar (EEPROM) behålls. Du skriver `FLASHA` för att bekräfta.

> **Nytt styrkort:** det behöver aktuatorinställningarna en gång. Där bestäms
> vilken VESC som är fart och vilken som är styrning, och i vilket läge. Det görs
> i robotstyrning: **⚙ Inställningar → Styrkortets aktuatorer**. Tryck
> **Läs från styrkortet**, ställ in raderna och tryck **Skriv till styrkortet**.
> Exempel för RobAnt: antal 3; VESC 28 och 36 = Fart, 76 = Styrning, alla Duty.
> Samma sak som Confcommon → Write i RControlStation. Roboten får inte vara aktiverad.

### 3.4 Första körningen
Roboten upphissad första gången.

1. **Datorn:** starta **Robotstyrning** (skriv `Robotstyrning` i en terminal, eller från programmenyn).
2. Skriv robotens namn eller WireGuard-IP och anslut.
3. Uppe till vänster ska det stå ungefär **Batteri: 52.6 V**, **VESC: 3/3 svarar**.
4. Tryck **AKTIVERA**. I CAM-läge krävs kamerabild. Byt till **LOS** om du kör
   med fri sikt. Står det något under knappen är det orsaken till att den är spärrad.
5. **Vänster spak upp/ner** = gas, **höger spak åt sidorna** = styrning.
   **Max** uppe till höger fungerar som Max i RControlStation. Värdet sparas.

Kör roboten åt fel håll: sätt `"invert_speed": true` eller `"invert_steering": true`
under `"drive"` i `/etc/robotd/config.json` på Pi:n och kör `sudo systemctl restart robotd`.

## 4. Uppdatera

**Datorn**, i robot-control:
```bash
git pull
bash scripts/install_client.sh                                  # klienten
PI=användare@192.168.200.x bash scripts/skicka_robotd.sh        # robotd på roboten
```
`skicka_robotd.sh --stang-av` stänger av robotd-tjänsten, och `--aktivera` slår på den igen.

## 5. Bra att veta

- **RControlStation och robotstyrning samtidigt går inte**, eftersom Car_Client bara tar
  en klient. Stäng den ena först. robotd släpper Car_Client inom några sekunder
  efter att robotstyrning kopplat ner.
- **Stopp:** släpp spaken, så skickas 0 direkt. Tappas anslutningen bromsar robotd
  in efter 0,15 s och står still efter 0,4 s. Går dosan ur slås AKTIVERA av.
- **Loggar på Pi:n:** `journalctl -u robotd -f` (robotd), `screen -r car` (Car_Client,
  lämna med Ctrl+A D).
- **Inställningar på Pi:n:** `/etc/robotd/config.json`. Fält som saknas får
  standardvärden. Efter ändring: `sudo systemctl restart robotd`.

## 6. Felsökning

| Symptom | Trolig orsak | Gör så här |
|---|---|---|
| Klienten: "Car_Client upptagen — är RControlStation ansluten?" | RControlStation håller Car_Client | Stäng RControlStation (Disconnect) |
| Klienten: "når inte Car_Client" | car_client.service kör inte | Pi:n: `systemctl status car_client` / `screen -r car` |
| VESC 0/3 svarar | Styrkortet saknar matning på strömplinten, eller CAN status av i VESC:erna | Se avsnitt 1 |
| Batteri ~2–3 V | Styrkortet går bara på USB | Mata strömplinten |
| AKTIVERA går inte att trycka | Texten under knappen säger varför (VESC, kamera) | Byt till LOS om kameran saknas |
| Kör men ingenting rör sig | Aktuatorerna på styrkortet saknar aktivitet 10/11 | ⚙ Inställningar → Styrkortets aktuatorer → Läs, rätta, Skriv |
| Går trögt | Max för lågt | Höj Max uppe till höger |
| Anslutningen bryts direkt | Två robotd kör (manuell + tjänst) | Pi:n: `sudo pkill -x robotd; sudo systemctl restart robotd` |

Mer bakgrund: [PROJECT_SPEC.md](PROJECT_SPEC.md) (arkitektur och beslut) och
[SESSION_STATUS.md](SESSION_STATUS.md) (senaste läget).
