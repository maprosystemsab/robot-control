# Kom igång med Robotstyrning 🤖

Välkommen! Den här guiden hjälper dig att få igång programmet **Robotstyrning** på din
dator, så att du kan köra roboten med en handkontroll.

**Roboten är redan färdig.** Datorn och styrkortet inuti den är inställda av oss.
Du behöver bara göra i ordning **din egen dator**. Det tar ungefär en halvtimme,
och det mesta av tiden väntar du bara.

---

> ### 📝 Fylls i av oss innan vi skickar guiden
>
> | | |
> |---|---|
> | Robotens adress | `192.168.200.___` |
> | Robotens namn i RControlStation (steg 8) | __________ |
> | Din dators nummer i nätverket (sista siffran) | `___` |
> | E-post för hjälp och för steg 3 | **maprocontroll@outlook.com** |
> | Telefon om något krånglar | __________ |

---

## Innan du börjar ✅

Bocka av att du har det här:

- ☐ En dator med **Ubuntu** (version 22.04 eller nyare) och internet.
- ☐ Ditt **lösenord** till datorn. Datorn frågar efter det några gånger.
- ☐ En **PS4-handkontroll** och en USB-sladd till den.
- ☐ Möjlighet att skicka ett **mejl** (i steg 3).

---

## Steg 1: Öppna terminalen 🖥️

Terminalen är ett fönster där man skriver kommandon till datorn. Den ser svart eller
lila ut och är inte farlig.

1. Håll ner **Ctrl** och **Alt** och tryck på **T**.
2. Ett fönster öppnas. Där står det något i stil med `anna@dator:~$`.

**Så klistrar du in i terminalen:** i den här guiden finns grå rutor med text. Markera
texten, kopiera med **Ctrl + C** och klistra in i terminalen med
**Ctrl + Shift + V** (obs: *Shift* också!). Tryck sedan **Enter**.

> 💡 När datorn frågar efter lösenord syns **ingenting** när du skriver, inte ens
> stjärnor. Det är normalt. Skriv lösenordet och tryck Enter.

---

## Steg 2: Hämta programmet 📥

Programmet hämtas från internet till en mapp som heter **robot-control** i din hemmapp.
Klistra in raderna, en i taget, och tryck Enter efter varje:

```
sudo apt install -y git
```
```
git clone https://github.com/maprosystemsab/robot-control.git ~/robot-control
```

Kontrollera att det blev rätt:
```
ls ~/robot-control
```
✅ **Rätt:** du ser en lista med bland annat `client`, `robotd`, `scripts` och `wireguard`.
❌ **Fel:** står det `Det finns ingen sådan fil eller katalog` gick hämtningen inte igenom.
Kontrollera att datorn har internet och försök igen.

---

## Steg 3: Robotens nätverk (WireGuard) och ett mejl till oss 🔐

Roboten och din dator pratar med varandra genom ett eget, säkert nätverk som heter
**WireGuard**. Din dator får en egen nyckel, och **vi** måste lägga in den hos oss innan
din dator släpps in. Därför skickar du ett mejl i slutet av det här steget.

### 3.1 Kör WireGuard-installationen
```
cd ~/robot-control/wireguard
```
```
sudo bash wireguard.sh
```

Programmet ställer fyra frågor. Svara så här:

| Frågan | Svara |
|---|---|
| `1. Ange önskat namn för den här enheten` | Ditt företags eller ditt namn, till exempel `Andersson_PC`. Inga mellanslag. |
| `Ange bara sista siffran X` | **Siffran från rutan överst** ("Din dators nummer"). **Bara siffran**, till exempel `23`. |
| `Vill du anpassa serverns IP, port eller Public Key? (y/N)` | Tryck bara **Enter** (betyder nej). |
| `Aktivera nu? (j/n)` | Skriv **j** och tryck Enter. |

### 3.2 Mejla oss din nyckel 📧

I slutet står det en blå text: **"DETTA MÅSTE DU GÖRA PÅ SERVERN"**. Det gör **inte du**,
det gör vi. Du ska bara mejla oss det som står i den gröna rutan under.

Klistra in den här raden. Den visar precis det vi behöver:
```
echo "Namn: $(hostname)"; sudo grep -E "Address" /etc/wireguard/wg0.conf; echo "PublicKey = $(sudo cat /etc/wireguard/public.key)"
```
Det ser ut ungefär så här:
```
Namn: andersson-dator
Address = 192.168.200.23/24
PublicKey = AbCdEf123...lång rad med bokstäver...=
```

**Markera de tre raderna**, kopiera med **Ctrl + Shift + C** och klistra in dem i ett mejl till:

> **maprocontroll@outlook.com**
> Ämne: *WireGuard – ny dator* och ditt företagsnamn

> ⚠️ Skicka **bara** det som kommandot ovan visar. Om du någon gång ser ordet
> **PrivateKey**: den nyckeln är hemlig och ska **aldrig** skickas till någon.

Vi svarar när din dator är inlagd. **Fortsätt med steg 4 och 5 medan du väntar.**

---

## Steg 4: Installera programmet ⚙️

Klistra in raderna, en i taget:
```
cd ~/robot-control
```
```
bash scripts/install_client.sh
```

Nu händer det här:
1. Datorn frågar efter **ditt lösenord**. Skriv det och tryck Enter.
2. Det rullar förbi massor av text. Det är datorn som hämtar och bygger programmet.
3. **Det tar 10–20 minuter.** Gå och ta en kaffe ☕. Stäng inte fönstret.

✅ **Klart** när det står `=== Klart ===` längst ner.

---

## Steg 5: Koppla in handkontrollen 🎮

**Enklast:** sätt i USB-sladden mellan handkontrollen och datorn. Klart!

**Utan sladd (Bluetooth):**
1. Öppna **Inställningar → Bluetooth** på datorn.
2. Håll ner **PS**-knappen och **Share**-knappen samtidigt på handkontrollen tills
   ljuset börjar blinka snabbt.
3. Klicka på **Wireless Controller** i listan på datorn.

---

## Steg 6: Testa kontakten med roboten 📡

**Vänta tills du har fått vårt svar på mejlet.** Slå sedan på roboten och vänta två minuter.
Byt `___` mot siffrorna i **robotens adress** (rutan överst):
```
ping -c 3 192.168.200.___
```
✅ **Rätt:** tre rader som slutar med `ms`, till exempel `tid=35 ms`.
❌ **Fel:** står det `100% packet loss`? Vänta två minuter till (roboten kanske inte har
startat klart) och försök igen. Hjälper inte det, mejla eller ring oss.

---

## Steg 7: Starta Robotstyrning 🚀

Öppna en **ny** terminal (Ctrl + Alt + T) och skriv:
```
Robotstyrning
```
Programmet finns också i programmenyn. Sök på **Robotstyrning**.

1. En svart ruta med ett textfält visas.
2. Skriv **robotens adress** (se rutan överst), till exempel `192.168.200.12`, och
   tryck **Enter**.
3. Nu ser du kamerabilden från roboten. 🎉

**Nästa gång räcker det med steg 7.** Adressen finns sparad under **Tidigare anslutna**,
så du kan klicka på den. WireGuard startar av sig själv när datorn startar.

---

## Steg 8 (om du vill): RControlStation 🗺️

**RControlStation** är ett annat program för roboten, med karta och GPS. Du behöver det
inte för att köra, men vill du ha det installerar du det så här. Klistra in raderna,
en i taget:
```
git clone https://github.com/maprosystemsab/rise_sdvp.git ~/rise_sdvp
```
```
cd ~/rise_sdvp
```
```
sudo bash install_dator.sh
```
- Datorn frågar efter ditt lösenord.
- När den frågar `Kompilera RControlStation? (y/n)` skriver du **y** och trycker Enter.
- **Det tar 10–20 minuter.** ☕

✅ **Klart** när det står `GRATULERAR! RCONTROLSTATION ÄR NU BYGGD OCH KLAR!`

Starta det genom att skriva i valfri terminal:
```
RControlStation
```

**Ansluta till roboten i RControlStation:**
1. **Stäng Robotstyrning först** (se varningen nedan).
2. Till vänster finns en lista med robotar (**Name** och **IP Address**). Klicka på **din
   robot** (namnet står i rutan överst i den här guiden).
3. Tryck på knappen **längst till vänster under listan**, den med kontakt-ikonen. Håller du
   musen över den står det *Connect to selected machine*.
4. Efter några sekunder visas roboten, och batteri och GPS börjar uppdateras.

Rutan **Status** till vänster visar GPS-lösning, ping och samma fordonsdata som
Robotstyrning: batteri, räckvidd, fart, temperatur, styrning, lutning, kurs och fel.
RControlStation sparar också en körlogg per anslutning i `Hämtningar/RControlStation-korloggar`.

**Står inte din robot i listan?** Tryck på knappen **längst till höger** under listan
(uppdatera). Syns den fortfarande inte: klicka i textrutan under listan, radera det som
står där, skriv **robotens adress** (till exempel `192.168.200.12`) och tryck på knappen
**Text**.

Handkontrollen är redan inställd och påslagen: vänster spak upp/ner = kör, höger spak åt
sidan = sväng. **Max** (hur fort den får köra) står på `0.42`.

> ⚠️ **Robotstyrning och RControlStation kan inte vara anslutna till roboten samtidigt.**
> Stäng det ena innan du ansluter med det andra.

---

## Så kör du roboten 🕹️

### Första gången: gör så här
- Ställ roboten så att hjulen **inte når marken** (på pallbockar eller liknande), eller
  se till att det är tomt och gott om plats runt den.
- Sätt **Max** lågt (uppe till höger), till exempel `0.20`.

### Skärmen
| Var | Vad |
|---|---|
| **Uppe till vänster** | Instrumentpanelen: batteri (procent, spänning, ⚡ laddar), räckvidd, fart, temperatur, motorer som svarar, styrning (V/H), lutning, kurs, ping och fel i klartext. |
| **Uppe till höger** | 🔒 Lås · 💡 Belysning · 📷 CAM/LOS · **Max** · **Loggar** · ⚙ Inställningar |
| **Mitten** | Den gröna knappen **AKTIVERA**. |
| **Ramen runt fönstret** | 🟢 grön = bra kontakt · 🟡 gul = dålig kontakt · 🔴 röd = ingen kontakt (roboten stannar) · 🟣 lila = handkontrollen saknas |

### Köra
1. Tryck på **AKTIVERA**. Står det en text under knappen berättar den varför det inte går
   än, till exempel att kamerabilden saknas.
2. **Vänster spak upp/ner** = kör framåt/bakåt.
3. **Höger spak åt sidan** = svänger.
4. Släpp spakarna, så stannar roboten.

### Stanna och låsa 🛑
- **Släpp spakarna**, så stannar roboten.
- Tryck **🔒 Lås** uppe till höger, eller **Esc** på tangentbordet. Då låses körningen och
  AKTIVERA måste tryckas igen.
- Om nätverket försvinner stannar roboten **av sig själv** inom en halv sekund.
- Om handkontrollen tappar kontakten låses körningen av sig själv.
- Rör du inte spakarna på 5 minuter låses körningen av sig själv.

### Bra att veta
- **Max** bestämmer hur fort roboten får köra. Lågt värde = lugnt. Programmet kommer ihåg
  det du ställer in.
- **CAM** betyder att du kör med kameran. **LOS** betyder att du ser roboten med egna ögon.
  I LOS-läget kan du köra även om kameran inte fungerar.
- **⚙ Inställningar** behöver du normalt inte röra. Där ligger robotens motorinställningar.
- **Batteriet:** procenten räknas fram ur spänningen när roboten står still och räknas sedan
  ner med det som förbrukas. Mitt i spannet (20–90 %) är den ungefärlig. **⚡ laddar** visas
  när roboten stått still en och en halv minut och spänningen stiger (t.ex. när elverket laddar).
- **Räckvidd** visas när roboten kört cirka 200 m, så att förbrukningen går att räkna fram.
- **Loggar** (uppe till höger) listar robotens körloggar. **Spara** lägger filen i
  `Hämtningar/Robotstyrning-körloggar`. Filerna går att öppna i Excel.

---

## Uppdatera programmet 🔄

När vi säger att det finns en ny version:

1. **Stäng Robotstyrning och RControlStation.**
2. Klistra in:
```
cd ~/robot-control && git pull && bash scripts/uppdatera_dator.sh
```

Det uppdaterar programmen på din dator, utan frågor: Robotstyrning och RControlStation
(om du har den). Datorn kan fråga efter ditt lösenord. **Det tar 5–20 minuter.** ☕

Roboten och dess styrkort behöver du inte göra något med: dem uppdaterar vi på distans.

✅ **Klart** när det står `Datorn är uppdaterad.` Står det något i rött: kör samma kommando
igen, det som redan är klart går fort. Hjälper inte det, mejla oss det som står.

---

## Om något krånglar 🔧

| Det här händer | Gör så här |
|---|---|
| `Robotstyrning: kommandot finns inte` | Stäng terminalen och öppna en ny. Hjälper inte det: gör om steg 4. |
| `git: kommandot finns inte` | Du hoppade över första raden i steg 2. Kör `sudo apt install -y git`. |
| "Anslutningen bröts" eller inget händer när du ansluter | Är roboten påslagen? Har du fått vårt svar på mejlet? Gör testet i steg 6. |
| "Car_Client upptagen" | RControlStation (eller någon annan) är ansluten till roboten. Stäng det först. |
| `RControlStation: kommandot finns inte` | Stäng terminalen och öppna en ny. Hjälper inte det: gör om steg 8. |
| "Ingen dosa ansluten" | Sätt i USB-sladden till handkontrollen, eller tryck på PS-knappen. |
| AKTIVERA går inte att trycka | Läs texten under knappen. Saknas kameran: tryck på **📷 CAM** så att det står **LOS**. |
| **VESC: 0/3 svarar** | Motorstyrningen har inte ström. Kontrollera batteriet och huvudbrytaren. |
| Roboten går trögt | Höj **Max** lite i taget. |
| Något annat | Mejla **maprocontroll@outlook.com** eller ring oss. Ta gärna en bild på skärmen (tryck **Print Screen**). |

---

**Lycka till och kör försiktigt! 🚜**
