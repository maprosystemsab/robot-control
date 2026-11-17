#!/usr/bin/env python3
"""satt_heartbeat.py — läser och sätter styrkortets heartbeat-timeout LIVE, utan
att flasha om firmwaren. Heartbeat = hur länge kortet behåller sista
körkommandot innan det bromsar när kommandon slutar komma (firmwarens default är
3,0 s, vilket ger flera dm rullning om kopplingen tappas).

Körs på robotens Pi via Car_Client (RControlStation/Robotstyrning måste vara
frånkopplade — Car_Client tar en TCP-klient).

  python3 satt_heartbeat.py            visa nuvarande heartbeat (ändrar inget)
  python3 satt_heartbeat.py 1.0        sätt heartbeat till 1,0 sekund

Säkerhet: heartbeat ligger som en float32_auto direkt före aktuatorerna i
main_config (offset rcs_slut-38). Verktyget läser av värdet där och vägrar skriva
om det inte ser ut som ett rimligt heartbeat-värde (0,05–20 s) — då är offseten
fel (annan firmware) och inget rörs. Skriver tillbaka exakt samma block som
RControlStations Write (t.o.m. aktuatorerna), bara med heartbeat ändrad.
"""
import math
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from styrkort_backup import Kort          # noqa: E402  (samma transport)
from testa_write import rcs_slut          # noqa: E402

RIMLIG_MIN, RIMLIG_MAX = 0.05, 20.0


def enc(number):
    """float32_auto-kodning, identisk med buffer_append_float32_auto i firmwaren."""
    if number == 0.0:
        return b"\x00\x00\x00\x00"
    sig, e = math.frexp(number)           # number = sig * 2^e, 0.5 <= |sig| < 1
    sig_abs = abs(sig)
    sig_i = 0
    if sig_abs >= 0.5:
        sig_i = int((sig_abs - 0.5) * 2.0 * 8388608.0)
        e += 126
    res = ((e & 0xFF) << 23) | (sig_i & 0x7FFFFF)
    if sig < 0:
        res |= 1 << 31
    return struct.pack(">I", res)


def dec(b):
    """Invers av enc()."""
    res = struct.unpack(">I", b)[0]
    e = (res >> 23) & 0xFF
    sig_i = res & 0x7FFFFF
    neg = res & (1 << 31)
    if e == 0 and sig_i == 0:
        return 0.0
    sig = sig_i / 8388608.0 / 2.0 + 0.5
    v = math.ldexp(sig, e - 126)
    return -v if neg else v


def main():
    mal = None
    if len(sys.argv) >= 2:
        try:
            mal = float(sys.argv[1].replace(",", "."))
        except ValueError:
            print("Ogiltigt värde: %s (ange sekunder, t.ex. 1.0)" % sys.argv[1])
            sys.exit(1)

    try:
        k = Kort()
    except ConnectionError as e:
        print("Styrkortet: %s" % e)
        sys.exit(2)

    conf = k.config()
    if not conf:
        print("Styrkortet svarar inte på läsning av inställningarna.")
        sys.exit(2)

    slut = rcs_slut(conf)
    off = slut - 38
    nuv = dec(conf[off:off + 4])
    print("Nuvarande heartbeat: %.3f s (bytes %s @ offset %d, blockslut %d)"
          % (nuv, conf[off:off + 4].hex(), off, slut))

    if mal is None:
        return
    if not (RIMLIG_MIN <= nuv <= RIMLIG_MAX):
        print("AVBRYTER: värdet på heartbeat-offseten (%.3f) ser inte ut som ett "
              "rimligt heartbeat — fel offset/annan firmware. Inget ändrat." % nuv)
        sys.exit(1)
    if not (RIMLIG_MIN <= mal <= RIMLIG_MAX):
        print("AVBRYTER: %.3f s ligger utanför det rimliga intervallet %.2f–%.1f s."
              % (mal, RIMLIG_MIN, RIMLIG_MAX))
        sys.exit(1)
    if conf[off:off + 4] == enc(mal):
        print("Heartbeat är redan %.3f s — inget att göra." % mal)
        return

    ny = bytearray(conf)
    ny[off:off + 4] = enc(mal)
    print("Skriver heartbeat %.3f s -> %.3f s ..." % (nuv, mal))
    if not k.skriv(bytes(ny)[:slut]):
        print("Kortet kvitterade inte skrivningen.")
        sys.exit(1)

    time.sleep(1.0)
    conf2 = k.config()
    if not conf2:
        print("Kunde inte kontrolläsa efter skrivningen.")
        sys.exit(1)
    efter = dec(conf2[rcs_slut(conf2) - 38:rcs_slut(conf2) - 34])
    if abs(efter - mal) < 1e-3:
        print("KLART: heartbeat är nu %.3f s (bekräftat av kortet)." % efter)
    else:
        print("VARNING: kortet rapporterar %.3f s efter skrivningen (väntat %.3f)." % (efter, mal))
        sys.exit(1)


if __name__ == "__main__":
    main()
