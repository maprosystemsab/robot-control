#!/usr/bin/env python3
"""Måttskiss för RobAnt mk1 ovanifrån (A4 liggande), att skriva ut och rita in GPS-antennerna
och lidarn på. Roboten ritas som i RControlStation (mapwidget.cpp paintRobAnt), i skala.

  python3 manualer/gor_robant_matt.py [utmapp]      (standard: ~/Hämtningar/Mapro Manualer)
"""

import os
import sys

from reportlab.lib import colors
from reportlab.lib.pagesizes import A4, landscape
from reportlab.lib.units import mm
from reportlab.pdfgen import canvas

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pdfstil  # noqa: E402,F401  (registrerar typsnitten)
from pdfstil import ACCENT, GREY  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/Hämtningar/Mapro Manualer")

# Mått mk1 (mm), som i RControlStation: x framåt, y vänster, origo = framaxelns mitt.
AXEL = 1420.0           # axelavstånd
HJUL_Y = 565.0          # hjulmitt i sidled
HJUL_L, HJUL_B = 460.0, 220.0
FASTE = 620.0           # hjulinfästningarna 1240 isär
BREDD = 1350.0
SKALA = 0.06 * mm       # 1:16,7 på papperet

W, H = landscape(A4)
CX = 70 * mm            # framaxelns mitt på sidan
CY = 150 * mm


def p(x, y):
    """Robotens mm (x fram, y vänster) -> sidan (fram uppåt, vänster till vänster)."""
    return CX - y * SKALA, CY + x * SKALA


def rekt(c, x0, y0, x1, y1, fyll, rund=0.0):
    ax, ay = p(max(x0, x1), max(y0, y1))
    bx, by = p(min(x0, x1), min(y0, y1))
    c.setFillColor(fyll)
    c.roundRect(min(ax, bx), min(ay, by), abs(bx - ax), abs(by - ay), rund * SKALA, stroke=1, fill=1)


def matt(c, a, b, text, forskj=(0, 0), vertikal=False):
    """Måttlinje mellan två punkter på sidan, med pilar och text."""
    (ax, ay), (bx, by) = a, b
    ax += forskj[0]; bx += forskj[0]; ay += forskj[1]; by += forskj[1]
    c.setStrokeColor(GREY)
    c.setLineWidth(0.6)
    c.line(ax, ay, bx, by)
    for (x, y) in ((ax, ay), (bx, by)):
        if vertikal:
            c.line(x - 2 * mm, y, x + 2 * mm, y)
        else:
            c.line(x, y - 2 * mm, x, y + 2 * mm)
    c.setFillColor(GREY)
    c.setFont("Sans", 8)
    if vertikal:
        c.saveState()
        c.translate(ax - 2.5 * mm, (ay + by) / 2)
        c.rotate(90)
        c.drawCentredString(0, 0, text)
        c.restoreState()
    else:
        c.drawCentredString((ax + bx) / 2, ay + 1.5 * mm, text)


def robot(c):
    mittx = -AXEL / 2
    # rutnät var 100 mm (svagt), som hjälp när man ritar in
    c.setStrokeColor(colors.HexColor("#e6e6e6"))
    c.setLineWidth(0.3)
    for i in range(-11, 12):
        x0, y0 = p(500, i * 100)
        x1, y1 = p(-AXEL - 500, i * 100)
        c.line(x0, y0, x1, y1)
    for i in range(-20, 6):
        x0, y0 = p(i * 100, 1100)
        x1, y1 = p(i * 100, -1100)
        c.line(x0, y0, x1, y1)
    # mittlinjen
    c.setStrokeColor(colors.HexColor("#b0b0b0"))
    c.setDash(3, 2)
    c.line(*p(450, 0), *p(-AXEL - 450, 0))
    c.setDash()

    c.setStrokeColor(colors.HexColor("#282828"))
    c.setLineWidth(1.0)
    for sida in (1, -1):       # ben
        rekt(c, mittx - AXEL / 2, sida * (FASTE - 70) - 45, mittx + AXEL / 2, sida * (FASTE - 70) + 45,
             colors.HexColor("#aaaaa0"))
    for hx in (0.0, -AXEL):     # hjul
        for sida in (1, -1):
            rekt(c, hx - HJUL_L / 2, sida * HJUL_Y - HJUL_B / 2, hx + HJUL_L / 2, sida * HJUL_Y + HJUL_B / 2,
                 colors.HexColor("#3c3c3c"), 60)
    lock_x = AXEL - HJUL_L - 80
    lock_y = 2 * (HJUL_Y - HJUL_B / 2) - 60
    rekt(c, mittx - lock_x / 2, -lock_y / 2, mittx + lock_x / 2, lock_y / 2, colors.HexColor("#e8f3ec"), 60)
    rekt(c, mittx - 110, FASTE - 40, mittx + 110, FASTE + 80, colors.HexColor("#f0c800"))   # nödstopp
    c.setFillColor(colors.HexColor("#c80000"))
    nx, ny = p(mittx, FASTE + 20)
    c.circle(nx, ny, 40 * SKALA, stroke=1, fill=1)
    # grön pil på framaxeln
    c.setFillColor(ACCENT)
    path = c.beginPath()
    path.moveTo(*p(150, 0)); path.lineTo(*p(-60, 110)); path.lineTo(*p(-60, -110)); path.close()
    c.drawPath(path, stroke=1, fill=1)

    # axlarna
    c.setFont("Sans-Bold", 8.5)
    c.setFillColor(colors.black)
    for x, text in ((0.0, "Framaxel"), (-AXEL, "Bakaxel (styr)")):
        c.setStrokeColor(colors.HexColor("#808080"))
        c.setDash(1, 2)
        c.line(*p(x, BREDD / 2 + 60), *p(x, -BREDD / 2 - 60))
        c.setDash()
        lx, ly = p(x, -BREDD / 2 - 80)
        c.drawString(lx + 1 * mm, ly - 1 * mm, text)
    # fram och sidorna
    fx, fy = p(560, 0)
    c.setFillColor(ACCENT)
    c.setFont("Sans-Bold", 11)
    c.drawCentredString(fx, fy, "FRAM ▲")
    c.setFillColor(GREY)
    c.setFont("Sans", 9)
    vx, vy = p(-AXEL / 2, BREDD / 2 + 330)
    c.drawCentredString(vx, vy, "VÄNSTER")
    hx, hy = p(-AXEL / 2, -BREDD / 2 - 330)
    c.drawCentredString(hx, hy, "HÖGER")
    c.setFont("Sans-Oblique", 7.5)
    ex, ey = p(-AXEL / 2 - 200, FASTE + 20)
    c.drawCentredString(ex, ey, "nödstopp")

    # kända mått
    matt(c, p(-AXEL, -BREDD / 2), p(0, -BREDD / 2), "axelavstånd 1420", forskj=(14 * mm, 0), vertikal=True)
    matt(c, p(-AXEL - 330, BREDD / 2), p(-AXEL - 330, -BREDD / 2), "bredd 1350", forskj=(0, -6 * mm))

    # skala
    sx, sy = 20 * mm, 18 * mm
    c.setStrokeColor(colors.black)
    c.setLineWidth(1.2)
    c.line(sx, sy, sx + 1000 * SKALA, sy)
    for i in range(0, 11):
        c.line(sx + i * 100 * SKALA, sy, sx + i * 100 * SKALA, sy + (2.5 if i % 5 == 0 else 1.2) * mm)
    c.setFont("Sans", 8)
    c.setFillColor(colors.black)
    c.drawString(sx, sy - 4 * mm, "0")
    c.drawString(sx + 1000 * SKALA - 3 * mm, sy - 4 * mm, "1 m")
    c.setFillColor(GREY)
    c.drawString(sx + 1000 * SKALA + 4 * mm, sy, "Skala ca 1:17 · rutor 10 cm")


def tabell(c):
    x0, y = 150 * mm, H - 22 * mm
    c.setFillColor(ACCENT)
    c.setFont("Sans-Bold", 16)
    c.drawString(x0, y, "RobAnt mk1: måttskiss")
    y -= 6 * mm
    c.setFillColor(GREY)
    c.setFont("Sans", 9)
    c.drawString(x0, y, "Rita in X1, X2 och lidarn i bilden. Mät till bakaxeln (de styrande hjulen),")
    y -= 4.2 * mm
    c.drawString(x0, y, "till mittlinjen och till marken. Mät till antennens mitt och lidarns mitt.")
    y -= 9 * mm

    rader = [
        ("GPS 1 (X1, positionen)", ["framför bakaxeln", "från mittlinjen (V/H)", "höjd över marken"]),
        ("GPS 2 (X2, kursen)", ["framför bakaxeln", "från mittlinjen (V/H)", "höjd över marken"]),
        ("Mellan antennerna", ["X1 till X2"]),
        ("Lidar (Mid-360)", ["framför bakaxeln", "från mittlinjen (V/H)", "höjd över marken (mitten)",
                             "lutning nedåt (°)", "framsidan pekar (fram/bak)"]),
        ("Roboten", ["främsta punkten framför bakaxeln", "bakersta punkten bakom bakaxeln",
                     "största bredd", "högsta punkten (inkl. lidar)"]),
    ]
    for rubrik, falt in rader:
        c.setFillColor(colors.black)
        c.setFont("Sans-Bold", 10)
        c.drawString(x0, y, rubrik)
        y -= 6 * mm
        c.setFont("Sans", 9)
        for f in falt:
            c.setFillColor(GREY)
            c.drawString(x0 + 3 * mm, y, f)
            c.setStrokeColor(colors.HexColor("#9a9a9a"))
            c.setLineWidth(0.5)
            c.line(x0 + 68 * mm, y - 0.8 * mm, x0 + 108 * mm, y - 0.8 * mm)
            c.drawString(x0 + 110 * mm, y, "mm" if "°" not in f and "pekar" not in f else "")
            y -= 6.2 * mm
        y -= 2.5 * mm

    c.setFillColor(GREY)
    c.setFont("Sans-Oblique", 8)
    c.drawString(x0, 14 * mm, "Mått från RControlStation (mk1 2026-10-16): längd 1,9 m, bredd 1,35 m, hjul Ø 0,46 m.")


def main():
    os.makedirs(UT, exist_ok=True)
    fil = os.path.join(UT, "RobAnt mk1 - måttskiss.pdf")
    c = canvas.Canvas(fil, pagesize=(W, H))
    c.setTitle("RobAnt mk1: måttskiss")
    c.setAuthor("Mapro Systems AB")
    robot(c)
    tabell(c)
    c.showPage()
    c.save()
    print("Klar:", fil)


if __name__ == "__main__":
    main()
