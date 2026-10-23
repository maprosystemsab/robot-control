// Låda för Upwis MP101/ROV_MCU-kortet, med märkning av alla kontakter och stift 1.
//
// Koordinater i mm från kortets hörn: x längs långsidan med kontaktraden (0 = Ethernet-änden),
// y in från kontaktkanten. Lägen uppmätta ur Upwis placeringsritning (MP101_323_TOP_CAM).
// Höjderna uppmätta på kort 2 (2026-10-23), från kortets underkant minus kortets tjocklek.
// Bottnen har båda kortsidorna (SMA-skotten sitter i den ena), locket är tak + långsidor.
// Montering: kortet i bottnen (4 × M3×6), SMA-kablarna, sist locket (4 × M3×12 i pelarna vid
// kortsidorna). Allt skruvas uppifrån och gängar sig i plasten (hål 3,0 mm).
//
// SKARM = true: locket får en inbyggd Waveshare 4,3" DSI-pekskärm (800×480, samma som MacBot)
// över kortets vänstra halva. Locket blir 4 mm högre, Ethernet-änden 7 mm längre och
// ventilationen flyttas till bakväggen. Skärmen läggs i en ficka i takets undersida, bakkanten
// under en list i bakväggen, framkanten hålls av två klämmor (del "klammor", 2 × M3×8).
// Skärmens M2.5-distanser monteras inte. Skärmmått ur Waveshares STEP (4_3inch-dsi-lcd-V1).
//
// Exportera delarna (del = "botten", "lock", "text", "klammor"):
//   openscad -D 'del="botten"' -D SKARM=true -o botten.stl mp101_lada.scad
// Färdig Bambu-fil med texten i egen färg: python3 gor_3mf.py [--skarm]

del = "alla";           // "botten", "lock", "text", "klammor" eller "alla" (förhandsvisning)
SKARM = false;          // inbyggd 4,3"-skärm i locket

// ---- Kortet ----
KORT_X = 160;
KORT_Y = 95;
KORT_T = 1.6;
HAL = [[4.0, 90.6], [156.0, 90.6], [155.6, 13.4], [85.8, 4.6]];   // M3-hål i kortet

// ---- Höjder över kortets ovansida (uppmätta) ----
H_LOCK_INV = SKARM ? 20 : 16;   // fri höjd under locket (USB-A är högst, 14,4; skärmen tar 10,4)
H_RJ45 = 11.4;
H_USB = 14.4;
H_USBC = 6.5;           // hålets höjd: kontaktens gjutna hylsa (uttaget självt är 3,4)
H_PLINT = 10;           // hålets höjd: plinten med kontakt, skruvar och kablar (plinten själv 7,1)
H_JST = 5.4;            // JST PH liggande
H_UNDER = 5;            // frigång under kortet (provat 23/10: 3 mm var för lite)
M3_GANG = 3.0;          // M3 som gängar sig i plasten (2,8 var för trångt i PLA, provat 23/10)
M3_FRI = 3.4;

// ---- Låda ----
VAGG = 2.2;
TAK = 2.0;
SPEL = 0.6;             // luft runt kortet
ANDE = 9;               // extra plats i varje kortände för lockets skruvar
ANDE_V = SKARM ? ANDE + 7 : ANDE;   // Ethernet-änden: med skärm ryms den mellan lockets skruvar
TEXT_DJUP = 0.6;        // inlagd text i taket (egen färg)
FONT = "DejaVu Sans:style=Bold";

IN_X = KORT_X + 2 * SPEL + ANDE + ANDE_V;
FRAM_Y = 0;             // kortets kontaktkant jäms med lådans utsida (kontakterna i nivå med väggen)
UT_Y = FRAM_Y + KORT_Y + SPEL + VAGG;
IN_Y = UT_Y - 2 * VAGG;
UT_X = IN_X + 2 * VAGG;
BOTTEN_T = 2.4;
BOTTEN_H = BOTTEN_T + H_UNDER;          // bottenplattans överkant = kortets underkant
LOCK_H = KORT_T + H_LOCK_INV + TAK;     // lockets väggar står på bottnens kant
TOPP_Z = BOTTEN_T + 1.2 + LOCK_H;       // lockets ovansida
TAK_Z = TOPP_Z - TAK;                   // takets undersida = kortsidornas överkant
Z_STOLPE = TAK_Z - 3;                   // pelarna slutar här, locket har en klack ner till dem
// lockets skruvar i pelare som sitter ihop med kortsidorna (kortets koordinater)
LOCK_SKRUV = [for (x = [-SPEL - ANDE_V + 3.5, KORT_X + SPEL + ANDE - 3.5], y = [12, 83]) [x, y]];
// SMA-skotten i kortsidan vid DI1 (kortets y): GPS2 vid X2, GPS1 vid X1, båda fria från PROG-hålet
SMA_KORT_Y = [25, 72];
SMA_Y = [for (y = SMA_KORT_Y) y + FRAM_Y];   // i lådans koordinater
SMA_Z = 10;             // SMA-skottens mitt över kortets ovansida (fritt från knappcellshållaren)

// Kontakter längs kontaktkanten: [namn, x0, x1, höjd, text, stift, stift 1-ände ("h" = hög x)]
KANT = [
    ["eth",   1.2, 19.0, H_RJ45,  "ETH",       0, ""],
    ["usb",  44.5, 69.7, H_USB,   "USB",       0, ""],
    ["usbc", 71.7, 83.6, H_USBC,  "USB-C",     0, ""],
    ["plint",92.6,102.1, H_PLINT, "12V IN",    0, ""],
    ["can1",104.4,114.5, H_JST,   "CAN1 VESC", 4, "h"],
    ["can2",115.2,125.2, H_JST,   "CAN2",      4, "h"],
    ["di4", 126.1,134.1, H_JST,   "DI4 HJUL",  3, "h"],
    ["di3", 134.5,142.5, H_JST,   "DI3",       3, "h"],
    ["di2", 142.9,150.9, H_JST,   "DI2",       3, "h"],
    ["di1", 151.2,159.2, H_JST,   "DI1",       3, "h"],
];

// Öppningar i taket: [x0, x1, y0, y1, text, textläge [x, y], textvinkel]
TOPP = [
    [101.8, 106.3, 46.1, 58.0, "VINKEL J30", [98.6, 52], 90],
    [144.5, 156.5, 39.2, 43.9, "I2C",        [150.5, 35.0], 0],
    [145.1, 153.7, 44.3, 64.9, "",           [0, 0], 0],     // LED-fönster
    [154.4, 158.9, 47.0, 61.0, "PROG",       [150.0, 68.0], 0],
    [142.0, 152.1, 89.9, 94.4, "RS232",      [147.0, 85.5], 0],
];
CM5 = [44, 102, 39, 83];   // ventilationsgaller över CM5 (utan skärm)

// Mapro-loggan (2D-snitt av väskans logga, 180 mm bred, centrerad): [x, y, vinkel, bredd, förtjockning]
// i kortets koordinater. Förtjockningen håller de tunnaste linjerna (SYSTEMS AB) utskrivbara.
LOGGA_FIL = "mapro_logga.dxf";
LOGGA = SKARM ? [118.5, 64.5, 90, 60, 0.1] : [45, 62, 0, 90, 0.05];

// ---- Skärmen (kortets koordinater) ----
SK_GLAS = [106.1, 67.8];               // glasets yttermått
SK_AKTIV = [95.5, 54.4];               // synlig bildyta, centrerad i SK_MITT
SK_GLAS_DY = -2.37;                    // glasets mitt ligger så här mycket mot kontaktkanten
SK_MITT = [-7.6 + SK_GLAS[0] / 2, 27 + SK_GLAS[1] / 2 - SK_GLAS_DY];
SK_FICKA = 0.8;                        // glaset sjunker in så här mycket i takets undersida
SK_PCB_UNDER = 8.14 - SK_FICKA;        // skärmkortets baksida under takets undersida
SK_KLAMMOR = [[28, 22.5], [80, 22.5]];

module rundad_lada(x, y, h, r = 3) {
    hull() for (i = [r, x - r], j = [r, y - r]) translate([i, j, 0]) cylinder(r = r, h = h, $fn = 32);
}

// Kortets nollpunkt i lådans koordinater
module pa_kortet() { translate([VAGG + SPEL + ANDE_V, FRAM_Y, 0]) children(); }

// Kortsidornas väggar (ingår i bottnen) och lockets ram (resten)
module kortsidor(h) {
    intersection() {
        rundad_lada(UT_X, UT_Y, h);
        union() {
            cube([VAGG, UT_Y, h]);
            translate([UT_X - VAGG, 0, 0]) cube([VAGG, UT_Y, h]);
        }
    }
}

module botten() {
    difference() {
        union() {
            rundad_lada(UT_X, UT_Y, BOTTEN_T);
            // låg kant längs långsidorna som locket står på
            difference() {
                rundad_lada(UT_X, UT_Y, BOTTEN_T + 1.2);
                translate([VAGG, VAGG, 0]) cube([IN_X, IN_Y, 10]);
            }
            kortsidor(TAK_Z);
            // framkant under kortkanten (lockets framvägg slutar strax ovanför kortet)
            intersection() {
                rundad_lada(UT_X, UT_Y, BOTTEN_H - 0.2);
                cube([UT_X, VAGG, BOTTEN_H - 0.2]);
            }
            // distanser för kortet och pelare för locket
            pa_kortet() for (h = HAL) translate([h[0], h[1], 0]) cylinder(d = 8, h = BOTTEN_H, $fn = 40);
            pa_kortet() for (p = LOCK_SKRUV) translate([p[0], p[1], 0]) cylinder(d = 8, h = Z_STOLPE, $fn = 40);
            // fästöron mot roboten på kortsidorna
            for (x = [-8, UT_X + 8], y = [UT_Y * 0.25, UT_Y * 0.75])
                hull() {
                    translate([x, y, 0]) cylinder(d = 14, h = BOTTEN_T, $fn = 40);
                    translate([x < 0 ? 0 : UT_X - 1, y - 7, 0]) cube([1, 14, BOTTEN_T]);
                }
        }
        // blinda skruvhål, uppifrån
        pa_kortet() for (h = HAL) translate([h[0], h[1], 0.8]) cylinder(d = M3_GANG, h = 20, $fn = 32);
        pa_kortet() for (p = LOCK_SKRUV) translate([p[0], p[1], 1.0]) cylinder(d = M3_GANG, h = 100, $fn = 32);
        for (x = [-8, UT_X + 8], y = [UT_Y * 0.25, UT_Y * 0.75])
            translate([x, y, -1]) cylinder(d = 5.2, h = 10, $fn = 32);
        // antenner X1/X2 i kortsidan vid DI1 (SMA-skott 6,5 mm)
        for (y = SMA_Y)
            translate([UT_X - VAGG - 1, y, BOTTEN_H + KORT_T + SMA_Z]) rotate([0, 90, 0]) cylinder(d = 6.6, h = VAGG + 2, $fn = 40);
    }
}

module lock_kropp() {
    difference() {
        union() {
            // tak och långsidor; kortsidorna hör till bottnen (0,3 mm luft mot dem)
            translate([0, 0, BOTTEN_T + 1.2]) difference() {
                rundad_lada(UT_X, UT_Y, LOCK_H);
                translate([VAGG, VAGG, -1]) cube([IN_X, IN_Y, LOCK_H - TAK + 1]);
                translate([-1, -1, -1]) cube([VAGG + 1.3, UT_Y + 2, LOCK_H - TAK + 1]);
                translate([UT_X - VAGG - 0.3, -1, -1]) cube([VAGG + 1.3, UT_Y + 2, LOCK_H - TAK + 1]);
            }
            
            if (SKARM) {
                // list i bakväggen som skärmkortets bakkant vilar på
                pa_kortet() translate([SK_MITT[0] - 45, KORT_Y + SPEL - 2.4, TAK_Z - SK_PCB_UNDER - 2])
                    cube([90, 2.5, 2]);
                // klackar för klämmorna fram
                pa_kortet() for (p = SK_KLAMMOR)
                    translate([p[0], p[1], TAK_Z - SK_PCB_UNDER]) cylinder(d = 7, h = SK_PCB_UNDER + 0.1, $fn = 40);
            }
            // klackar ner mot bottnens pelare
            pa_kortet() for (p = LOCK_SKRUV)
                translate([p[0], p[1], Z_STOLPE + 0.2]) cylinder(d = 8, h = TOPP_Z - Z_STOLPE - 0.2, $fn = 40);
        }
        // framväggen slutar 0,8 mm över kortet (kortkanten ligger i väggens plan)
        translate([-1, -1, 0]) cube([UT_X + 2, VAGG + 1.1, BOTTEN_H + KORT_T + 0.8]);
        // kontakterna i kanten: öppning från kortets ovansida och uppåt, öppen nedåt
        pa_kortet() for (k = KANT)
            translate([k[1] - 0.6, -FRAM_Y - 1, BOTTEN_H - 0.5])
                cube([k[2] - k[1] + 1.2, VAGG + 3, KORT_T + k[3] + 1.0]);
        // tak-öppningar
        pa_kortet() for (o = TOPP)
            translate([o[0] - 0.8, o[2] - 0.8, 0]) cube([o[1] - o[0] + 1.6, o[3] - o[2] + 1.6, 100]);
        if (SKARM) {
            // ventilation i bakväggen, under listen
            for (x = [UT_X * 0.12 : 6 : UT_X * 0.88])
                translate([x, UT_Y - VAGG - 1, BOTTEN_H + KORT_T + 2.5]) cube([3, VAGG + 2, 5.5]);
            // skärmfönster med fas utåt, och ficka för glaset underifrån
            pa_kortet() translate([SK_MITT[0], SK_MITT[1], TAK_Z - 0.01]) hull() {
                translate([-SK_AKTIV[0] / 2 - 0.5, -SK_AKTIV[1] / 2 - 0.5, 0]) cube([SK_AKTIV[0] + 1, SK_AKTIV[1] + 1, 0.01]);
                translate([-SK_AKTIV[0] / 2 - 2.5, -SK_AKTIV[1] / 2 - 2.5, TAK + 0.02]) cube([SK_AKTIV[0] + 5, SK_AKTIV[1] + 5, 0.01]);
            }
            pa_kortet() translate([SK_MITT[0] - SK_GLAS[0] / 2 - 0.2, SK_MITT[1] + SK_GLAS_DY - SK_GLAS[1] / 2 - 0.2, TAK_Z - 1])
                cube([SK_GLAS[0] + 0.4, SK_GLAS[1] + 0.4, 1 + SK_FICKA]);
            // klämmornas skruvar (M3×8)
            pa_kortet() for (p = SK_KLAMMOR)
                translate([p[0], p[1], TAK_Z - SK_PCB_UNDER - 1]) cylinder(d = M3_GANG, h = SK_PCB_UNDER, $fn = 32);
        } else {
            // ventilation i bakväggen (taket har loggan)
            for (x = [UT_X * 0.12 : 6 : UT_X * 0.88])
                translate([x, UT_Y - VAGG - 1, BOTTEN_H + KORT_T + 2.5]) cube([3, VAGG + 2, 10]);
        }
        // lockets skruvar: fritt hål och försänkt skalle
        pa_kortet() for (p = LOCK_SKRUV) {
            translate([p[0], p[1], 0]) cylinder(d = M3_FRI, h = 100, $fn = 32);
            translate([p[0], p[1], TOPP_Z - 3]) cylinder(d = 6.4, h = 10, $fn = 40);
        }
        text_kropp(1);
    }
}

// Texten: tunna kroppar i takets ovansida. luft = 1 ger ursparningen (lite djupare).
module text_kropp(luft = 0) {
    z = BOTTEN_T + 1.2 + LOCK_H - TEXT_DJUP;
    translate([0, 0, z - luft * 0.01]) linear_extrude(TEXT_DJUP + luft) pa_kortet() {
        for (k = KANT) {
            mitt = (k[1] + k[2]) / 2;
            can = k[0] == "can1" || k[0] == "can2";
            // namnet stående, läses från kontaktkanten och inåt (CAN större)
            translate([mitt + (can ? 1.9 : 1.6), k[0] == "plint" ? 5.5 : can ? 3.4 : 3, 0]) rotate(90)
                text(k[4], size = can ? 3.8 : 3.2, font = FONT, valign = "baseline");
            // stift: CAN får H och L vid CANH (stift 2) och CANL (stift 3), övriga en triangel vid stift 1
            if (k[5] > 0) {
                steg = 2.0;
                for (i = [0 : k[5] - 1]) {
                    nr = k[6] == "h" ? k[5] - i : i + 1;
                    px = mitt + (i - (k[5] - 1) / 2) * steg;
                    if (can && (nr == 2 || nr == 3))
                        translate([px, 0.3, 0]) text(nr == 2 ? "H" : "L", size = 2.2, font = FONT, halign = "center");
                    if (!can && nr == 1) translate([px, 0.2, 0]) polygon([[-1.1, 0], [1.1, 0], [0, 1.8]]);
                }
            }
        }
        for (o = TOPP) if (o[4] != "")
            translate(o[5]) rotate(o[6]) text(o[4], size = 2.8, font = FONT, halign = "center", valign = "center");
        // plinten: + närmast USB-C
        translate([95.6, 0.8, 0]) text("+", size = 3.6, font = FONT, halign = "center");
        translate([100.2, 0.8, 0]) text("−", size = 3.6, font = FONT, halign = "center");
        // J30 stift 1 sitter närmast kontaktraden
        translate([104.05, 42.8, 0]) polygon([[-1.1, 0], [1.1, 0], [0, 1.8]]);
        // SMA-hålen i kortänden: GPS1 på samma sida som X1, GPS2 som X2
        for (g = [["GPS1", SMA_KORT_Y[1]], ["GPS2", SMA_KORT_Y[0]]])
            translate([KORT_X + SPEL + ANDE / 2 - 0.5, g[1], 0]) rotate(90)
                text(g[0], size = 2.8, font = FONT, halign = "center");
        if (SKARM) {
            translate([131, 66]) rotate(90) text("MP101", size = 5, font = FONT);
            translate([138, 56]) rotate(90) text("▲ = stift 1 (+5 V)", size = 2.6, font = FONT);
        } else {
            translate([40, 88]) text("MP101", size = 5, font = FONT);
            translate([40, 82]) text("▲ = stift 1 (+5 V)", size = 3, font = FONT);
        }
        translate([LOGGA[0], LOGGA[1]]) rotate(LOGGA[2]) offset(delta = LOGGA[4])
            scale(LOGGA[3] / 180) import(LOGGA_FIL);
    }
}

// Klämmorna som håller skärmens framkant (skrivs ut liggande; placerade som monterade)
module klammor() {
    pa_kortet() for (p = SK_KLAMMOR)
        difference() {
            translate([p[0] - 6, p[1] - 4, TAK_Z - SK_PCB_UNDER - 2]) cube([12, 12, 2]);
            translate([p[0], p[1], 0]) cylinder(d = M3_FRI, h = 100, $fn = 32);
        }
}

if (del == "botten") botten();
else if (del == "klammor") klammor();
else if (del == "lock") lock_kropp();
else if (del == "text") intersection() { text_kropp(); lock_kropp_hel(); }
else {
    color("dimgray") botten();
    color("dimgray") lock_kropp();
    color("white") text_kropp();
    if (SKARM) color("dimgray") klammor();
}

module lock_kropp_hel() { translate([0, 0, BOTTEN_T + 1.2]) rundad_lada(UT_X, UT_Y, LOCK_H); }
