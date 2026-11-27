#!/usr/bin/env python3
"""Ritar SentinAnt Lites logga och ikon ur SentinAnts: samma bilder, med ett tunt grönt
"Lite" snett över.

    python3 client/assets/gor_bilder.py [MAPP_FÖR_PROVBILD]

Läser sentinant_logga.png och sentinant_ikon.png (kopior av SentinAnts client/assets/logga.png
och ikon.png) och skriver logga.png, ikon.png (256 px) och ikon.ico (16–256 px) i samma mapp.
Med en mapp som argument sparas också ikon_storlekar.png där: ikonen i 256, 48 och 32 px
bredvid varandra. Kör igen om SentinAnts logga eller ikon ändras. Kräver PIL (python3-pil)
och typsnitten DejaVu Sans Bold och Ubuntu (båda följer med Ubuntu).

Ordet är satt i en tunn snittvikt (Ubuntu Light Italic) med en hårfin mörk kant, så att
loggan syns mellan strecken. Läget, vinkeln och storleken bestäms av en mall: ordet i
loggans eget feta typsnitt (DejaVu Sans Bold), som aldrig ritas ut. Det tunna ordet får
samma höjd och bredd som mallen och läggs på samma plats.

Ikonen ritas för sig i varje storlek (inte bara förminskad). I 64, 48 och 32 px är
snittvikten stegvis kraftigare, annars försvinner strecken. I 16 och 24 px får ordet inte
plats: där är märket en grön snedrand.
"""
import io
import os
import struct
import sys
from PIL import Image, ImageDraw, ImageFont

HAR = os.path.dirname(os.path.abspath(__file__))
MALLSNITT = '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'
UBUNTU = '/usr/share/fonts/truetype/ubuntu/'
TUNN, NORMAL, HALVFET, FET = (UBUNTU + n for n in ('Ubuntu-LI.ttf', 'Ubuntu-RI.ttf', 'Ubuntu-MI.ttf', 'Ubuntu-BI.ttf'))
GRON = (64, 214, 100, 255)      # ordet "Lite", i loggan och i ikonen
KANT = (4, 22, 10, 255)         # hårfin mörk kant, så att ordet går att läsa över vitt och orange
STORLEKAR = [16, 24, 32, 48, 64, 128, 256]
ORD = 'Lite'


def rita_ord(typsnitt, storlek, mellan, farg, kant=0):
    """Ordet på en egen duk, med `mellan` bildpunkter extra mellan bokstäverna."""
    font = ImageFont.truetype(typsnitt, storlek)
    rita = ImageDraw.Draw(Image.new('RGBA', (8, 8)))
    bredder = [rita.textlength(c, font=font) for c in ORD]
    upp, ned = font.getmetrics()
    marginal = storlek // 2
    bild = Image.new('RGBA', (int(sum(bredder) + mellan * (len(ORD) - 1)) + 2 * marginal, upp + ned + 2 * marginal), (0, 0, 0, 0))
    rita = ImageDraw.Draw(bild)
    x = marginal
    for tecken, bredd in zip(ORD, bredder):
        rita.text((x, marginal), tecken, font=font, fill=farg, stroke_width=kant, stroke_fill=farg)
        x += bredd + mellan
    return bild


def mall(storlek, mellan, lutning=0.2):
    """Mallen: ordet i loggans feta typsnitt, kursiverat (skjuvat). Ger duken det ligger på."""
    bild = rita_ord(MALLSNITT, storlek, mellan, GRON)
    return bild.transform(bild.size, Image.AFFINE, (1, lutning, -lutning * bild.height / 2, 0, 1, 0), resample=Image.BICUBIC)


def tunt_ord(typsnitt, mallen, kant):
    """Ordet i `typsnitt` på en duk som mallens, med samma höjd, bredd och läge som mallens ord."""
    ruta = mallen.getbbox()
    bredd, hojd = ruta[2] - ruta[0], ruta[3] - ruta[1]
    prov = rita_ord(typsnitt, 400, 0, GRON).getbbox()
    storlek = round(400 * hojd / (prov[3] - prov[1]))
    utan = rita_ord(typsnitt, storlek, 0, GRON).getbbox()
    mellan = (bredd - (utan[2] - utan[0])) / (len(ORD) - 1)
    ordet = rita_ord(typsnitt, storlek, mellan, GRON)
    r = ordet.getbbox()
    flytt = (round((ruta[0] + ruta[2] - r[0] - r[2]) / 2), round((ruta[1] + ruta[3] - r[1] - r[3]) / 2))
    ut = Image.new('RGBA', mallen.size, (0, 0, 0, 0))
    for lager in ([rita_ord(typsnitt, storlek, mellan, KANT, kant)] if kant else []) + [ordet]:
        hel = Image.new('RGBA', mallen.size, (0, 0, 0, 0))
        hel.paste(lager, flytt, lager)
        ut = Image.alpha_composite(ut, hel)
    return ut


def mitt_pa(bas, lager, dx=0, dy=0):
    hel = Image.new('RGBA', bas.size, (0, 0, 0, 0))
    hel.paste(lager, ((bas.width - lager.width) // 2 + dx, (bas.height - lager.height) // 2 + dy), lager)
    return Image.alpha_composite(bas, hel)


def logga():
    bas = Image.open(os.path.join(HAR, 'sentinant_logga.png')).convert('RGBA')
    ordet = tunt_ord(TUNN, mall(236, 14), kant=3).rotate(12, expand=True, resample=Image.BICUBIC)
    mitt_pa(bas, ordet, dy=-4).convert('RGB').save(os.path.join(HAR, 'logga.png'), optimize=True)


def ikon(sida):
    """Ikonen i en storlek. Ritas fyra gånger så stor och förminskas, för jämna kanter."""
    grund = Image.open(os.path.join(HAR, 'sentinant_ikon.png')).convert('RGBA')
    k = 1 if sida >= 256 else 4
    b = sida * k
    bas = grund.resize((b, b), Image.LANCZOS)
    if sida >= 32:
        # Mallens storlek och vinkel per ikonstorlek, och snittvikten: tunn som i loggan i de
        # stora, stegvis kraftigare ned till 32 px, där tunna streck annars inte syns alls.
        andel, vinkel, typsnitt, kant = {32: (0.40, 20, FET, 0.012), 48: (0.39, 21, HALVFET, 0.011), 64: (0.38, 22, NORMAL, 0.010)}.get(sida, (0.38, 22, TUNN, 0.008))
        mallen = mall(int(b * andel), 0)
        ordet = tunt_ord(typsnitt, mallen, max(1, round(b * kant))).rotate(vinkel, expand=True, resample=Image.BICUBIC)
        # Mallens ord (vridet) hamnar mitt på ikonen, något under mitten; det tunna ordet följer med.
        r = mallen.rotate(vinkel, expand=True, resample=Image.BICUBIC).getbbox()
        bas = mitt_pa(bas, ordet, dx=round(ordet.width / 2 - (r[0] + r[2]) / 2), dy=round(ordet.height / 2 - (r[1] + r[3]) / 2) + int(b * 0.06))
    else:
        # 16 och 24 px: en grön snedrand med mörk kant, klippt till ikonens form.
        rand = Image.new('RGBA', (b, b), (0, 0, 0, 0))
        rita = ImageDraw.Draw(rand)
        rita.line([(-b * 0.1, b * 0.86), (b * 1.1, b * 0.42)], fill=KANT, width=int(b * 0.34))
        rita.line([(-b * 0.1, b * 0.86), (b * 1.1, b * 0.42)], fill=GRON, width=int(b * 0.22))
        rand.putalpha(Image.composite(rand.split()[3], Image.new('L', (b, b), 0), bas.split()[3]))
        bas = Image.alpha_composite(bas, rand)
    return bas.resize((sida, sida), Image.LANCZOS) if k > 1 else bas


def ico(ikoner):
    """ikon.ico för Windows, uppbyggd som SentinAnts: de små storlekarna som vanliga
    bitmappar (det äldre delar av Windows och NSIS väntar sig) och 256 px som PNG."""
    def bild(sida, form):
        # PIL skriver en ico med en enda storlek; bilddatat plockas ur den.
        buf = io.BytesIO()
        ikoner[sida].save(buf, format='ICO', sizes=[(sida, sida)], bitmap_format=form)
        d = buf.getvalue()
        post = bytearray(d[6:22])
        langd, plats = struct.unpack('<II', post[8:16])
        return post, d[plats:plats + langd]
    delar = [bild(s, 'png' if s >= 256 else 'bmp') for s in STORLEKAR]
    ut = bytearray(struct.pack('<HHH', 0, 1, len(delar)))
    plats = 6 + 16 * len(delar)
    for post, data in delar:
        post[12:16] = struct.pack('<I', plats)
        ut += post
        plats += len(data)
    for _, data in delar:
        ut += data
    open(os.path.join(HAR, 'ikon.ico'), 'wb').write(ut)


def main():
    logga()
    ikoner = {s: ikon(s) for s in STORLEKAR}
    ikoner[256].save(os.path.join(HAR, 'ikon.png'), optimize=True)
    ico(ikoner)
    print('Skrev logga.png, ikon.png och ikon.ico i', HAR)
    if len(sys.argv) > 1:
        # Provbild: 256, 48 och 32 px bredvid varandra, på mörk och ljus botten.
        prov = Image.new('RGBA', (256 + 48 + 32 + 4 * 24, 2 * 256 + 3 * 24), (30, 30, 30, 255))
        ImageDraw.Draw(prov).rectangle([0, 256 + 36, prov.width, prov.height], fill=(236, 236, 236, 255))
        for rad in (0, 1):
            x = 24
            for s in (256, 48, 32):
                prov.alpha_composite(ikoner[s], (x, 24 + rad * (256 + 24) + (256 - s) // 2))
                x += s + 24
        ut = os.path.join(sys.argv[1], 'ikon_storlekar.png')
        prov.convert('RGB').save(ut)
        print('Skrev', ut)


if __name__ == '__main__':
    main()
