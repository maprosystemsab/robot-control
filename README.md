# Robotstyrning

Fjärrstyr en robot med en PS4-handkontroll och se kamerabilden live, från en vanlig
Ubuntu-dator, var du än är. Datorn och roboten pratar över ett eget krypterat nätverk
(WireGuard), och roboten stannar av sig själv om kontakten försvinner.

```
 Din dator                          Roboten (Raspberry Pi)
┌───────────────┐   WireGuard   ┌──────────┐  TCP  ┌────────────┐  USB  ┌───────────┐  CAN  ┌──────────┐
│ Robotstyrning │──────────────▶│  robotd  │──────▶│ Car_Client │──────▶│ styrkortet│──────▶│ motorer  │
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
  Knappen **Loggar** hämtar dem till `Hämtningar/Robotstyrning-körloggar`.
- **Max-reglage** för hur fort roboten får köra. Värdet sparas till nästa gång.
- **🔒 Lås** (eller Esc) slår av körningen direkt.
- **CAM/LOS:** kör på kamerabilden, eller med roboten i sikte om kameran inte fungerar.
- **⚙ Inställningar:** styrkortets motorinställningar (vilken motor som är fart och styrning)
  läses och skrivs härifrån.

### Säkerhet
- Släpper du spakarna stannar roboten.
- Försvinner nätverket stannar roboten **av sig själv** inom en halv sekund (watchdog på roboten).
- Tappar handkontrollen kontakten, eller rör du den inte på 5 minuter, låses körningen.
- Farten ändras mjukt (ramper), aldrig ryckigt.

## Installera

**Roboten är färdig och du ska bara köra den?**
Följ **[dokument/installationsguide_kund.md](dokument/installationsguide_kund.md)**, steg för steg med skärmtexter.
Kort version för en Ubuntu-dator (22.04 eller nyare):

```bash
sudo apt install -y git
git clone https://github.com/maprosystemsab/robot-control.git ~/robot-control
cd ~/robot-control
sudo bash wireguard/wireguard.sh        # datorn med i robotens nätverk (skicka nyckeln till oss)
bash scripts/install_client.sh          # bygger och installerar Robotstyrning (10–20 min)
```

Starta sedan med `Robotstyrning` i en terminal eller från programmenyn, och skriv
robotens adress (t.ex. `192.168.200.12`).

**Uppdatera:**
```bash
cd ~/robot-control && git pull && bash scripts/install_client.sh
```

**Bygger du en ny robot** (Raspberry Pi, styrkort, WireGuard-server)?
Följ **[dokument/installationsguide.md](dokument/installationsguide.md)**. Kort sagt:

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
Robotstyrning och RControlStation kan inte vara anslutna till samma robot samtidigt.

## Innehåll

| Mapp | Vad |
|---|---|
| `client/` | **Robotstyrning**, appen på din dator (Rust, paketnamn `robotstyrning`) |
| `robotd/` | Tjänsten på robotens Pi: tar emot styrningen, watchdog, ramper, video, status |
| `relay-protocol/` | Meddelandena mellan appen och robotd (delas av båda) |
| `mock-robotd/` | En låtsasrobot för att testa appen utan hårdvara |
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
