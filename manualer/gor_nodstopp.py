#!/usr/bin/env python3
"""Bygger installationsguiden för nödstoppsknappen (NC-brytare på Pi:ns GPIO27) som PDF.

  python3 manualer/gor_nodstopp.py [utmapp]      (standard: ~/Robotstyrning/manualer/Mapro Manualer, annars ~/Hämtningar/Mapro Manualer)
"""

import os
import sys

from reportlab.graphics.shapes import Circle, Drawing, Line, Rect, String
from reportlab.lib import colors

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfstil import ACCENT, BREDD, GREY, WARN, bygg, h1, h2, p, punkter, tabell, tips, varning  # noqa: E402
from pdfstil import standard_utmapp  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else standard_utmapp()

BLA = colors.HexColor("#1f5fa8")


def kopplingsschema():
    """Pi:ns 40-poliga stiftlist med stift 13 (GPIO27) och 14 (GND) och knappens NC-kontakt."""
    w, h = BREDD, 270
    d = Drawing(w, h)
    steg = 15
    x0, y_udda, y_jamn = 60, 200, 215  # udda stift (1, 3 …) inre raden, jämna (2, 4 …) kortets kant

    d.add(Rect(x0 - 12, y_udda - 11, steg * 20 + 9, 37, fillColor=colors.HexColor("#1b1b1b"), strokeColor=None))
    for k in range(20):
        x = x0 + k * steg
        for nr, y in ((2 * k + 1, y_udda), (2 * k + 2, y_jamn)):
            if nr == 13:
                fyll, r = BLA, 5.5
            elif nr == 14:
                fyll, r = colors.black, 5.5
            elif nr == 11:
                fyll, r = colors.HexColor("#8a8a8a"), 4
            else:
                fyll, r = colors.HexColor("#d4af37"), 4
            d.add(Circle(x, y, r, fillColor=fyll, strokeColor=colors.white if nr in (13, 14) else None, strokeWidth=1.2))

    d.add(String(x0 - 14, y_jamn + 20, "Stift 2", fontName="Sans", fontSize=7.5, fillColor=GREY))
    d.add(String(x0 - 14, y_udda - 22, "Stift 1 (fyrkantig lödö)", fontName="Sans", fontSize=7.5, fillColor=GREY))
    d.add(String(x0 + 19 * steg - 30, y_jamn + 20, "Stift 40", fontName="Sans", fontSize=7.5, fillColor=GREY))
    d.add(String(x0 + 20 * steg + 2, y_udda + 4, "→ mot USB-portarna", fontName="Sans", fontSize=8, fillColor=GREY))
    d.add(String(x0 + 20 * steg + 2, y_jamn + 14, "kortets ytterkant", fontName="Sans", fontSize=8, fillColor=GREY))

    x13 = x0 + 6 * steg
    d.add(String(x13 + 8, y_jamn + 20, "14 = GND", fontName="Sans-Bold", fontSize=9, fillColor=colors.black))
    d.add(String(x13 + 8, y_udda - 22, "13 = GPIO27", fontName="Sans-Bold", fontSize=9, fillColor=BLA))

    # Kablarna ner till knappen.
    kx = x0 + 240
    ky = 70
    d.add(Line(x13, y_udda - 6, x13, 120, strokeColor=BLA, strokeWidth=2))
    d.add(Line(x13, 120, kx - 30, 120, strokeColor=BLA, strokeWidth=2))
    d.add(Line(kx - 30, 120, kx - 30, ky + 14, strokeColor=BLA, strokeWidth=2))
    d.add(Line(x13, y_jamn + 6, x13, y_jamn + 32, strokeColor=colors.black, strokeWidth=2))
    d.add(Line(x13, y_jamn + 32, x0 - 30, y_jamn + 32, strokeColor=colors.black, strokeWidth=2))
    d.add(Line(x0 - 30, y_jamn + 32, x0 - 30, 40, strokeColor=colors.black, strokeWidth=2))
    d.add(Line(x0 - 30, 40, kx + 30, 40, strokeColor=colors.black, strokeWidth=2))
    d.add(Line(kx + 30, 40, kx + 30, ky - 14, strokeColor=colors.black, strokeWidth=2))
    d.add(String(x13 - 4, 126, "blå/vit ledare", fontName="Sans", fontSize=8, fillColor=BLA, textAnchor="end"))
    d.add(String(x0 - 24, 46, "svart ledare (GND)", fontName="Sans", fontSize=8, fillColor=colors.black))

    # Nödstoppet: kontaktblock NC (11–12) och röd svamp.
    d.add(Rect(kx - 48, ky - 14, 96, 28, fillColor=colors.HexColor("#f2f2f2"), strokeColor=colors.black))
    d.add(Circle(kx - 30, ky, 3.5, fillColor=colors.black))
    d.add(Circle(kx + 30, ky, 3.5, fillColor=colors.black))
    d.add(Line(kx - 30, ky + 4, kx + 30, ky + 4, strokeColor=colors.black, strokeWidth=2.5))
    d.add(String(kx - 30, ky - 11, "11", fontName="Sans", fontSize=7.5, textAnchor="middle"))
    d.add(String(kx + 30, ky - 11, "12", fontName="Sans", fontSize=7.5, textAnchor="middle"))
    d.add(String(kx, ky - 11, "NC", fontName="Sans-Bold", fontSize=8, textAnchor="middle", fillColor=WARN))
    d.add(Rect(kx - 6, ky + 14, 12, 18, fillColor=colors.HexColor("#999999"), strokeColor=None))
    d.add(Circle(kx, ky + 42, 17, fillColor=colors.HexColor("#d32f2f"), strokeColor=colors.HexColor("#8b0000")))
    d.add(String(kx + 60, ky + 40, "NÖDSTOPP", fontName="Sans-Bold", fontSize=10, fillColor=WARN))
    d.add(String(kx + 60, ky + 27, "(låsande, vrid för att", fontName="Sans", fontSize=8, fillColor=GREY))
    d.add(String(kx + 60, ky + 16, "återställa)", fontName="Sans", fontSize=8, fillColor=GREY))
    d.add(String(kx + 60, ky - 4, "Ute = sluten krets = kör", fontName="Sans", fontSize=8, fillColor=ACCENT))
    d.add(String(kx + 60, ky - 16, "Intryckt = bruten = STOPP", fontName="Sans", fontSize=8, fillColor=WARN))
    return d


def guide():
    c = [
        h1("1. Vad nödstoppet gör"),
        p("Nödstoppsknappen kopplas till robotens Raspberry Pi. När knappen trycks in, eller om kabeln går av, "
          "<b>stoppar Pi:n all körning</b>:"),
        punkter([
            "Inga kör- eller styrkommandon släpps fram till styrkortet, varken från Robotstyrning eller RControlStation.",
            "Pi:n skickar själv <i>fart = 0</i> till drivningen och stänger av autopiloten.",
            "Statusskärmen visar <b>Nödstopp INTRYCKT</b> i rött och Robotstyrning visar <b>NÖDSTOPP INTRYCKT</b>.",
            "När knappen dras ut börjar roboten <b>inte</b> köra av sig själv. Föraren måste först släppa spaken "
            "och aktivera igen (Robotstyrning), eller trycka Stopp/Esc (RControlStation).",
        ]),
        varning("Det här är ett stopp genom Pi:ns programvara. Det fungerar så länge Pi:n och Car_Client är igång, "
                "men det bryter inte strömmen till motorerna. Ett nödstopp som bryter matningen till VESC:erna "
                "via en kontaktor ska komma senare som komplement."),

        h1("2. Material"),
        tabell([
            ["Del", "Krav"],
            ["Nödstoppsknapp", "Röd svamp på gul botten, <b>låsande</b> (stannar intryckt, vrid för att återställa). "
             "Kontaktblock <b>NC</b> (normalt slutet), märkt 11–12 eller med ett rött block."],
            ["Kabel", "2-ledare, gärna partvinnad eller skärmad, 0,25–0,5 mm². Så kort som möjligt, högst cirka 3 m."],
            ["Kontaktdon mot Pi:n", "2 st Dupont-hylsor (hona) för 2,54 mm stiftlist, eller en 2-polig kontakt."],
            ["Övrigt", "Kabelskor till knappens skruvar, krympslang, buntband, multimeter."],
        ], [1.2, 4]),

        h1("3. Kopplingsschema"),
        p("Knappens NC-kontakt kopplas mellan <b>stift 13 (GPIO27)</b> och <b>stift 14 (GND)</b> på Pi:ns stiftlist. "
          "Inget annat behövs: Pi:n har ett inbyggt motstånd som håller signalen hög när kontakten är bruten."),
        kopplingsschema(),
        tabell([
            ["Från knappen", "Till Pi:ns stiftlist", "Signal"],
            ["NC-kontakt, plint 11", "<b>Stift 13</b> (inre raden, 7:e stiftet från stift 1)", "GPIO27"],
            ["NC-kontakt, plint 12", "<b>Stift 14</b> (yttre raden, bredvid stift 13)", "GND (jord)"],
        ], [1.4, 2.6, 1]),
        varning("Koppla <b>ingen</b> spänning till knappen eller Pi:n. Knappen ska vara potentialfri: bara de två "
                "ledarna till Pi:n. 12 V eller 48 V på stiftlisten förstör Pi:n direkt. Använd inte en knapp med lampa."),
        tips("Stift 1 har en <b>fyrkantig</b> lödö på kortets undersida och sitter i den ände av stiftlisten som är "
             "längst från USB-portarna. Stift 11 (GPIO17) används redan till belysningsreläet. Rör inte det."),

        h1("4. Så här kopplar du"),
        punkter([
            "Stäng av roboten och Pi:n (huvudbrytaren).",
            "Montera knappen väl synlig och lätt att nå från båda sidor av roboten, gärna baktill eller ovanpå.",
            "Mät med multimetern på knappens kontaktblock: <b>knappen ute = 0 Ω (sluten)</b>, "
            "<b>intryckt = ingen kontakt (bruten)</b>. Visar den tvärtom sitter du på NO-blocket (13–14). Byt till NC.",
            "Dra kabeln till Pi:n. Håll den borta från motorkablar och 48 V-ledningar, och fäst den med buntband.",
            "Kläm Dupont-hylsorna på ledarna och isolera med krympslang.",
            "Sätt ledaren från plint 11 på <b>stift 13</b> och ledaren från plint 12 på <b>stift 14</b>.",
            "Om Pi:n har ett kort (HAT) ovanpå stiftlisten: koppla till kortets genomgående stift 13 och 14, "
            "eller använd en förhöjd stiftlist.",
            "Dra ut knappen (vrid) och slå på roboten.",
        ], numrerad=True),

        h1("5. Provning"),
        p("Provningen görs när roboten har vårt nya SD-kort och nödstoppet är påslaget i programvaran. "
          "Vi gör det tillsammans över telefon, med roboten <b>upphissad</b> eller stående på fritt underlag."),
        tabell([
            ["Steg", "Gör så här", "Ska hända"],
            ["1", "Knappen ute, titta på statusskärmen.", "Rutan <b>Nödstopp</b> är grön: <i>OK</i>. "
             "(Gul <i>utdragen</i> direkt efter start är normalt tills föraren har skickat ett stopp.)"],
            ["2", "Tryck in knappen.", "Rutan blir röd och blinkar: <i>INTRYCKT</i>. Robotstyrning visar "
             "<b>NÖDSTOPP INTRYCKT</b> och AKTIVERA går inte att trycka."],
            ["3", "Försök köra med dosan.", "Inget rör sig."],
            ["4", "Dra ut knappen.", "Rutan blir gul: <i>utdragen – väntar på stopp från föraren</i>."],
            ["5", "Släpp spaken (AKTIVERA av), tryck AKTIVERA.", "Rutan blir grön. Roboten går att köra igen."],
            ["6", "Kör långsamt och tryck in knappen.", "Roboten stannar direkt."],
            ["7", "Dra ur en av kablarna vid Pi:n (kabelbrott).", "Samma som intryckt knapp: röd ruta, inget rör sig."],
        ], [0.4, 2, 3]),

        h1("6. Felsökning"),
        tabell([
            ["Det händer", "Orsak och åtgärd"],
            ["Rutan är röd fast knappen är ute", "Kabelbrott, glapp i Dupont-hylsan, fel stift eller NO-block i stället för NC. "
             "Mät knappen och kontrollera stift 13 och 14."],
            ["Rutan blinkar till rött ibland", "Glapp eller störning i kabeln. Använd partvinnad eller skärmad kabel och "
             "dra den bort från motorkablarna."],
            ["Ingen nödstoppsruta på statusskärmen", "Nödstoppet är inte påslaget i programvaran. Hör av dig till oss."],
            ["Rutan säger <i>inget svar från Car_Client</i>", "Car_Client har stannat. Tryck på Car_Client-rutan "
             "för omstart. All körning är spärrad under tiden."],
        ], [1.6, 3.4]),

        h2("6.1 För Mapro: slå på nödstoppet i programvaran"),
        p("På en ny Pi: svara <b>j</b> på frågan om nödstopp i <b>scripts/ny_robot.sh</b>. På en Pi som redan är "
          "installerad: lägg till <b>--nodstopp-gpio 27</b> sist i Car_Client-raden i <b>~/start_car.sh</b> och starta "
          "om car_client. Utan knapp inkopplad läses pinnen som intryckt nödstopp, så slå bara på det där knappen finns."),
    ]
    bygg(os.path.join(UT, "Nödstopp - installation.pdf"), "Nödstopp", "Koppla in nödstoppsknappen på RobAnt",
         "Så här kopplar du in en nödstoppsknapp (NC-brytare) till robotens Raspberry Pi, och så provar du den.",
         c, innehallsforteckning=False)


if __name__ == "__main__":
    os.makedirs(UT, exist_ok=True)
    guide()
    print("Klart:", UT)
