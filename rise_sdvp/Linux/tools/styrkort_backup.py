#!/usr/bin/env python3
"""styrkort_backup.py — sparar styrkortets inställningar och VESC-listan före en
uppdatering, och kontrollerar (och vid behov återställer) dem efteråt.

Körs på robotens Pi via Car_Client (RControlStation/Robotstyrning ska vara
frånkopplade, Car_Client tar en TCP-klient). Används av scripts/uppdatera_pi.sh.

  python3 styrkort_backup.py spara FIL.json     läs kortet och spara (ändrar inget)
  python3 styrkort_backup.py jamfor FIL.json    jämför kortet med FIL, ändrar inget
  python3 styrkort_backup.py aterstall FIL.json jämför; skiljer inställningarna
                                                 skrivs de sparade tillbaka (som Write
                                                 i RControlStation) och kontrolläses

Inställningarna = allt som RControlStations Write skickar: ID, fordonsdata,
vinkelgivarens kalibrering och motorlistan (aktuatorerna). Återställningen görs
bara om inställningsblocket har samma storlek som förut; har firmwaren ändrat
blockets uppbyggnad sägs det rakt ut i stället.

Avslutar med kod 0 = lika (eller återställt), 1 = skiljer/fel, 2 = kortet svarar inte.
"""
import json
import os
import socket
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from testa_write import rama_in, plocka_paket, rcs_slut  # noqa: E402

PORT = 8300
CMD_GET_STATE = 120
CMD_SET_MAIN_CONFIG = 77
CMD_GET_MAIN_CONFIG = 78
CMD_PING_VESC_CAN = 141
TIDSGRANS = 6.0


class Kort:
    def __init__(self, host="127.0.0.1", forsok=8):
        sista = None
        for _ in range(forsok):
            try:
                self.s = socket.create_connection((host, PORT), timeout=TIDSGRANS)
                self.buf = b""
                return
            except OSError as e:
                sista = e
                time.sleep(2)
        raise ConnectionError("når inte Car_Client: %s" % sista)

    def fraga(self, payload, cmd, tid=TIDSGRANS):
        self.s.sendall(rama_in(payload))
        slut = time.time() + tid
        while time.time() < slut:
            self.s.settimeout(max(0.1, slut - time.time()))
            try:
                d = self.s.recv(4096)
            except socket.timeout:
                break
            if not d:
                raise ConnectionError("Car_Client stängde anslutningen (är RControlStation/Robotstyrning ansluten?)")
            self.buf += d
            paket, self.buf = plocka_paket(self.buf)
            for p in paket:
                if len(p) >= 2 and p[1] == cmd:
                    return p
        return None

    def version(self):
        p = self.fraga(bytes([255, CMD_GET_STATE]), CMD_GET_STATE)
        return "%d.%d" % (p[2], p[3]) if p and len(p) >= 4 else None

    def config(self):
        return self.fraga(bytes([255, CMD_GET_MAIN_CONFIG]), CMD_GET_MAIN_CONFIG)

    def vescer(self):
        """VESC-ID som svarar på ping (firmware 30.4+). None = kortet kan inte pinga."""
        p = self.fraga(bytes([255, CMD_PING_VESC_CAN]), CMD_PING_VESC_CAN, tid=4.0)
        return sorted(p[2:]) if p else None

    def skriv(self, forsta_delen):
        payload = bytes([forsta_delen[0], CMD_SET_MAIN_CONFIG]) + forsta_delen[2:]
        return self.fraga(payload, CMD_SET_MAIN_CONFIG, tid=10.0)


def aktuatorer(conf):
    """Motorlistan i klartext: [(typ, vesc_id, activity, mode), ...]. Blocket är
    antal (u16) + 4 platser med typ, motorid, activity, mode (u16 var), commands.c."""
    start = rcs_slut(conf) - 34
    n = struct.unpack(">H", conf[start:start + 2])[0]
    ut = []
    for i in range(min(n, 4)):
        o = start + 2 + i * 8
        ut.append(list(struct.unpack(">4H", conf[o:o + 8])))
    return ut


def las(k):
    conf = k.config()
    if not conf:
        return None
    return {
        "tid": time.strftime("%Y-%m-%d %H:%M:%S"),
        "firmware": k.version(),
        "id": conf[0],
        "config_hex": conf.hex(),
        "installningar_langd": rcs_slut(conf),
        "aktuatorer": aktuatorer(conf),
        "vesc": k.vescer(),
    }


def jamfor(fore, nu):
    """Skillnader i klartext (tom lista = lika)."""
    fel = []
    if fore["id"] != nu["id"]:
        fel.append("bil-ID %s -> %s" % (fore["id"], nu["id"]))
    a, b = bytes.fromhex(fore["config_hex"]), bytes.fromhex(nu["config_hex"])
    if fore["installningar_langd"] != nu["installningar_langd"]:
        fel.append("inställningsblocket har ändrat storlek (%d -> %d byte): ny firmware med annan uppbyggnad"
                   % (fore["installningar_langd"], nu["installningar_langd"]))
    elif a[:fore["installningar_langd"]] != b[:nu["installningar_langd"]]:
        if fore["aktuatorer"] != nu["aktuatorer"]:
            fel.append("motorlistan %s -> %s" % (fore["aktuatorer"], nu["aktuatorer"]))
        else:
            fel.append("inställningarna (fordonsdata/kalibrering) skiljer sig")
    if fore.get("vesc") is not None and nu.get("vesc") is not None:
        saknas = sorted(set(fore["vesc"]) - set(nu["vesc"]))
        if saknas:
            fel.append("VESC som svarade före men inte nu: %s" % saknas)
    return fel


def main():
    if len(sys.argv) < 3 or sys.argv[1] not in ("spara", "jamfor", "aterstall"):
        print(__doc__)
        sys.exit(1)
    lage, fil = sys.argv[1], sys.argv[2]
    try:
        k = Kort()
        nu = las(k)
    except ConnectionError as e:
        print("Styrkortet: %s" % e)
        sys.exit(2)
    if not nu:
        print("Styrkortet svarar inte på läsning av inställningarna.")
        sys.exit(2)

    if lage == "spara":
        with open(fil, "w") as f:
            json.dump(nu, f, indent=1)
        print("Sparat: firmware %s, bil-ID %d, %d motorer %s, VESC som svarar: %s"
              % (nu["firmware"], nu["id"], len(nu["aktuatorer"]), nu["aktuatorer"],
                 nu["vesc"] if nu["vesc"] is not None else "(kan inte pinga, äldre firmware)"))
        return

    with open(fil) as f:
        fore = json.load(f)
    fel = jamfor(fore, nu)
    if not fel:
        print("Styrkortet: inställningar, motorlista och VESC som före (firmware %s -> %s)."
              % (fore["firmware"], nu["firmware"]))
        return
    for r in fel:
        print("Skillnad: %s" % r)
    if lage != "aterstall":
        sys.exit(1)

    if fore["installningar_langd"] != nu["installningar_langd"]:
        print("Återställer inte automatiskt (annan uppbyggnad). Kontrollera i RControlStation, "
              "de gamla värdena finns i %s." % fil)
        sys.exit(1)
    if not any("motorlistan" in r or "inställningarna" in r or "bil-ID" in r for r in fel):
        sys.exit(1)  # bara VESC saknas: inget att skriva tillbaka

    gammal = bytes.fromhex(fore["config_hex"])[:fore["installningar_langd"]]
    print("Skriver tillbaka de sparade inställningarna...")
    if not k.skriv(gammal):
        print("Kortet kvitterade inte skrivningen.")
    time.sleep(1.0)
    efter = las(k)
    if efter and not [r for r in jamfor(fore, efter) if "VESC" not in r]:
        print("Återställt: inställningarna och motorlistan är som före uppdateringen.")
        sys.exit(1 if any("VESC" in r for r in jamfor(fore, efter)) else 0)
    print("Återställningen lyckades inte. De gamla värdena finns i %s." % fil)
    sys.exit(1)


if __name__ == "__main__":
    main()
