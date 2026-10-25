#!/usr/bin/env python3
"""Bygger mp101_lada.step (riktiga ytor, för SolidWorks m.fl.) med FreeCAD.

  freecadcmd hardvara/mp101-lada/gor_step.py
  SKARM=1 freecadcmd hardvara/mp101-lada/gor_step.py     (mp101_lada_skarm.step, med 4,3"-skärm)

Måtten läses ur mp101_lada.scad, så ändringar görs bara där. Geometrin här ska
spegla modulerna botten(), lock_kropp() och text_kropp() i .scad-filen.
Kroppar i monterat läge: botten, lock, text och loggans streck (infällda i locket), plus
skärmklämmorna i skärmvarianten.
"""

import os
import re

import FreeCAD
import Import
import Part
from FreeCAD import Vector

HAR = os.path.dirname(os.path.abspath(__file__)) if "__file__" in globals() else os.getcwd()
SCAD = os.path.join(HAR, "mp101_lada.scad")
SKARM = os.environ.get("SKARM", "") not in ("", "0")
UT = os.path.join(HAR, "mp101_lada_skarm.step" if SKARM else "mp101_lada.step")
TYPSNITT = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
TEXT_SKALA = 1.0776   # OpenSCAD text(size) blir så här mycket större än FreeCADs

# ---- Mått ur .scad: enkla tilldelningar och listorna HAL, KANT, TOPP, CM5 ----
k = {"SKARM": SKARM}
kalla = open(SCAD, encoding="utf-8").read()
for namn, uttryck in re.findall(r"^([A-Z][A-Z0-9_]*)\s*=\s*([^;]+);", kalla, re.M | re.S):
    uttryck = re.sub(r"//[^\n]*", "", uttryck).strip()
    if uttryck.startswith("[for") or namn == "SKARM":
        continue
    # OpenSCAD:s  a ? b : c  (en nivå) -> Pythons  (b if a else c)
    uttryck = re.sub(r"^(\w+)\s*\?\s*(.+?)\s*:\s*(.+)$", r"(\2 if \1 else \3)", uttryck)
    try:  # eval av vår egen .scad-fil (siffror, listor, tidigare namn), aldrig av indata utifrån
        k[namn] = eval(uttryck, {"__builtins__": {}}, k)
    except Exception:
        pass
g = type("K", (), k)
LOCK_SKRUV = [(x, y) for x in (-g.SPEL - g.ANDE_V + 3.5, g.KORT_X + g.SPEL + g.ANDE - 3.5) for y in (12, 83)]
KX0, KY0 = g.VAGG + g.SPEL + g.ANDE_V, g.FRAM_Y   # kortets nollpunkt i lådan


def box(x, y, z, dx, dy, dz):
    return Part.makeBox(dx, dy, dz, Vector(x, y, z))


def cyl(x, y, z, d, h, axel=Vector(0, 0, 1)):
    return Part.makeCylinder(d / 2, h, Vector(x, y, z), axel)


def rundad_lada(x, y, h, r=3.0, z=0.0):
    b = box(0, 0, z, x, y, h)
    kanter = [e for e in b.Edges if abs(e.Vertexes[0].Point.x - e.Vertexes[1].Point.x) < 1e-6
              and abs(e.Vertexes[0].Point.y - e.Vertexes[1].Point.y) < 1e-6]
    return b.makeFillet(r, kanter)


def rundad_lada_fas(x, y, h, z=0.0, f=None):
    """Som rundad_lada men med fas runt ovansidan (TAK_FAS i .scad)."""
    f = g.TAK_FAS if f is None else f
    b = rundad_lada(x, y, h, z=z)
    topp = [e for e in b.Edges if all(abs(v.Point.z - (z + h)) < 1e-6 for v in e.Vertexes)]
    return b.makeChamfer(f, topp)


def fusion(former):
    s = former[0]
    for f in former[1:]:
        s = s.fuse(f)
    return s.removeSplitter()


def kort(x, y):
    return x + KX0, y + KY0


# ---- Text ----
def textytor(text, storlek):
    """Plana ytor för en textrad, baslinjen i origo. Hål i bokstäver (o, 0, A) hanteras."""
    ut = []
    for tecken in Part.makeWireString(text, TYPSNITT, storlek * TEXT_SKALA, 0):
        ytor = sorted((Part.Face(w) for w in tecken if w.isClosed()), key=lambda f: -f.Area)
        form = None
        for i, f in enumerate(ytor):
            p = f.CenterOfMass if f.isInside(f.CenterOfMass, 1e-6, True) else f.Vertexes[0].Point
            djup = sum(1 for o in ytor[:i] if o.isInside(p, 1e-6, True))
            if form is None:
                form = f
            elif djup % 2:
                form = form.cut(f)
            else:
                form = form.fuse(f)
        if form is not None:
            ut.append(form)
    return Part.Compound(ut)


def ytor_med_hal(slingor):
    """Plana ytor av slutna trådar; trådar inuti andra blir hål (udda nästlingsdjup)."""
    ytor = sorted((Part.Face(w) for w in slingor), key=lambda f: -f.Area)
    form = None
    for i, f in enumerate(ytor):
        p = f.CenterOfMass if f.isInside(f.CenterOfMass, 1e-6, True) else f.Vertexes[0].Point
        djup = sum(1 for o in ytor[:i] if o.isInside(p, 1e-6, True))
        if form is None:
            form = f
        elif djup % 2:
            form = form.cut(f)
        else:
            form = form.fuse(f)
    return form


def logga_2d(fil):
    """Mapro-loggan (eller dess streck) ur en DXF med LINE-segment från OpenSCAD, skalad, förtjockad
    och placerad."""
    data = open(os.path.join(HAR, fil), encoding="utf-8").read().split("\n")
    par = [(data[i].strip(), data[i + 1].strip()) for i in range(0, len(data) - 1, 2)]
    seg, cur = [], None
    for kod, varde in par:
        if kod == "0":
            if cur and len(cur) == 4:
                seg.append(((cur["10"], cur["20"]), (cur["11"], cur["21"])))
            cur = {} if varde == "LINE" else None
        elif cur is not None and kod in ("10", "20", "11", "21"):
            cur[kod] = float(varde)
    # Kedja ihop segmenten till slutna slingor
    nyckel = lambda p: (round(p[0], 4), round(p[1], 4))
    fran = {}
    for a, b in seg:
        fran.setdefault(nyckel(a), []).append(b)
    slingor, anvand = [], set()
    for a, b in seg:
        if (nyckel(a), nyckel(b)) in anvand:
            continue
        pkt, start = [a], nyckel(a)
        p = b
        anvand.add((nyckel(a), nyckel(b)))
        while nyckel(p) != start:
            pkt.append(p)
            nasta = next(q for q in fran[nyckel(p)] if (nyckel(p), nyckel(q)) not in anvand)
            anvand.add((nyckel(p), nyckel(nasta)))
            p = nasta
        slingor.append(pkt + [pkt[0]])
    x, y, vinkel, bredd, tjock = g.LOGGA
    k = bredd / 180   # skala punkterna (en skalad yta blir inte längre plan för FreeCAD)
    form = ytor_med_hal([Part.makePolygon([Vector(px * k, py * k, 0) for px, py in sl]) for sl in slingor])
    if tjock > 0:
        form = Part.Compound([f.makeOffset2D(tjock, join=2) for f in form.Faces])
    form.rotate(Vector(0, 0, 0), Vector(0, 0, 1), vinkel)
    form.translate(Vector(x, y, 0))
    return form


def placera(form, x, y, vinkel=0, halign="left", valign="baseline"):
    bb = form.BoundBox
    dx = -(bb.XMin + bb.XMax) / 2 if halign == "center" else 0
    dy = -(bb.YMin + bb.YMax) / 2 if valign == "center" else 0
    form = form.copy()
    form.translate(Vector(dx, dy, 0))
    form.rotate(Vector(0, 0, 0), Vector(0, 0, 1), vinkel)
    form.translate(Vector(x, y, 0))
    return form


def triangel(x, y):
    return Part.Face(Part.makePolygon([Vector(x - 1.1, y, 0), Vector(x + 1.1, y, 0),
                                       Vector(x, y + 1.8, 0), Vector(x - 1.1, y, 0)]))


def text_2d():
    """Samma text som text_kropp() i .scad, i kortets koordinater."""
    ytor = []
    for namn, x0, x1, _h, txt, stift, ande in g.KANT:
        mitt = (x0 + x1) / 2
        can = namn in ("can1", "can2")
        ytor.append(placera(textytor(txt, 3.8 if can else 3.2), mitt + (1.9 if can else 1.6),
                            5.5 if namn == "plint" else 3.4 if can else 3, 90))
        if stift > 0:
            for i in range(stift):
                nr = stift - i if ande == "h" else i + 1
                px = mitt + (i - (stift - 1) / 2) * 2.0
                if can and nr in (2, 3):
                    ytor.append(placera(textytor("H" if nr == 2 else "L", 2.2), px, 0.3, 0, "center"))
                elif not can and nr == 1:
                    ytor.append(triangel(px, 0.2))
    for o in g.TOPP:
        if o[4]:
            ytor.append(placera(textytor(o[4], 2.8), o[5][0], o[5][1], o[6], "center", "center"))
    ytor.append(placera(textytor("+", 3.6), 95.6, 0.8, 0, "center"))
    ytor.append(placera(textytor("−", 3.6), 100.2, 0.8, 0, "center"))
    ytor.append(triangel(104.05, 42.8))
    for txt, y in (("GPS1", g.SMA_KORT_Y[1]), ("GPS2", g.SMA_KORT_Y[0])):
        ytor.append(placera(textytor(txt, 2.8), g.KORT_X + g.SPEL + g.ANDE / 2 - 0.5, y, 90, "center"))
    if SKARM:
        ytor.append(placera(textytor("▲ = stift 1 (+5 V)", 2.6), 131, 56, 90))
    else:
        ytor.append(placera(textytor("▲ = stift 1 (+5 V)", 3), 40, 84))
    ytor += logga_2d(g.LOGGA_FIL).Faces
    platt = Part.Compound(ytor)
    platt.translate(Vector(KX0, KY0, 0))
    return platt


# ---- Kroppar ----
TOPP_Z = g.BOTTEN_T + 1.2 + g.LOCK_H
TAK_Z = TOPP_Z - g.TAK
Z_STOLPE = TAK_Z - 3
SMA_Y = [y + g.FRAM_Y for y in g.SMA_KORT_Y]
ORON = [(x, y) for x in (-8, g.UT_X + 8) for y in (g.UT_Y * 0.25, g.UT_Y * 0.75)]


def kortsidor(h):
    sidor = box(0, 0, 0, g.VAGG, g.UT_Y, h).fuse(box(g.UT_X - g.VAGG, 0, 0, g.VAGG, g.UT_Y, h))
    return rundad_lada(g.UT_X, g.UT_Y, h).common(sidor)


def botten():
    delar = [rundad_lada(g.UT_X, g.UT_Y, g.BOTTEN_T),
             rundad_lada(g.UT_X, g.UT_Y, g.BOTTEN_T + 1.2).cut(box(g.VAGG, g.VAGG, 0, g.IN_X, g.IN_Y, 10)),
             kortsidor(TAK_Z),
             rundad_lada(g.UT_X, g.UT_Y, g.BOTTEN_H - 0.2).common(box(0, 0, 0, g.UT_X, g.VAGG, g.BOTTEN_H - 0.2))]
    delar += [cyl(*kort(*h), 0, 8, g.BOTTEN_H) for h in g.HAL]
    delar += [cyl(*kort(*p), 0, 8, Z_STOLPE) for p in LOCK_SKRUV]
    for x, y in ORON:
        kant = box(0 if x < 0 else g.UT_X - 1, y - 7, 0, 1, 14, g.BOTTEN_T)
        delar.append(cyl(x, y, 0, 14, g.BOTTEN_T).fuse(kant).fuse(
            box(min(x, 0 if x < 0 else g.UT_X - 1), y - 7, 0, abs(x - (0 if x < 0 else g.UT_X - 1)), 14, g.BOTTEN_T)))
    s = fusion(delar)
    hal = [cyl(*kort(*h), 0.8, g.M3_GANG, 20) for h in g.HAL]
    hal += [cyl(*kort(*p), 1.0, g.M3_GANG, 100) for p in LOCK_SKRUV]
    hal += [cyl(x, y, -1, 5.2, 10) for x, y in ORON]
    hal += [cyl(g.UT_X - g.VAGG - 1, y, g.BOTTEN_H + g.KORT_T + g.SMA_Z, 6.6, g.VAGG + 2, Vector(1, 0, 0)) for y in SMA_Y]
    for h in hal:
        s = s.cut(h)
    return s.removeSplitter()


def lock(text3d):
    z0 = g.BOTTEN_T + 1.2
    skal = rundad_lada_fas(g.UT_X, g.UT_Y, g.LOCK_H, z=z0)
    inre = g.LOCK_H - g.TAK + 1
    for b in (box(g.VAGG, g.VAGG, z0 - 1, g.IN_X, g.IN_Y, inre),
              box(-1, -1, z0 - 1, g.VAGG + 1.3, g.UT_Y + 2, inre),
              box(g.UT_X - g.VAGG - 0.3, -1, z0 - 1, g.VAGG + 1.3, g.UT_Y + 2, inre)):
        skal = skal.cut(b)
    delar = [skal] + [cyl(*kort(*p), Z_STOLPE + 0.2, 8, TOPP_Z - Z_STOLPE - 0.2) for p in LOCK_SKRUV]
    for x0, x1 in g.SKYDD:   # utbuktningen framför kortkanten
        x, y = kort(x0, -g.FRAM_Y - g.SKYDD_D)
        b = box(x, y, 0, x1 - x0, g.SKYDD_D + g.TAK_FAS, TOPP_Z)
        fas = [e for e in b.Edges if all(abs(v.Point.z - TOPP_Z) < 1e-6 for v in e.Vertexes)
               and not all(abs(v.Point.y - (y + g.SKYDD_D + g.TAK_FAS)) < 1e-6 for v in e.Vertexes)]
        delar.append(b.makeChamfer(g.TAK_FAS, fas))   # fram och på sidorna, som taket
    if SKARM:
        x, y = kort(g.SK_MITT[0] - 45, g.KORT_Y + g.SPEL - 2.4)
        delar.append(box(x, y, TAK_Z - g.SK_PCB_UNDER - 2, 90, 2.5, 2))          # list i bakväggen
        delar += [cyl(*kort(*p), TAK_Z - g.SK_PCB_UNDER, 7, g.SK_PCB_UNDER + 0.1) for p in g.SK_KLAMMOR]
    s = fusion(delar)
    hal = [box(-1, -1, 0, g.UT_X + 2, g.VAGG + 1.1, g.BOTTEN_H + g.KORT_T + 0.8)]   # framväggen slutar över kortet
    for _n, x0, x1, h, *_ in g.KANT:
        x, y = kort(x0 - 0.6, -g.FRAM_Y - 1)
        hal.append(box(x, y, g.BOTTEN_H - 0.5, x1 - x0 + 1.2, g.VAGG + 3, g.KORT_T + h + 1.0))
    for o in g.TOPP:
        x, y = kort(o[0] - 0.8, o[2] - 0.8)
        hal.append(box(x, y, 0, o[1] - o[0] + 1.6, o[3] - o[2] + 1.6, 100))
    if SKARM:
        x = g.UT_X * 0.12
        while x <= g.UT_X * 0.88 + 1e-9:     # ventilation i bakväggen
            hal.append(box(x, g.UT_Y - g.VAGG - 1, g.BOTTEN_H + g.KORT_T + 2.5, 3, g.VAGG + 2, 5.5))
            x += 6
        # fönster med fas utåt (stympad pyramid) och ficka för glaset
        mx, my = kort(*g.SK_MITT)
        nere = Part.makePolygon([Vector(mx + sx * (g.SK_AKTIV[0] / 2 + 0.5), my + sy * (g.SK_AKTIV[1] / 2 + 0.5), TAK_Z - 0.01)
                                 for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1), (-1, -1))])
        uppe = Part.makePolygon([Vector(mx + sx * (g.SK_AKTIV[0] / 2 + 2.5), my + sy * (g.SK_AKTIV[1] / 2 + 2.5), TOPP_Z + 0.01)
                                 for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1), (-1, -1))])
        hal.append(Part.makeLoft([nere, uppe], True))
        x, y = kort(g.SK_MITT[0] - g.SK_GLAS[0] / 2 - 0.2, g.SK_MITT[1] + g.SK_GLAS_DY - g.SK_GLAS[1] / 2 - 0.2)
        hal.append(box(x, y, TAK_Z - 1, g.SK_GLAS[0] + 0.4, g.SK_GLAS[1] + 0.4, 1 + g.SK_FICKA))
        hal += [cyl(*kort(*p), TAK_Z - g.SK_PCB_UNDER - 1, g.M3_GANG, g.SK_PCB_UNDER) for p in g.SK_KLAMMOR]
    else:
        x = g.UT_X * 0.12
        while x <= g.UT_X * 0.88 + 1e-9:     # ventilation i bakväggen (taket har loggan)
            hal.append(box(x, g.UT_Y - g.VAGG - 1, g.BOTTEN_H + g.KORT_T + 2.5, 3, g.VAGG + 2, 10))
            x += 6
    for p in LOCK_SKRUV:
        hal.append(cyl(*kort(*p), 0, g.M3_FRI, 100))
        hal.append(cyl(*kort(*p), TOPP_Z - 3, 6.4, 10))
    for h in hal:
        s = s.cut(h)
    s = s.cut(text3d)
    return s.removeSplitter()


def inlagg(platt):
    kropp = Part.Compound([f.extrude(Vector(0, 0, g.TEXT_DJUP)) for f in platt.Faces])
    kropp.translate(Vector(0, 0, TOPP_Z - g.TEXT_DJUP))
    return kropp


def main():
    text3d = inlagg(text_2d())
    streck = logga_2d(g.LOGGA_STRECK)
    streck.translate(Vector(KX0, KY0, 0))
    streck3d = inlagg(streck)
    dok = FreeCAD.newDocument("MP101_lada")
    objekt = []
    delar = [("MP101_botten", botten()), ("MP101_lock", lock(text3d.fuse(streck3d))), ("MP101_text", text3d),
             ("MP101_logga_streck", streck3d)]
    if SKARM:
        for i, p in enumerate(g.SK_KLAMMOR):
            x, y = kort(p[0] - 6, p[1] - 4)
            klamma = box(x, y, TAK_Z - g.SK_PCB_UNDER - 2, 12, 12, 2).cut(cyl(*kort(*p), 0, g.M3_FRI, 100))
            delar.append((f"MP101_skarmklamma_{i + 1}", klamma))
    for namn, form in delar:
        o = dok.addObject("Part::Feature", namn)
        o.Shape = form
        objekt.append(o)
        print(namn, "giltig" if form.isValid() else "OGILTIG", "volym %.0f mm³" % form.Volume)
    Import.export(objekt, UT)
    print("Klar:", UT)


main()
