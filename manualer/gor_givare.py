#!/usr/bin/env python3
"""Bygger "Kopplingsschema – givare" som PDF: hastighetsgivaren (ZF GS1001) och
vinkelgivaren (DIS QR30N-360) på styrkortets TX- och RX-kontakter.
Innehållet kommer från Kopplingsschema_givare.html (2026-09-24).

  python3 manualer/gor_givare.py [utmapp]      (standard: ~/Hämtningar/Mapro Manualer)
"""

import math
import os
import sys

from reportlab.graphics.shapes import Circle, Drawing, Line, PolyLine, Rect, String
from reportlab.lib import colors

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfstil import BREDD, bild, bygg, h1, h2, p, punkter, tabell, tips, varning  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Hämtningar/Mapro Manualer")

INK = colors.HexColor("#1B2220")
MUTED = colors.HexColor("#5A6661")
ACC = colors.HexColor("#1F6B4F")
BOARD = colors.HexColor("#E6EFEA")
SENSOR = colors.HexColor("#E9ECEA")
BRUN = colors.HexColor("#8A5A2E")
SVART = colors.HexColor("#15191A")
BLA = colors.HexColor("#2563C9")

# Färgringar på motstånden (femringade, 1 %). Sista ringen (tolerans) sitter en bit ifrån de andra.
RINGFARG = {"svart": "#15191A", "brun": "#8A5A2E", "röd": "#C8281E", "orange": "#E8771A", "gul": "#E8C51A",
            "grön": "#2E9A4A", "blå": "#2563C9", "guld": "#B8902A"}
R1_RINGAR = ("brun", "grön", "svart", "röd", "brun")    # 15 kΩ
R2_RINGAR = ("röd", "röd", "svart", "röd", "brun")      # 22 kΩ


class Ritning:
    """Ritar i samma koordinater som SVG:n (y nedåt) och skalar till sidbredden."""

    def __init__(self, w, h):
        self.s = BREDD / w
        self.h = h
        self.d = Drawing(BREDD, h * self.s)

    def X(self, x):
        return x * self.s

    def Y(self, y):
        return (self.h - y) * self.s

    def rect(self, x, y, w, h, fyll, kant, bredd=1.5):
        self.d.add(Rect(self.X(x), self.Y(y + h), w * self.s, h * self.s, rx=4, ry=4,
                        fillColor=fyll, strokeColor=kant, strokeWidth=bredd))

    def text(self, x, y, t, storlek=12, farg=INK, fet=False, mono=False, ankare="start"):
        font = "Mono" if mono else ("Sans-Bold" if fet else "Sans")
        self.d.add(String(self.X(x), self.Y(y), t, fontName=font, fontSize=storlek * self.s,
                          fillColor=farg, textAnchor=ankare))

    def linje(self, punkter, farg, bredd, halo=False):
        pts = []
        for x, y in punkter:
            pts += [self.X(x), self.Y(y)]
        if halo:
            self.d.add(PolyLine(pts, strokeColor=colors.white, strokeWidth=(bredd + 3) * self.s, strokeLineCap=1))
        self.d.add(PolyLine(pts, strokeColor=farg, strokeWidth=bredd * self.s, strokeLineCap=1))

    def ringar(self, x, y, w, h, farger, liggande):
        """Färgringar på ett motstånd. Liggande: läses från vänster, stående: uppifrån."""
        lang = w if liggande else h
        pos = [0.14, 0.26, 0.38, 0.50, 0.84]
        for f, t in zip(farger, pos):
            if liggande:
                rx, ry, rw, rh = x + lang * t - 3, y + 1, 6, h - 2
            else:
                rx, ry, rw, rh = x + 1, y + lang * t - 3, w - 2, 6
            self.d.add(Rect(self.X(rx), self.Y(ry + rh), rw * self.s, rh * self.s,
                            fillColor=colors.HexColor(RINGFARG[f]), strokeColor=None))

    def stift(self, x, y, namn):
        self.d.add(Rect(self.X(x), self.Y(y + 16), 16 * self.s, 16 * self.s, fillColor=colors.white, strokeColor=ACC))
        self.text(x + 26, y + 13, namn, 13, fet=True, mono=True)


def hastighet():
    r = Ritning(720, 290)
    r.rect(20, 50, 190, 200, SENSOR, MUTED)
    r.text(36, 80, "Hastighetsgivare", 15, fet=True)
    r.text(36, 100, "ZF GS1001 / GS1002", 12, MUTED, mono=True)
    r.text(36, 118, "hall, öppen kollektor", 12, MUTED)
    r.text(36, 136, "matning 5–24 V", 12, MUTED)
    r.rect(470, 30, 230, 240, BOARD, ACC)
    r.text(486, 258, "Styrkort · TX-kontakt", 14, ACC, fet=True)
    r.text(486, 240, "troligen PA2 (TIM2 CH3)", 10, MUTED, mono=True)
    for y, farg in ((88, SVART), (150, BRUN), (210, BLA)):
        r.linje([(210, y), (462, y)], farg, 4, halo=True)
    r.stift(462, 80, "S")
    r.stift(462, 142, "5V")
    r.stift(462, 202, "GND")
    r.text(228, 80, "svart · signal", 13)
    r.text(228, 142, "brun · +", 13)
    r.text(228, 202, "blå · jord", 13)
    # Intern pull-up (streckad)
    for pts in ([(510, 88), (560, 88), (560, 76)], [(560, 46), (560, 38), (600, 38)]):
        xy = []
        for x, y in pts:
            xy += [r.X(x), r.Y(y)]
        r.d.add(PolyLine(xy, strokeColor=ACC, strokeWidth=1.5 * r.s, strokeDashArray=[4 * r.s, 3 * r.s]))
    r.d.add(Rect(r.X(551), r.Y(76), 18 * r.s, 30 * r.s, fillColor=BOARD, strokeColor=ACC, strokeWidth=1.5 * r.s,
                 strokeDashArray=[4 * r.s, 3 * r.s]))
    r.text(580, 66, "intern pull-up", 11, ACC)
    r.text(580, 80, "≈40 kΩ", 11, ACC, mono=True)
    r.text(606, 42, "3,3 V", 11, ACC, mono=True)
    return r.d


def vinkel():
    r = Ritning(720, 330)
    r.rect(20, 50, 190, 230, SENSOR, MUTED)
    r.text(36, 80, "Vinkelgivare", 15, fet=True)
    r.text(36, 100, "DIS QR30N-360HB-VK-5V", 12, MUTED, mono=True)
    r.text(36, 118, "0–5 V för 0–360°", 12, MUTED)
    r.text(36, 136, "matning 5 V, ≤25 mA", 12, MUTED)
    r.rect(520, 30, 180, 270, BOARD, ACC)
    r.text(530, 288, "Styrkort · RX-kontakt", 12.5, ACC, fet=True)
    r.text(536, 270, "troligen PA3 (ADC 3)", 11, MUTED, mono=True)
    r.linje([(210, 100), (290, 100)], SVART, 4, halo=True)
    r.linje([(370, 100), (512, 100)], INK, 2)
    r.linje([(420, 100), (420, 130)], INK, 2)
    r.linje([(420, 200), (420, 240)], INK, 2)
    # Brun ledare med en båge över R2:s ben (x = 420).
    bage = [(410 + 10 - 10 * math.cos(a * math.pi / 12), 170 - 10 * math.sin(a * math.pi / 12)) for a in range(13)]
    r.linje([(210, 170)] + bage + [(512, 170)], BRUN, 4, halo=True)
    r.linje([(210, 240), (512, 240)], BLA, 4, halo=True)
    r.rect(290, 91, 80, 18, colors.HexColor("#F2E3C6"), INK, 2)
    r.ringar(290, 91, 80, 18, R1_RINGAR, True)
    r.text(330, 84, "R1 15 kΩ", 13, fet=True, ankare="middle")
    r.text(330, 126, "brun grön svart röd · brun", 10, MUTED, ankare="middle")
    r.rect(411, 130, 18, 70, colors.HexColor("#F2E3C6"), INK, 2)
    r.ringar(411, 130, 18, 70, R2_RINGAR, False)
    r.text(438, 148, "R2 22 kΩ", 13, fet=True)
    r.text(436, 214, "röd röd svart", 10, MUTED)
    r.text(436, 227, "röd · brun", 10, MUTED)
    for y in (100, 240):
        r.d.add(Circle(r.X(420), r.Y(y), 4.5 * r.s, fillColor=INK, strokeColor=None))
    r.stift(512, 92, "S")
    r.stift(512, 162, "5V")
    r.stift(512, 232, "GND")
    r.text(222, 92, "svart", 13)
    r.text(228, 162, "brun · +5 V", 13)
    r.text(228, 232, "blå · jord", 13)
    r.text(440, 92, "max 2,97 V", 11, ACC, mono=True)
    r.text(228, 315, "R1 och R2 sitter nära styrkortet. Givaren ser 37 kΩ last (kräver minst 20 kΩ).", 12, MUTED)
    return r.d


def ringtext(*farger):
    return " ".join(f"<font color='{RINGFARG[f]}'>●</font> {f}" for f in farger)


def tråd(färg, namn):
    return f"<font color='{färg}'>●</font> {namn}"


def guide():
    c = [
        h1("Kontakten på kortet: blå längst till vänster"),
        varning("<b>Det viktiga är att den blå ledaren sitter längst till vänster, mot den stora svarta "
                "fyrkanten</b> (spolen märkt 100), precis som på bilden. De andra ledarnas färger varierar mellan "
                "kablarna, så gå bara efter den blå. Kontakterna är limmade ihop, så sitter den blå rätt sitter "
                "resten också rätt."),
        bild("givarkontakt_bla.jpg", 135, "Kontakten J9 bredvid GNSS-uttaget. Grön ram: kontakten. Vit ram: den "
             "svarta fyrkanten som den blå ledaren ska vara mot."),

        varning("<b>Delvis bekräftat.</b> Kortet är CarController v1.0 (DY-TECH) med u-blox ZED-F9P. På baksidan är "
                "TX- och RX-kontakterna märkta <b>(TX) GND · 5V · S</b> och <b>(RX) GND · 5V · S</b>. <b>S är den "
                "fyrkantiga lödpunkten</b>, närmast u-blox-modulen. Att TX-S går till processorns PA2 och RX-S till "
                "PA3 är tolkat ur firmwaren (pwm_esc.c). Namnen TX/RX stämmer med det, eftersom PA2/PA3 är processorns UART2."),

        h1("1. Hastighetsgivare → TX-kontakten"),
        p("<font color='#1F6B4F'><b>Kan kopplas in nu.</b></font>"),
        hastighet(),
        tabell([
            ["Givarens tråd", "Till TX-kontakten", "Anmärkning"],
            [tråd("#8A5A2E", "Brun"), "<b>5V</b>", "Matning. Givaren klarar 5–24 V."],
            [tråd("#2563C9", "Blå"), "<b>GND</b>", "Jord."],
            [tråd("#15191A", "Svart"), "<b>S</b>", "Pulser. Pull-up slås på i firmwaren, inget motstånd behövs."],
        ], [1.2, 1.3, 3]),
        tips("Blir signalen störd när motorerna går: löd ett <b>2,2 kΩ</b>-motstånd från S till en 3,3 V-punkt på "
             "kortet, med en egen kort tråd."),

        h1("2. Vinkelgivare → spänningsdelare → RX-kontakten"),
        varning("<b>Koppla in först när kortet har firmware 30.4 eller senare.</b> I äldre firmware är RX (PA3) en "
                "utgång. Givaren tål inte heller omvänd polaritet. <b>Utan spänningsdelaren</b> får RX 5 V, och det tål "
                "processorn inte."),
        vinkel(),
        tabell([
            ["Givarens tråd", "Till", "Anmärkning"],
            [tråd("#8A5A2E", "Brun"), "<b>5V</b>", "Exakt 5 V. Utgången följer matningen."],
            [tråd("#2563C9", "Blå"), "<b>GND</b>", "Jord, samma punkt som R2:s nedre ben."],
            [tråd("#15191A", "Svart"), "R1 (15 kΩ) → <b>S</b>", "Mellan R1 och S går R2 (22 kΩ) till GND."],
        ], [1.2, 1.5, 3]),
        p("Spänningsdelaren: 5,00 V × 22 / (15 + 22) = <b>2,97 V</b>. Processorns analoga ingång tål högst 3,3 V. "
          "12 kΩ som R1 går också (3,24 V), men <b>10 kΩ är för lite</b> (3,44 V)."),
        h2("Färgringarna"),
        p("Ett motstånd har ingen riktning: det fungerar lika bra vänt åt båda hållen. Ringarna läses från den ände där "
          "de sitter tätt. Den ensamma ringen längst bort är toleransen. Det som spelar roll är att <b>15 kΩ sitter "
          "mellan givaren och S</b> och <b>22 kΩ mellan S och GND</b>. Mät gärna med multimetern innan du löder."),
        tabell([
            ["Motstånd", "Fem ringar (1 %)", "Fyra ringar (5 %)", "Plats"],
            ["<b>R1 15 kΩ</b>", ringtext("brun", "grön", "svart", "röd", "brun"),
             ringtext("brun", "grön", "orange", "guld"), "Givarens svarta tråd → S"],
            ["<b>R2 22 kΩ</b>", ringtext("röd", "röd", "svart", "röd", "brun"),
             ringtext("röd", "röd", "orange", "guld"), "S → GND"],
            ["<font color='#A8261E'><b>Fel: 22 Ω</b></font>", ringtext("röd", "röd", "svart", "guld", "brun"),
             ringtext("röd", "röd", "svart", "guld"), "Tusen gånger för litet, ger bara ca 0,01 V"],
        ], [1.1, 2.0, 1.7, 1.7]),

        h1("3. Vad varje kontakt används till"),
        tabell([
            ["Kontakt", "Stift", "Troligen i processorn", "Används till"],
            ["<b>TX</b>", "GND · 5V · S (fyrkantig = S)", "PA2", "<font color='#1F6B4F'><b>Hastighetsgivare</b></font>"],
            ["<b>RX</b>", "GND · 5V · S (fyrkantig = S)", "PA3", "<font color='#1F6B4F'><b>Vinkelgivare</b></font> efter ny firmware"],
            ["<b>J6</b>", "GND · 5V · S", "PB0 eller PB1", "Lämna fri (servoutgång)"],
            ["<b>J7</b>", "GND · 5V · S", "PB1 eller PB0", "Lämna fri (servoutgång)"],
            ["<b>Programmering</b>", "VCC · SWCLK · GND · SWDIO · NRST · NC", "SWD",
             "<font color='#A8261E'><b>Rör ej</b></font>: programmeraren sitter här"],
            ["<b>CAN</b>", "4 stift", "CAN-buss", "<font color='#A8261E'><b>Rör ej</b></font>: VESC-motorerna"],
        ], [1.1, 2.1, 1.3, 1.9]),

        h1("4. Montering"),
        tabell([
            ["Vad", "Så här"],
            ["<b>Skivan: stål, inte aluminium</b>", "Givaren känner bara av järn. Lågkolhaltigt kallvalsat stål är bäst. "
             "Hålen blir pulserna."],
            ["<b>Luftspalt ca 1,5 mm</b>", "Mellan givarens spets och skivan. Riktvärden: hål och mellanrum ca 10 mm, "
             "material mellan hålen minst 2,5 mm, tjocklek ca 6 mm."],
            ["<b>Magneten 0–7 mm från vinkelgivaren</b>", "Centrerad inom 1 mm, helst under 0,3 mm. Magneten följer med givaren."],
            ["<b>Rakt fram ≈ 180° ≈ 2,5 V</b>", "Givaren hoppar från 5 V till 0 V vid 360°. Vrid den så att hela "
             "styrutslaget ligger långt från hoppet."],
        ], [1.6, 3.4]),

        h1("5. Gör så här"),
        punkter([
            "<b>Mät</b> med kortet på 48 V: ca 5,0 V mellan 5V och GND på TX och RX.",
            "<b>Bekräfta stiften</b> med strömmen av. Processorn har 64 ben. Är den en STM32F4 i LQFP64 är PA2 ben 16 "
            "och PA3 ben 17 (räkna moturs från pricken). Mät med summer från TX-S till ben 16 och från RX-S till ben 17.",
            "<b>Ny firmware</b>: pull-up på PA2, PA3 som analog ingång, samt hålantal och hjuldiameter. Flashas med "
            "programmeraren.",
            "<b>Hastighetsgivaren</b> kopplas in på TX. Prova med hjulen i luften, eller för en stålbit förbi givaren.",
            "<b>Vinkelgivaren</b> med R1 (15 kΩ) och R2 (22 kΩ) kopplas in på RX, <b>först efter steg 3</b>. Kalibrera rakt fram och "
            "fullt utslag åt båda hållen.",
        ], numrerad=True),
        h2("Underlag"),
        p("<font color='#5f6368'>ZF GS1001–GS1002 datablad (2024-09-11), DIS QR30N-360HB-VK-5V datablad (2020-03-23), "
          "firmware rise_sdvp RC_Controller (pwm_esc.c, wheelspeed.c, adconv.c).</font>"),
    ]
    bygg(os.path.join(UT, "Kopplingsschema - givare.pdf"), "Kopplingsschema",
         "Hastighetsgivare och vinkelgivare på RobAnt",
         "Hur ZF GS1001 (hastighet) och DIS QR30N-360 (vinkel) kopplas till styrkortets TX- och RX-kontakter. "
         "Programmeringskabeln används inte och ska inte klippas.",
         c, innehallsforteckning=False)


if __name__ == "__main__":
    os.makedirs(UT, exist_ok=True)
    guide()
    print("Klart:", UT)
