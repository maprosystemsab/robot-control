#!/usr/bin/env python3
"""Listar tredjepartskoden som följer med SentinAnt Lite (Rust-bibliotek som länkas in
i programmet) och deras licenser, och skriver TREDJEPART.md. Varnar
för licenser som kan kräva att vår källkod lämnas ut (GPL, AGPL, LGPL) eller som
har villkor att ta hänsyn till (MPL m.fl.).

    python3 scripts/licenser.py            (skriver TREDJEPART.md, avslutar med 1 vid stark copyleft)

Bara bibliotek som byggs in i programmet räknas (inte de som bara används i
tester eller vid byggandet). Kör igen när beroenden ändras, före en publicering.
Samma skript som SentinAnts (autorobot, scripts/licenser.py), för det här repot.
"""
import collections, os, re, subprocess, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# Det som delas ut som färdigt program (paketet, installationsfilen, uppdateringen).
PROGRAMS = {'robotstyrning': 'SentinAnt Lite (kundens dator)'}
EGNA = {'robotstyrning', 'relay-protocol', 'robotuppdatering'}
STARK = re.compile(r'\b(A?GPL|LGPL)', re.I)
SVAG = re.compile(r'\b(MPL|EPL|CDDL|CC-BY-SA|OSL)', re.I)


# SentinAnt Lite levereras också för Windows: den plattformens bibliotek räknas med, inte
# bara den här datorns.
EXTRA_MAL = {'robotstyrning': ['x86_64-pc-windows-gnu']}


def tree(pkg):
    out = ''
    for mal in [None] + EXTRA_MAL.get(pkg, []):
        out += subprocess.run(['cargo', 'tree', '-p', pkg, '-e', 'normal', '--prefix', 'none', '-f', '{p}|{l}']
                              + (['--target', mal] if mal else []),
                              cwd=ROOT, capture_output=True, text=True, check=True).stdout
    deps = {}
    for line in out.splitlines():
        line = line.replace(' (*)', '').strip()
        if '|' not in line:
            continue
        p, lic = line.split('|', 1)
        parts = p.split()
        name, ver = parts[0], parts[1].lstrip('v')
        if name in EGNA:
            continue
        deps[(name, ver)] = lic.strip() or 'OKÄND'
    return deps


def risk(lic):
    """Kräver licensen något av oss? Ett 'OR' räcker att ett alternativ är tillåtande."""
    alts = [a.strip() for a in re.split(r'\bOR\b|/', lic)]
    if any(not STARK.search(a) and not SVAG.search(a) and a != 'OKÄND' for a in alts):
        return None
    if any(STARK.search(a) for a in alts):
        return 'stark'
    if any(SVAG.search(a) for a in alts):
        return 'svag'
    return 'okänd'


def main():
    per_prog = {p: tree(p) for p in PROGRAMS}
    alla = {}
    for deps in per_prog.values():
        alla.update(deps)
    lines = ['# Tredjepartskod', '',
             '*Skapad av `scripts/licenser.py`. Rust-biblioteken som byggs in i SentinAnt Lite,',
             'med sina licenser. Följer med i installationspaketet (`/usr/share/doc/`) och i',
             'Windows-installationen.*', '',
             'SentinAnt Lite är Mapro Systems AB:s eget program. Det använder bibliotek med',
             'öppen källkod under tillåtande licenser (MIT, Apache-2.0, BSD m.fl.), som kräver',
             'att upphovsrätten och licenstexten följer med programmet men inte att vår källkod',
             'lämnas ut.', '']
    flagged = []
    for prog, title in PROGRAMS.items():
        lines.append(f'- **{title}:** {len(per_prog[prog])} bibliotek')
    lines += ['', '## Licenser', '', '| Licens | Antal |', '|---|---|']
    count = collections.Counter(alla.values())
    for lic, n in count.most_common():
        lines.append(f'| {lic} | {n} |')
    lines += ['', '## Att se över', '']
    for (name, ver), lic in sorted(alla.items()):
        r = risk(lic)
        if r:
            flagged.append((r, name, ver, lic))
    if not flagged:
        lines.append('Inga bibliotek med copyleft-licens (GPL, LGPL, AGPL, MPL m.fl.).')
    for r, name, ver, lic in flagged:
        what = {'stark': 'STARK COPYLEFT: får inte länkas in i en stängd produkt utan vidare',
                'svag': 'svag copyleft: ändringar i just det biblioteket ska lämnas ut; vår kod påverkas inte',
                'okänd': 'licensen okänd: kontrollera'}[r]
        lines.append(f'- `{name} {ver}` ({lic}): {what}')
    lines += ['', '## Alla bibliotek', '', '| Bibliotek | Version | Licens |', '|---|---|---|']
    for (name, ver), lic in sorted(alla.items()):
        lines.append(f'| {name} | {ver} | {lic} |')
    open(os.path.join(ROOT, 'TREDJEPART.md'), 'w').write('\n'.join(lines) + '\n')
    print(f'{len(alla)} bibliotek; licenser: ' + ', '.join(f'{l} ({n})' for l, n in count.most_common(6)))
    for r, name, ver, lic in flagged:
        print(f'  {r.upper()}: {name} {ver} ({lic})')
    print('Skrev TREDJEPART.md')
    sys.exit(1 if any(r == 'stark' for r, *_ in flagged) else 0)


if __name__ == '__main__':
    main()
