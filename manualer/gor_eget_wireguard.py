#!/usr/bin/env python3
"""Bygger kundguiden "Eget WireGuard" som PDF: kundens egen tunnel mellan deras dator och
deras robot, medan robotens tunnel till Mapro (wg0) ligger kvar för support.

  python3 manualer/gor_eget_wireguard.py [utmapp]     (standard: ~/Robotstyrning/manualer/Mapro Manualer, annars ~/Hämtningar/Mapro Manualer)
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from pdfstil import bygg, h1, h2, kod, p, punkter, tabell, tips, varning  # noqa: E402
from pdfstil import standard_utmapp  # noqa: E402

UT = sys.argv[1] if len(sys.argv) > 1 else standard_utmapp()


def guide():
    c = [
        h1("1. Så blir det"),
        p("I dag går både er dator och er robot genom <b>Mapros</b> WireGuard-tunnel. Efter den här guiden har ni "
          "<b>en egen tunnel</b> mellan er dator och er robot. Robotens tunnel till Mapro ligger kvar, så att vi "
          "kan hjälpa er och uppdatera roboten även i fortsättningen."),
        tabell([
            ["", "Mapros tunnel (wg0)", "Er egen tunnel (egen)"],
            ["Roboten", "<b>Ligger kvar</b>, 192.168.200.x. Används bara för support och uppdateringar.",
             "Ny: <b>10.77.0.2</b>"],
            ["Er dator", "<b>Tas bort</b> i kapitel 9.", "Ny: <b>10.77.0.1</b> (navet som roboten ringer upp)"],
            ["Ni kör roboten via", "–", "10.77.0.2 i Robotstyrning och RControlStation"],
        ], [1.1, 2.4, 2.4]),
        varning("Rör <b>inte</b> filen <b>/etc/wireguard/wg0.conf</b> eller tjänsten <b>wg-quick@wg0</b> på roboten. "
                "Det är Mapros väg in till roboten. Tas den bort kan vi inte hjälpa er eller uppdatera roboten på distans."),
        p("Adresserna 10.77.0.x är exempel. Ni kan välja ett annat område, men det får <b>inte</b> vara "
          "192.168.200.x (Mapros) och inte samma som ert kontors- eller robotnät (ofta 192.168.0.x eller 192.168.1.x)."),

        h1("2. Det här behöver ni"),
        punkter([
            "Er dator med Linux (samma dator som ni kör Robotstyrning och RControlStation på) och sudo-lösenordet.",
            "Inloggning till roboten (användarnamn och lösenord). Fråga Mapro om ni inte har det.",
            "En <b>publik adress</b> där datorn står, med <b>portvidarebefordran</b> av UDP-port 51821 till datorn. "
            "Saknar kontoret fast publik adress går det med en DDNS-tjänst (till exempel DuckDNS).",
        ]),
        tips("Roboten sitter på 4G och kan inte ta emot anslutningar. Därför är det <b>datorn</b> som är navet, och "
             "roboten ringer upp den. Står datorn inte på ett fast ställe, eller går portvidarebefordran inte att "
             "ordna, kan navet i stället vara en liten molnserver (VPS). Upplägget blir detsamma, med navet på servern."),

        h1("3. Innan ni börjar: Mapro förbereder roboten"),
        p("Programmet <b>robotd</b> på roboten (det Robotstyrning ansluter till) lyssnar i dag bara på Mapros "
          "tunnel. <b>Kontakta Mapro innan ni börjar</b>, så ställer vi om det till att lyssna även på er tunnel. "
          "Det tar några minuter och görs på distans. RControlStation fungerar redan på alla adresser."),

        h1("4. Datorn"),
        p("Öppna en terminal på datorn."),
        h2("4.1 Installera och skapa nycklar"),
        kod("sudo apt install wireguard-tools\n"
            "cd /etc/wireguard\n"
            "sudo sh -c 'umask 077; wg genkey | tee egen_privat | wg pubkey > egen_publik'\n"
            "sudo cat egen_publik"),
        p("Skriv upp den publika nyckeln som visas. Den behövs på roboten i kapitel 6. Den privata nyckeln lämnar aldrig datorn."),
        h2("4.2 Skapa tunneln"),
        kod("sudo nano /etc/wireguard/egen.conf"),
        p("Klistra in följande. Byt <b>&lt;datorns privata nyckel&gt;</b> mot innehållet i "
          "<b>/etc/wireguard/egen_privat</b> (visa det med <b>sudo cat /etc/wireguard/egen_privat</b>). "
          "Robotens nyckel fyller ni i efter kapitel 6."),
        kod("[Interface]\n"
            "Address = 10.77.0.1/24\n"
            "ListenPort = 51821\n"
            "PrivateKey = <datorns privata nyckel>\n"
            "MTU = 1280\n"
            "\n"
            "[Peer]\n"
            "# Roboten\n"
            "PublicKey = <robotens publika nyckel>\n"
            "AllowedIPs = 10.77.0.2/32"),
        p("Spara med Ctrl+O, Enter och Ctrl+X."),
        h2("4.3 Brandväggen"),
        p("Om datorn har brandväggen ufw påslagen, öppna porten:"),
        kod("sudo ufw allow 51821/udp"),

        h1("5. Routern"),
        p("Logga in i kontorets router och lägg in en <b>portvidarebefordran</b>: UDP, extern port 51821, till "
          "datorns lokala adress (visas med <b>hostname -I</b>), intern port 51821. Ge gärna datorn en fast lokal "
          "adress i routern, så att vidarebefordran inte tappar bort den."),
        p("Ta reda på kontorets publika adress, till exempel på whatismyip.com, eller använd ert DDNS-namn. "
          "Det kallas <b>&lt;kontorets adress&gt;</b> nedan."),

        h1("6. Roboten"),
        p("Logga in på roboten från datorn. Det går genom Mapros tunnel som datorn fortfarande har. Byt användare "
          "och adress mot er robots:"),
        kod("ssh <användare>@<robotens adress hos Mapro, t.ex. 192.168.200.10>"),
        p("På roboten:"),
        kod("cd /etc/wireguard\n"
            "sudo sh -c 'umask 077; wg genkey | tee egen_privat | wg pubkey > egen_publik'\n"
            "sudo cat egen_publik\n"
            "sudo nano /etc/wireguard/egen.conf"),
        p("Skriv upp robotens publika nyckel. Klistra in följande i filen, med robotens privata nyckel "
          "(<b>sudo cat /etc/wireguard/egen_privat</b>) och datorns publika nyckel från kapitel 4:"),
        kod("[Interface]\n"
            "Address = 10.77.0.2/24\n"
            "PrivateKey = <robotens privata nyckel>\n"
            "MTU = 1280\n"
            "\n"
            "[Peer]\n"
            "# Datorn\n"
            "PublicKey = <datorns publika nyckel>\n"
            "Endpoint = <kontorets adress>:51821\n"
            "AllowedIPs = 10.77.0.0/24\n"
            "PersistentKeepalive = 25"),
        varning("Filen ska heta <b>egen.conf</b>, inte wg0.conf. Skriv <b>aldrig</b> AllowedIPs = 0.0.0.0/0, "
                "för då går all robotens trafik genom er tunnel och Mapros support slutar fungera."),
        p("Starta tunneln på roboten. Den startar sedan av sig själv vid varje uppstart:"),
        kod("sudo systemctl enable --now wg-quick@egen"),

        h1("7. Starta tunneln på datorn"),
        p("Fyll i robotens publika nyckel i <b>/etc/wireguard/egen.conf</b> på datorn (raden PublicKey under "
          "# Roboten) och starta tunneln:"),
        kod("sudo systemctl enable --now wg-quick@egen\n"
            "sudo wg show egen"),
        p("Under <b>peer</b> ska det stå <b>latest handshake: … seconds ago</b> inom en halv minut. Då är tunneln uppe."),

        h1("8. Prova"),
        punkter([
            "<b>ping 10.77.0.2</b> från datorn ska ge svar.",
            "<b>Robotstyrning:</b> anslut till <b>10.77.0.2</b>. Kamerabilden och panelen ska komma upp.",
            "<b>RControlStation:</b> skriv <b>10.77.0.2</b> i adressrutan vid maskinlistan och tryck på knappen "
            "<b>Text</b> (Connect). Porten (8300) väljs automatiskt.",
        ]),
        p("Fungerar allt: gå vidare till kapitel 9. Fungerar det inte: se Felsökning. Datorn har kvar Mapros tunnel "
          "tills ni gör kapitel 9, så ni kan alltid gå tillbaka."),

        h1("9. Ta bort Mapros tunnel från datorn"),
        p("Först när kapitel 8 fungerar. På <b>datorn</b> (inte på roboten):"),
        kod("sudo systemctl disable --now wg-quick@wg0\n"
            "sudo mv /etc/wireguard/wg0.conf /etc/wireguard/wg0.conf.mapro-avstangd"),
        p("Filen sparas under ett nytt namn. Behöver ni Mapros hjälp på datorn senare kan den slås på igen. "
          "<b>Meddela Mapro</b> när ni har gjort det, så tar vi bort er dator ur vår tunnel."),

        h1("10. Det här ändras i RControlStation"),
        punkter([
            "<b>Robotlistan, gårdarna och fälten</b> hämtas från Mapros server. De syns inte längre när datorn har "
            "lämnat Mapros tunnel. Skriv robotens adress (10.77.0.2) i adressrutan och tryck <b>Text</b> i stället.",
            "<b>Spara era banor och fält som filer</b> innan kapitel 9, så att ni har dem lokalt (Save Routes).",
            "<b>Uppdateringar</b> av programmen på datorn (uppdatera_dator.sh) fungerar som förut. De hämtas över internet.",
            "<b>Robotens RTK</b> påverkas inte. Korrektionerna hämtas av roboten själv.",
        ]),

        h1("11. Felsökning"),
        tabell([
            ["Det händer", "Gör så här"],
            ["Ingen <i>latest handshake</i>", "Kontrollera portvidarebefordran (UDP 51821), kontorets adress i "
             "Endpoint och att nycklarna inte är förväxlade: datorns publika nyckel på roboten, robotens på datorn."],
            ["Handskakning, men ping svarar inte", "Kontrollera Address och AllowedIPs i båda filerna (10.77.0.1 och 10.77.0.2)."],
            ["Robotstyrning: <i>Connection refused</i>", "robotd lyssnar inte på er tunnel än. Kontakta Mapro (kapitel 3)."],
            ["Kamerabilden hackar mer än förut", "Kontrollera att MTU = 1280 står i båda filerna."],
            ["Mapro når inte roboten", "Kontrollera på roboten att <b>sudo wg show wg0</b> visar en handskakning och "
             "att ingen fil har AllowedIPs = 0.0.0.0/0."],
        ], [1.6, 3.4]),
    ]
    bygg(os.path.join(UT, "Eget WireGuard - för kunder.pdf"), "Eget WireGuard",
         "Er egen tunnel mellan dator och robot",
         "Så sätter ni upp en egen WireGuard-tunnel mellan er dator och er robot. Robotens tunnel till Mapro "
         "ligger kvar för support och uppdateringar, medan er dator lämnar Mapros tunnel.",
         c)


if __name__ == "__main__":
    os.makedirs(UT, exist_ok=True)
    guide()
    print("Klart:", UT)
