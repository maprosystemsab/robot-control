#!/usr/bin/env python3
"""Provar uppdateringen av styrkortets firmware i den RIKTIGA robotd, utan hårdvara: ett
låtsas-Car_Client (TCP, samma paket som styrkortet), en liten webbserver i stället för vår
server och ett skript i stället för openocd. Listorna signeras med PROVNYCKELN, så robotd
måste vara byggd med den påslagen:

    cargo build -p robotd --features provnyckel
    python3 scripts/testa_firmware_latsas.py            # --behall sparar mappen

Kontrollerar hela varvet (sök, uppdatera, klart med ny version), spärrarna (AKTIVERA,
rörelse, fler anslutna, bara föraren), att inga körvärden når kortet under skrivningen,
att kortets inställningar sparas och skrivs tillbaka om skrivningen ändrat dem, och att
den förra firmwaren skrivs tillbaka när kortet svarar med fel version. Rör ingen hårdvara.
"""
import base64, functools, hashlib, http.server, json, os, shutil, socket, struct, subprocess, sys, tempfile, threading, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PAGAR = 'Styrkortet uppdateras'
CMD_GET_STATE, CMD_SET_MAIN_CONFIG, CMD_GET_MAIN_CONFIG, CMD_RC_CONTROL_ADV = 120, 77, 78, 125
# VESC-ping på CAN (141), provsnurr (142) och terminalkommandon (1): ska aldrig nå kortet under en uppdatering.
ANDRA_KOMMANDON = (141, 142, 1)


# --- paket som Car_Client och styrkortet: [2][längd][innehåll][CRC16][3] ---
def crc16(data):
    crc = 0
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc


def rama_in(p):
    huvud = bytes([2, len(p)]) if len(p) <= 255 else bytes([3]) + struct.pack('>H', len(p))
    return huvud + p + struct.pack('>H', crc16(p)) + bytes([3])


def plocka_paket(buf):
    paket = []
    while buf:
        if buf[0] == 2 and len(buf) >= 2:
            n, start = buf[1], 2
        elif buf[0] == 3 and len(buf) >= 3:
            n, start = struct.unpack('>H', buf[1:3])[0], 3
        elif buf[0] in (2, 3):
            break
        else:
            buf = buf[1:]
            continue
        if len(buf) < start + n + 3:
            break
        p = buf[start:start + n]
        if struct.unpack('>H', buf[start + n:start + n + 2])[0] == crc16(p) and buf[start + n + 2] == 3:
            paket.append(p)
            buf = buf[start + n + 3:]
        else:
            buf = buf[1:]
    return paket, buf


class Kort:
    """Låtsas-Car_Client med ett styrkort bakom: svarar på tillstånd och inställningar.
    Versionen står i filen fw_version (som "flashningen" byter). Filer i mappen styr felen."""

    def __init__(self, mapp, car_id=4):
        self.mapp, self.car_id = mapp, car_id
        self.installningar = bytes(range(256)) + b'robant\0' + bytes(200)
        self.korvarden = []          # (tid, värde) för körkommandon som inte är noll
        self.andra = []              # kommandon utöver tillstånd, inställningar och körvärden (ping, provsnurr …)
        self.srv = socket.socket()
        self.srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.srv.bind(('127.0.0.1', 0))
        self.srv.listen(1)
        self.port = self.srv.getsockname()[1]
        threading.Thread(target=self.kor, daemon=True).start()

    def flagga(self, namn):
        return os.path.exists(os.path.join(self.mapp, namn))

    def version(self):
        try:
            a, b = open(os.path.join(self.mapp, 'fw_version')).read().strip().split('.')
            return int(a), int(b)
        except (OSError, ValueError):
            return None

    def tillstand(self, fw):
        p = bytearray(129)
        p[0], p[1], p[2], p[3] = self.car_id, CMD_GET_STATE, fw[0], fw[1]
        struct.pack_into('>i', p, 64, int(53.0 * 1e6))   # batterispänning
        struct.pack_into('>i', p, 68, int(30.0 * 1e6))   # temperatur
        return bytes(p)

    def kor(self):
        while True:
            s, _ = self.srv.accept()
            buf = b''
            try:
                while True:
                    d = s.recv(4096)
                    if not d:
                        break
                    paket, buf = plocka_paket(buf + d)
                    for p in paket:
                        fw = self.version()
                        if fw is None or len(p) < 2:
                            continue   # kortet är tyst (ingen fungerande firmware)
                        if p[1] == CMD_GET_STATE:
                            s.sendall(rama_in(self.tillstand(fw)))
                        elif p[1] == CMD_GET_MAIN_CONFIG and not self.flagga('tyst_installningar'):
                            if self.flagga('andra_installningar'):
                                os.remove(os.path.join(self.mapp, 'andra_installningar'))
                                self.installningar = bytes([self.installningar[0] ^ 0x55]) + self.installningar[1:]
                            s.sendall(rama_in(bytes([self.car_id, CMD_GET_MAIN_CONFIG]) + self.installningar))
                        elif p[1] == CMD_SET_MAIN_CONFIG:
                            self.installningar = bytes(p[2:])
                            s.sendall(rama_in(bytes([self.car_id, CMD_SET_MAIN_CONFIG])))
                        elif p[1] == CMD_RC_CONTROL_ADV and len(p) >= 7 and p[3:7] != b'\0\0\0\0':
                            self.korvarden.append((time.time(), p[2]))
                        elif p[1] in ANDRA_KOMMANDON:
                            self.andra.append(p[1])
            except OSError:
                pass
            finally:
                s.close()


FLASHA = r"""#!/bin/sh
D='MAPPEN'
[ -e "$D/langsam" ] && sleep "$(cat "$D/langsam")"
case "$1" in
  dumpa)
    [ -e "$D/fel_dump" ] && { echo "Error: kortet svarar inte"; exit 1; }
    { printf '\000\000\002\040\245\301\000\010'; head -c 1048568 /dev/zero | tr '\000' '\377'; } > "$2" ;;
  skriv)
    cp "$D/fw_version" "$D/fw_forra"
    if [ -e "$D/fel_version" ]; then echo 30.1 > "$D/fw_version"; else tail -c +21 "$2" > "$D/fw_version"; fi
    if [ -e "$D/raderar_installningar" ]; then touch "$D/andra_installningar"; fi ;;
  aterstall) cp "$D/fw_forra" "$D/fw_version" ;;
esac
exit 0
"""


class Server:
    """Uppdateringsmappen på en webbserver på den här datorn, signerad med provnyckeln."""

    def __init__(self, bas):
        self.mapp = os.path.join(bas, 'srv')
        os.makedirs(self.mapp)
        fro = hashlib.sha256('SentinAnt provnyckel - INTE for riktiga uppdateringar'.encode()).digest()
        self.nyckel = os.path.join(bas, 'provnyckel.pem')
        with open(os.open(self.nyckel, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), 'w') as f:
            f.write('-----BEGIN PRIVATE KEY-----\n' + base64.encodebytes(bytes.fromhex('302e020100300506032b657004220420') + fro).decode() + '-----END PRIVATE KEY-----\n')
        self.lopnummer = 100

        class Tyst(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *a):
                pass

        self.httpd = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Tyst, directory=self.mapp))
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.adress = f'http://127.0.0.1:{self.httpd.server_address[1]}/'

    def publicera(self, version, variant, kraver=None):
        """En provfirmware: ELF-huvud för ARM följt av versionen (låtsasflasharen läser den)."""
        namn = f'firmware-{version}-{variant}.elf'
        data = bytes([0x7f, 0x45, 0x4c, 0x46, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 40, 0]) + version.encode()
        open(os.path.join(self.mapp, namn), 'wb').write(data)
        self.lopnummer += 1
        paket = {'version': version, 'nyheter': 'Provfirmware', 'filer': {variant: {'fil': namn, 'storlek': len(data), 'sha256': hashlib.sha256(data).hexdigest()}}}
        if kraver:
            paket['kraver'] = kraver
        text = json.dumps({'format': 1, 'lopnummer': self.lopnummer, 'skapad': '2026-11-26T20:00:00Z', 'paket': {'firmware': paket}}).encode()
        inn, ut = os.path.join(self.mapp, '.m'), os.path.join(self.mapp, '.sig')
        open(inn, 'wb').write(text)
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-inkey', self.nyckel, '-rawin', '-in', inn, '-out', ut], check=True)
        kuvert = {'manifest': text.decode(), 'signaturer': [open(ut, 'rb').read().hex()]}
        json.dump(kuvert, open(os.path.join(self.mapp, 'uppdatering.json'), 'w'))


# --- minsta möjliga websocket-klient ---
def ws_connect(port):
    for _ in range(100):
        try:
            s = socket.create_connection(('127.0.0.1', port), timeout=5)
            break
        except OSError:
            time.sleep(0.1)
    nyckel = base64.b64encode(os.urandom(16)).decode()
    s.sendall(f'GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {nyckel}\r\nSec-WebSocket-Version: 13\r\n\r\n'.encode())
    h = b''
    while b'\r\n\r\n' not in h:
        h += s.recv(1)
    assert b' 101 ' in h, h
    s.settimeout(0.1)
    return s


def ws_send(s, obj):
    d = json.dumps(obj).encode()
    mask, n = os.urandom(4), len(d)
    huvud = bytes([0x81]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack('>H', n))
    s.sendall(huvud + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(d)))


def ws_recv(s):
    def las(n):
        b = b''
        while len(b) < n:
            c = s.recv(n - len(b))
            if not c:
                raise ConnectionError('stängd')
            b += c
        return b
    b0, b1 = las(2)
    n = b1 & 0x7F
    if n == 126:
        n = struct.unpack('>H', las(2))[0]
    elif n == 127:
        n = struct.unpack('>Q', las(8))[0]
    d = las(n)
    return json.loads(d) if b0 & 0x0F == 1 else None


def control(throttle=0.0, activated=False):
    return {'Control': {'throttle': throttle, 'steering': 0.0, 'accessory_a': 0, 'accessory_b': 0, 'lights': False,
                        'activated': activated, 'timestamp_ms': int(time.time() * 1000), 'max_output': 1.0}}


class Robot:
    """Den riktiga robotd mot låtsaskortet, och en förare ansluten."""

    def __init__(self, bas, srv, variant, port):
        self.mapp = os.path.join(bas, 'robot')
        os.makedirs(self.mapp)
        open(f'{self.mapp}/fw_version', 'w').write('30.32')
        open(f'{self.mapp}/flasha.sh', 'w').write(FLASHA.replace('MAPPEN', self.mapp))
        os.chmod(f'{self.mapp}/flasha.sh', 0o755)
        self.kort = Kort(self.mapp)
        cfg = {'robot_id': 'prov', 'bind_addr': f'127.0.0.1:{port}', 'car_client_addr': f'127.0.0.1:{self.kort.port}', 'car_client_id': 4,
               'video': {'enabled': False},
               # Ingen vinkelgivare: annars frågar länken kortet om vinkeln med ett terminalkommando varje sekund.
               'vehicle': {'steering_sensor': False},
               'firmware': {'variant': variant, 'kalla': srv.adress, 'flashkommando': f'{self.mapp}/flasha.sh', 'mapp': f'{self.mapp}/robotuppdatering'}}
        json.dump(cfg, open(f'{self.mapp}/config.json', 'w'))
        self.proc = subprocess.Popen([f'{ROOT}/target/debug/robotd'], env={**os.environ, 'ROBOTD_CONFIG': f'{self.mapp}/config.json', 'RUST_LOG': 'info'},
                                     stdout=open(f'{self.mapp}/robotd.log', 'w'), stderr=subprocess.STDOUT)
        self.port = port
        self.last, self.svar, self.skanningar, self.aktuatorfel = {}, [], [], []
        self.s = self.anslut()

    def anslut(self, tittare=False):
        s = ws_connect(self.port)
        ws_send(s, {'Connect': {'as_viewer': tittare, 'view_code': None, 'client': 'robotstyrning'}})
        return s

    def run(self, secs, cmd=None, until=None):
        t0 = time.time()
        while time.time() - t0 < secs:
            ws_send(self.s, cmd or control())
            try:
                m = ws_recv(self.s)
            except socket.timeout:
                continue
            if not m:
                continue
            if 'Status' in m:
                self.last.update(m['Status'])
            if 'FirmwareSvar' in m:
                self.svar.append(m['FirmwareSvar'])
            if 'VescBusResult' in m:
                self.skanningar.append(m['VescBusResult'])
            if 'ActuatorError' in m:
                self.aktuatorfel.append(m['ActuatorError'])
            if until and until():
                return True
        return False

    def fw(self):
        return self.last.get('firmware') or {}

    def lage(self):
        l = self.fw().get('lage', 'Vilar')
        return (l, {}) if isinstance(l, str) else next(iter(l.items()))

    def vanta(self, *namn, secs=60):
        self.run(secs, until=lambda: self.lage()[0] in namn)
        return self.lage()

    def redo(self):
        return self.run(20, until=lambda: self.fw().get('kortversion') and not self.fw().get('sparrar'))

    def sok(self):
        self.run(0.8)
        ws_send(self.s, {'Firmware': 'Sok'})
        self.run(2.0, until=lambda: self.lage()[0] == 'Soker')
        return self.vanta('Aktuell', 'NyFinns', 'Hinder', 'Misslyckad', secs=20)

    def uppdatera(self, version, secs=90):
        self.svar.clear()
        ws_send(self.s, {'Firmware': {'Uppdatera': {'version': version}}})
        self.run(3.0, until=lambda: self.lage()[0] == 'Pagar' or self.svar)
        if self.svar:
            return ('Nej', self.svar[-1])
        return self.vanta('Klar', 'Misslyckad', secs=secs)

    def flagga(self, namn, pa=True, text=''):
        fil = f'{self.mapp}/{namn}'
        if pa:
            open(fil, 'w').write(text)
        elif os.path.exists(fil):
            os.remove(fil)

    def kortet(self):
        return open(f'{self.mapp}/fw_version').read().strip()

    def stop(self):
        self.proc.terminate()
        self.proc.wait()


def main():
    bas = tempfile.mkdtemp(prefix='firmware_latsas_')
    ok = True
    r = None

    def check(namn, villkor, extra=''):
        nonlocal ok
        print(f"{'OK ' if villkor else 'FEL'} {namn} {extra if not villkor else ''}")
        ok &= bool(villkor)

    try:
        srv = Server(bas)
        V = 'gamla-robant'
        srv.publicera('30.33', V)
        r = Robot(bas, srv, V, 39_900 + os.getpid() % 50)
        r.run(15, until=lambda: r.fw().get('kortversion'))
        f = r.fw()
        check('status: kortets version och variant', (f.get('kortversion'), f.get('variant')) == ('30.32', V), f)
        check('robotd säger att den är ett provbygge', f.get('provbygge') is True, f)
        check('ny firmware hittas', r.sok() == ('NyFinns', {'version': '30.33', 'nyheter': 'Provfirmware'}), r.lage())

        # Spärrarna.
        r.run(1.5, control(0.0, True))
        check('AKTIVERA syns som spärr', any('AKTIVERA' in s for s in r.fw().get('sparrar', [])), r.fw().get('sparrar'))
        nej = r.uppdatera('30.33')
        check('robotd vägrar uppdatera med AKTIVERA på', nej[0] == 'Nej' and 'AKTIVERA' in nej[1]['text'], nej)
        r.run(2.0, control(0.5, True))
        check('gasen når kortet när ingen uppdatering pågår', len(r.kort.korvarden) > 0)
        r.run(0.8, control(0.0, False))
        nej = r.uppdatera('30.33')
        check('robotd vägrar uppdatera en robot som just rört sig', nej[0] == 'Nej', nej)
        check('spärrarna släpper när roboten stått still', r.redo(), r.fw().get('sparrar'))
        tittare = r.anslut(tittare=True)
        r.run(2.0, until=lambda: any('anslutna' in s for s in r.fw().get('sparrar', [])))
        check('en till ansluten syns som spärr', any('anslutna' in s for s in r.fw().get('sparrar', [])), r.fw().get('sparrar'))
        ws_send(tittare, {'Firmware': {'Uppdatera': {'version': '30.33'}}})
        ws_send(tittare, 'ScanVescBus')
        svar, skanning = None, None
        for _ in range(40):
            try:
                m = ws_recv(tittare)
            except socket.timeout:
                continue
            if m and 'FirmwareSvar' in m:
                svar = m['FirmwareSvar']
            if m and 'VescBusResult' in m:
                skanning = m['VescBusResult']
            if svar and skanning is not None:
                break
        check('bara föraren får begära uppdatering', svar and not svar['ok'], svar)
        check('en tittare får inte skanna CAN-bussen (tomt svar, inget till kortet)', skanning == [] and not r.kort.andra, (skanning, r.kort.andra))
        tittare.close()
        check('spärren släpper när den andra kopplat från', r.redo(), r.fw().get('sparrar'))
        check('bara den erbjudna versionen går att begära', r.uppdatera('30.34')[0] == 'Nej')

        # Hela varvet, långsamt. Under skrivningen når inga körvärden kortet.
        r.flagga('langsam', text='4')
        r.svar.clear()
        ws_send(r.s, {'Firmware': {'Uppdatera': {'version': '30.33'}}})
        check('uppdateringen börjar', r.run(4, until=lambda: r.lage()[0] == 'Pagar'), (r.lage(), r.svar))
        check('skrivningen börjar (steg 6)', r.run(40, until=lambda: r.lage()[1].get('steg') == 6), r.lage())
        fore = len(r.kort.korvarden)
        r.run(2.5, control(0.6, True))
        check('körningen är spärrad under skrivningen', PAGAR in (r.last.get('kor_sparr') or ''), r.last.get('kor_sparr'))
        check('inga körvärden når kortet under skrivningen', len(r.kort.korvarden) == fore, r.kort.korvarden[fore:])
        # Inget annat som går till kortet släpps igenom under uppdateringen.
        r.svar.clear()
        for begaran in ('ScanVescBus', 'ReadActuators', {'TestSpinVesc': {'can_id': 28}}, {'SetAutocenter': {'on': True}}):
            ws_send(r.s, begaran)
        r.run(2.0, until=lambda: r.skanningar and r.aktuatorfel and r.svar)
        check('CAN-skanning avvisas under uppdateringen', r.skanningar == [[]] and any(PAGAR in s['text'] for s in r.svar), (r.skanningar, r.svar))
        check('läsning av aktuatorerna avvisas under uppdateringen', any(PAGAR in e for e in r.aktuatorfel), r.aktuatorfel)
        check('skanningen och provsnurret nådde aldrig kortet', not r.kort.andra, r.kort.andra)
        klar = r.vanta('Klar', 'Misslyckad', secs=90)
        check('klart med ny version', klar == ('Klar', {'version': '30.33', 'fran': '30.32', 'anmarkningar': []}), klar)
        r.run(3.0, until=lambda: r.fw().get('kortversion') == '30.33')
        check('kortet uppger den nya versionen', (r.kortet(), r.fw().get('kortversion')) == ('30.33', '30.33'), r.fw())
        check('inställningarna sparades före skrivningen', any(f == 'installningar.bin' for _, _, fs in os.walk(f'{r.mapp}/robotuppdatering/backup') for f in fs))
        r.flagga('langsam', False)
        check('samma version igen är ingen uppdatering', r.sok()[0] == 'Aktuell', r.lage())

        # Inställningar som skrivningen ändrat läggs tillbaka (över den riktiga länken).
        srv.publicera('30.34', V)
        check('30.34 hittas', r.sok()[0] == 'NyFinns', r.lage())
        r.redo()
        fore = r.kort.installningar
        r.flagga('raderar_installningar')
        l = r.uppdatera('30.34')
        check('ändrade inställningar skrivs tillbaka och det meddelas', l[0] == 'Klar' and len(l[1].get('anmarkningar', [])) == 1 and 'skrivits tillbaka' in l[1]['anmarkningar'][0], l)
        check('kortets inställningar är som före', r.kort.installningar == fore)
        r.flagga('raderar_installningar', False)

        # Inställningar som inte går att spara: ingenting skrivs.
        srv.publicera('30.35', V)
        r.sok()
        r.redo()
        r.flagga('tyst_installningar')
        l = r.uppdatera('30.35')
        check('inställningar som inte går att spara stoppar', l[0] == 'Misslyckad' and 'inställningar gick inte att spara' in l[1]['text'] and r.kortet() == '30.34', (l, r.kortet()))
        r.flagga('tyst_installningar', False)

        # Fel version efter skrivningen: den förra skrivs tillbaka.
        r.sok()
        r.redo()
        r.flagga('fel_version')
        l = r.uppdatera('30.35')
        check('fel version efter skrivningen: den förra skrivs tillbaka', l[0] == 'Misslyckad' and l[1].get('aterstalld') is True and r.kortet() == '30.34', (l, r.kortet()))
        r.flagga('fel_version', False)

        # Krav som inte är uppfyllt.
        srv.publicera('30.36', V, kraver={'robotd': '9.0.0'})
        l = r.sok()
        check('ouppfyllt krav stoppar', l[0] == 'Hinder' and 'uppdateras av Mapro' in l[1]['text'], l)
    finally:
        if r:
            r.stop()
        if '--behall' in sys.argv or not ok:
            print(f'(mappen finns kvar i {bas})')
        else:
            shutil.rmtree(bas, ignore_errors=True)
    print('ALLT OK' if ok else 'NÅGOT SLOG FEL')
    sys.exit(0 if ok else 1)


if __name__ == '__main__':
    main()
