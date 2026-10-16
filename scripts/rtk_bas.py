#!/usr/bin/env python3
"""RTK-korrektioner från Swepos med robotens egen position (car_rtk.service).

Swepos räknar fram en virtuell basstation där roboten säger att den står
(GGA som str2str skickar med `-p lat lon höjd`). Förut stod en fast,
påhittad punkt i car_rtk.service, ibland mil från roboten.

Nu:
1. Ingen uppkoppling mot Swepos förrän roboten själv har en GPS-position
   (GGA från rtkrcv på port 2948, kvalitet 1/2/4/5, minst 4 satelliter,
   några meningar i rad). Så hamnar basen aldrig där roboten stod förra
   gången (hemma, när den startas på en mässa).
2. Sedan startas str2str med den positionen.
3. Flyttas roboten mer än FLYTT_KM utan omstart startas str2str om med den
   nya positionen.

Läget skrivs till STATUSFIL för statusskärmen.

Användning (allt efter skriptets namn är str2str-kommandot utan -p):
  rtk_bas.py /usr/bin/str2str -in ntrip://... -out tcpsvr://:1234 -msg ... -n 1
"""

import json
import math
import os
import signal
import socket
import subprocess
import sys
import time

GGA_ADDR = (os.environ.get("RTK_BAS_GGA_HOST", "127.0.0.1"), int(os.environ.get("RTK_BAS_GGA_PORT", "2948")))
STATUSFIL = os.environ.get("RTK_BAS_STATUS", "/run/rtk_bas.json")
GODA_I_RAD = 3          # GGA-meningar i rad med position innan basen sätts
MIN_SAT = 4
FLYTT_KM = 5.0
OMSTART_S = 5

barn = None


def logg(text):
    print(text, flush=True)


def skriv_status(lage, pos=None):
    data = {"lage": lage, "tid": int(time.time())}
    if pos:
        data.update({"lat": round(pos[0], 6), "lon": round(pos[1], 6), "hojd": round(pos[2], 1)})
    try:
        tmp = STATUSFIL + ".tmp"
        with open(tmp, "w") as f:
            json.dump(data, f)
        os.chmod(tmp, 0o644)
        os.replace(tmp, STATUSFIL)
    except OSError as e:
        logg(f"Kan inte skriva {STATUSFIL}: {e}")


def grader(v, h):
    """NMEA ddmm.mmmm + N/S/E/W -> decimalgrader."""
    if not v:
        return None
    p = v.index(".") if "." in v else len(v)
    d = float(v[:p - 2]) + float(v[p - 2:]) / 60.0
    return -d if h in ("S", "W") else d


def tolka_gga(rad):
    """(lat, lon, höjd) om meningen är en GGA med användbar position, annars None."""
    rad = rad.strip()
    if not rad.startswith("$") or "GGA" not in rad[:7]:
        return None
    f = rad.split("*")[0].split(",")
    if len(f) < 12:
        return None
    try:
        kval = int(f[6] or 0)
        sat = int(f[7] or 0)
        if kval not in (1, 2, 4, 5) or sat < MIN_SAT:
            return None
        lat, lon = grader(f[2], f[3]), grader(f[4], f[5])
        hojd = float(f[9] or 0) + float(f[11] or 0)  # över havet + geoid = ellipsoidhöjd
    except ValueError:
        return None
    if lat is None or lon is None or (abs(lat) < 0.1 and abs(lon) < 0.1):
        return None
    return (lat, lon, hojd)


def avstand_km(a, b):
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    x = (lo2 - lo1) * math.cos((la1 + la2) / 2)
    return 6371.0 * math.hypot(x, la2 - la1)


def gga_strom():
    """Ger positioner (eller None för meningar utan position) från rtkrcv. Återansluter själv."""
    while True:
        try:
            with socket.create_connection(GGA_ADDR, timeout=5) as s:
                s.settimeout(15)
                buf = b""
                while True:
                    data = s.recv(4096)
                    if not data:
                        break
                    buf += data
                    while b"\n" in buf:
                        rad, buf = buf.split(b"\n", 1)
                        text = rad.decode(errors="replace")
                        if "GGA" in text:
                            yield tolka_gga(text)
                        else:
                            yield "annat"
        except OSError:
            pass
        yield None
        time.sleep(2)


def starta(cmd, pos):
    global barn
    full = cmd + ["-p", f"{pos[0]:.6f}", f"{pos[1]:.6f}", f"{pos[2]:.1f}"]
    logg(f"Basen sätts vid robotens position {pos[0]:.6f} {pos[1]:.6f} {pos[2]:.1f} m. Startar str2str.")
    barn = subprocess.Popen(full)
    skriv_status("ansluten", pos)


def stoppa():
    global barn
    if barn and barn.poll() is None:
        barn.terminate()
        try:
            barn.wait(5)
        except subprocess.TimeoutExpired:
            barn.kill()
    barn = None


def avsluta(*_):
    stoppa()
    skriv_status("stoppad")
    sys.exit(0)


def main():
    cmd = sys.argv[1:]
    if not cmd or "-p" in cmd:
        logg("Användning: rtk_bas.py /usr/bin/str2str -in ... -out ... (utan -p)")
        sys.exit(2)
    signal.signal(signal.SIGTERM, avsluta)
    signal.signal(signal.SIGINT, avsluta)

    skriv_status("vantar_gps")
    logg("Väntar på robotens egen GPS-position innan Swepos kopplas upp.")
    bas = None          # positionen str2str startades med
    senaste = None      # senaste användbara position
    i_rad = 0
    langt_bort = 0
    global barn
    for pos in gga_strom():
        # str2str har avslutats (nät borta e.d.): starta om med senaste positionen.
        if barn is not None and barn.poll() is not None:
            logg(f"str2str avslutades (kod {barn.returncode}), startar om om {OMSTART_S} s.")
            barn = None
            time.sleep(OMSTART_S)
            bas = senaste or bas
            starta(cmd, bas)
            continue

        if pos == "annat":
            continue    # GSV/GSA m.m.: räknas inte, nollställer inte heller
        if pos is None:
            i_rad = 0
        else:
            i_rad += 1
            senaste = pos

        if bas is None:
            if i_rad >= GODA_I_RAD:
                bas = senaste
                starta(cmd, bas)
            continue

        if pos is not None and avstand_km(bas, pos) > FLYTT_KM:
            langt_bort += 1
            if langt_bort >= GODA_I_RAD:
                logg(f"Roboten har flyttats {avstand_km(bas, pos):.1f} km från basen: ny bas.")
                stoppa()
                bas = pos
                langt_bort = 0
                starta(cmd, bas)
        else:
            langt_bort = 0


if __name__ == "__main__":
    main()
