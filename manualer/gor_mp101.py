#!/usr/bin/env python3
"""Bygger kopplingsguiden för Upwis MP101/ROV_MCU-kortet (var givare, CAN och antenner ska in).

  python3 manualer/gor_mp101.py [utmapp]      (standard: ~/Hämtningar/Mapro Manualer)

Kortbilden ritas från Upwis placeringsritning (MP101_323_TOP_CAM output.pdf), som är
konfidentiell och därför inte ligger i git. Sökväg: MP101_UNDERLAG eller
~/Hämtningar/mp101-underlag.
"""

import os
import subprocess
import sys
import tempfile

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfstil import bygg, h1, h2, p, punkter, tabell, tips, varning  # noqa: E402
import pdfstil  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Hämtningar/Mapro Manualer")
UNDERLAG = os.environ.get("MP101_UNDERLAG", os.path.expanduser("~/Hämtningar/mp101-underlag"))
RITNING = os.path.join(UNDERLAG, "MP101_323_TOP_CAM output.pdf")

GRON = (29, 111, 66)
BLA = (31, 95, 168)
ROD = (179, 38, 30)
ORANGE = (214, 120, 0)
GRA = (95, 99, 104)

# Kontakter i ritningen renderad med 600 dpi och beskuren till kortet
# (x 560, y 2350, 4180 × 2550 px): (nummer, text, ruta x0, y0, x1, y1, färg)
MARKERINGAR = [
    (1, "Matning 12 V", 2435, 2220, 2655, 2460, ROD),
    (2, "CAN1 (VESC)", 2714, 2257, 2972, 2463, GRON),
    (3, "CAN2", 2990, 2257, 3247, 2463, GRA),
    (4, "DI4 hjulgivare", 3271, 2257, 3477, 2463, GRON),
    (5, "DI3 lampa", 3487, 2257, 3693, 2463, GRON),
    (6, "DI2 lägesknapp", 3703, 2257, 3908, 2463, GRON),
    (7, "DI1 nödstopp", 3918, 2257, 4123, 2463, ORANGE),
    (8, "J30 analog in (vinkelgivare)", 2647, 976, 2763, 1283, GRON),
    (9, "X1 antenn GPS 1", 3487, 507, 3554, 573, BLA),
    (10, "X2 antenn GPS 2", 3568, 2018, 3635, 2086, BLA),
    (11, "Ethernet", 60, 1836, 516, 2454, GRA),
    (12, "USB-A", 1172, 1805, 1820, 2472, GRA),
    (13, "USB-C (CM5)", 1872, 2192, 2126, 2472, GRA),
    (14, "GPS 1 (U10)", 3015, 105, 3450, 666, BLA),
    (15, "GPS 2 (U7)", 3549, 1548, 4119, 1986, BLA),
]

# Stiftnumrering, uppmätt på kort 2 (2026-10-23): kontaktnummer -> (antal stift, var stift 1 sitter).
# CAN och DI räknas från änden längst från matningsplinten, J30 från änden närmast kontaktraden.
STIFT = {
    2: (4, "hoger"),
    3: (4, "hoger"),
    4: (3, "hoger"),
    5: (3, "hoger"),
    6: (3, "hoger"),
    7: (3, "hoger"),
    8: (5, "nere"),
}
# Förstoringen av kontaktraden (samma koordinater som MARKERINGAR)
RAD = (2380, 2050, 4180, 2550)


def stiftsiffror(d, x0, y0, x1, y1, farg, antal, ettan, font):
    """Ritar stiftnumren i en kontakt. Stift 1 fylls med kontaktens färg."""
    r = 27
    for i in range(antal):
        nr = i + 1 if ettan == "vanster" else antal - i
        if ettan == "nere":
            cx, cy = x0 - 12 - r - 10, y0 + (y1 - y0) * (i + 0.5) / antal
        else:
            cx, cy = x0 + (x1 - x0) * (i + 0.5) / antal, y0 + 50
        if nr == 1:
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=farg, outline=farg, width=4)
            d.text((cx, cy), "1", fill="white", font=font, anchor="mm")
        else:
            d.ellipse((cx - r, cy - r, cx + r, cy + r), fill="white", outline=farg, width=4)
            d.text((cx, cy), str(nr), fill="black", font=font, anchor="mm")


def kortbild(fil, radfil):
    """Renderar placeringsritningen och ritar numrerade rutor runt kontakterna, med stiftnummer.
    Sparar också en förstoring av kontaktraden i radfil."""
    with tempfile.TemporaryDirectory() as tmp:
        bas = os.path.join(tmp, "kort")
        subprocess.run(["pdftoppm", "-r", "600", "-x", "560", "-y", "2350", "-W", "4180", "-H", "2550",
                        "-png", "-singlefile", RITNING, bas], check=True)
        im = Image.open(bas + ".png").convert("RGB")
    d = ImageDraw.Draw(im)
    try:
        font = ImageFont.truetype(os.path.join(pdfstil.FONT_DIR, "DejaVuSans-Bold.ttf"), 64)
        liten = ImageFont.truetype(os.path.join(pdfstil.FONT_DIR, "DejaVuSans-Bold.ttf"), 34)
    except OSError:
        font = liten = ImageFont.load_default()
    for nr, _text, x0, y0, x1, y1, farg in MARKERINGAR:
        if nr in STIFT:
            stiftsiffror(d, x0, y0, x1, y1, farg, *STIFT[nr], liten)
        d.rectangle((x0 - 12, y0 - 12, x1 + 12, y1 + 12), outline=farg, width=14)
        r = 52
        cx, cy = (x0 + x1) // 2, y0 - 12 - r - 8
        if cy - r < 0:  # GPS 1 och X1 ligger vid överkanten: siffran under rutan
            cy = y1 + 12 + r + 8
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=farg)
        d.text((cx, cy), str(nr), fill="white", font=font, anchor="mm")
    im.crop(RAD).save(radfil)
    im = im.resize((im.width // 2, im.height // 2))
    im.save(fil)


def main():
    if not os.path.exists(RITNING):
        sys.exit(f"Hittar inte Upwis ritning: {RITNING}")
    bildfil = os.path.join(pdfstil.BILDER, "_mp101_kort.png")  # genereras, inte i git
    radfil = os.path.join(pdfstil.BILDER, "_mp101_rad.png")
    kortbild(bildfil, radfil)

    innehall = [
        h1("Kortet ovanifrån"),
        p("Kortet är nästan inte märkt. Bilden är Upwis placeringsritning med kontakterna "
          "inringade och numrerade. Kontakterna längs underkanten sitter i en rad: från "
          "matningen till vänster till DI1 längst till höger. Kortet ligger som när du tittar "
          "ner på det med kontaktraden mot dig."),
        pdfstil.bild("_mp101_kort.png", 175,
                     "Grönt = används av roboten (DI2/DI3 bara med lägesknapp) · orange = nödstopp · blått = GPS · rött = matning · grått = ledigt. "
                     "Små siffror = stiftnummer, stift 1 är ifyllt."),
        h2("Kontaktraden förstorad"),
        pdfstil.bild("_mp101_rad.png", 175,
                     "Stift 1 är ifyllt."),
        tips("På CAN och DI sitter stift 1 i änden <b>längst bort från matningsplinten</b> "
             "(till höger i bilden). Stift 1 är +5 V."),
        tabell([
            ["Nr", "Kontakt", "Används till"],
            ["1", "J6 / J28", "Matning 12 V (7–60 V) från batteriet"],
            ["2", "J20 / J12", "<b>CAN1</b>: VESC-motorstyrningarna (den som firmwaren använder)"],
            ["3", "J22 / J15", "CAN2: ledig (avstängd i firmwaren)"],
            ["4", "J26 / J42", "<b>DI4</b>: hastighetsgivaren (hjulpulser)"],
            ["5", "J24 / J37", "<b>DI3</b>: lampan i lägesknappen (bara med både RCS och SentinAnt), annars ledig"],
            ["6", "J23 / J32", "<b>DI2</b>: lägesknappen RCS ⇄ SentinAnt (NO-knapp), se nedan, annars ledig"],
            ["7", "J18 / J27", "<b>DI1</b>: nödstopp (NC-knapp), se nedan"],
            ["8", "J30", "<b>Analoga ingångar</b>: vinkelgivaren för styrningen (stift 2)"],
            ["9", "X1", "Antenn till GPS 1 (U10), den som används för position och RTK"],
            ["10", "X2", "Antenn till GPS 2 (U7), till kurs med två antenner senare"],
            ["11", "J29", "Ethernet till routern"],
            ["12", "J11, J13, J10", "USB-A (kamera m.m.)"],
            ["13", "USB-C", "CM5:ans USB (värdport, går inte att koppla mot en dator)"],
        ], bredder=[14, 45, 116]),
        tips("Två namn per kontaktplats (till exempel J26 / J42) betyder att kortet har plats för "
             "både en stående och en liggande kontakt. Bara den ena är monterad."),

        h1("Hastighetsgivaren: DI4 (nr 4)"),
        p("Hjulpulserna läses på DI4. Det fungerar redan i firmwaren."),
        tabell([
            ["Stift", "Funktion"],
            ["1", "+5 V ut (säkrad), behövs normalt inte"],
            ["2", "<b>Signal</b> (ingång)"],
            ["3", "<b>GND</b>"],
        ], bredder=[20, 155]),
        tips("Stift 1 sitter i änden <b>längst bort från matningsplinten</b> (till höger i bilden)."),
        punkter([
            "En givare som drar signalen mot jord (NPN/öppen kollektor eller en kontakt, som på det "
            "gamla kortet) kopplas till stift 2 och stift 3.",
            "Behöver givaren 12 V för att fungera, behåll den matningen som i dag. Koppla bara signal "
            "och jord hit.",
        ]),
        varning("Koppla aldrig 12 V till stift 2. Ingången tål bara 5 V."),

        h1("Nödstoppet: DI1 (nr 7)"),
        p("Nödstoppet läses direkt av styrkortet (STM32), inte av Pi:n. Det stoppar alltså även om "
          "Pi:n eller Car_Client har hängt sig. Knappen ska vara en <b>NC-knapp</b> (sluten i vila)."),
        tabell([
            ["Stift", "Funktion"],
            ["1", "+5 V ut (säkrad): till pull-up-motståndet"],
            ["2", "<b>Signal</b> (ingång): till knappen och motståndet"],
            ["3", "<b>GND</b>: till knappens andra sida"],
        ], bredder=[20, 155]),
        tips("Stift 1 sitter i änden <b>längst bort från matningsplinten</b> (till höger i bilden)."),
        punkter([
            "NC-knappen kopplas mellan <b>stift 2</b> och <b>stift 3</b>.",
            "DI1 saknar pull-up på kortet (R130 är inte monterad från Upwis). Sätt ett motstånd på "
            "<b>1–4,7 kΩ mellan stift 1 och stift 2</b>, enklast i knappens skruvplintar. "
            "Inget behöver lödas på kortet. (Eller be Upwis montera R130, 1 kΩ.)",
            "Knappen sluten = signalen låg = körning tillåten. Intryckt <b>eller kabelbrott</b> = "
            "signalen hög = STOPP.",
            "Intryckt: autopiloten stängs av, farten sätts till 0 och alla körkommandon spärras. "
            "När knappen dragits ut förblir körningen spärrad tills föraren släpper spaken eller "
            "trycker Lås/Stopp, så roboten börjar aldrig köra av sig själv.",
            "RControlStation visar läget på Stopp-knappen, som med nödstoppet på det gamla kortet.",
        ]),
        h2("Firmwaren"),
        p("Nödstoppet byggs bara in när det slås på, eftersom ingången flyter utan knapp och motstånd. "
          "Flasha MP101 med <i>./flash_styrkort.sh --maskin rovmcu --profil robant --nodstopp-di1</i>. "
          "Skapa sedan filen <i>/etc/robotd/nodstopp-di1</i> på roboten (<i>sudo touch "
          "/etc/robotd/nodstopp-di1</i>), så kommer nödstoppet med när roboten uppdateras."),
        varning("Prova nödstoppet med roboten upphissad innan den körs: tryck in knappen, dra ur "
                "kabeln, och kontrollera att RControlStation visar nödstopp båda gångerna. Använd "
                "inte Car_Clients nödstopp (--nodstopp-gpio) samtidigt."),

        h1("Lägesknappen: DI2 och DI3 (nr 6 och 5)"),
        p("Bara för robotar som har <b>både</b> RControlStation/Robotstyrning och SentinAnt. "
          "Bara ett av programmen kan styra roboten åt gången, eftersom båda behöver styrkortet, "
          "GPS:en och samma nätportar. Lägesknappen byter mellan dem utan att lådan behöver öppnas. "
          "Robotar med bara det ena programmet har ingen knapp."),
        p("<b>DI2 (nr 6): knappen.</b>"),
        tabell([
            ["Stift", "Funktion"],
            ["1", "+5 V ut (säkrad): till pull-up-motståndet"],
            ["2", "<b>Signal</b> (ingång): till knappen och motståndet"],
            ["3", "<b>GND</b>: till knappens andra sida"],
        ], bredder=[20, 155]),
        punkter([
            "Knappen ska vara en <b>NO-knapp</b> (öppen i vila, sluter när man trycker; multimetern "
            "piper bara när knappen är intryckt). Momentan, inte låsande.",
            "Koppla knappen mellan <b>stift 2</b> och <b>stift 3</b>.",
            "DI2 saknar pull-up på kortet (R129 är inte monterad). Sätt ett motstånd på "
            "<b>1–10 kΩ mellan stift 1 och stift 2</b> (4,7 kΩ provat), enklast i knappens skruvplintar.",
            "Kontroll med multimeter mellan stift 2 och 3: <b>cirka 5 V släppt, 0 V intryckt</b>.",
        ]),
        varning("Flasha aldrig med lägesknappen utan knapp och motstånd på DI2. Ingången flyter då och "
                "kan läsas som intryckt. Lägesväxlaren byter ändå inte läge förrän den sett knappen "
                "släppt en gång, men knappen fungerar inte."),
        h2("DI3: lampan"),
        punkter([
            "Lysdioden i knappen: <b>+ till stift 2</b>, <b>− till stift 3</b> (GND).",
            "DI3 ger cirka 5 V genom 220 Ω (R77) på kortet. Välj en knapp med lysdiod för "
            "<b>3–5 V</b> (inte 12/24 V). Bara en lysdiod direkt; större last kräver en transistor.",
            "Fast sken = SentinAnt-läget. Släckt = RCS-läget. Långsam blink = ingen styr kortet "
            "(till exempel mitt i ett byte).",
        ]),
        h2("Byta läge"),
        punkter([
            "<b>Håll in knappen i 3 s.</b> Roboten byter till det andra läget. Det tar några "
            "sekunder; ett tryck ger ett byte.",
            "Bytet görs bara när roboten står still och ingen är ansluten: ingen RControlStation, "
            "Robotstyrning eller SentinAnt (koppla från först). Annars händer ingenting.",
            "Läget sparas och gäller efter omstart.",
            "Ansluter man med fel program säger det vilket läge roboten är i och att knappen ska "
            "hållas in i 3 s.",
            "Utan knapp, från en terminal på roboten: <i>sudo byt_lage.sh sentinant</i>, "
            "<i>sudo byt_lage.sh rcs</i> eller <i>byt_lage.sh status</i>.",
        ]),
        h2("Firmwaren och programmen"),
        p("Knappen byggs in i firmwaren bara när den slås på, som nödstoppet: "
          "<i>./flash_styrkort.sh --maskin rovmcu --profil robant --nodstopp-di1 --lagesknapp-di2</i>. "
          "Skapa filen <i>/etc/robotd/lagesknapp-di2</i> (<i>sudo touch /etc/robotd/lagesknapp-di2</i>), "
          "så kommer knappen med när roboten uppdateras. SentinAnt installeras bredvid med "
          "SentinAnts <i>scripts/ny_robot.sh</i>; det lägger också in lägesväxlaren."),
        tips("Uppdatera robot-control-sidan (<i>uppdatera.sh</i>) i RCS-läget och SentinAnt-sidan "
             "(<i>skicka_robotd.sh</i>) när som helst. Uppdateringen av robot-control säger till om "
             "roboten står i SentinAnt-läget."),

        h1("Vinkelgivaren: J30 (nr 8)"),
        p("J30 har tre analoga ingångar (0–5 V, spänningsdelare 20k/10k på kortet). Vinkelgivaren "
          "kopplas till den första."),
        tabell([
            ["Stift", "Funktion"],
            ["1", "Matning ut: 5 V (uppmätt; säkring R138 monterad)"],
            ["2", "<b>Analog in 1 (PA4)</b>: vinkelgivarens signal"],
            ["3", "Analog in 2 (PA5)"],
            ["4", "Analog in 3 (PA6)"],
            ["5", "<b>GND</b>"],
        ], bredder=[20, 155]),
        tips("Stift 1 (5 V) sitter i änden <b>närmast kontaktraden</b> (nederst i bilden), "
             "stift 5 (GND) längst bort."),
        punkter([
            "Behåll vinkelgivarens matning som i dag. Koppla signalen till stift 2 och jord till stift 5.",
            "Inom styrutslaget (±30°) ger givaren cirka 2,1–2,9 V, med god marginal till 5 V.",
            "Efter inkoppling: kör <i>Kalibrera styrvinkel…</i> i RControlStation som vanligt.",
        ]),
        tips("Firmwaren läser vinkelgivaren på J30 stift 2 från och med 2026-10-22 (rise_sdvp 74aced19). "
             "Självcentrering, autocentrering och vändning fungerar som på det gamla kortet. "
             "Det gamla kortets firmware är oförändrad."),

        h1("CAN till VESC: CAN1 (nr 2)"),
        tabell([
            ["Stift", "Funktion"],
            ["1", "+5 V ut (säkrad), koppla inte till VESC"],
            ["2", "<b>CANH</b>"],
            ["3", "<b>CANL</b>"],
            ["4", "<b>GND</b>"],
        ], bredder=[20, 155]),
        tips("Stift 1 sitter i änden <b>längst bort från matningsplinten</b> (till höger i bilden)."),
        punkter([
            "Koppla CANH, CANL och GND till VESC:arnas CAN-buss.",
            "Kortet har inbyggd terminering (2 × 60 Ω), så det ska sitta i ena änden av bussen.",
            "CAN fungerar bara när kortet får 12 V på matningen (nr 1). USB räcker inte.",
        ]),

        h1("Antennerna: X1 och X2 (nr 9 och 10)"),
        punkter([
            "<b>X1 = GPS 1</b>: Car_Client och RTK använder den. Den har normal antennmatning och "
            "starkast signal. Koppla alltid huvudantennen hit.",
            "<b>X2 = GPS 2</b>: ger svagare signal (utreds med Upwis). Den används för kurs med två "
            "antenner längre fram.",
            "Antennerna (u-blox ANN-MB) får 3,3 V från kortet via kabeln.",
        ]),

        h1("Kontrollera stift 1"),
        p("Kortet saknar märkning för stift 1. Riktningen i bilderna är uppmätt på kort 2 "
          "(2026-10-23). Kontrollera med multimeter på ett nytt kort, mot minus på matningsplinten:"),
        punkter([
            "<b>CAN och DI:</b> stift 1 har +5 V. Stiftet i andra änden är GND (0 V).",
            "<b>J30:</b> stift 1 har +5 V, de andra cirka 0 V.",
        ], numrerad=False),
        tips("Märk stift 1 med en prick av lackpenna, så slipper nästa person mäta."),
    ]
    try:
        bygg(os.path.join(UT, "MP101-kortet - anslutningar.pdf"), "MP101-kortet",
             "Var givare, CAN och antenner ska kopplas in",
             "Kopplingsguide för Upwis MP101/ROV_MCU-kortet (Raspberry Pi CM5 + STM32 + två GPS). "
             "Kortet saknar nästan all märkning, så den här guiden visar var varje kontakt sitter "
             "och vad stiften gör.",
             innehall, innehallsforteckning=False)
    finally:
        os.remove(bildfil)
        os.remove(radfil)
    print("Klar:", os.path.join(UT, "MP101-kortet - anslutningar.pdf"))


if __name__ == "__main__":
    main()
