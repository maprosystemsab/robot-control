"""Gemensam stil för Mapro Systems handböcker (reportlab). Används av gor_manualer.py.

Innehållet skrivs som en lista med enkla anrop: h1(), h2(), p(), punkter(), tabell(),
bild(), tips(), varning(), kod(). Fetstil i text: <b>...</b>, kursiv: <i>...</i>.
"""

import os


def standard_utmapp():
    """Där PDF:erna hamnar: ~/Robotstyrning/manualer/Mapro Manualer på kontorsdatorn
    (sedan 2026-10-31), annars ~/Hämtningar/Mapro Manualer."""
    ny = os.path.expanduser("~/Robotstyrning/manualer/Mapro Manualer")
    if os.path.isdir(os.path.dirname(ny)):
        return ny
    return os.path.expanduser("~/Hämtningar/Mapro Manualer")
from datetime import date

from reportlab.lib import colors
from reportlab.lib.enums import TA_LEFT
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import ParagraphStyle
from reportlab.lib.units import mm
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
from reportlab.platypus import (
    BaseDocTemplate, Frame, Image, KeepTogether, ListFlowable, ListItem, PageBreak,
    PageTemplate, Paragraph, Spacer, Table, TableStyle,
)
from reportlab.platypus.tableofcontents import TableOfContents

FONT_DIR = "/usr/share/fonts/truetype/dejavu"
pdfmetrics.registerFont(TTFont("Sans", os.path.join(FONT_DIR, "DejaVuSans.ttf")))
pdfmetrics.registerFont(TTFont("Sans-Bold", os.path.join(FONT_DIR, "DejaVuSans-Bold.ttf")))
_KURSIV = next((f for f in ("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf",
                             "/usr/share/fonts/truetype/liberation2/LiberationSans-Italic.ttf",
                             "/usr/share/fonts/truetype/liberation/LiberationSans-Italic.ttf") if os.path.exists(f)),
               os.path.join(FONT_DIR, "DejaVuSans.ttf"))
pdfmetrics.registerFont(TTFont("Sans-Oblique", _KURSIV))
pdfmetrics.registerFont(TTFont("Mono", os.path.join(FONT_DIR, "DejaVuSansMono.ttf")))
pdfmetrics.registerFontFamily("Sans", normal="Sans", bold="Sans-Bold", italic="Sans-Oblique", boldItalic="Sans-Bold")

ACCENT = colors.HexColor("#1d6f42")      # grön, som roboten
ACCENT_LIGHT = colors.HexColor("#e8f3ec")
WARN = colors.HexColor("#b3261e")
WARN_LIGHT = colors.HexColor("#fbeaea")
TIP_LIGHT = colors.HexColor("#eef4fb")
TIP = colors.HexColor("#1f5fa8")
GREY = colors.HexColor("#5f6368")
CODE_BG = colors.HexColor("#f3f3f3")

BILDER = os.path.join(os.path.dirname(os.path.abspath(__file__)), "bilder")

S = {
    "body": ParagraphStyle("body", fontName="Sans", fontSize=10, leading=14.5, spaceAfter=6, alignment=TA_LEFT),
    "h1": ParagraphStyle("h1", fontName="Sans-Bold", fontSize=17, leading=22, textColor=ACCENT, spaceBefore=6, spaceAfter=8, keepWithNext=1),
    "h2": ParagraphStyle("h2", fontName="Sans-Bold", fontSize=12.5, leading=17, textColor=colors.black, spaceBefore=10, spaceAfter=4, keepWithNext=1),
    "h3": ParagraphStyle("h3", fontName="Sans-Bold", fontSize=10.5, leading=14, textColor=GREY, spaceBefore=6, spaceAfter=2, keepWithNext=1),
    "cell": ParagraphStyle("cell", fontName="Sans", fontSize=9, leading=12),
    "cellb": ParagraphStyle("cellb", fontName="Sans-Bold", fontSize=9, leading=12, textColor=colors.white),
    "code": ParagraphStyle("code", fontName="Mono", fontSize=9, leading=12.5),
    "caption": ParagraphStyle("caption", fontName="Sans-Oblique", fontSize=8.5, leading=11, textColor=GREY, spaceAfter=8),
    "title": ParagraphStyle("title", fontName="Sans-Bold", fontSize=28, leading=34, textColor=ACCENT),
    "subtitle": ParagraphStyle("subtitle", fontName="Sans", fontSize=14, leading=19, textColor=GREY),
    "toc1": ParagraphStyle("toc1", fontName="Sans", fontSize=10.5, leading=16, leftIndent=0),
    "toc2": ParagraphStyle("toc2", fontName="Sans", fontSize=9.5, leading=13, leftIndent=14, textColor=GREY),
}

BREDD = A4[0] - 40 * mm


def h1(t):
    return Paragraph(t, S["h1"])


def h2(t):
    return Paragraph(t, S["h2"])


def h3(t):
    return Paragraph(t, S["h3"])


def p(t):
    return Paragraph(t, S["body"])


def punkter(items, numrerad=False):
    flow = [ListItem(Paragraph(i, S["body"]), leftIndent=14, value=None) for i in items]
    if numrerad:
        return ListFlowable(flow, bulletType="1", bulletFontName="Sans-Bold", bulletFontSize=9.5, leftIndent=16)
    return ListFlowable(flow, bulletType="bullet", start="•", bulletFontName="Sans", leftIndent=14)


def tabell(rader, bredder=None, rubrik=True):
    data = []
    for i, r in enumerate(rader):
        st = S["cellb"] if (rubrik and i == 0) else S["cell"]
        data.append([Paragraph(str(c), st) for c in r])
    n = len(rader[0])
    if bredder is None:
        bredder = [BREDD / n] * n
    else:
        tot = sum(bredder)
        bredder = [BREDD * b / tot for b in bredder]
    t = Table(data, colWidths=bredder, repeatRows=1 if rubrik else 0)
    stil = [
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("GRID", (0, 0), (-1, -1), 0.4, colors.HexColor("#c8c8c8")),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
        ("ROWBACKGROUNDS", (0, 1 if rubrik else 0), (-1, -1), [colors.white, colors.HexColor("#f7f9f8")]),
    ]
    if rubrik:
        stil.append(("BACKGROUND", (0, 0), (-1, 0), ACCENT))
    t.setStyle(TableStyle(stil))
    return KeepTogether([t, Spacer(1, 8)]) if len(rader) < 12 else t


def _ruta(text, kant, bak, rubrik):
    inner = Paragraph(f"<b>{rubrik}</b> {text}", S["body"])
    t = Table([[inner]], colWidths=[BREDD])
    t.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, -1), bak),
        ("LINEBEFORE", (0, 0), (0, -1), 3, kant),
        ("LEFTPADDING", (0, 0), (-1, -1), 10),
        ("RIGHTPADDING", (0, 0), (-1, -1), 8),
        ("TOPPADDING", (0, 0), (-1, -1), 6),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 2),
    ]))
    return KeepTogether([t, Spacer(1, 8)])


def tips(text):
    return _ruta(text, TIP, TIP_LIGHT, "Tips:")


def varning(text):
    return _ruta(text, WARN, WARN_LIGHT, "Viktigt:")


def kod(text):
    rader = "<br/>".join(
        r.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace(" ", "&nbsp;")
        for r in text.strip("\n").split("\n")
    )
    t = Table([[Paragraph(rader, S["code"])]], colWidths=[BREDD])
    t.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, -1), CODE_BG),
        ("BOX", (0, 0), (-1, -1), 0.4, colors.HexColor("#d0d0d0")),
        ("LEFTPADDING", (0, 0), (-1, -1), 8),
        ("TOPPADDING", (0, 0), (-1, -1), 6),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 6),
    ]))
    return KeepTogether([t, Spacer(1, 8)])


def bild(fil, bredd_mm=170, text=None):
    path = os.path.join(BILDER, fil)
    from PIL import Image as PImage
    w, h = PImage.open(path).size
    bw = min(bredd_mm * mm, BREDD)
    img = Image(path, width=bw, height=bw * h / w)
    img.hAlign = "CENTER"
    delar = [img]
    if text:
        delar.append(Spacer(1, 3))
        delar.append(Paragraph(text, S["caption"]))
    else:
        delar.append(Spacer(1, 8))
    return KeepTogether(delar)


def sidbryt():
    return PageBreak()


class _Doc(BaseDocTemplate):
    def __init__(self, fil, titel, **kw):
        super().__init__(fil, pagesize=A4, leftMargin=20 * mm, rightMargin=20 * mm,
                         topMargin=20 * mm, bottomMargin=18 * mm, title=titel,
                         author="Mapro System", **kw)
        self.titel = titel
        frame = Frame(self.leftMargin, self.bottomMargin, self.width, self.height, id="f")
        self.addPageTemplates([
            PageTemplate(id="forsta", frames=[frame], onPage=self._forsta),
            PageTemplate(id="vanlig", frames=[frame], onPage=self._sida),
        ])

    def _forsta(self, c, doc):
        c.saveState()
        c.setFillColor(ACCENT)
        c.rect(0, A4[1] - 14 * mm, A4[0], 14 * mm, stroke=0, fill=1)
        c.setFillColor(colors.white)
        c.setFont("Sans-Bold", 11)
        c.drawString(20 * mm, A4[1] - 9 * mm, "MAPRO SYSTEM")
        c.restoreState()

    def _sida(self, c, doc):
        c.saveState()
        c.setStrokeColor(ACCENT)
        c.setLineWidth(0.6)
        c.line(20 * mm, A4[1] - 13 * mm, A4[0] - 20 * mm, A4[1] - 13 * mm)
        c.setFont("Sans", 8)
        c.setFillColor(GREY)
        c.drawString(20 * mm, A4[1] - 11 * mm, "Mapro System")
        c.drawRightString(A4[0] - 20 * mm, A4[1] - 11 * mm, self.titel)
        c.drawRightString(A4[0] - 20 * mm, 10 * mm, f"Sida {doc.page}")
        c.drawString(20 * mm, 10 * mm, f"Version {date.today().isoformat()}")
        c.restoreState()

    def afterFlowable(self, f):
        if isinstance(f, Paragraph) and f.style.name in ("h1", "h2"):
            niva = 0 if f.style.name == "h1" else 1
            self.notify("TOCEntry", (niva, f.getPlainText(), self.page))


def bygg(fil, titel, undertitel, ingress, innehall, innehallsforteckning=True):
    from reportlab.platypus import NextPageTemplate
    doc = _Doc(fil, titel)
    story = [
        Spacer(1, 30 * mm),
        Paragraph(titel, S["title"]),
        Spacer(1, 4 * mm),
        Paragraph(undertitel, S["subtitle"]),
        Spacer(1, 10 * mm),
        p(ingress),
        Spacer(1, 6 * mm),
        p(f"<font color='#5f6368'>Mapro System · version {date.today().isoformat()}</font>"),
        NextPageTemplate("vanlig"),
    ]
    if innehallsforteckning:
        toc = TableOfContents()
        toc.levelStyles = [S["toc1"], S["toc2"]]
        story += [PageBreak(), Paragraph("Innehåll", S["h1"].clone("tocrub")), toc]
    story += [PageBreak()] + _rubriker_ihop(innehall)
    doc.multiBuild(story)


def _rubriker_ihop(innehall):
    """Rubrik + nästa block hålls på samma sida (keepWithNext räcker inte före tabeller)."""
    ut, i = [], 0
    while i < len(innehall):
        grupp = []
        while i < len(innehall) and isinstance(innehall[i], Paragraph) and innehall[i].style.name in ("h1", "h2", "h3"):
            grupp.append(innehall[i])
            i += 1
        if grupp and i < len(innehall):
            grupp.append(innehall[i])
            i += 1
            ut.append(KeepTogether(grupp))
        elif grupp:
            ut += grupp
        else:
            ut.append(innehall[i])
            i += 1
    return ut
