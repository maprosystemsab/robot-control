# Styrkortets firmware från SentinAnt Lite (f.d. Robotstyrning)

Under **Inställningar**, överst, står styrkortets firmwareversion och knappen **Sök ny
firmware**. Finns en nyare version på vår server blir **Uppdatera roboten** grön. Före
skrivningen kommer en varning med stor röd text (roboten ska stå helt stilla, helst
parkerad på öppen yta, under uppsikt, nödstoppet inom räckhåll), tre rutor att bocka i och
en knapp som hålls in. Byggt 2026-11-26 och provat utan hårdvara; **inte provat på ett
riktigt styrkort**.

Hela beskrivningen (hur det fungerar, säkerheten, hur firmware publiceras, checklistan för
första provet på ett riktigt kort) står i SentinAnts dokument, eftersom koden är gemensam:
`~/SentinAnt/program/dokument/uppdatering.md`, avsnitt 7. Det som gäller SentinAnt Lite:

- **Roboten gör allt själv.** robotd hämtar den signerade listan från
  `http://192.168.200.1:8088/sentinant/`, kontrollerar signaturen, hämtar filen för sin
  kortvariant, sparar kortets inställningar och hela flashminnet, skriver med openocd och
  skriver tillbaka säkerhetskopian om kortet inte svarar med rätt version. Klienten skickar
  bara "sök" och "uppdatera".
- **Koden:** `robotuppdatering/` är en ordagrann kopia av craten i autorobot (originalet).
  Ändra där och kör `bash scripts/jamfor_robotuppdatering.sh --kopiera`. robotd:s del är
  `robotd/src/firmware.rs` (spärrarna och inställningarna över länken till Car_Client),
  klientens `client/src/firmware.rs` (samma fil som i SentinAnt).
- **Kortvarianten** ska stå i `/etc/robotd/config.json`: `"firmware": { "variant": "gamla-robant" }`
  (eller t.ex. `mp101-robant-nodstopp`). Den gissas aldrig. Ny robot: `FW_VARIANT=…` till
  `scripts/install_pi.sh`. Befintlig robot: `sudo bash scripts/satt_kortvariant.sh VARIANT`
  (vägrar om varianten säger emot CM5/märkfilerna). `uppdatera_pi.sh` säger vilken variant
  den själv byggde. robotd jämför också med märkfilerna `/etc/robotd/nodstopp-di1` och
  `lagesknapp-di2` och vägrar om de säger något annat.
- **Det äldre styrkortet** går bara att uppdatera med en ST-Link ansluten i roboten; utan
  den säger robotd det och gör ingenting.
- **Rättigheter:** robotd körs som root (systemd-tjänsten, `User=root`), samma som när
  `flash_styrkort.sh` kör openocd med sudo. Är den inte root, eller saknas openocd, syns
  det som en spärr i klienten i stället för att upptäckas mitt i uppdateringen.
- **Efter skrivningen** ska kortet svara med rätt version, och (variant med lägesknapp) en
  rapport om lägesknappen ska komma från den nya firmwaren, annars skrivs den förra
  tillbaka. Nödstoppet på DI1 går inte att se här (Car_Client skickar ett eget paket för
  nödstoppet på Pi:n), så för det gäller varianten och märkfilerna.
- **Spärrar i robotd:** AKTIVERA av, roboten stilla i 3 s, ingen annan ansluten (robotd
  håller Car_Client medan föraren är ansluten, så RControlStation kan inte vara inne
  samtidigt), batteri över 20 %, aldrig bakåt i version. Under skrivningen skickas inga
  körvärden till kortet.
- **Krav:** firmware som kräver ny Car_Client publiceras med `--kraver car_client=X.Y`;
  `uppdatera_pi.sh` antecknar Car_Clients version i `/var/lib/robotuppdatering/komponenter/`.
- **Prov utan hårdvara:** `cargo build -p robotd --features provnyckel && python3 scripts/testa_firmware_latsas.py`
  (den riktiga robotd mot ett låtsas-Car_Client), och för klienten
  `mock-robotd --firmware http://127.0.0.1:8765/ --variant mp101-robant-nodstopp` (byggd med
  `--features provnyckel`). Ett bygge med provnyckeln säger PROVBYGGE och installeras inte
  av skripten.

Bilder från provet i klienten: `~/Robotstyrning/program/dokument/bilder/firmware/`.
