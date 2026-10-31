#!/usr/bin/env python3
"""Bygger Mapro Systems fyra handböcker som PDF:

  1. Robotstyrning – användarhandbok
  2. RControlStation – användarhandbok
  3. Uppdatera datorn (scripts/uppdatera_dator.sh)
  4. Uppdatera roboten (scripts/uppdatera.sh)

  python3 manualer/gor_manualer.py [utmapp]      (standard: ~/Robotstyrning/manualer/Mapro Manualer, annars ~/Hämtningar/Mapro Manualer)

Skärmbilderna ligger i manualer/bilder/. Kräver python3-reportlab och DejaVu-typsnitt.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfstil import bild, bygg, h1, h2, h3, kod, p, punkter, sidbryt, tabell, tips, varning  # noqa: E402
from pdfstil import standard_utmapp  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else standard_utmapp()


# ---------------------------------------------------------------------------
# 1. Robotstyrning
# ---------------------------------------------------------------------------

def robotstyrning():
    c = []
    c += [
        h1("1. Om Robotstyrning"),
        p("<b>Robotstyrning</b> är programmet du kör roboten med på avstånd. Du ser robotens "
          "kamerabild i helskärm, styr med en handkontroll (PS4-dosa) och har robotens viktigaste "
          "värden framför dig i en instrumentpanel: batteri, räckvidd, fart, styrvinkel, lutning, "
          "signal och eventuella fel."),
        p("Programmet pratar med roboten över WireGuard-tunneln (robotens adress börjar med "
          "<b>192.168.200.</b>). På roboten tar programmet <i>robotd</i> emot kommandona och skickar "
          "dem vidare till styrkortet."),
        punkter([
            "<b>Kamerabild i helskärm</b> (H.264, anpassas efter nätet).",
            "<b>Handkontroll:</b> vänster spak = fram/bak, höger spak = sväng, L1/L2 och R1/R2 = tillbehör.",
            "<b>AKTIVERA-knapp:</b> roboten kan inte köras förrän du aktiverar den.",
            "<b>Instrumentpanel</b> med fordonsdata och fel i klartext.",
            "<b>Körloggar:</b> varje körning sparas på roboten och kan hämtas till datorn.",
            "<b>Säkerhet:</b> roboten stannar av sig själv om nätet, dosan eller programmet försvinner.",
        ]),
        varning("Robotstyrning och RControlStation kan <b>inte</b> vara anslutna till roboten samtidigt. "
                "Stäng det ena innan du ansluter med det andra."),

        h1("2. Starta och ansluta"),
        h2("2.1 Starta programmet"),
        p("Starta <b>Robotstyrning</b> från programmenyn, eller skriv i en terminal:"),
        kod("robotstyrning"),
        h2("2.2 Ansluta till roboten"),
        punkter([
            "Skriv robotens adress i textfältet, till exempel <b>192.168.200.11</b>, och tryck <b>Enter</b>.",
            "Adresser du har anslutit till förut står under <b>Tidigare anslutna</b>. Klicka på en för att ansluta direkt.",
            "När anslutningen är klar visas körvyn med kamerabilden och instrumentpanelen.",
        ], numrerad=True),
        p("Om anslutningen inte går eller bryts visas en röd text på startsidan med orsaken, till "
          "exempel <i>Connection refused</i> (robotd är inte igång) eller <i>roboten körs redan av "
          "någon annan</i>. Bara en förare åt gången kan styra roboten."),
        tips("Kontrollera kontakten med roboten i en terminal: <b>ping 192.168.200.11</b> "
             "(byt till din robots adress). Svarar den med tider i ms är tunneln uppe."),

        h1("3. Körvyn"),
        bild("rs_korvy.png", 165, "Körvyn: instrumentpanelen uppe till vänster, knapparna uppe till höger, "
             "AKTIVERA i mitten. (Bilden är tagen utan kamera, därför syns ingen kamerabild.)"),
        tabell([
            ["Var", "Vad"],
            ["Uppe till vänster", "Instrumentpanelen (se kapitel 4)."],
            ["Uppe till höger", "<b>Lås</b> (bara när roboten är aktiverad) · <b>Belysning</b> · "
             "<b>CAM/LOS</b> · <b>Max</b> · <b>Loggar</b> · <b>⚙ Inställningar</b>"],
            ["Mitten", "Den gröna knappen <b>AKTIVERA</b>, eller texten <i>Ingen dosa ansluten</i>."],
            ["Ramen runt fönstret", "Grön = bra kontakt · Gul = dålig kontakt · Röd = ingen kontakt "
             "(roboten stannar) · Lila = handkontrollen saknas"],
        ], [1, 3]),
        h2("3.1 Ramens färger"),
        tabell([
            ["Färg", "Betyder", "Gör så här"],
            ["Grön", "Bra kontakt med roboten.", "Kör som vanligt."],
            ["Gul", "Fördröjning eller tappade paket.", "Kör försiktigt och långsamt, eller stanna."],
            ["Röd", "Ingen kontakt. Roboten stannar av sig själv.", "Vänta, eller kontrollera nätet."],
            ["Lila", "Handkontrollen saknas.", "Anslut dosan (USB eller Bluetooth)."],
        ], [1, 2.2, 2.2]),
        h2("3.2 Kamerabilden"),
        p("Kamerabilden visas i helskärm. Står bilden still visas <b>BILDEN FRUSEN – ingen ny bild "
          "från roboten</b> över den. Har ingen bild kommit än visas <i>Ingen bild ännu</i> med orsaken. "
          "I CAM-läget går det inte att aktivera roboten utan kamerabild."),

        h1("4. Instrumentpanelen"),
        bild("rs_panel.png", 110, "Instrumentpanelen."),
        tabell([
            ["Rad", "Visar", "Bra att veta"],
            ["Batteri", "Mätare i procent, spänning och <b>⚡ laddar</b> / <i>laddar inte</i>.",
             "Grön över 40 %, gul 20–40 %, röd under 20 %. Under 15 % blinkar mätaren och "
             "<b>Lågt batteri – kör hem och ladda</b> visas."],
            ["Räckvidd", "Uppskattad sträcka kvar (km), förbrukning (Wh/km) och effekt (W).",
             "Visas när roboten har kört cirka 200 m i den här körningen."],
            ["Fart", "Fart i km/h.", ""],
            ["Temp", "Motorstyrningens (VESC) temperatur.", "Gul över 65 °C, röd över 80 °C."],
            ["VESC", "Hur många motorstyrningar som svarar, t.ex. 3/3.",
             "Röd om någon saknas. Då går det inte att aktivera."],
            ["Styrning", "Stapel och text: <i>rakt</i>, <b>V 40 %</b> eller <b>H 25 %</b>, och vinkeln i grader.",
             "Kommer från vinkelgivaren på svängarmarna."],
            ["Lutning", "Lutning åt sidan och fram/bak i grader.", "Gul över 15°, röd över 25°."],
            ["Kurs", "Riktning i grader (0–360).", ""],
            ["Signal / Ping", "Svarstid till roboten i millisekunder.", "Under 150 ms är bra."],
            ["Video", "Videons status.", ""],
            ["Röd ruta", "Robotens fel i klartext, t.ex. <b>Underspänning – batteriet är för lågt (ladda)</b>.",
             "Se kapitel 8."],
        ], [1, 2.4, 2.4]),
        h2("4.1 Hur batteriprocenten räknas"),
        p("Roboten har fyra 12,8 V LiFePO4-batterier i serie (51,2 V, 4096 Wh). LiFePO4 har nästan "
          "samma spänning mellan 20 och 90 %, och spänningen sjunker när motorerna drar ström. Därför:"),
        punkter([
            "Procenten bestäms ur spänningen när roboten <b>står still</b>.",
            "Under körning räknas procenten <b>ner med den energi som förbrukas</b>, så att den inte hoppar.",
            "Mitt i spannet (20–90 %) är procenten ungefärlig. Nära fullt och tomt är den säkrare.",
        ]),
        h2("4.2 Laddar batteriet?"),
        p("När roboten har stått still i cirka <b>1,5 minut</b> och spänningen stiger visas "
          "<b>⚡ laddar</b>. Stiger den inte visas <i>laddar inte</i>. På så sätt ser du direkt om "
          "elverket eller laddaren faktiskt laddar. Medan roboten kör visas ingetdera."),

        h1("5. Köra roboten"),
        h2("5.1 Första gången"),
        punkter([
            "Ställ roboten så att hjulen <b>inte når marken</b> (pallbockar), eller se till att det är tomt runt den.",
            "Sätt <b>Max</b> lågt, till exempel 0.20.",
            "Prova försiktigt fram, bak och sväng innan du höjer Max.",
        ]),
        h2("5.2 Handkontrollen"),
        tabell([
            ["Kontroll", "Gör"],
            ["Vänster spak upp/ner", "Kör framåt/bakåt."],
            ["Höger spak åt sidan", "Svänger."],
            ["L1 / L2", "Tillbehör A, t.ex. lastarm (om det är inställt under ⚙ Inställningar)."],
            ["R1 / R2", "Tillbehör B, t.ex. tilt (om det är inställt)."],
            ["Släpp spakarna", "Roboten stannar."],
        ], [1.4, 3]),
        p("Spakarna har en liten dödzon i mitten, så att roboten inte kryper när dosan ligger still."),
        h2("5.3 Aktivera och köra"),
        punkter([
            "Tryck på <b>AKTIVERA</b>. Står det en text under knappen berättar den varför det inte går än, "
            "till exempel <i>Ingen kamerabild – byt till LOS för att köra utan</i> eller <i>VESC svarar inte</i>.",
            "Kör med spakarna.",
            "Släpp spakarna för att stanna.",
        ], numrerad=True),
        h2("5.4 Max – hur fort roboten får köra"),
        p("<b>Max</b> uppe till höger är värdet som skickas vid fullt spakutslag (samma som Max i "
          "RControlStation). Dra i rutan eller dubbelklicka och skriv ett värde mellan 0.05 och 1.00. "
          "Ändringen gäller direkt, även under körning, och sparas till nästa gång."),
        h2("5.5 CAM och LOS"),
        tabell([
            ["Läge", "Betyder", "Krav"],
            ["CAM", "Du kör med kamerabilden.", "Kamerabilden måste fungera för att kunna aktivera."],
            ["LOS", "<i>Line of sight</i>: du ser roboten med egna ögon.", "Ingen kamera krävs."],
        ], [1, 2.4, 2.4]),
        p("Läget byts med knappen <b>CAM/LOS</b> och kan bara bytas när roboten inte är aktiverad."),
        h2("5.6 Belysning"),
        p("Knappen <b>Belysning</b> tänder och släcker robotens arbetsbelysning (om roboten har det). "
          "Knappen visar <b>Belysning PÅ</b> när den är tänd."),

        h1("6. Stanna, låsa och säkerhet"),
        punkter([
            "<b>Släpp spakarna</b>: roboten stannar.",
            "<b>Lås</b> (röd knapp) uppe till höger, eller <b>Esc</b> på tangentbordet: körningen låses direkt och "
            "AKTIVERA måste tryckas igen.",
            "<b>Nätet försvinner:</b> roboten stannar av sig själv inom en halv sekund (vakt på roboten).",
            "<b>Dosan tappar kontakten:</b> körningen låses.",
            "<b>Ingen rörelse på spakarna i 5 minuter:</b> körningen låses.",
            "Farten ändras mjukt (ramper), aldrig ryckigt.",
        ]),
        varning("Låset i programmet ersätter inte robotens fysiska nödstopp. Lär dig var nödstoppet "
                "sitter innan du kör."),

        h1("7. Körloggar"),
        p("Roboten sparar en <b>körlogg per körning</b>: en CSV-fil med en rad per sekund. Loggarna "
          "sparas i 90 dagar på roboten."),
        bild("rs_loggar.png", 85, "Fönstret Körloggar."),
        punkter([
            "Tryck på <b>Loggar</b> uppe till höger. Listan med robotens loggar visas, nyast först.",
            "Tryck <b>Spara</b> vid en logg. Filen hamnar i <b>Hämtningar/Robotstyrning-körloggar</b>.",
            "<b>Öppna mappen</b> öppnar mappen i filhanteraren. <b>Uppdatera</b> hämtar listan igen.",
        ], numrerad=True),
        p("Filen öppnas i Excel eller LibreOffice Calc (avgränsare: semikolon). Kolumner:"),
        tabell([
            ["Kolumn", "Innehåll"],
            ["tid", "Datum och tid."],
            ["aktiverad, gas, styr", "Om roboten var aktiverad, och spakarnas lägen (−1 till 1)."],
            ["fart_kmh, spanning_v, batteri_procent, laddar", "Fart, batterispänning, procent, laddning."],
            ["effekt_w, wh_per_km, rackvidd_km", "Effekt, förbrukning och räckvidd."],
            ["styrvinkel_grad, styrning_procent", "Styrvinkel."],
            ["roll_grad, pitch_grad, kurs_grad", "Lutning och kurs."],
            ["vesc_temp_c, vesc_svarar, fel", "Temperatur, vilka VESC som svarar, fel i klartext."],
            ["rssi_dbm, signal", "Mobilsignal (bara om roboten är inställd för det)."],
        ], [2, 3]),

        h1("8. Felmeddelanden"),
        tabell([
            ["Meddelande", "Betyder", "Gör så här"],
            ["Underspänning – batteriet är för lågt (ladda)", "Batterispänningen är under motorstyrningens gräns.",
             "Kör hem och ladda. Motorerna begränsas eller stängs av för att skydda batteriet."],
            ["Överspänning – batterispänningen är för hög", "T.ex. vid inbromsning med fullt batteri.",
             "Kör lugnare nedför. Kontakta oss om det återkommer."],
            ["För hög motorström", "Motorn har dragit för mycket ström.", "Minska lasten eller farten."],
            ["VESC överhettad / Motorn överhettad", "För varmt.", "Låt roboten svalna."],
            ["Fel i motordrivaren (DRV)", "Fel i motorstyrningens elektronik.", "Starta om roboten. Kontakta oss om det återkommer."],
            ["VESC svarar inte", "En motorstyrning hörs inte.", "Kontrollera huvudbrytare och batteri."],
            ["Ingen kamerabild", "Videon fungerar inte.", "Byt till LOS, eller vänta på bilden."],
            ["roboten körs redan av någon annan", "En annan förare är ansluten.", "Den andra måste koppla ner först."],
        ], [1.8, 1.8, 2]),

        h1("9. Inställningar (⚙)"),
        p("Under <b>⚙</b> finns robotens motorinställningar. Normalt behöver du inte röra dem."),
        h2("9.1 VESC-mappning"),
        punkter([
            "<b>Skanna CAN-bussen</b> letar upp robotens motorstyrningar (VESC).",
            "Varje VESC får en roll: <b>Drift vänster</b>, <b>Drift höger</b>, <b>Styrning</b> eller "
            "<b>Annat…</b> (t.ex. Lastarm), med min, max och vilka knappar som styr den (L1/L2 eller R1/R2).",
        ]),
        h2("9.2 Styrkortets aktuatorer"),
        punkter([
            "<b>Läs från styrkortet</b> visar vilken VESC som har vilken aktivitet: Fart (Speed Control), "
            "Styrning (Steering Control), Främre/Bakre lyft, Redskapsposition eller Nödstopp, och om det är VESC eller hydraulik.",
            "<b>Skriv till styrkortet</b> sparar ändringarna i kortets minne och läser tillbaka dem för kontroll. "
            "Roboten får inte vara aktiverad.",
        ]),
        varning("Fel inställningar här kan göra att roboten kör åt fel håll eller inte alls. "
                "Ändra bara om vi har bett dig."),

        h1("10. Om något krånglar"),
        tabell([
            ["Problem", "Prova"],
            ["Ansluter inte", "Är roboten påslagen? Svarar <b>ping</b> på adressen? Är RControlStation stängd?"],
            ["Lila ram", "Anslut dosan, eller tryck på PS-knappen så att den parar sig igen."],
            ["Röd eller gul ram ofta", "Dålig mobiltäckning. Kör långsammare, eller flytta roboten."],
            ["Bilden frusen", "Vänta några sekunder. Hjälper det inte: koppla ner och anslut igen."],
            ["VESC 0/3", "Motorerna har inte ström. Kontrollera batteriet och huvudbrytaren."],
            ["Batteri visar –", "Styrkortet svarar inte. Starta om roboten."],
        ], [1.3, 3]),
        p("Uppdatera programmet enligt handboken <b>Uppdatera datorn</b>. Hör av dig till oss om felet sitter i."),
    ]
    bygg(os.path.join(UT, "Robotstyrning - användarhandbok.pdf"), "Robotstyrning",
         "Användarhandbok",
         "Så kör du roboten på distans med Robotstyrning: anslutning, körvyn, instrumentpanelen, "
         "körning, säkerhet, körloggar, felmeddelanden och inställningar.", c)


# ---------------------------------------------------------------------------
# 2. RControlStation
# ---------------------------------------------------------------------------

def rcontrolstation():
    c = []
    c += [
        h1("1. Om RControlStation"),
        p("<b>RControlStation</b> är programmet för <b>karta, GPS, rutter och autopilot</b>. Här ser du "
          "roboten på kartan, ritar och genererar rutter, laddar upp dem till roboten och låter den köra "
          "själv. Du kan också köra med handkontroll, läsa robotens status och ändra styrkortets inställningar."),
        p("För att köra på distans med kamerabild använder du i stället <b>Robotstyrning</b>."),
        varning("Robotstyrning och RControlStation kan <b>inte</b> vara anslutna till roboten samtidigt. "
                "Stäng det ena innan du ansluter med det andra."),

        h1("2. Starta och ansluta"),
        p("Starta från programmenyn, eller skriv i en terminal:"),
        kod("RControlStation"),
        h2("2.1 Ansluta till roboten"),
        punkter([
            "Till vänster finns listan med robotar (<b>Name</b> och <b>IP Address</b>). Klicka på din robot.",
            "Tryck på knappen <b>längst till vänster under listan</b> (kontakt-ikonen, <i>Connect to selected machine</i>).",
            "Efter några sekunder visas roboten på kartan, och statusrutan börjar uppdateras. "
            "Längst ner till höger står <b>Connected</b>.",
        ], numrerad=True),
        tabell([
            ["Knapp under listan", "Gör"],
            ["Kontakt-ikonen (längst till vänster)", "Ansluter till roboten som är markerad i listan."],
            ["<b>Text</b>", "Ansluter till adressen som står i textrutan, t.ex. 192.168.200.11."],
            ["Bruten kontakt", "Kopplar ner från roboten."],
            ["Uppdatera (pilar)", "Hämtar listan med robotar från servern igen."],
        ], [1.6, 3]),
        tips("Står inte din robot i listan: tryck på uppdatera. Syns den fortfarande inte, skriv robotens "
             "adress i textrutan och tryck <b>Text</b>."),
        p("Kartan centreras på roboten när den ansluts, och nollpunkten (ENU) sätts där roboten står."),

        h1("3. Statusrutan"),
        p("Rutan <b>Status</b> till vänster visar läget för GPS, länk och fordon. Den uppdateras två gånger per sekund."),
        bild("rcs_status.png", 95, "Statusrutan med fordonsdata."),
        tabell([
            ["Rad", "Visar"],
            ["Lösning", "GPS-läge: <b>RTK fix</b> (grön, bäst), <b>RTK float</b> eller <b>DGPS</b> (gul), "
             "<b>SPP</b> eller <b>Ingen fix</b> (röd). <i>ingen GPS-data</i> om inget kommer."],
            ["Satelliter / Korr. ålder", "Antal satelliter och hur gamla RTK-korrektionerna är."],
            ["Ping (bil)", "Svarstid program → robot → styrkort → tillbaka. Grön under 150 ms, gul under 500 ms."],
            ["4G/5G", "Mobilsignal, bara om roboten är inställd för det."],
            ["Batteri", "Procent (grön/gul/röd), spänning, <b>⚡ laddar</b> eller <i>laddar inte</i>. "
             "Varning under 15 %."],
            ["Räckvidd", "Km kvar och Wh/km. Visas efter cirka 200 m körning."],
            ["Fart / Temp", "Fart i km/h och motorstyrningens temperatur."],
            ["Styrning", "Svängarmarnas läge: <i>rakt</i>, V eller H i procent, och grader."],
            ["Lutning / Kurs", "Lutning åt sidan och fram/bak (gul över 15°, röd över 25°), och kurs."],
            ["Röd text", "Fel i klartext, t.ex. <b>Underspänning – batteriet är för lågt (ladda)</b>."],
        ], [1.3, 3.5]),
        p("Batteriprocenten bestäms när roboten står still och räknas sedan ner med förbrukad energi. "
          "Mellan 20 och 90 % är den ungefärlig (LiFePO4-batterier har nästan samma spänning där). "
          "<b>⚡ laddar</b> visas när roboten stått still i cirka 1,5 minut och spänningen stiger."),
        p("<b>Batteri och fordon…</b> (knappen under statusrutan) ställer in batteriet för den maskin du är "
          "ansluten till: <b>LiFePO4</b> med antal celler i serie, eller <b>linjärt</b> mellan tom och full "
          "spänning, samt kapacitet i Wh och största styrvinkel. Inställningen sparas per maskin och visas i "
          "grått under batteriraden. Utan egen inställning gäller standard: 16 celler LiFePO4, 4096 Wh."),
        tips("Kapaciteten är spänning × amperetimmar, t.ex. 4 × 12,8 V 80 Ah = 4096 Wh. Den behövs för "
             "nedräkningen under körning och för räckvidden."),
        h2("3.1 Körlogg"),
        p("Medan RControlStation är ansluten sparas en <b>körlogg</b> på datorn, en CSV-fil per anslutning "
          "med en rad per sekund, i mappen <b>Hämtningar/RControlStation-korloggar</b>. Den innehåller fart, "
          "spänning, batteri, laddning, effekt, förbrukning, räckvidd, styrvinkel, lutning, kurs, temperatur och fel."),

        h1("4. Flikarna"),
        bild("rcs_flikar.png", 150, "Flikarna överst i fönstret."),
        tabell([
            ["Flik", "Innehåll"],
            ["Bil-ikonen", "<b>Bilarna</b>: en flik per ansluten robot med terminal, inställningar m.m. (kapitel 7)."],
            ["Kart-ikonen", "<b>Kartan</b> med rutter, autopilot och ruttgenerering (kapitel 5–6)."],
            ["Farm", "Gårdar, fält och sparade rutter (kapitel 8)."],
            ["Machines", "Robotarna i listan, och deras motorer (kapitel 8)."],
            ["Signal-ikonen", "Loggning och analys av nätverk och GPS-korrektioner."],
            ["Log", "Analys av körloggar mot fält (kapitel 8)."],
            ["Handkontroll-ikonen", "Vilken spak som styr vad (kapitel 9)."],
            ["File administration", "Fält och rutter på servern som inte hör till någon gård."],
            ["Kart-ikonen längst till höger", "<b>Actions Management</b>: åtgärder (actions) som kan läggas på ruttpunkter."],
        ], [1.4, 3.5]),

        h1("5. Kartan"),
        h2("5.1 Flytta och zooma"),
        punkter([
            "<b>Dra</b> med musen för att flytta kartan, <b>scrollhjulet</b> för att zooma.",
            "Rutnätet visar meter från nollpunkten (ENU). Kartbilden hämtas från OpenStreetMap/satellit.",
            "Rutorna överst till höger: <b>Car</b> (vilken robot), <b>Route</b> (vilken rutt), "
            "<b>Info Trace</b>, och <b>Follow</b> (kartan följer roboten), <b>Trace</b> (rita spår), <b>Show Text</b>.",
        ]),
        h2("5.2 Kortkommandon på kartan"),
        tabell([
            ["Kommando", "Gör"],
            ["Shift + vänsterklick", "Lägger till en ruttpunkt (eller ankare)."],
            ["Shift + dra med vänster", "Flyttar en ruttpunkt."],
            ["Shift + högerklick", "Tar bort en ruttpunkt."],
            ["Ctrl + högerklick", "Uppdaterar ruttpunktens inställningar (fart, tid, tillstånd)."],
            ["Ctrl + vänsterklick", "Sätter den valda robotens position där du klickar. Används inte när roboten har RTK-GPS."],
            ["Ctrl + Shift + vänsterklick", "Nollställer kartans nollpunkt (ENU) där du klickar."],
        ], [1.6, 3]),
        p("Samma lista får du med <b>?</b>-knappen (<i>Show keyboard shortcuts</i>) på fliken <b>Edit</b>."),
        h2("5.3 Knapparna ovanför flikarna till höger"),
        tabell([
            ["Knapp", "Gör"],
            ["Dator-ikonen", "Lämnar över styrningen till <b>autopiloten</b> (roboten kör rutten)."],
            ["Handkontroll-ikonen", "<b>Manuell styrning</b> med handkontroll eller tangentbord."],
            ["STOP", "<b>Nödstopp</b> för den valda roboten."],
            ["Rullista + värde", "Ett styrtillstånd (t.ex. <i>Emergency Stop</i>) och värdet som skickas till roboten."],
        ], [1.6, 3]),

        h1("6. Rutter och autopilot"),
        h2("6.1 Car control – autopilot"),
        tabell([
            ["Knapp", "Gör"],
            ["Reset &amp; Start", "Startar rutten från början."],
            ["Start", "Startar, eller fortsätter där roboten var."],
            ["Pause (keep state)", "Pausar och kommer ihåg var i rutten roboten var."],
            ["Stop (reset state)", "Stoppar och nollställer."],
            ["Follow route / Follow Me", "Följ rutten, eller följ en annan position."],
            ["ENU Reference Point", "Hämta nollpunkten från roboten, eller skicka kartans nollpunkt till roboten. "
             "<i>Base station position</i> använder basstationens position som nollpunkt."],
            ["Set Abs Yaw", "Sätter robotens riktning för hand."],
        ], [1.6, 3]),
        h2("6.2 Edit – rita och ladda upp rutter"),
        punkter([
            "Välj <b>Edit Mode</b>: <b>Route</b> (ruttpunkter) eller <b>Anchor</b>.",
            "Rita med Shift + klick på kartan (se 5.2).",
            "<b>R</b> tar bort den aktuella rutten, <b>A</b> tar bort ankarna.",
            "<b>Upload Current Route</b>: pil upp skickar rutten till roboten (procent visar hur långt det kommit), "
            "pil ner läser rutten från roboten.",
            "<b>Current Route Point</b>: fart (<b>V</b> i km/h) med knapp för att sätta samma fart på alla punkter, "
            "starttid (<b>T</b>), tillägg (<b>Add</b>) och läge (<b>Pos Default</b>).",
            "<b>Implement down</b>: redskapet nere vid punkten.",
            "<b>Control States</b>: <b>Add State</b>/<b>Remove State</b> lägger till eller tar bort tillstånd "
            "(t.ex. ett redskaps läge) på punkten.",
        ]),
        h2("6.3 Import"),
        punkter([
            "<b>Import NMEA</b>: läser en NMEA-fil (GPS-logg) till en rutt. <b>Zero ENU</b> sätter nollpunkten på första punkten.",
            "<b>Stream NMEA over TCP</b>: visar en GPS-ström live (adress och port), med <b>RTK only</b>, "
            "<b>Follow</b>, <b>Clear trace</b> och <b>Forward UDP</b>.",
        ]),
        h2("6.4 View – kartinställningar"),
        punkter([
            "<b>General</b>: rutnät, kantutjämning, text vid ruttpunkter.",
            "<b>OpenStreetMap</b>: kartkälla, upplösning, högsta zoom, <b>Clear cached tiles</b>.",
            "<b>Trace</b>: hur tätt GPS- och robotspåret ritas (mm).",
            "<b>Remove</b>: rensa robotspår, alla rutter, infospår eller importerade bilder.",
        ]),
        h2("6.5 Route Generation – ZigZag"),
        p("Fyller ett område med fram-och-tillbaka-körning:"),
        punkter([
            "<b>Bound</b>: vilken rutt som är gränsen. <b>Use Active</b> tar den aktiva.",
            "<b>Speed</b> och <b>Speed in Turns</b> (km/h), <b>Spacing</b> (m mellan raderna), <b>Visit Every … Row</b>.",
            "<b>Keep Turns within Bounds</b>: vändningarna stannar innanför gränsen.",
            "<b>Front Tool Actuation</b>: sänk redskapet på raksträckorna och lyft i vändningarna, med avstånd i meter.",
            "<b>Fill</b> skapar rutten.",
        ]),
        h2("6.6 GL-fill – försöksrutor"),
        p("Skapar en rutt för försöksrutor (plots) i ett rutnät:"),
        punkter([
            "Klicka två punkter: <b>Point 1</b> (röd) och <b>Point 2</b> (grön), och <b>Generate line</b>.",
            "Fyll i rutornas längd och bredd, redskapets bredd och längd, antal rutor i körriktningen (dd) och "
            "tvärs (ndd), avstånd mellan rutorna, fart, svängradie och antal steg i svängen.",
            "<b>Flip side</b> och <b>Switch startpoint</b> vänder rutnätet. <b>Research plots</b>, <b>Randomized</b>, "
            "<b>Add insert drive</b> och <b>Produce pieces</b> styr hur rutten byggs.",
            "<b>Generate path</b> skapar rutten. <b>Prepend</b>/<b>Append route</b> lägger den före eller efter en befintlig rutt. "
            "<b>Show Shapefile</b> visar en shapefil.",
        ]),
        h2("6.7 GL actions – flytta och klippa rutter"),
        punkter([
            "<b>Move X/Y</b> (m) och <b>Rotate</b> (grader), sedan <b>Transform</b>.",
            "<b>Cut</b> tar bort punkter före eller efter en vald punkt. <b>Create new path</b> gör en ny rutt av resultatet.",
        ]),

        h1("7. Bilfliken (roboten)"),
        p("Varje ansluten robot får en egen flik med flera underflikar:"),
        tabell([
            ["Underflik", "Innehåll"],
            ["Orientation", "Robotens vinklar (roll, pitch, yaw)."],
            ["Log", "Loggning på roboten."],
            ["IMU realtime", "Accelerometer och gyro i realtid."],
            ["Terminal", "Skicka kommandon till styrkortet (<b>Send command to car</b>, <b>Clear terminal</b>). "
             "Skriv <b>help</b> för en lista. T.ex. <b>vinkel</b> visar vinkelgivaren."],
            ["Calibration", "Kalibrering av magnetometern."],
            ["GPS", "u-blox-information: Version, NAV_SAT, SOL, RELPOSNED, CFG_GNSS."],
            ["Configuration", "Styrkortets inställningar (se 7.1)."],
            ["Motor", "VESC, servo, styrning och magnetometer."],
        ], [1.3, 3.5]),
        h2("7.1 Inställningar på styrkortet"),
        punkter([
            "<b>Read configuration from car</b> läser inställningarna, <b>Write configuration to car</b> skriver dem. "
            "<b>Read default configuration</b> läser standardvärdena.",
            "<b>DB</b>-knapparna läser och skriver inställningarna till databasen.",
            "Flikarna: <b>IMU</b>, <b>GPS</b> (antennens position, krav på RTK, korrektioner), <b>UWB</b>, "
            "<b>Autopilot</b> (t.ex. upprepa rutter, nollställ vid nödstopp) och <b>Logging</b>.",
            "<b>Car Settings</b>: t.ex. <i>Disable Motor</i> och <i>Simulate Motor</i> för test.",
        ]),
        varning("Ändra bara styrkortets inställningar om vi har bett dig. Fel värden kan göra att roboten "
                "kör fel. Skriv aldrig inställningar medan roboten kör."),
        h2("7.2 Övrigt på bilfliken"),
        punkter([
            "<b>Poll Data</b>: hämta status från roboten (ska vara på).",
            "<b>Keyboard Control</b> och <b>AutoPilot</b>: av/på.",
            "<b>Route from map</b>: ladda upp ruttpunkter direkt när de ritas.",
            "<b>Clear stored route</b>: rensa rutten i roboten.",
            "<b>STM</b> / <b>Pi</b>: ställ klockan på styrkortet eller Pi:n.",
            "<b>Reboot PI</b>, <b>Shutdown PI</b>, <b>Restart Car Client</b>: starta om eller stäng av robotens dator.",
            "<b>Zero Gyro</b>: nollställ gyrot (roboten ska stå still).",
            "Uppe syns firmwareversionen (<b>FW x.y</b>), klockan och felkoden.",
        ]),
        tips("Stäng av Pi:n med <b>Shutdown PI</b> innan strömmen till roboten bryts. Det skyddar SD-kortet."),

        h1("8. Gårdar, fält och maskiner"),
        h2("8.1 Farm"),
        punkter([
            "<b>Add farm</b> lägger till en gård. Under gården finns <b>Fields</b> (fält) och <b>Paths</b> (rutter).",
            "<b>Add field</b>: rita fältets gräns på kartan, ge det ett namn och tryck <b>Save field</b>. Arean räknas ut.",
            "<b>Add path</b>: spara en rutt med namn. <b>Load Shape file</b> läser en shapefil.",
            "<b>use electronic fence</b>: fältets gräns används som elektroniskt staket.",
        ]),
        h2("8.2 Machines"),
        p("Listan med robotar (namn och IP-adress) och deras motorer. <b>Add Machine</b> lägger till en robot. "
          "Listan hämtas från servern, och du ser bara dina egna robotar."),
        h2("8.3 Log"),
        p("Välj gård, fält och rutt, <b>Load Log</b> för en körlogg, och kör analysverktyget (<b>Run</b>) för att se "
          "vilken yta som körts. <b>Cut Path</b> klipper rutten mot ett område."),

        h1("9. Handkontroll och styrning"),
        h2("9.1 Aktivera dosa, Joystick och Control (nere till vänster)"),
        tabell([
            ["Inställning", "Gör"],
            ["🎮 Aktivera dosa", "Dosan är <b>låst</b> när programmet startar. Klicka för att köra med dosan: "
             "kryssar i <b>Active</b> och <b>Keyboard control</b> för vald bil. Går bara med robot och dosa "
             "anslutna och spakarna i mitten. Knappen blir röd: <b>🔒 Lås dosa</b>."],
            ["Active", "Dosan/tangentbordet styr roboten (följer knappen ovan)."],
            ["C / D / I", "Styrläge: C (fart), D (duty), I (ström). Normalt C."],
            ["Max", "Utslag vid fullt spakläge. Standard 0,42."],
            ["Två värden under Max", "Hur mycket tangentbordet ger (Keyboard gain) för gas och styrning."],
            ["Poll Interval", "Hur ofta status hämtas från roboten (ms)."],
            ["STOP (stor knapp)", "Stannar alla robotar."],
        ], [1.5, 3]),
        h2("9.2 Handkontrollen"),
        p("Dosan ansluts automatiskt (rutan <b>Joystick</b> visar om den är ansluten), även om den dras ur och "
          "sätts i igen. Vänster spak upp/ner = kör, höger spak åt sidan = sväng. L1/L2 och R1/R2 styr tillbehör. "
          "På handkontroll-fliken ser du vilken spak som är kopplad till vilken funktion."),
        varning("Dosan <b>låses av sig själv</b> (roboten får fart 0) när anslutningen till roboten bryts, när "
                "dosan kopplas ur och efter 5 minuter utan spakrörelse. Klicka <b>Aktivera dosa</b> igen för att köra."),

        h1("10. Om något krånglar"),
        tabell([
            ["Problem", "Prova"],
            ["Roboten står inte i listan", "Tryck uppdatera, eller skriv adressen och tryck <b>Text</b>."],
            ["Kopplar ner direkt", "Programmet känner inte till styrkortets firmware. Uppdatera datorn (egen handbok)."],
            ["Ping (bil): inget svar", "Roboten svarar inte. Är Robotstyrning ansluten? Är roboten påslagen?"],
            ["Lösning: ingen GPS-data", "RTK-tjänsten på roboten är inte igång eller saknar korrektioner."],
            ["Batteri visas inte", "Ingen status från roboten. Kontrollera <b>Poll Data</b> på bilfliken."],
        ], [1.6, 3]),
    ]
    bygg(os.path.join(UT, "RControlStation - användarhandbok.pdf"), "RControlStation",
         "Användarhandbok",
         "Karta, GPS, rutter, autopilot och robotens status: allt du kan göra i RControlStation, flik för flik.", c)


# ---------------------------------------------------------------------------
# 3. Uppdatera datorn
# ---------------------------------------------------------------------------

def uppdatera_dator():
    c = [
        h1("Uppdatera programmen på datorn"),
        p("När vi ber dig uppdatera hämtar du de senaste versionerna av <b>Robotstyrning</b> och "
          "<b>RControlStation</b> med ett enda kommando. Roboten och dess styrkort rörs inte. Dem "
          "uppdaterar vi på distans, eller med handboken <b>Uppdatera roboten</b> om vi ber dig."),
        h2("Innan du börjar"),
        punkter([
            "Datorn ska ha internet.",
            "<b>Stäng Robotstyrning och RControlStation.</b> Är något av dem öppet avbryter skriptet och ber dig stänga.",
            "Ha datorns lösenord till hands.",
        ]),
        h2("Så gör du"),
        punkter([
            "Öppna en terminal: tryck <b>Ctrl + Alt + T</b>.",
            "Kopiera och klistra in raden nedan (klistra in i terminalen med <b>Ctrl + Shift + V</b>) och tryck <b>Enter</b>:",
        ], numrerad=True),
        kod("cd ~/robot-control && git pull && bash scripts/uppdatera_dator.sh"),
        punkter([
            "Skriv datorns lösenord när det frågas (det syns inget när du skriver, det är normalt) och tryck <b>Enter</b>.",
            "Vänta. Robotstyrning tar några minuter. RControlStation kan ta 10–20 minuter om mycket har ändrats.",
        ], numrerad=True),
        h2("Vad skriptet gör"),
        tabell([
            ["Steg", "Gör"],
            ["1. git pull", "Hämtar den senaste koden till mappen robot-control."],
            ["2. Robotstyrning", "Bygger och installerar Robotstyrning."],
            ["3. RControlStation", "Om den finns: hämtar den senaste koden och bygger om. Dina inställningar "
             "(t.ex. handkontrollen) behålls."],
        ], [1.2, 3.5]),
        h2("Klart"),
        p("Längst ner står en sammanfattning. Allt gick bra när det står i grönt:"),
        kod("Datorn är uppdaterad. Starta Robotstyrning eller RControlStation som vanligt."),
        p("Sammanfattningen har en rad per program, till exempel:"),
        kod("Robotstyrning: uppdaterad\nRControlStation: uppdaterad"),
        p("<i>RControlStation: inte installerad</i> betyder att du inte har RControlStation, och det är i så fall helt i sin ordning."),
        h2("Om något blir rött"),
        tabell([
            ["Meddelande", "Gör så här"],
            ["Stäng Robotstyrning och RControlStation först…", "Stäng programmen och kör samma kommando igen."],
            ["git pull gick inte", "Någon fil i robot-control har ändrats för hand. Kontakta oss."],
            ["FEL vid bygget", "Kör samma kommando igen. Det som redan är klart går fort. Hjälper det inte: "
             "skicka oss en bild av terminalen."],
            ["Något blev inte klart", "Kör samma kommando igen."],
        ], [2, 3]),
        tips("Det är alltid säkert att köra kommandot igen. Det som redan är uppdaterat hoppas över eller går fort."),
    ]
    bygg(os.path.join(UT, "Uppdatera datorn.pdf"), "Uppdatera datorn",
         "Robotstyrning och RControlStation",
         "Så uppdaterar du programmen på din dator när vi ber dig. Ett kommando, inga frågor utom lösenordet.",
         c, innehallsforteckning=False)


# ---------------------------------------------------------------------------
# 4. Uppdatera roboten
# ---------------------------------------------------------------------------

def uppdatera_robot():
    c = [
        h1("1. Uppdatera roboten"),
        p("Normalt uppdaterar vi roboten åt dig på distans. Ber vi dig göra det själv gör du det från "
          "<b>din dator</b> med ett kommando. Skriptet uppdaterar först datorn och sedan robotens dator (Pi:n) "
          "och styrkortet:"),
        tabell([
            ["Del", "Vad som uppdateras"],
            ["Datorn", "Robotstyrning och RControlStation (samma som handboken <i>Uppdatera datorn</i>)."],
            ["Car_Client", "Programmet på roboten som pratar med styrkortet. Byggs om och startas om."],
            ["robotd", "Programmet som Robotstyrning ansluter till. Byggs om och startas om."],
            ["Styrkortet", "Ny firmware, <b>bara om versionen är ny</b>. Kortets inställningar behålls."],
        ], [1.2, 3.5]),
        p("Robotens inställningar, WireGuard-nycklar och styrkortets inställningar rörs inte."),
        h2("1.1 Innan du börjar"),
        punkter([
            "Roboten ska vara <b>påslagen</b> och nåbar (WireGuard uppe). Kontrollera med "
            "<b>ping 192.168.200.11</b> (byt till din robots adress).",
            "<b>Roboten ska stå still och inte köras.</b> Ingen får vara ansluten med Robotstyrning eller "
            "RControlStation, varken på din dator eller någon annans.",
            "<b>Stäng Robotstyrning och RControlStation</b> på din dator.",
            "Ha <b>datorns lösenord</b> och <b>robotens lösenord</b> (Pi:n) till hands. Vi har gett dig robotens "
            "användarnamn och lösenord.",
            "Ska styrkortet flashas måste <b>programmeraren (ST-Link)</b> sitta i, som när roboten levererades.",
        ]),
        varning("Styrkortet startar om när det flashas, och motorerna stannar. Uppdatera aldrig medan "
                "roboten används."),

        h1("2. Så gör du"),
        punkter([
            "Öppna en terminal: <b>Ctrl + Alt + T</b>.",
            "Skriv kommandot nedan med <b>din robots användarnamn och adress</b> (exemplet: användaren "
            "robant1 på 192.168.200.11) och tryck <b>Enter</b>:",
        ], numrerad=True),
        kod("cd ~/robot-control && git pull && PI=robant1@192.168.200.11 bash scripts/uppdatera.sh"),
        punkter([
            "Skriv <b>datorns lösenord</b> när datorn frågar.",
            "Skriv <b>robotens lösenord</b> när roboten frågar (<i>robant1@192.168.200.11's password</i> och "
            "<i>[sudo] password</i>).",
            "Vänta. Hela uppdateringen tar 10–30 minuter, mest beroende på hur mycket som har ändrats.",
        ], numrerad=True),
        tips("Adressen sparas. Nästa gång räcker det med: "
             "<b>cd ~/robot-control &amp;&amp; git pull &amp;&amp; bash scripts/uppdatera.sh</b>"),

        h1("3. Klart"),
        p("Längst ner står en sammanfattning. Allt gick bra när det står i grönt:"),
        kod("Datorn: uppdaterad\nRoboten: Car_Client, robotd och styrkortet uppdaterade\n\n"
            "Allt är uppdaterat: datorn, roboten och styrkortet."),
        p("Har styrkortet fått ny firmware står också <i>Styrkortet kör nu firmware 30.x</i>."),
        h2("3.1 Kontrollera efteråt"),
        punkter([
            "Starta Robotstyrning och anslut. Instrumentpanelen ska visa batteri och <b>VESC 3/3</b> (eller ditt antal).",
            "Eller anslut med RControlStation: statusrutan ska visa <b>Ping (bil)</b> och batteri.",
            "Prova försiktigt med hjulen fria från marken, eller med gott om plats runt roboten.",
        ]),

        h1("4. Om något blir rött"),
        tabell([
            ["Meddelande", "Gör så här"],
            ["Når inte roboten", "Är roboten påslagen? Svarar <b>ping</b>? Vänta två minuter efter start och försök igen."],
            ["Stäng Robotstyrning och RControlStation först", "Stäng dem och kör samma kommando igen."],
            ["Någon är ansluten till Car_Client … kortet flashas inte nu", "Någon är ansluten till roboten. Koppla ner alla och kör igen."],
            ["Ingen ST-Link hittades på USB", "Programmeraren sitter inte i. Sätt i den och kör igen."],
            ["Styrkortet svarar inte", "Kontrollera strömmen till styrkortet och USB-kabeln."],
                        ["Något blev inte klart", "Kör samma kommando igen. Det som redan är klart går fort."],
        ], [2, 3]),
        p("Hjälper det inte: skicka oss en bild av terminalen, så hjälper vi dig."),

        h1("5. Starta om och stänga av roboten"),
        p("Stäng alltid av robotens dator innan strömmen bryts. Det skyddar SD-kortet."),
        h2("5.1 Med statusskärmen (om roboten har en)"),
        bild("statusskarm_pi.png", 110, "Statusskärmen: tryck på Pi-rutan."),
        punkter([
            "Tryck på <b>Pi</b>-rutan.",
            "Välj <b>Starta om</b>, <b>Stäng av</b> eller <b>Avbryt</b>.",
            "Efter <b>Stäng av</b>: vänta tills skärmen är svart och den gröna lampan på Pi:n har slutat blinka. "
            "Bryt sedan strömmen. Pi:n startar igen när strömmen slås på.",
        ], numrerad=True),
        h2("5.2 Med RControlStation"),
        p("På bilfliken finns <b>Reboot PI</b> (starta om) och <b>Shutdown PI</b> (stäng av)."),
        h2("5.3 Starta om enskilda delar"),
        p("På statusskärmen kan rutorna <b>Car_Client</b>, <b>RTK</b> och <b>Internet</b> tryckas på när de "
          "inte är gröna. Då startas just den delen om, efter en fråga."),
    ]
    bygg(os.path.join(UT, "Uppdatera roboten.pdf"), "Uppdatera roboten",
         "Robotens dator och styrkort",
         "Så uppdaterar du robotens program och styrkortets firmware från din dator, när vi ber dig. "
         "Dessutom: hur du startar om och stänger av roboten säkert.", c)


if __name__ == "__main__":
    os.makedirs(UT, exist_ok=True)
    robotstyrning()
    rcontrolstation()
    uppdatera_dator()
    uppdatera_robot()
    for f in sorted(os.listdir(UT)):
        if f.endswith(".pdf"):
            print(os.path.join(UT, f))
