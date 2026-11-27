#!/usr/bin/env python3
"""Skriver och signerar uppdateringsbeskrivningen (uppdatering.json) i en uppdateringsmapp.

    python3 scripts/uppdatering_manifest.py --mapp paket/uppdatering --paket sentinant \\
        --version 0.1.0+202611261256 --nyheter "Kort text" \\
        --fil linux-x86_64=sentinant-0.1.0-202611261256-linux-x86_64 \\
        --fil windows-x86_64=sentinant-0.1.0-202611261256-windows-x86_64.exe

Filerna ska redan ligga i mappen. Skriptet räknar storlek och SHA-256, byter posten för
paketet i beskrivningen (andra pakets poster, t.ex. robotd, ligger kvar orörda), signerar
med den privata nyckeln (ed25519, openssl) och skriver uppdatering.json i ett enda steg.

Formen (det SentinAnt läser, se client/src/uppdatering.rs):
    { "manifest": "<JSON-text>", "signaturer": ["<hex>"] }
där JSON-texten är
    { "format": 1, "lopnummer": N, "skapad": "...", "paket": { "<komponent>": { "version": "...",
      "nyheter": "...", "filer": { "<plattform>": { "fil": "...", "storlek": N, "sha256": "..." } } } } }
Signaturen gäller texten i "manifest", tecken för tecken.

Löpnumret växer med varje publicering (tiden i sekunder, dock alltid högre än förra).
Programmen minns det högsta de sett och avvisar en beskrivning med lägre nummer, så en
gammal beskrivning kan inte visas upp igen. Utgå därför alltid från mappen som den ligger
på servern. --giltig-dagar N sätter "giltig_till": efter den tiden varnar programmen för
att listan är gammal (men uppdaterar ändå). Använd det bara om listan förnyas (publiceras
om) oftare än så.

Nyckeln: --nyckel, annars miljövariabeln SENTINANT_UPPDATERINGSNYCKEL, annars
~/Claude/hemligt/sentinant-uppdateringsnyckel-privat.pem. Flera --nyckel ger flera
signaturer (vid nyckelbyte signerar man en tid med både den gamla och den nya).
Körs bara av publicera_uppdatering.sh och publicera_firmware.sh.

Styrkortets firmware (--paket firmware): versionen är X.Y (t.ex. 30.33), "plattformen" är
kortvarianten (mp101-robant-nodstopp, gamla-robant …), --kraver KOMPONENT=VERSION säger vad
som måste finnas på roboten, och --kallkod TEXT antecknar vilken källkod filerna är byggda av.

En befintlig beskrivning i mappen används bara om den är signerad med vår nyckel (eller en
av --nyckel). --utga-fran ADRESS hämtar dessutom serverns beskrivning och utgår från den om
den är nyare än mappens, så att andra pakets poster inte tappas och löpnumret blir högre än
serverns; filer som de posterna nämner och som saknas i mappen hämtas och kontrolleras.
"""
import argparse, datetime, hashlib, json, os, re, subprocess, sys, tempfile, urllib.request

STANDARDNYCKEL = os.path.expanduser('~/Claude/hemligt/sentinant-uppdateringsnyckel-privat.pem')
FILNAMN = re.compile(r'^[A-Za-z0-9_-][A-Za-z0-9._-]{0,119}$')
VERSION = re.compile(r'^(\d+)\.(\d+)\.(\d+)(?:\+(\d+))?$')
FWVERSION = re.compile(r'^(0|[1-9]\d{0,2})\.(0|[1-9]\d{0,2})$')
# Samma form som robotuppdatering/src/firmware.rs (Variant::tolka).
VARIANT = re.compile(r'^(gamla-(robant|mactrac|drangen)|mp101-(robant|mactrac|drangen)(-nodstopp)?(-lagesknapp)?)$')
KRAV = re.compile(r'^[a-z][a-z0-9_-]{0,39}=\d+(\.\d+){0,3}(\+\d+)?$')
# Den publika halvan av vår nyckel (samma som PUBLIKA_NYCKLAR i robotuppdatering/src/beskrivning.rs).
VAR_PUBLIKA_NYCKEL = 'a25b48745b8670e89083a17daf3a8a0fcb4a6523b49e3c0969b974e30f628a87'


def fel(text):
    print(f'FEL: {text}', file=sys.stderr)
    sys.exit(1)


def version(text, paket='sentinant'):
    """Samma ordning som client/src/version.rs: siffra för siffra, bygget sist.
    Firmwaren har två tal (X.Y), som kortet uppger dem."""
    if paket == 'firmware':
        m = FWVERSION.match(text)
        if not m or any(int(x) > 255 for x in m.groups()):
            fel(f'obegriplig firmwareversion: {text} (ska vara X.Y, t.ex. 30.33)')
        return tuple(int(x) for x in m.groups())
    m = VERSION.match(text)
    if not m:
        fel(f'obegriplig version: {text} (ska vara X.Y.Z eller X.Y.Z+bygge)')
    return tuple(int(x or 0) for x in m.groups())


def sha256(fil):
    h = hashlib.sha256()
    with open(fil, 'rb') as f:
        for bit in iter(lambda: f.read(1 << 20), b''):
            h.update(bit)
    return h.hexdigest()


def signera(text, nyckel):
    """ed25519-signatur (hex) över texten, och kontroll att den går att verifiera."""
    with tempfile.TemporaryDirectory() as d:
        inn, ut, pub = (os.path.join(d, n) for n in ('m', 'sig', 'pub'))
        open(inn, 'wb').write(text)
        subprocess.run(['openssl', 'pkeyutl', '-sign', '-inkey', nyckel, '-rawin', '-in', inn, '-out', ut], check=True)
        subprocess.run(['openssl', 'pkey', '-in', nyckel, '-pubout', '-out', pub], check=True)
        subprocess.run(['openssl', 'pkeyutl', '-verify', '-pubin', '-inkey', pub, '-rawin', '-in', inn, '-sigfile', ut],
                       check=True, stdout=subprocess.DEVNULL)
        signatur = open(ut, 'rb').read()
    if len(signatur) != 64:
        fel(f'{nyckel} är ingen ed25519-nyckel (signaturen blev {len(signatur)} byte)')
    return signatur.hex()


def publik_hex(nyckel):
    der = subprocess.run(['openssl', 'pkey', '-in', nyckel, '-pubout', '-outform', 'DER'], check=True, capture_output=True).stdout
    return der[-32:].hex()


def signaturen_stammer(text, signatur_hex, publik):
    """Stämmer ed25519-signaturen över texten med den publika nyckeln (32 byte som hex)?"""
    try:
        signatur = bytes.fromhex(signatur_hex)
    except ValueError:
        return False
    if len(signatur) != 64:
        return False
    with tempfile.TemporaryDirectory() as d:
        inn, sig, pub = (os.path.join(d, n) for n in ('m', 'sig', 'pub'))
        open(inn, 'wb').write(text)
        open(sig, 'wb').write(signatur)
        # SubjectPublicKeyInfo för ed25519: fast huvud + nyckelns 32 byte.
        open(pub, 'wb').write(bytes.fromhex('302a300506032b6570032100') + bytes.fromhex(publik))
        r = subprocess.run(['openssl', 'pkeyutl', '-verify', '-pubin', '-keyform', 'DER', '-inkey', pub, '-rawin', '-in', inn, '-sigfile', sig],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return r.returncode == 0


def oppna(kuvert_bytes, publika, var):
    """Beskrivningen ur ett kuvert, bara om någon signatur stämmer med någon av nycklarna."""
    try:
        kuvert = json.loads(kuvert_bytes)
        text = kuvert['manifest'].encode('utf-8')
        signaturer = list(kuvert.get('signaturer', []))
    except (ValueError, KeyError, AttributeError, TypeError):
        fel(f'{var} går inte att läsa som en uppdateringsbeskrivning')
    if not any(signaturen_stammer(text, s, p) for s in signaturer for p in publika):
        fel(f'{var} är inte signerad med vår nyckel. Den används inte som grund (ta bort den eller rätta källan).')
    manifest = json.loads(text)
    if manifest.get('format') != 1:
        fel(f'{var} har formen {manifest.get("format")}, det här skriptet skriver form 1')
    return manifest


def hamta(adress, storst):
    with urllib.request.urlopen(urllib.request.Request(adress, headers={'User-Agent': 'uppdatering_manifest'}), timeout=20) as svar:
        data = svar.read(storst + 1)
    if len(data) > storst:
        fel(f'{adress} är större än väntat')
    return data


def main():
    p = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    p.add_argument('--mapp', required=True, help='uppdateringsmappen (det som läggs på servern)')
    p.add_argument('--paket', required=True, help='komponent, t.ex. sentinant')
    p.add_argument('--version', required=True)
    p.add_argument('--nyheter', default='')
    p.add_argument('--fil', action='append', default=[], metavar='PLATTFORM=FILNAMN', help='en per plattform')
    p.add_argument('--nyckel', action='append', default=[], help='privat nyckel (PEM); flera ger flera signaturer')
    p.add_argument('--giltig-dagar', type=int, default=0, help='sätt giltig_till så många dagar framåt (0 = ingen gräns)')
    p.add_argument('--tvinga', action='store_true', help='tillåt en version som inte är nyare än den som redan ligger där')
    p.add_argument('--kraver', action='append', default=[], metavar='KOMPONENT=VERSION', help='det mottagaren måste ha (firmware), t.ex. car_client=30.33')
    p.add_argument('--kallkod', default='', help='vilken källkod filerna är byggda av (antecknas i posten)')
    p.add_argument('--utga-fran', default='', metavar='ADRESS', help='hämta serverns beskrivning och utgå från den om den är nyare än mappens')
    a = p.parse_args()
    nycklar = a.nyckel or [os.environ.get('SENTINANT_UPPDATERINGSNYCKEL') or STANDARDNYCKEL]
    for n in nycklar:
        if not os.path.isfile(n):
            fel(f'den privata nyckeln saknas: {n}')
        if os.stat(n).st_mode & 0o077:
            fel(f'{n} går att läsa av andra användare: kör chmod 600 {n}')
    if not a.fil:
        fel('minst en --fil PLATTFORM=FILNAMN behövs')

    for k in a.kraver:
        if not KRAV.match(k):
            fel(f'--kraver ska vara KOMPONENT=VERSION, t.ex. car_client=30.33: {k}')
    publika = [VAR_PUBLIKA_NYCKEL] + [publik_hex(n) for n in nycklar]

    sokvag = os.path.join(a.mapp, 'uppdatering.json')
    manifest = {'format': 1, 'paket': {}}
    if os.path.exists(sokvag):
        manifest = oppna(open(sokvag, 'rb').read(), publika, sokvag)
    if a.utga_fran:
        kalla = a.utga_fran.rstrip('/') + '/'
        try:
            serverns = oppna(hamta(kalla + 'uppdatering.json', 256 * 1024), publika, kalla + 'uppdatering.json')
        except OSError as e:
            fel(f'serverns beskrivning gick inte att hämta från {kalla} ({e}). Utan den kan andra pakets poster tappas '
                'och löpnumret bli för lågt. Kontrollera VPN:et (eller kör utan serverkontroll om det är ett prov).')
        if int(serverns.get('lopnummer', 0)) > int(manifest.get('lopnummer', 0)):
            print(f'  utgår från serverns beskrivning (löpnummer {serverns.get("lopnummer")}, nyare än mappens)')
            manifest = serverns
        # Filer som andra paket nämner ska finnas i mappen (den läggs ut som en helhet).
        for namn, pk in manifest['paket'].items():
            if namn == a.paket:
                continue
            for f in pk.get('filer', {}).values():
                mal = os.path.join(a.mapp, f['fil'])
                if not FILNAMN.match(f['fil']):
                    fel(f'otillåtet filnamn i serverns beskrivning: {f["fil"]}')
                if os.path.isfile(mal) and os.path.getsize(mal) == f['storlek'] and sha256(mal) == f['sha256']:
                    continue
                print(f'  hämtar {f["fil"]} från servern ({f["storlek"]} byte)')
                data = hamta(kalla + f['fil'], int(f['storlek']))
                if len(data) != f['storlek'] or hashlib.sha256(data).hexdigest() != f['sha256']:
                    fel(f'{f["fil"]} från servern stämmer inte med beskrivningen')
                with open(mal + '.del', 'wb') as ut:
                    ut.write(data)
                os.replace(mal + '.del', mal)
    forut = manifest['paket'].get(a.paket)
    if forut and version(a.version, a.paket) <= version(forut['version'], a.paket) and not a.tvinga:
        fel(f'{a.paket} {forut["version"]} ligger redan i {a.mapp}; {a.version} är inte nyare. '
            'Ingen mottagare tar emot en äldre version (--tvinga om det ändå ska skrivas).')
    version(a.version, a.paket)

    filer = {}
    for post in a.fil:
        plattform, _, namn = post.partition('=')
        if not plattform or not FILNAMN.match(namn):
            fel(f'--fil ska vara PLATTFORM=FILNAMN med bara bokstäver, siffror, punkt, - och _: {post}')
        if a.paket == 'firmware' and not VARIANT.match(plattform):
            fel(f'okänd kortvariant: {plattform} (t.ex. gamla-robant, mp101-robant-nodstopp-lagesknapp)')
        fil = os.path.join(a.mapp, namn)
        if not os.path.isfile(fil) or os.path.getsize(fil) == 0:
            fel(f'{fil} saknas eller är tom')
        if a.paket == 'firmware' and open(fil, 'rb').read(4) != b'\x7fELF':
            fel(f'{namn} är ingen ELF-fil. Firmwaren ska vara ELF (en .bin skriver över kortets inställningar).')
        filer[plattform] = {'fil': namn, 'storlek': os.path.getsize(fil), 'sha256': sha256(fil)}
    nu = datetime.datetime.now(datetime.timezone.utc)
    manifest['skapad'] = nu.strftime('%Y-%m-%dT%H:%M:%SZ')
    # Löpnumret: tiden i sekunder, men alltid högre än förra (även om klockan gått fel).
    manifest['lopnummer'] = max(int(nu.timestamp()), int(manifest.get('lopnummer', 0)) + 1)
    manifest.pop('giltig_till', None)
    if a.giltig_dagar > 0:
        manifest['giltig_till'] = (nu + datetime.timedelta(days=a.giltig_dagar)).strftime('%Y-%m-%dT%H:%M:%SZ')
    manifest['paket'][a.paket] = {'version': a.version, 'nyheter': a.nyheter.strip(), 'filer': filer}
    if a.kraver:
        manifest['paket'][a.paket]['kraver'] = dict(k.split('=', 1) for k in a.kraver)
    if a.kallkod.strip():
        manifest['paket'][a.paket]['kallkod'] = a.kallkod.strip()

    text = json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=1).encode('utf-8')
    kuvert = {'manifest': text.decode('utf-8'), 'signaturer': [signera(text, n) for n in nycklar]}
    tmp = sokvag + '.ny'
    with open(tmp, 'w', encoding='utf-8') as f:
        json.dump(kuvert, f, ensure_ascii=False, indent=1)
        f.write('\n')
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, sokvag)
    print(f'  {sokvag}: {a.paket} {a.version}, löpnummer {manifest["lopnummer"]}'
          + (f', giltig till {manifest["giltig_till"]}' if 'giltig_till' in manifest else ''))
    for plattform, f in sorted(filer.items()):
        print(f'    {plattform}: {f["fil"]} ({f["storlek"]} byte, sha256 {f["sha256"][:16]}…)')
    # Filer för paketet som inte längre nämns tas bort (äldre versioner).
    kvar = {f['fil'] for pk in manifest['paket'].values() for f in pk['filer'].values()}
    for namn in sorted(os.listdir(a.mapp)):
        if namn.startswith(a.paket + '-') and namn not in kvar and os.path.isfile(os.path.join(a.mapp, namn)):
            os.remove(os.path.join(a.mapp, namn))
            print(f'    tog bort äldre fil: {namn}')


if __name__ == '__main__':
    main()
