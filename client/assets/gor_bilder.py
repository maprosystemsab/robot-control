#!/usr/bin/env python3
"""Ritar SentinAnt Lites logga och ikon ur SentinAnts: samma bilder, med ett grönt "Lite"
snett över.

    python3 client/assets/gor_bilder.py [MAPP_FÖR_PROVBILD]

Läser sentinant_logga.png och sentinant_ikon.png (kopior av SentinAnts client/assets/logga.png
och ikon.png) och skriver logga.png, ikon.png (256 px) och ikon.ico (16–256 px) i samma mapp.
Med en mapp som argument sparas också ikon_storlekar.png där: ikonen i 256, 48 och 32 px
bredvid varandra. Kör igen om SentinAnts logga eller ikon ändras. Kräver PIL (python3-pil)
och typsnittet DejaVu Sans Bold, som är det loggan är satt i.

Ikonen ritas för sig i varje storlek (inte bara förminskad), så att ordet går att läsa även
i 32 och 48 px. I 16 och 24 px får ordet inte plats: där är märket en grön snedrand.
"""
import io
import os
import struct
import sys
from PIL import Image, ImageDraw, ImageFilter, ImageFont

HAR = os.path.dirname(os.path.abspath(__file__))
TYPSNITT = '/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'
GRON = (64, 214, 100, 255)      # ordet "Lite", i loggan och i ikonen
KANT = (4, 22, 10, 255)         # mörk kant runt ordet, så att det går att läsa över vitt och orange
STORLEKAR = [16, 24, 32, 48, 64, 128, 256]


def text(ord_, storlek, farg, kant=0, mellan=0, lutning=0.2):
    """Ordet som eget lager, kursiverat (skjuvat). Lagren får samma storlek oavsett kant,
    så att flera lager av samma ord passar på varandra."""
    font = ImageFont.truetype(TYPSNITT, storlek)
    rita = ImageDraw.Draw(Image.new('RGBA', (8, 8)))
    bredder = [rita.textlength(c, font=font) for c in ord_]
    upp, ned = font.getmetrics()
    marginal = storlek // 2
    bild = Image.new('RGBA', (int(sum(bredder) + mellan * (len(ord_) - 1)) + 2 * marginal, upp + ned + 2 * marginal), (0, 0, 0, 0))
    rita = ImageDraw.Draw(bild)
    x = marginal
    for tecken, bredd in zip(ord_, bredder):
        rita.text((x, marginal), tecken, font=font, fill=farg, stroke_width=kant, stroke_fill=farg)
        x += bredd + mellan
    return bild.transform(bild.size, Image.AFFINE, (1, lutning, -lutning * bild.height / 2, 0, 1, 0), resample=Image.BICUBIC)


def marke(storlek, kant, mellan, vinkel, skugga):
    """Det gröna ordet med mörk kant, vridet, och (om skugga > 0) en mjuk mörk skugga bakom."""
    vrid = lambda lager: lager.rotate(vinkel, expand=True, resample=Image.BICUBIC)
    ord_ = Image.alpha_composite(vrid(text('Lite', storlek, KANT, kant, mellan)), vrid(text('Lite', storlek, GRON, 0, mellan)))
    if skugga:
        moln = Image.new('RGBA', ord_.size, (0, 0, 0, 0))
        moln.putalpha(ord_.split()[3].filter(ImageFilter.GaussianBlur(skugga)).point(lambda a: min(255, a * 2)))
        ord_ = Image.alpha_composite(moln, ord_)
    return ord_


def mitt_pa(bas, lager, dx=0, dy=0):
    hel = Image.new('RGBA', bas.size, (0, 0, 0, 0))
    hel.paste(lager, ((bas.width - lager.width) // 2 + dx, (bas.height - lager.height) // 2 + dy), lager)
    return Image.alpha_composite(bas, hel)


def logga():
    bas = Image.open(os.path.join(HAR, 'sentinant_logga.png')).convert('RGBA')
    mitt_pa(bas, marke(storlek=236, kant=11, mellan=14, vinkel=12, skugga=10), dy=-4).convert('RGB').save(os.path.join(HAR, 'logga.png'), optimize=True)


def ikon(sida):
    """Ikonen i en storlek. Ritas fyra gånger så stor och förminskas, för jämna kanter."""
    grund = Image.open(os.path.join(HAR, 'sentinant_ikon.png')).convert('RGBA')
    k = 1 if sida >= 256 else 4
    b = sida * k
    bas = grund.resize((b, b), Image.LANCZOS)
    if sida >= 32:
        # Mindre ikon: större ord och kraftigare kant i förhållande till ytan.
        andel, kant, vinkel = {32: (0.40, 0.045, 20), 48: (0.39, 0.04, 21)}.get(sida, (0.38, 0.032, 22))
        ord_ = marke(storlek=int(b * andel), kant=max(1, int(b * kant)), mellan=0, vinkel=vinkel, skugga=int(b * 0.02))
        ord_ = ord_.crop(ord_.getbbox())
        bas = mitt_pa(bas, ord_, dy=int(b * 0.06))
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
