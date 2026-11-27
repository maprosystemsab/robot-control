# SentinAnt Lite (robot-control)

*Programmet hette **Robotstyrning** till 2026-11-27. Repot (`robot-control`), cargo-paketet
(`robotstyrning`) och inställningsmappen (`~/.config/robotstyrning`) heter fortfarande så.*

Fjärrstyr en robot med en PS4-handkontroll och se kamerabilden live, från en vanlig
Ubuntu-dator, var du än är. Datorn och roboten pratar över ett eget krypterat nätverk
(WireGuard), och roboten stannar av sig själv om kontakten försvinner.

```
 Din dator                          Roboten (Raspberry Pi)
┌───────────────┐   WireGuard   ┌──────────┐  TCP  ┌────────────┐  USB  ┌───────────┐  CAN  ┌──────────┐
│ SentinAnt Lite│──────────────▶│  robotd  │──────▶│ Car_Client │──────▶│ styrkortet│──────▶│ motorer  │
│  + PS4-dosa   │ 192.168.200.x │ :9000    │       └────────────┘       └───────────┘       │ (VESC)   │
└───────────────┘               └──────────┘                                               └──────────┘
```

## Vad appen gör

- **Kamerabild i helskärm** från roboten (H.264 över nätverket).
- **Kör med handkontrollen:** vänster spak upp/ner = fram/bak, höger spak åt sidan = sväng.
- **AKTIVERA-knapp:** roboten kan inte köras förrän du aktiverar den, och bara när alla
  motorer svarar (och kameran fungerar i CAM-läge). Texten under knappen säger vad som saknas.
- **Färgad ram runt bilden** visar läget: grön = bra kontakt, gul = dålig kontakt,
  röd = ingen kontakt (roboten stannar), lila = handkontrollen saknas.
- **Instrumentpanel:** batterimätare (procent, spänning, ⚡ laddar), räckvidd och Wh/km,
  fart, temperatur, hur många motorer som svarar, styrvinkel (V/H i %), lutning och kurs,
  ping, och robotens fel i klartext (t.ex. underspänning).
- **Körloggar:** roboten sparar en CSV-fil per körning (en rad per sekund, 90 dagar).
  Knappen **Loggar** hämtar dem till `Hämtningar/SentinAnt Lite-körloggar` (den som redan
  har mappen `Robotstyrning-körloggar` fortsätter i den).
- **Max-reglage** för hur fort roboten får köra. Värdet sparas till nästa gång.
- **🔒 Lås** (eller Esc) slår av körningen direkt.
- **CAM/LOS:** kör på kamerabilden, eller med roboten i sikte om kameran inte fungerar.
- **⚙ Inställningar:** styrkortets motorinställningar (vilken motor som är fart och styrning)
  läses och skrivs härifrån.
- **Uppdatering med en knapp:** längst ned på startskärmen står versionen. Finns en nyare
  blir knappen grön, **Uppdatera nu**: ett tryck hämtar, kontrollerar, installerar och
  startar om programmet (se nedan).

### Säkerhet
- Släpper du spakarna stannar roboten.
- Försvinner nätverket stannar roboten **av sig själv** inom en halv sekund (watchdog på roboten).
- Tappar handkontrollen kontakten, eller rör du den inte på 5 minuter, låses körningen.
- Farten ändras mjukt (ramper), aldrig ryckigt.

## Installera

**Roboten är färdig och du ska bara köra den?**
Följ **`~/Robotstyrning/program/dokument/installationsguide_kund.md`** (på kontorsdatorn; skickas till kunden), steg för steg med skärmtexter.
Kort version för en Ubuntu-dator (22.04 eller nyare):

```bash
sudo apt install -y git
git clone https://github.com/maprosystemsab/robot-control.git ~/robot-control
cd ~/robot-control
sudo bash wireguard/wireguard.sh        # datorn med i robotens nätverk (skicka nyckeln till oss)
bash scripts/install_client.sh          # bygger och installerar SentinAnt Lite (10–20 min)
```

Starta sedan **SentinAnt Lite** från programmenyn (eller `sentinant-lite` i en terminal),
och skriv robotens adress (t.ex. `192.168.200.12`).

En ny dator kan i stället få det färdiga programmet, utan källkod och utan att bygga:
`sudo apt install ./sentinant-lite_amd64.deb` (Ubuntu 22.04 eller nyare) eller
`SentinAnt-Lite-installation.exe` (Windows 10/11, ingen administratör). Båda ligger på
uppdateringsservern, `http://192.168.200.1:8088/sentinant/`, nåbar genom tunneln.

**Uppdatera:** öppna SentinAnt Lite och tryck på den gröna knappen längst ned på
startskärmen. VPN:et (WireGuard) ska vara igång. Ingen terminal behövs.

**Har du Robotstyrning sedan förut** (installerat från källkod)? Kör det vanliga
kommandot en sista gång, med programmet stängt:
```bash
cd ~/robot-control && git pull && bash scripts/uppdatera_dator.sh
```
Det bygger den nya versionen, lägger programmet i `~/.local/share/sentinant-lite/`, byter
genvägen Robotstyrning mot **SentinAnt Lite** (i programmenyn, och i Favoriter och på
skrivbordet om den låg där) och uppdaterar RControlStation som förut. Sparade robotar och
inställningar finns kvar, och kommandot `Robotstyrning` i en terminal startar samma
program. Därefter sköter knappen uppdateringarna av SentinAnt Lite. RControlStation har
ingen knapp: den uppdateras fortfarande med kommandot ovan.

**Bygger du en ny robot** (Raspberry Pi, styrkort, WireGuard-server)?
Följ **`~/Robotstyrning/program/dokument/installationsguide.md`** (på kontorsdatorn; skickas till kunden). Kort sagt:

| Skript | Körs på | Gör |
|---|---|---|
| `scripts/ny_robot.sh` | datorn | Gör en ny robots Pi klar: WireGuard, Car_Client, RTK, robotd som tjänst |
| `scripts/skicka_robotd.sh` | datorn | Uppdaterar robotd på en robot (`PI=användare@ip`) |
| `flash_styrkort.sh` | Pi:n | Bygger och flashar styrkortets firmware (behåller inställningarna) |
| `wireguard/wireguard_admin.sh` | VPN-servern | Sätter upp servern och lägger till nya datorer/robotar |

## RControlStation

Robotarna fungerar också med **RControlStation** (karta, GPS/RTK, rutter), från
[rise_sdvp](https://github.com/maprosystemsab/rise_sdvp). Installeras med
`sudo bash install_dator.sh` i det repot, se steg 8 i kundguiden.
SentinAnt Lite och RControlStation kan inte vara anslutna till samma robot samtidigt.
RControlStation följer inte med i SentinAnt Lites uppdatering med knappen.

## Uppdateringen med en knapp

Samma uppdatering som i SentinAnt, och samma lista på vår server:
`http://192.168.200.1:8088/sentinant/uppdatering.json`, där SentinAnt Lite har posten
`sentinant-lite` (bredvid `sentinant` och `firmware`). Allt om hur det fungerar,
säkerheten, nyckeln och servern står i det interna dokumentet
`~/SentinAnt/program/dokument/uppdatering.md` (avsnitt 8 om SentinAnt Lite). Kort:

- Programmet hämtar listan vid start och var sjätte timme medan startskärmen visas.
  Listan är signerad (ed25519) med vår nyckel; den publika halvan är inbyggd i programmet
  (craten `robotuppdatering`, gemensam med SentinAnt).
- **Uppdatera nu** hämtar programfilen, kontrollerar storlek och SHA-256, provstartar den
  (`--version` ska ge den utlovade versionen), byter programfilen och startar om. Aldrig
  bakåt, aldrig med en robot ansluten, och en lista med lägre löpnummer än en redan sedd
  avvisas.
- Versionen är `0.1.0+ÅÅÅÅMMDDTTMM`: paketets version + tiden för senaste commit i UTC
  (`client/build.rs`). `sentinant-lite --version` skriver den.

| Installation | Var programmet ligger | Vid uppdatering |
|---|---|---|
| Från källkod (`install_client.sh`, `uppdatera_dator.sh`) | `~/.local/share/sentinant-lite/sentinant-lite` | byts på plats |
| Paketet (`.deb`) | `/usr/bin/sentinant-lite` | nya versionen läggs i `~/.local/share/sentinant-lite/program/`, och programmet i `/usr/bin` lämnar över till den vid start |
| Windows (installationsfilen) | `%LOCALAPPDATA%\Programs\SentinAnt Lite\sentinant-lite.exe` | byts på plats; den gamla filen byter namn och tas bort vid nästa start |
| Direkt ur cargos `target/` (utvecklingsdatorn) | `target/release/robotstyrning` | ingen knapp: "Utvecklingsversion … uppdateras inte härifrån" |

### Publicera en ny version

Med allt committat (versionen är commitens tid):
```bash
bash scripts/publicera_lite.sh --nyheter "Kort text som visas vid knappen"
```
Det bygger Linux- och Windows-programmet, `.deb`-paketet och installationsfilen, hämtar
serverns lista (bara läsning) så att SentinAnts och firmwarens poster ligger kvar och
löpnumret blir högre, signerar med nyckeln i `~/Claude/hemligt/` och lägger allt i
`paket/uppdatering/`. Sist kontrollerar programmet självt mappen
(`sentinant-lite --kontrollera-uppdatering MAPP`). **Skriptet skickar ingenting.** Lägg ut
mappen för hand, programfilerna först och listan sist:
```bash
rsync -a --exclude uppdatering.json paket/uppdatering/ mapro-server@192.168.200.1:/srv/sentinant-uppdatering/sentinant/
rsync -a paket/uppdatering/uppdatering.json mapro-server@192.168.200.1:/srv/sentinant-uppdatering/sentinant/
```
Bara delarna: `bash scripts/bygg_paket.sh` (`.deb`), `bash scripts/bygg_windows.sh`
(programfil och installationsfil för Windows; kräver `rustup target add
x86_64-pc-windows-gnu` och `sudo apt install g++-mingw-w64-x86-64-posix mingw-w64-x86-64-dev nsis nasm`).
`python3 scripts/licenser.py` skriver om `TREDJEPART.md` när beroenden ändrats.

### Prova en uppdatering utan servern

```bash
python3 -m http.server 8765 --bind 127.0.0.1 --directory paket/uppdatering
SENTINANT_UPPDATERING=http://127.0.0.1:8765 /sökväg/till/en/äldre/sentinant-lite
```
Programmet som provas får inte ligga i cargos `target/` (kopiera det till en egen mapp).
En "äldre" version byggs med `SENTINANT_LITE_BYGGE=202601010000 cargo build --release -p robotstyrning`.
Listor för prov kan signeras med provnyckeln (`--nyckel`, se uppdatering.md 7.6); de
godkänns bara av ett program byggt med `--features provnyckel`. Ett sådant program säger
PROVBYGGE i `--version` och i rött på startskärmen, och varken `install_client.sh`,
`bygg_paket.sh`, `bygg_windows.sh` eller `publicera_lite.sh` tar i det.

## Innehåll

| Mapp | Vad |
|---|---|
| `client/` | **SentinAnt Lite**, appen på din dator (Rust, paketnamn `robotstyrning`) |
| `robotd/` | Tjänsten på robotens Pi: tar emot styrningen, watchdog, ramper, video, status |
| `relay-protocol/` | Meddelandena mellan appen och robotd (delas av båda) |
| `mock-robotd/` | En låtsasrobot för att testa appen utan hårdvara |
| `robotuppdatering/` | Den signerade uppdateringslistan (appens egen uppdatering) och uppdateringen av styrkortets firmware från appen. Gemensam med SentinAnt, se [UPPDATERING_FIRMWARE.md](UPPDATERING_FIRMWARE.md) |
| `robotctl/` | Litet verktyg på Pi:n |
| `rise_sdvp/` | Car_Client och styrkortets firmware (från rise_sdvp), så att repot klarar sig självt |
| `wireguard/` | Skript och guider för VPN:et |
| `scripts/` | Installation och uppdatering |
| `firmware/` | Patchar till styrkortets firmware |
| `relay-server/` | Arkiv: den gamla relälösningen före WireGuard, byggs inte |

## Testa utan robot

```bash
# Terminal 1: låtsasroboten
cargo run -p mock-robotd -- --bind 0.0.0.0:9000

# Terminal 2: appen, anslut till 127.0.0.1
cargo run -p robotstyrning
```

Det testar anslutning, AKTIVERA-flödet, ramen och statusen, men inte att motorerna faktiskt rör sig.

## Bygga själv

```bash
cargo build --release -p robotstyrning     # appen (kräver systempaketen i install_client.sh)
cargo build --release -p robotd            # på Pi:n
```

## Licens

`rise_sdvp/` (Car_Client och firmware) är GPL-3.0, se [rise_sdvp/LICENSE](rise_sdvp/LICENSE).
Biblioteken med öppen källkod som byggs in i SentinAnt Lite står i [TREDJEPART.md](TREDJEPART.md).
