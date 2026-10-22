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
    (5, "DI3", 3487, 2257, 3693, 2463, GRA),
    (6, "DI2", 3703, 2257, 3908, 2463, GRA),
    (7, "DI1", 3918, 2257, 4123, 2463, GRA),
    (8, "J30 analog in (vinkelgivare)", 2647, 976, 2763, 1283, GRON),
    (9, "X1 antenn GPS 1", 3487, 507, 3554, 573, BLA),
    (10, "X2 antenn GPS 2", 3568, 2018, 3635, 2086, BLA),
    (11, "Ethernet", 60, 1836, 516, 2454, GRA),
    (12, "USB-A", 1172, 1805, 1820, 2472, GRA),
    (13, "USB-C (CM5)", 1872, 2192, 2126, 2472, GRA),
    (14, "GPS 1 (U10)", 3015, 105, 3450, 666, BLA),
    (15, "GPS 2 (U7)", 3549, 1548, 4119, 1986, BLA),
]


def kortbild(fil):
    """Renderar placeringsritningen och ritar numrerade rutor runt kontakterna."""
    with tempfile.TemporaryDirectory() as tmp:
        bas = os.path.join(tmp, "kort")
        subprocess.run(["pdftoppm", "-r", "600", "-x", "560", "-y", "2350", "-W", "4180", "-H", "2550",
                        "-png", "-singlefile", RITNING, bas], check=True)
        im = Image.open(bas + ".png").convert("RGB")
    d = ImageDraw.Draw(im)
    try:
        font = ImageFont.truetype(os.path.join(pdfstil.FONT_DIR, "DejaVuSans-Bold.ttf"), 64)
    except OSError:
        font = ImageFont.load_default()
    for nr, _text, x0, y0, x1, y1, farg in MARKERINGAR:
        d.rectangle((x0 - 12, y0 - 12, x1 + 12, y1 + 12), outline=farg, width=14)
        r = 52
        cx, cy = (x0 + x1) // 2, y0 - 12 - r - 8
        if cy - r < 0:  # GPS 1 och X1 ligger vid överkanten: siffran under rutan
            cy = y1 + 12 + r + 8
        d.ellipse((cx - r, cy - r, cx + r, cy + r), fill=farg)
        d.text((cx, cy), str(nr), fill="white", font=font, anchor="mm")
    im = im.resize((im.width // 2, im.height // 2))
    im.save(fil)


def main():
    if not os.path.exists(RITNING):
        sys.exit(f"Hittar inte Upwis ritning: {RITNING}")
    bildfil = os.path.join(pdfstil.BILDER, "_mp101_kort.png")  # genereras, inte i git
    kortbild(bildfil)

    innehall = [
        h1("Kortet ovanifrån"),
        p("Kortet är nästan inte märkt. Bilden är Upwis placeringsritning med kontakterna "
          "inringade och numrerade. Kontakterna längs underkanten sitter i en rad: från "
          "matningen till vänster till DI1 längst till höger."),
        pdfstil.bild("_mp101_kort.png", 175,
                     "Grönt = används av roboten · blått = GPS · rött = matning · grått = ledigt"),
        tabell([
            ["Nr", "Kontakt", "Används till"],
            ["1", "J6 / J28", "Matning 12 V (7–60 V) från batteriet"],
            ["2", "J20 / J12", "<b>CAN1</b>: VESC-motorstyrningarna (den som firmwaren använder)"],
            ["3", "J22 / J15", "CAN2: ledig (avstängd i firmwaren)"],
            ["4", "J26 / J42", "<b>DI4</b>: hastighetsgivaren (hjulpulser)"],
            ["5–7", "J24/J37, J23/J32, J18/J27", "DI3, DI2, DI1: lediga (utgångar i firmwaren)"],
            ["8", "J30", "<b>Analoga ingångar</b>: vinkelgivaren för styrningen"],
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
            ["2", "<b>Signal</b>, med 1 kΩ pull-up till 5 V på kortet"],
            ["3", "<b>GND</b>"],
        ], bredder=[20, 155]),
        punkter([
            "En givare som drar signalen mot jord (NPN/öppen kollektor eller en kontakt, som på det "
            "gamla kortet) kopplas till stift 2 och stift 3.",
            "Behöver givaren 12 V för att fungera, behåll den matningen som i dag. Koppla bara signal "
            "och jord hit.",
        ]),
        varning("Koppla aldrig 12 V till stift 2. Ingången tål bara 5 V."),

        h1("Vinkelgivaren: J30 (nr 8)"),
        p("J30 har tre analoga ingångar (0–5 V, spänningsdelare 20k/10k på kortet). Vinkelgivaren "
          "kopplas till den första."),
        tabell([
            ["Stift", "Funktion"],
            ["1", "Matning ut: 3,3 V eller 5 V beroende på monterad säkring (R137/R138). Mät först."],
            ["2", "<b>Analog in 1 (PA4)</b>: vinkelgivarens signal"],
            ["3", "Analog in 2 (PA5)"],
            ["4", "Analog in 3 (PA6)"],
            ["5", "<b>GND</b>"],
        ], bredder=[20, 155]),
        punkter([
            "Behåll vinkelgivarens matning som i dag. Koppla signalen till stift 2 och jord till stift 5.",
            "Inom styrutslaget (±30°) ger givaren cirka 2,1–2,9 V, med god marginal till 5 V.",
            "Efter inkoppling: kör <i>Kalibrera styrvinkel…</i> i RControlStation som vanligt.",
        ]),
        varning("Firmwaren för det nya kortet läser ännu inte vinkelgivaren på J30. Utan den fungerar "
                "inte självcentrering, autocentrering och vändning. Stödet ska läggas till innan kortet "
                "sätts i en maskin."),

        h1("CAN till VESC: CAN1 (nr 2)"),
        tabell([
            ["Stift", "Funktion"],
            ["1", "+5 V ut (säkrad), koppla inte till VESC"],
            ["2", "<b>CANH</b>"],
            ["3", "<b>CANL</b>"],
            ["4", "<b>GND</b>"],
        ], bredder=[20, 155]),
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

        h1("Hitta stift 1"),
        p("Stift 1 syns inte i ritningen. Mät med multimeter när kortet är påslaget:"),
        punkter([
            "<b>DI-kontakterna och CAN:</b> stift 1 har +5 V mot GND. Stiftet i andra änden är GND.",
            "<b>J30:</b> stift 1 har 3,3 V eller 5 V. Stift 5 i andra änden är GND.",
        ], numrerad=False),
        tips("Märk kontakterna med tejp när du har mätt upp dem, så slipper nästa person göra om det."),
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
    print("Klar:", os.path.join(UT, "MP101-kortet - anslutningar.pdf"))


if __name__ == "__main__":
    main()
