#!/usr/bin/env python3
"""kursdelare — kurs med två antenner på MP101 (moving base), och delning av GPS 1.

MP101 har två ZED-F9P på USB. För kursen ska GPS 1 (moving base) skicka RTCM 4072.0 + MSM7
till GPS 2, som räknar vektorn mellan antennerna (UBX-NAV-RELPOSNED). Kortet saknar kabel
mellan dem, så det här programmet gör det över USB. Samtidigt behöver Car_Client (RCS-läget)
eller robotd (SentinAnt-läget) GPS 1, och en serieport kan bara ha en ägare. Därför:

  - kursdelaren äger de riktiga portarna /dev/ublox-gps1 och /dev/ublox-gps2 (udev-regeln
    12-kursdelare.rules, se install_kursdelare.sh),
  - /dev/ublox och /dev/rtk blir länkar till en pty med exakt GPS 1:s trafik åt båda håll,
    så att Car_Client/robotd fungerar som förut,
  - RTCM-ramar från GPS 1 skickas vidare till GPS 2,
  - kursen skrivs till /run/kurs: "<kurs> <noggrannhet> <unix-s> <fix> <längd_m>\\n"
    (kurs i kompassgrader för robotens färdriktning, noggrannhet i grader, fix 0/1/2 =
    ingen/float/FIX). Car_Client och robotd läser filen och skickar kursen till styrkortet.

Inställningarna i GPS:erna görs bara i RAM och skrivs om var 20:e sekund (Car_Client ställer
också in GPS 1). /etc/kursdelare.json: {"kurs_offset_deg": -91, "min_langd_m": 0.5,
"max_langd_m": 1.5}. kurs_offset_deg = färdriktning − baslinje (GPS 1 → GPS 2), uppmätt
genom att köra rakt fram (autorobot scripts/prova_kurs.py). mk1 2026-10-28: −91 (GPS 1 till
vänster, GPS 2 till höger).

    sudo python3 scripts/kursdelare.py            (normalt som tjänst, kursdelare.service)
"""
import fcntl, json, os, select, signal, struct, sys, time, tty

GPS1, GPS2 = '/dev/ublox-gps1', '/dev/ublox-gps2'
LANKAR = ['/dev/ublox', '/dev/rtk']
KURS_FIL = '/run/kurs'
KONF = '/etc/kursdelare.json'

# CFG-MSGOUT-..._USB (u-blox F9 HPG): moving base på GPS 1, RELPOSNED från GPS 2
RTCM_UT = [0x20910301, 0x20910361, 0x20910366, 0x2091036b, 0x20910370, 0x20910306]
RELPOSNED_USB = 0x20910090
OMSTALL_S = 20.0
TIOCNXCL = 0x540D   # Car_Client (QSerialPort) låser porten exklusivt; låset sitter kvar så
                    # länge vi håller slavsidan öppen, och då kan den aldrig öppna igen


def logg(t):
    print(time.strftime('%H:%M:%S'), t, flush=True)


def ubx(cls, mid, payload=b''):
    body = bytes([cls, mid]) + struct.pack('<H', len(payload)) + payload
    a = b = 0
    for x in body:
        a = (a + x) & 0xFF
        b = (b + a) & 0xFF
    return b'\xb5\x62' + body + bytes([a, b])


def valset(items):
    p = struct.pack('<BBH', 0, 0x01, 0)   # RAM
    for k, v in items:
        p += struct.pack('<IB', k, v)
    return ubx(0x06, 0x8A, p)


def oppna(dev):
    fd = os.open(dev, os.O_RDWR | os.O_NOCTTY)
    tty.setraw(fd)
    return fd


def skriv(fd, data):
    """Skriver allt (blockerande); en port som inte tar emot får inte stoppa resten länge."""
    slut = time.time() + 1.0
    while data and time.time() < slut:
        try:
            n = os.write(fd, data)
            data = data[n:]
        except BlockingIOError:
            time.sleep(0.005)


class RtcmRamar:
    """Plockar hela RTCM3-ramar ur en ström (annat ignoreras)."""
    def __init__(self):
        self.buf = b''

    def mata(self, data):
        self.buf += data
        ut = []
        while True:
            i = self.buf.find(b'\xd3')
            if i < 0:
                self.buf = b''
                return ut
            self.buf = self.buf[i:]
            if len(self.buf) < 3:
                return ut
            n = ((self.buf[1] & 0x03) << 8) | self.buf[2]
            if self.buf[1] & 0xFC or n > 1023:
                self.buf = self.buf[1:]
                continue
            if len(self.buf) < n + 6:
                return ut
            ut.append(self.buf[:n + 6])
            self.buf = self.buf[n + 6:]


class Relposned:
    def __init__(self):
        self.buf = b''

    def mata(self, data):
        self.buf = (self.buf + data)[-8192:]
        ut = []
        while True:
            i = self.buf.find(b'\xb5\x62\x01\x3c\x40\x00')
            if i < 0 or len(self.buf) < i + 6 + 64 + 2:
                if i > 0:
                    self.buf = self.buf[i:]
                return ut
            p = self.buf[i + 6:i + 70]
            self.buf = self.buf[i + 72:]
            langd_cm, head = struct.unpack_from('<ii', p, 20)
            hp = struct.unpack_from('<b', p, 35)[0]   # relPosHPLength, 0,1 mm
            acc_h = struct.unpack_from('<I', p, 52)[0]
            flags = struct.unpack_from('<I', p, 60)[0]
            ut.append(dict(langd_m=langd_cm / 100.0 + hp / 10000.0, kurs=head * 1e-5,
                           acc=acc_h * 1e-5, fix=(flags >> 3) & 3, giltig=bool(flags & 0x100)))


def satt_lankar(slav):
    for l in LANKAR:
        tmp = l + '.ny'
        try:
            os.remove(tmp)
        except FileNotFoundError:
            pass
        os.symlink(slav, tmp)
        os.replace(tmp, l)


def ta_bort_lankar(slav):
    for l in LANKAR:
        try:
            if os.readlink(l) == slav:
                os.remove(l)
        except OSError:
            pass


def main():
    konf = {'kurs_offset_deg': -91.0, 'min_langd_m': 0.5, 'max_langd_m': 1.5}
    try:
        konf.update(json.load(open(KONF)))
    except FileNotFoundError:
        pass
    except ValueError as e:
        logg(f'{KONF}: {e}; använder standardvärden')

    master, slav_fd = os.openpty()
    tty.setraw(slav_fd)
    tty.setraw(master)
    slav = os.ttyname(slav_fd)
    os.chmod(slav, 0o666)   # Car_Client kör som vanlig användare
    satt_lankar(slav)
    logg(f'GPS 1 delas på {slav} ({", ".join(LANKAR)}); kursoffset {konf["kurs_offset_deg"]:+.1f}°')

    f1 = f2 = None
    rtcm, rp = RtcmRamar(), Relposned()
    senast_cfg = 0.0
    senast_logg = 0.0
    senast_lås = 0.0
    sist = None
    antal_rtcm = 0
    try:
        while True:
            if f1 is None:
                try:
                    f1 = oppna(GPS1)
                    senast_cfg = 0
                    logg('GPS 1 öppnad')
                except OSError as e:
                    logg(f'GPS 1: {e}; försöker igen')
                    time.sleep(2)
                    continue
            if f2 is None:
                try:
                    f2 = oppna(GPS2)
                    senast_cfg = 0
                    logg('GPS 2 öppnad')
                except OSError:
                    pass   # utan GPS 2 delas GPS 1 ändå, men ingen kurs
            if time.time() - senast_lås > 1.0:
                senast_lås = time.time()
                try:
                    fcntl.ioctl(slav_fd, TIOCNXCL)
                except OSError:
                    pass
            if time.time() - senast_cfg > OMSTALL_S:
                senast_cfg = time.time()
                skriv(f1, valset([(k, 1) for k in RTCM_UT]))
                if f2 is not None:
                    skriv(f2, valset([(RELPOSNED_USB, 1)]))
            lasa = [f1, master] + ([f2] if f2 is not None else [])
            try:
                r, _, _ = select.select(lasa, [], [], 0.5)
            except (OSError, ValueError):
                r = []
            try:
                if f1 in r:
                    d = os.read(f1, 16384)
                    if not d:
                        raise OSError('GPS 1 stängd')
                    try:
                        os.write(master, d)   # till Car_Client/robotd; tappas om ingen läser
                    except OSError:
                        pass
                    if f2 is not None:
                        for ram in rtcm.mata(d):
                            antal_rtcm += 1
                            skriv(f2, ram)
                if master in r:
                    d = os.read(master, 16384)
                    if d:
                        skriv(f1, d)   # kommandon och RTCM från Car_Client/robotd till GPS 1
            except OSError as e:
                logg(f'GPS 1: {e}; öppnar igen')
                try:
                    os.close(f1)
                except OSError:
                    pass
                f1 = None
                continue
            if f2 is not None and f2 in r:
                try:
                    d = os.read(f2, 16384)
                    if not d:
                        raise OSError('stängd')
                    for m in rp.mata(d):
                        sist = m
                        ok = (m['giltig'] and m['fix'] >= 1 and
                              konf['min_langd_m'] <= m['langd_m'] <= konf['max_langd_m'])
                        if ok:
                            kurs = (m['kurs'] + konf['kurs_offset_deg']) % 360.0
                            tmp = KURS_FIL + '.ny'
                            with open(tmp, 'w') as f:
                                f.write(f"{kurs:.2f} {m['acc']:.2f} {time.time():.1f} {m['fix']} {m['langd_m']:.3f}\n")
                            os.replace(tmp, KURS_FIL)
                except OSError as e:
                    logg(f'GPS 2: {e}; öppnar igen')
                    try:
                        os.close(f2)
                    except OSError:
                        pass
                    f2 = None
            if time.time() - senast_logg > 60:
                senast_logg = time.time()
                if sist:
                    logg(f"kurs: fix {sist['fix']}, längd {sist['langd_m']:.3f} m, ±{sist['acc']:.1f}°, "
                         f"giltig {int(sist['giltig'])}, RTCM vidare {antal_rtcm}")
                else:
                    logg(f'ingen RELPOSNED än (GPS 2 {"öppen" if f2 is not None else "saknas"}), RTCM vidare {antal_rtcm}')
    finally:
        ta_bort_lankar(slav)
        for fd, k in ((f1, RTCM_UT), (f2, [RELPOSNED_USB])):
            if fd is not None:
                try:
                    skriv(fd, valset([(x, 0) for x in k]))
                except OSError:
                    pass


if __name__ == '__main__':
    # systemctl stop: städa länkarna och GPS-inställningarna (finally i main)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(0))
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(0)
