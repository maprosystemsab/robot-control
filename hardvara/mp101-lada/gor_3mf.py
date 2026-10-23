#!/usr/bin/env python3
"""Bygger mp101_lada.3mf för Bambu Studio: locket med texten som egen del (filament 2) och
bottenplattan, färdigplacerade på plattan.

  python3 gor_3mf.py            (kör OpenSCAD först om STL-filerna saknas eller är äldre än .scad)
  python3 gor_3mf.py --skarm    mp101_lada_skarm.3mf: lock med 4,3"-skärm, klämmorna på platta 2

Locket vänds upp och ner så att texten skrivs i de första lagren mot plattan.
"""

import json
import os
import struct
import subprocess
import sys
import zipfile

HAR = os.path.dirname(os.path.abspath(__file__))
SCAD = os.path.join(HAR, "mp101_lada.scad")
SKARM = "--skarm" in sys.argv
NAMN = "_skarm" if SKARM else ""
UT = os.path.join(HAR, f"mp101_lada{NAMN}.3mf")
PLATTA = 256  # Bambu P2S, mm


def stl(del_):
    fil = os.path.join(HAR, del_ + NAMN + ".stl")
    if not os.path.exists(fil) or os.path.getmtime(fil) < os.path.getmtime(SCAD):
        subprocess.run(["openscad", "-q", "-D", f'del="{del_}"', "-D", f"SKARM={str(SKARM).lower()}",
                        "-o", fil, SCAD], check=True)
    return las_stl(fil)


def las_stl(fil):
    """Läser ASCII- eller binär-STL. Returnerar (hörn, trianglar) med gemensamma hörn."""
    data = open(fil, "rb").read()
    tri = []
    if data[:5] == b"solid" and b"facet" in data[:400]:
        p = []
        for rad in data.decode("ascii", "replace").splitlines():
            rad = rad.strip()
            if rad.startswith("vertex"):
                p.append(tuple(float(v) for v in rad.split()[1:4]))
                if len(p) == 3:
                    tri.append(p)
                    p = []
    else:
        n = struct.unpack("<I", data[80:84])[0]
        for i in range(n):
            v = struct.unpack("<12f", data[84 + 50 * i:84 + 50 * i + 48])
            tri.append([v[3:6], v[6:9], v[9:12]])
    index, horn, ut = {}, [], []
    for t in tri:
        rad = []
        for p in t:
            nyckel = tuple(round(c, 5) for c in p)
            if nyckel not in index:
                index[nyckel] = len(horn)
                horn.append(nyckel)
            rad.append(index[nyckel])
        if len(set(rad)) == 3:
            ut.append(rad)
    return horn, ut


def flytta(nat, f):
    horn, tri = nat
    return [f(*p) for p in horn], tri


def granser(*nat):
    alla = [p for h, _ in nat for p in h]
    return [min(p[i] for p in alla) for i in range(3)], [max(p[i] for p in alla) for i in range(3)]


def mesh_xml(oid, nat):
    horn, tri = nat
    v = "".join(f'<vertex x="{x:.4f}" y="{y:.4f}" z="{z:.4f}"/>' for x, y, z in horn)
    t = "".join(f'<triangle v1="{a}" v2="{b}" v3="{c}"/>' for a, b, c in tri)
    return f'<object id="{oid}" type="model"><mesh><vertices>{v}</vertices><triangles>{t}</triangles></mesh></object>'


def main():
    lock, text, botten = stl("lock"), stl("text"), stl("botten")

    # Locket och texten upp och ner (rotation 180° runt x), samma flytt för båda
    lo, hi = granser(lock, text)
    vand = lambda x, y, z: (x, -y, hi[2] - z)
    lock, text = flytta(lock, vand), flytta(text, vand)
    # Locket på platta 1, framför tvättornet (x 165, y 174). Bottnen, som är enfärgad, på
    # platta 2 så att den slipper tornet. Bambu lägger plattorna 1,2 × plattbredden isär.
    lo, hi = granser(lock, text)
    dx, dy = PLATTA / 2 - (lo[0] + hi[0]) / 2, 15 - lo[1]
    lock = flytta(lock, lambda x, y, z: (x + dx, y + dy, z))
    text = flytta(text, lambda x, y, z: (x + dx, y + dy, z))
    blo, bhi = granser(botten)
    bdx = PLATTA * 1.2 + PLATTA / 2 - (blo[0] + bhi[0]) / 2
    bdy = PLATTA / 2 - (blo[1] + bhi[1]) / 2
    botten = flytta(botten, lambda x, y, z: (x + bdx, y + bdy, z - blo[2]))
    if SKARM:
        # klämmorna liggande framför bottnen på platta 2
        klammor = stl("klammor")
        klo, khi = granser(klammor)
        botten_lo = granser(botten)[0]
        kdx = PLATTA * 1.2 + PLATTA / 2 - (klo[0] + khi[0]) / 2
        kdy = botten_lo[1] - 6 - khi[1]
        klammor = flytta(klammor, lambda x, y, z: (x + kdx, y + kdy, z - klo[2]))

    # Bambu-format: meshen i 3D/Objects/, objekten i 3dmodel.model som komponenter med
    # centrerade mesh och placering i build-transformen. Del-id = mesh-objektets id.
    def centrera(*nat):
        lo, hi = granser(*nat)
        c = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2]
        return [flytta(n, lambda x, y, z: (x - c[0], y - c[1], z - c[2])) for n in nat], c

    (lock_c, text_c), pos_lock = centrera(lock, text)
    (botten_c,), pos_botten = centrera(botten)
    if SKARM:
        (klammor_c,), pos_klammor = centrera(klammor)
    ns = ('xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02" '
          'xmlns:BambuStudio="http://schemas.bambulab.com/package/2021" '
          'xmlns:p="http://schemas.microsoft.com/3dmanufacturing/production/2015/06" requiredextensions="p"')
    huvud = '<?xml version="1.0" encoding="UTF-8"?>\n'

    def objektfil(*mesh):
        return (huvud + f'<model unit="millimeter" xml:lang="en-US" {ns}>'
                '<metadata name="BambuStudio:3mfVersion">1</metadata><resources>'
                + "".join(mesh_xml(oid, n) for oid, n in mesh) + "</resources><build/></model>")

    def komponent(fil, oid):
        return (f'<component p:path="/3D/Objects/{fil}" objectid="{oid}" '
                'transform="1 0 0 0 1 0 0 0 1 0 0 0"/>')

    def plats(c):
        return f"1 0 0 0 1 0 0 0 1 {c[0]:.4f} {c[1]:.4f} {c[2]:.4f}"

    modell = (
        huvud + f'<model unit="millimeter" xml:lang="sv-SE" {ns}>'
        '<metadata name="Application">BambuStudio-02.08.02.61</metadata>'
        '<metadata name="BambuStudio:3mfVersion">1</metadata>'
        '<metadata name="Title">MP101-låda</metadata>'
        "<resources>"
        '<object id="4" type="model"><components>'
        + komponent("object_1.model", 1) + komponent("object_1.model", 2)
        + "</components></object>"
        '<object id="5" type="model"><components>'
        + komponent("object_2.model", 3)
        + "</components></object>"
        + ('<object id="7" type="model"><components>' + komponent("object_3.model", 6)
           + "</components></object>" if SKARM else "")
        + "</resources><build>"
        f'<item objectid="4" transform="{plats(pos_lock)}" printable="1"/>'
        f'<item objectid="5" transform="{plats(pos_botten)}" printable="1"/>'
        + (f'<item objectid="7" transform="{plats(pos_klammor)}" printable="1"/>' if SKARM else "")
        + "</build></model>"
    )
    modell_rels = (huvud + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                   '<Relationship Target="/3D/Objects/object_1.model" Id="rel-1" '
                   'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>'
                   '<Relationship Target="/3D/Objects/object_2.model" Id="rel-2" '
                   'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>'
                   + ('<Relationship Target="/3D/Objects/object_3.model" Id="rel-3" '
                      'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>' if SKARM else "")
                   + "</Relationships>")

    def del_xml(pid, namn, extruder):
        return (f'    <part id="{pid}" subtype="normal_part">\n'
                f'      <metadata key="name" value="{namn}"/>\n'
                '      <metadata key="matrix" value="1 0 0 0 0 1 0 0 0 0 1 0 0 0 0 1"/>\n'
                f'      <metadata key="extruder" value="{extruder}"/>\n'
                "    </part>\n")

    def platta(nr, namn, *oid):
        return (f'  <plate>\n    <metadata key="plater_id" value="{nr}"/>\n'
                f'    <metadata key="plater_name" value="{namn}"/>\n'
                '    <metadata key="locked" value="false"/>\n'
                + "".join(f'    <model_instance>\n      <metadata key="object_id" value="{o}"/>\n'
                          '      <metadata key="instance_id" value="0"/>\n    </model_instance>\n' for o in oid)
                + "  </plate>\n")

    installningar = (
        huvud + "<config>\n"
        '  <object id="4">\n    <metadata key="name" value="MP101 lock"/>\n'
        '    <metadata key="extruder" value="1"/>\n'
        + del_xml(1, "lock", 1) + del_xml(2, "text", 2) + "  </object>\n"
        '  <object id="5">\n    <metadata key="name" value="MP101 botten"/>\n'
        '    <metadata key="extruder" value="1"/>\n'
        + del_xml(3, "botten", 1) + "  </object>\n"
        + ('  <object id="7">\n    <metadata key="name" value="Skärmklämmor"/>\n'
           '    <metadata key="extruder" value="1"/>\n'
           + del_xml(6, "klammor", 1) + "  </object>\n" if SKARM else "")
        + platta(1, "Lock (2 färger)", 4)
        + (platta(2, "Botten + klämmor", 5, 7) if SKARM else platta(2, "Botten", 5)) + "</config>\n")
    typer = (huvud + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
             '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
             '<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>'
             '<Default Extension="png" ContentType="image/png"/>'
             "</Types>")
    rels = (huvud + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Target="/3D/3dmodel.model" Id="rel-1" '
            'Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>'
            "</Relationships>")
    with zipfile.ZipFile(UT, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", typer)
        z.writestr("_rels/.rels", rels)
        z.writestr("3D/3dmodel.model", modell)
        z.writestr("3D/_rels/3dmodel.model.rels", modell_rels)
        z.writestr("3D/Objects/object_1.model", objektfil((1, lock_c), (2, text_c)))
        z.writestr("3D/Objects/object_2.model", objektfil((3, botten_c)))
        if SKARM:
            z.writestr("3D/Objects/object_3.model", objektfil((6, klammor_c)))
        z.writestr("Metadata/model_settings.config", installningar)
        # Projektinställningar för P2S med två filament (orange låda, svart text); mall från en
        # tvåfärgsfil sparad i Bambu Studio 2.7.
        projekt = json.load(open(os.path.join(HAR, "bambu_p2s_projekt.json")))
        projekt["filament_colour"] = ["#FF6A13", "#000000"]
        # Skarp text (den skrivs i de första lagren, mot plattan): tunt, smalt och långsamt
        # första lager, och mer rensning så att svart inte stänker in i orange.
        projekt["initial_layer_print_height"] = "0.16"
        projekt["initial_layer_line_width"] = "0.42"
        projekt["initial_layer_speed"] = ["30", "30"]
        projekt["elefant_foot_compensation"] = "0.15"
        projekt["flush_multiplier"] = ["1.3"]
        andrade = "initial_layer_print_height;initial_layer_line_width;initial_layer_speed"
        skillnad = projekt["different_settings_to_system"]
        skillnad[0] = ";".join(sorted(set(filter(None, skillnad[0].split(";") + andrade.split(";")))))
        z.writestr("Metadata/project_settings.config", json.dumps(projekt, indent=4))
    print("Klar:", UT)


if __name__ == "__main__":
    main()
