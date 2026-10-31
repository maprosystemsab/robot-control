#!/bin/bash
# Kollar en robots RTK mot verkstadens baspunkt (brunnslocket, se ~/RControlStation/blandade projektfiler/banor/BASPUNKT_VERKSTAD.txt).
# Lägg robotens antenn mitt på lockets östra halva och kör:
#   scripts/kolla_rtk.sh mapro@192.168.200.16 [sekunder]     (standard 60 s)
# Läser bara GGA från rtkrcv (port 2948) på roboten; ändrar ingenting.
# Slutkod 0 = godkänt, 1 = underkänt, 2 = ingen kontakt / ingen data.
set -u
ROBOT="${1:?Användning: $0 användare@ip [sekunder]}"
SEK="${2:-60}"

ssh -o BatchMode=yes -o ConnectTimeout=10 "$ROBOT" "python3 - $SEK" <<'PYEOF'
import socket, sys, time, math, statistics as st

BAS_N, BAS_E, BAS_H = 6662440.246, 671361.706, 39.62   # mitt på lockets östra halva, ±3 cm
GRANS_PLAN = 0.05      # m, godkänd avvikelse i plan
GRANS_HOJD = 0.15      # m, större höjdskillnad = antennen ligger inte på locket eller Fixen är falsk
GRANS_STD = 0.02       # m, spridning under mätningen
sek = float(sys.argv[1])

def sweref(lat, lon):
    a=6378137.0; f=1/298.257222101; k0=0.9996; e2=f*(2-f); n=f/(2-f); ah=a/(1+n)*(1+n*n/4+n**4/64)
    A=e2; B=(5*e2**2-e2**3)/6; C=(104*e2**3-45*e2**4)/120; D=(1237*e2**4)/1260
    b=[n/2-2*n*n/3+5*n**3/16+41*n**4/180, 13*n*n/48-3*n**3/5+557*n**4/1440, 61*n**3/240-103*n**4/140, 49561*n**4/161280]
    p=math.radians(lat); l=math.radians(lon-15); s=math.sin(p)
    ps=p-s*math.cos(p)*(A+B*s*s+C*s**4+D*s**6); xi=math.atan(math.tan(ps)/math.cos(l)); eta=math.atanh(math.cos(ps)*math.sin(l))
    x=k0*ah*(xi+sum(b[k]*math.sin(2*(k+1)*xi)*math.cosh(2*(k+1)*eta) for k in range(4)))
    y=k0*ah*(eta+sum(b[k]*math.cos(2*(k+1)*xi)*math.sinh(2*(k+1)*eta) for k in range(4)))+500000
    return x, y

try:
    s = socket.create_connection(("127.0.0.1", 2948), timeout=5)
except OSError as e:
    print(f"Får ingen kontakt med rtkrcv på port 2948: {e}"); sys.exit(2)
s.settimeout(5)
buf = b""; fix = []; kval = {}
slut = time.time() + sek
print(f"Läser GGA i {sek:.0f} s ...", flush=True)
while time.time() < slut:
    try:
        data = s.recv(4096)
    except socket.timeout:
        continue
    if not data:
        print("rtkrcv stängde porten."); break
    buf += data
    *rader, buf = buf.split(b"\n")
    for r in rader:
        f = r.decode(errors="ignore").strip().split(",")
        if len(f) < 12 or not f[0].endswith("GGA") or not f[2]: continue
        kval[f[6]] = kval.get(f[6], 0) + 1
        if f[6] != "4": continue
        lat = int(f[2][:2]) + float(f[2][2:]) / 60; lon = int(f[4][:3]) + float(f[4][3:]) / 60
        N, E = sweref(lat, lon)
        fix.append((N, E, float(f[9]) + float(f[11] or 0)))

totalt = sum(kval.values())
namn = {"4": "Fix", "5": "Float", "2": "DGPS", "1": "enkel", "0": "ingen"}
print("Lösningar: " + ", ".join(f"{namn.get(k, k)} {v}" for k, v in sorted(kval.items())) if kval else "Inga GGA-meningar alls.")
if totalt == 0:
    sys.exit(2)
if len(fix) < 10:
    print("UNDERKÄNT: ingen Fix. Vänta tills rtkrcv får Fix (efter flytt: starta gärna om rtkrcv) och kör igen.")
    sys.exit(1)

N = st.mean(p[0] for p in fix); E = st.mean(p[1] for p in fix); H = st.mean(p[2] for p in fix)
sN = st.pstdev(p[0] for p in fix); sE = st.pstdev(p[1] for p in fix)
dN, dE, dH = N - BAS_N, E - BAS_E, H - BAS_H
plan = math.hypot(dN, dE)
print(f"Medel av {len(fix)} Fix:  N {N:.3f}  E {E:.3f}  höjd {H:.2f}  (spridning {sN*100:.1f}/{sE*100:.1f} cm)")
print(f"Mot baspunkten:  norr {dN*100:+.1f} cm  öster {dE*100:+.1f} cm  =  {plan*100:.1f} cm i plan,  höjd {dH*100:+.1f} cm")

fel = []
if len(fix) < 0.8 * totalt: fel.append(f"Fix bara {100*len(fix)/totalt:.0f} % av tiden")
if max(sN, sE) > GRANS_STD: fel.append("Fixen vandrar (spridning över 2 cm)")
if plan > GRANS_PLAN: fel.append(f"{plan*100:.0f} cm fel i plan (gräns {GRANS_PLAN*100:.0f} cm)")
if abs(dH) > GRANS_HOJD: fel.append(f"höjden {dH:+.2f} m fel: falsk Fix, eller ligger inte antennen på locket?")
if fel:
    print("UNDERKÄNT: " + "; ".join(fel)); sys.exit(1)
print("GODKÄNT: RTK-kedjan (antenn, mottagare, Swepos via servern, rtkrcv) ger rätt position.")
PYEOF
kod=$?
[ $kod -eq 255 ] && { echo "Får ingen ssh-kontakt med $ROBOT"; exit 2; }
exit $kod
