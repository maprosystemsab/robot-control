//! Hela varvet mot en låtsasrobot: en liten webbserver i stället för vår server, ett
//! skript i stället för openocd och en tråd som spelar robotd (skickar lägesbilder med den
//! version "kortet" har just nu).

use super::*;
use crate::beskrivning::prov;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// Fröet till nyckeln som testernas listor signeras med.
pub const FRO: u8 = 9;

type Filer = Arc<Mutex<HashMap<String, Vec<u8>>>>;

fn server(filer: Filer) -> String {
    let lyssnare = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let adress = format!("http://{}/fw/", lyssnare.local_addr().unwrap());
    std::thread::spawn(move || {
        for strom in lyssnare.incoming() {
            let Ok(mut strom) = strom else { continue };
            let mut buf = [0u8; 2048];
            let n = strom.read(&mut buf).unwrap_or(0);
            let fraga = String::from_utf8_lossy(&buf[..n]).to_string();
            let namn = fraga.split_whitespace().nth(1).unwrap_or("").trim_start_matches("/fw/").to_string();
            let svar = filer.lock().unwrap().get(&namn).cloned();
            let _ = match svar {
                Some(b) => strom.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", b.len()).as_bytes()).and_then(|_| strom.write_all(&b)),
                None => strom.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
            };
        }
    });
    adress
}

/// Låtsaskortets inställningar: vad som ska hända när robotd ber om dem.
#[derive(Default)]
struct Kortprov {
    spara_fel: AtomicBool,
    andrade_efter: AtomicBool,
    sparade: AtomicBool,
}

struct Lats(Arc<Kortprov>);

impl Kortet for Lats {
    fn spara_installningar(&mut self, mapp: &Path) -> Result<(), String> {
        if self.0.spara_fel.load(Ordering::SeqCst) {
            return Err("kortet svarade inte".into());
        }
        std::fs::write(mapp.join("installningar"), b"aktuatorer").map_err(|e| e.to_string())?;
        self.0.sparade.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn kontrollera_installningar(&mut self, mapp: &Path) -> Result<Installningar, String> {
        assert!(mapp.join("installningar").is_file());
        Ok(if self.0.andrade_efter.load(Ordering::SeqCst) { Installningar::Aterstallda } else { Installningar::Lika })
    }
}

struct Rigg {
    d: PathBuf,
    filer: Filer,
    kort: Arc<Kortprov>,
    u: Uppdaterare,
    /// Det robotd säger utöver versionen (spärrar, nödstopp).
    bild: Arc<Mutex<Lagesbild>>,
}

const VARIANT: &str = "mp101-robant-nodstopp";

fn elf(version: &str) -> Vec<u8> {
    flash::prov::elf(version.as_bytes())
}

fn lista(lopnummer: u64, version: &str, variant: &str, fil: &[u8], kraver: &str) -> Vec<u8> {
    let text = format!(
        r#"{{"format":1,"lopnummer":{lopnummer},"skapad":"2026-11-26T20:00:00Z","paket":{{"sentinant":{{"version":"0.1.0+1","filer":{{}}}},"firmware":{{"version":"{version}","nyheter":"Bättre broms","kraver":{{{kraver}}},"filer":{{"{variant}":{{"fil":"firmware-{version}-{variant}.elf","storlek":{},"sha256":"{}"}}}}}}}}}}"#,
        fil.len(),
        prov::hex(&prov::sha256(fil))
    );
    prov::signerat(&text, &prov::nyckel(FRO).0)
}

impl Rigg {
    /// En robot med firmware 30.32 och varianten `variant`, och en server med 30.33 för [`VARIANT`].
    fn ny(namn: &str, variant: Option<&str>) -> Rigg {
        let d = std::env::temp_dir().join(format!("robotuppdatering-prov-{}-{namn}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("version"), "30.32").unwrap();
        std::fs::write(d.join("kortet.bin"), flash::prov::dump()).unwrap();
        // Låtsasflasharen: filer i mappen styr vad som går fel. ELF-filens innehåll efter
        // huvudet (20 byte) är versionen "kortet" får.
        let skript = d.join("flasha.sh");
        std::fs::write(
            &skript,
            format!(
"#!/bin/sh\nD='{}'\necho \"$1\" >> \"$D/anrop\"\n[ -e \"$D/langsam\" ] && sleep 0.4\ncase \"$1\" in\n\
                 dumpa) [ -e \"$D/stannar_vid_dump\" ] && {{ mv \"$D/version\" \"$D/stoppad\"; echo 'Error: läsningen avbröts'; exit 1; }}; [ -e \"$D/tyst_efter_dump\" ] && cp \"$D/version\" \"$D/stoppad\"; [ -e \"$D/fel_dump\" ] && {{ echo 'kortet svarar inte'; exit 1; }}; if [ -e \"$D/tom_dump\" ]; then head -c 1048576 /dev/zero > \"$2\"; else cp \"$D/kortet.bin\" \"$2\"; fi ;;\n\
                 skriv) cp \"$D/version\" \"$D/forra\"; [ -e \"$D/fel_skriv\" ] && {{ echo 'Error: verify failed'; exit 1; }}; if [ -e \"$D/fel_version\" ]; then echo 30.1 > \"$D/version\"; elif [ -e \"$D/dott_kort\" ]; then rm \"$D/version\"; else tail -c +21 \"$2\" > \"$D/version\"; fi ;;\n\
                 starta) [ -e \"$D/startar_inte\" ] && exit 1; [ -e \"$D/stoppad\" ] && mv \"$D/stoppad\" \"$D/version\" ;;\n\
                 aterstall) [ -e \"$D/fel_aterstall\" ] && {{ echo 'kortet svarar inte'; exit 1; }}; cp \"$D/forra\" \"$D/version\" ;;\n\
                 esac\n",
                d.display()
            ),
        )
        .unwrap();
        std::os::unix::fs::PermissionsExt::set_mode(&mut std::fs::metadata(&skript).unwrap().permissions(), 0o755);
        std::fs::set_permissions(&skript, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let filer: Filer = Default::default();
        let rigg_filer = filer.clone();
        let kalla = server(filer);
        let kort = Arc::new(Kortprov::default());
        let inst = Installning { kalla, variant: variant.map(str::to_string), mapp: d.join("tillstand"), flashkommando: Some(skript), rot: d.join("rot") };
        let u = Uppdaterare::starta(inst, Box::new(Lats(kort.clone())));
        // Som SentinAnts robotd: båda tillvalen går att se; kortet rapporterar nödstoppet men ingen lägesknapp.
        let bild = Arc::new(Mutex::new(Lagesbild {
            nodstopp: Tillval { synligt: true, hors: Some(true), senast: None },
            lagesknapp: Tillval { synligt: true, hors: Some(false), senast: None },
            ..Default::default()
        }));
        let r = Rigg { d, filer: rigg_filer, kort, u, bild };
        r.publicera(10, "30.33", VARIANT, &elf("30.33"), "");
        r.robotd();
        r.tills(|s| s.kortversion.is_some());
        r
    }

    fn publicera(&self, lopnummer: u64, version: &str, variant: &str, fil: &[u8], kraver: &str) {
        let mut f = self.filer.lock().unwrap();
        f.insert(format!("firmware-{version}-{variant}.elf"), fil.to_vec());
        f.insert(MANIFESTFIL.into(), lista(lopnummer, version, variant, fil, kraver));
    }

    /// Tråden som spelar robotd: läser "kortets" version ur filen och skickar lägesbilder.
    fn robotd(&self) {
        let (d, delat, bild) = (self.d.clone(), self.u.delat.clone(), self.bild.clone());
        std::thread::spawn(move || {
            // Med filen "seg" läser "robotd" kortet bara var 700:e ms och skickar under
            // tiden vidare den gamla uppgiften (som en riktig robotd, vars senaste svar från
            // kortet kan vara från före omstarten).
            let mut last: Option<(Option<FwVersion>, Instant)> = None;
            loop {
                let seg = d.join("seg").exists();
                if !seg || last.map_or(true, |(_, nar)| nar.elapsed() > Duration::from_millis(700)) {
                    last = Some((std::fs::read_to_string(d.join("version")).ok().and_then(|t| FwVersion::tolka(t.trim())), Instant::now()));
                }
                let (version, nar) = last.unwrap();
                let mut b = bild.lock().unwrap().clone();
                b.kortversion = version;
                b.kort_svarade = version.map(|_| nar);
                for t in [&mut b.nodstopp, &mut b.lagesknapp] {
                    if t.hors == Some(true) {
                        t.senast = Some(nar);
                    }
                }
                {
                    let mut i = delat.inre.lock().unwrap();
                    i.bild = b;
                    i.bild_nar = Some(Instant::now());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }

    fn satt(&self, flagga: &str) {
        std::fs::write(self.d.join(flagga), b"").unwrap();
    }

    fn kortversion(&self) -> String {
        std::fs::read_to_string(self.d.join("version")).map(|t| t.trim().to_string()).unwrap_or_default()
    }

    fn anrop(&self) -> String {
        std::fs::read_to_string(self.d.join("anrop")).unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Väntar tills uppdateraren är klar med en order till (räknat från `fore`), och ger läget.
    fn vanta(&self, fore: u64) -> FirmwareLage {
        for _ in 0..1200 {
            if self.u.klara_order() > fore {
                return self.u.status().lage;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("blev aldrig klar: {:?}", self.u.status());
    }

    /// Väntar tills "robotd" har skickat en lägesbild som stämmer med `villkor`.
    fn tills(&self, villkor: impl Fn(&FirmwareStatus) -> bool) {
        for _ in 0..400 {
            if villkor(&self.u.status()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("lägesbilden kom aldrig: {:?}", self.u.status());
    }

    fn sok(&self) -> FirmwareLage {
        let fore = self.u.klara_order();
        self.u.sok();
        self.vanta(fore)
    }

    fn uppdatera(&self) -> FirmwareLage {
        assert_eq!(self.sok(), FirmwareLage::NyFinns { version: "30.33".into(), nyheter: "Bättre broms".into() });
        let fore = self.u.klara_order();
        self.u.uppdatera("30.33").unwrap();
        self.vanta(fore)
    }
}

fn misslyckad(l: &FirmwareLage) -> (String, Option<bool>, bool) {
    match l {
        FirmwareLage::Misslyckad { text, aterstalld, kan_aterstallas, .. } => (text.clone(), *aterstalld, *kan_aterstallas),
        annat => panic!("väntade en misslyckad uppdatering: {annat:?}"),
    }
}

#[test]
fn hela_varvet() {
    let r = Rigg::ny("varv", Some(VARIANT));
    let s = r.u.status();
    assert_eq!((s.kortversion.as_deref(), s.variant.as_deref(), &s.lage, s.sparrar.is_empty()), (Some("30.32"), Some(VARIANT), &FirmwareLage::Vilar, true));
    // Medan kortet uppdateras finns filen som hindrar lägesbytet; efteråt är den borta.
    assert!(!r.d.join("rot").join(PAGARFIL).exists());
    r.satt("langsam");
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    r.tills(|s| s.lage.pagar());
    assert!(r.d.join("rot").join(PAGARFIL).exists());
    assert!(r.u.pagar() && r.u.uppdatera("30.33").is_err());
    assert_eq!(r.vanta(fore), FirmwareLage::Klar { version: "30.33".into(), fran: "30.32".into(), anmarkningar: vec![] });
    assert!(!r.d.join("rot").join(PAGARFIL).exists());
    assert_eq!(r.kortversion(), "30.33");
    assert_eq!(r.anrop(), "dumpa skriv");
    assert!(r.kort.sparade.load(Ordering::SeqCst));
    let s = r.u.status();
    assert_eq!((s.kortversion.as_deref(), s.listans_datum.as_str(), r.u.pagar()), (Some("30.33"), "2026-11-26", false));
    // Den hämtade filen är borta, säkerhetskopian finns kvar, och läget står på disken:
    // en robotd som startar om (eller en klient som ansluter igen) ser att det blev klart.
    assert_eq!(std::fs::read_dir(r.d.join("tillstand/hamtat")).unwrap().count(), 0);
    let sparat = ladda(&r.d.join("tillstand"));
    assert!(matches!(sparat.lage, FirmwareLage::Klar { .. }));
    assert!(flash::kontrollera_dump(sparat.dump.as_deref().unwrap()).is_ok());
    // Nästa sökning: kortet har den senaste. Och det går inte att uppdatera till den igen.
    assert_eq!(r.sok(), FirmwareLage::Aktuell);
    assert!(r.u.uppdatera("30.33").is_err());
    assert_eq!(r.anrop(), "dumpa skriv");
}

#[test]
fn gammal_uppgift_om_kortet_raknas_inte_efter_skrivningen() {
    // robotd:s senaste svar från kortet är upp till 0,7 s gammalt: direkt efter skrivningen
    // säger det fortfarande den gamla versionen. Det får inte tas för "fel version".
    let r = Rigg::ny("seg", Some(VARIANT));
    r.satt("seg");
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(r.uppdatera(), FirmwareLage::Klar { version: "30.33".into(), fran: "30.32".into(), anmarkningar: vec![] });
    assert_eq!(r.anrop(), "dumpa skriv");
}

#[test]
fn installningar_som_andrats_meddelas() {
    let r = Rigg::ny("installningar", Some(VARIANT));
    r.kort.andrade_efter.store(true, Ordering::SeqCst);
    match r.uppdatera() {
        FirmwareLage::Klar { anmarkningar, .. } => assert!(anmarkningar.len() == 1 && anmarkningar[0].contains("skrivits tillbaka"), "{anmarkningar:?}"),
        annat => panic!("{annat:?}"),
    }
}

#[test]
fn fel_signatur_och_aterspelad_lista_avvisas() {
    let r = Rigg::ny("signatur", Some(VARIANT));
    // Signerad med en annan nyckel.
    let text = r#"{"format":1,"lopnummer":99,"paket":{}}"#;
    r.filer.lock().unwrap().insert(MANIFESTFIL.into(), prov::signerat(text, &prov::nyckel(FRO + 1).0));
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("signaturen stämmer inte")));
    // En ändrad bokstav i en äkta lista.
    let mut andrad = lista(10, "30.33", VARIANT, &elf("30.33"), "");
    let plats = andrad.windows(5).position(|w| w == b"30.33").unwrap();
    andrad[plats + 4] = b'4';
    r.filer.lock().unwrap().insert(MANIFESTFIL.into(), andrad);
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("signaturen stämmer inte")));
    // Skräp i stället för en lista, och ingen server alls.
    r.filer.lock().unwrap().insert(MANIFESTFIL.into(), b"<html>404</html>".to_vec());
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("signaturen stämmer inte")));
    r.filer.lock().unwrap().remove(MANIFESTFIL);
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("når inte uppdateringsservern")));
    // En äkta lista godkänns, och en äldre äkta lista efter den avvisas (löpnumret).
    r.publicera(20, "30.33", VARIANT, &elf("30.33"), "");
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    r.publicera(19, "30.33", VARIANT, &elf("30.33"), "");
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("äldre uppdateringslista")));
    assert!(r.u.uppdatera("30.33").is_err());
    assert_eq!(r.anrop(), "");
}

#[test]
fn andrad_fil_skrivs_aldrig() {
    let r = Rigg::ny("fil", Some(VARIANT));
    // Listan lovar en fil; servern lämnar en annan med samma längd.
    let mut annan = elf("30.33");
    *annan.last_mut().unwrap() = b'4';
    r.filer.lock().unwrap().insert(format!("firmware-30.33-{VARIANT}.elf"), annan);
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("stämmer inte med beskrivningen") && text.contains("Ingenting har skrivits"), "{text}");
    assert_eq!((aterstalld, kan, r.kortversion().as_str(), r.anrop().as_str()), (None, false, "30.32", ""));
    assert_eq!(std::fs::read_dir(r.d.join("tillstand/hamtat")).unwrap().count(), 0);
    // För lång fil, och en rätt signerad fil som inte är en ELF för ARM.
    r.filer.lock().unwrap().insert(format!("firmware-30.33-{VARIANT}.elf"), [elf("30.33"), b"mer".to_vec()].concat());
    assert!(misslyckad(&r.uppdatera()).0.contains("stämmer inte med beskrivningen"));
    r.publicera(11, "30.33", VARIANT, b"detta ar ingen firmware alls, bara text", "");
    assert!(misslyckad(&r.uppdatera()).0.contains("ingen firmwarefil"));
    assert_eq!((r.kortversion().as_str(), r.anrop().as_str()), ("30.32", ""));
}

#[test]
fn aldrig_bakat_och_bara_den_erbjudna_versionen() {
    let r = Rigg::ny("bakat", Some(VARIANT));
    std::fs::write(r.d.join("version"), "30.33").unwrap();
    r.tills(|s| s.kortversion.as_deref() == Some("30.33"));
    assert_eq!(r.sok(), FirmwareLage::Aktuell);
    std::fs::write(r.d.join("version"), "30.40").unwrap();
    r.tills(|s| s.kortversion.as_deref() == Some("30.40"));
    assert_eq!(r.sok(), FirmwareLage::Aktuell);
    assert!(r.u.uppdatera("30.33").is_err());
    // Kortet har 30.32 och får ett erbjudande, men klienten ber om något annat.
    std::fs::write(r.d.join("version"), "30.32").unwrap();
    r.tills(|s| s.kortversion.as_deref() == Some("30.32"));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    for annan in ["30.34", "30.31", "", "30.33 ", "../../etc/passwd"] {
        assert!(r.u.uppdatera(annan).is_err(), "{annan:?}");
    }
    // Listan byts mellan sökningen och uppdateringen: ingenting görs.
    r.publicera(11, "30.34", VARIANT, &elf("30.34"), "");
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    let (text, ..) = misslyckad(&r.vanta(fore));
    assert!(text.contains("erbjuder nu version 30.34"), "{text}");
    assert_eq!((r.kortversion().as_str(), r.anrop().as_str()), ("30.32", ""));
}

#[test]
fn varianten_gissas_aldrig() {
    // Ingen variant angiven, en som inte går att tolka, och en som servern inte har en fil för.
    for (namn, variant, del) in [("ingen", None, "inte angiven"), ("skrap", Some("mp101-robant-nodstop"), "inte en giltig variant"), ("annan", Some("mp101-robant"), "ingen fil för den här robotens kort")] {
        let r = Rigg::ny(&format!("variant-{namn}"), variant);
        if variant == Some("mp101-robant") {
            r.bild.lock().unwrap().nodstopp.hors = Some(false);
            std::thread::sleep(Duration::from_millis(150));
        }
        assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains(del)), "{namn}");
        assert!(r.u.uppdatera("30.33").is_err());
        assert_eq!(r.anrop(), "");
    }
    // Varianten säger utan nödstopp, men kortet rapporterar nödstoppet: uppgiften är fel.
    let r = Rigg::ny("variant-motsagd", Some("mp101-robant"));
    r.publicera(11, "30.33", "mp101-robant", &elf("30.33"), "");
    r.tills(|s| s.sparrar.iter().any(|s| s.contains("utan nödstopp på DI1")));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    assert!(r.u.uppdatera("30.33").unwrap_err().contains("nödstopp på DI1"));
    assert_eq!(r.anrop(), "");
}

#[test]
fn ouppfyllt_krav_stoppar() {
    let r = Rigg::ny("krav", Some(VARIANT));
    // Car_Client finns på roboten (tjänsten är installerad) men versionen är inte känd.
    std::fs::create_dir_all(r.d.join("rot/etc/systemd/system")).unwrap();
    std::fs::write(r.d.join("rot/etc/systemd/system/car_client.service"), b"").unwrap();
    r.publicera(11, "30.33", VARIANT, &elf("30.33"), r#""car_client":"30.33""#);
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("uppdateras av Mapro") && text.contains("car_client")));
    assert!(r.u.uppdatera("30.33").is_err());
    registrera_komponent(&r.d.join("tillstand"), "car_client", "30.32");
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("är version 30.32")));
    registrera_komponent(&r.d.join("tillstand"), "car_client", "30.33");
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    // Ett krav på något roboten aldrig hört talas om.
    r.publicera(12, "30.33", VARIANT, &elf("30.33"), r#""nagot_nytt":"1.0""#);
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("nagot_nytt")));
    assert_eq!(r.anrop(), "");
}

#[test]
fn sparrarna_stoppar_fore_och_under() {
    let r = Rigg::ny("sparr", Some(VARIANT));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    r.bild.lock().unwrap().sparrar = vec!["Roboten är aktiverad.".into()];
    r.tills(|s| s.sparrar == vec!["Roboten är aktiverad.".to_string()]);
    assert!(r.u.uppdatera("30.33").unwrap_err().contains("aktiverad"));
    assert_eq!(r.anrop(), "");
    // Kortet svarar inte: ingen sökning och ingen uppdatering.
    r.bild.lock().unwrap().sparrar.clear();
    std::fs::remove_file(r.d.join("version")).unwrap();
    r.tills(|s| s.kortversion.is_none() && s.sparrar.is_empty());
    assert!(r.u.uppdatera("30.33").unwrap_err().contains("svarar inte"));
    assert!(matches!(r.sok(), FirmwareLage::Hinder { text } if text.contains("svarar inte")));
    assert_eq!(r.anrop(), "");
}

#[test]
fn utan_sakerhetskopia_skrivs_ingenting() {
    let r = Rigg::ny("backup", Some(VARIANT));
    r.kort.spara_fel.store(true, Ordering::SeqCst);
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("inställningar gick inte att spara") && text.contains("Ingenting har skrivits"), "{text}");
    assert_eq!((aterstalld, kan, r.anrop().as_str()), (None, false, ""));
    // Inställningarna går att spara, men minnet går inte att läsa ut (eller blir bara nollor).
    r.kort.spara_fel.store(false, Ordering::SeqCst);
    r.satt("fel_dump");
    let (text, ..) = misslyckad(&r.uppdatera());
    assert!(text.contains("gick inte att säkerhetskopiera") && text.contains("kortet svarar inte"), "{text}");
    std::fs::remove_file(r.d.join("fel_dump")).unwrap();
    r.satt("tom_dump");
    let (text, ..) = misslyckad(&r.uppdatera());
    assert!(text.contains("ser inte ut som ett program"), "{text}");
    // Efter varje utläsning som inte godkändes gjordes ett försök att starta kortet.
    assert_eq!((r.kortversion().as_str(), r.anrop().as_str()), ("30.32", "dumpa starta dumpa starta"));
}

#[test]
fn misslyckad_utlasning_lamnar_inte_kortet_stoppat() {
    // Utläsningen stannar processorn och går sedan fel: kortet är tyst. Ingenting har
    // skrivits, och ett försök görs att starta det igen.
    let r = Rigg::ny("stoppat", Some(VARIANT));
    r.satt("stannar_vid_dump");
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("gick inte att säkerhetskopiera") && text.contains("Ingenting har skrivits") && !text.contains("strömmen"), "{text}");
    assert_eq!((aterstalld, kan, r.kortversion().as_str(), r.anrop().as_str()), (None, false, "30.32", "dumpa starta"));
    // Går kortet inte att starta sägs det rakt ut vad föraren ska göra.
    let r = Rigg::ny("stoppat-kvar", Some(VARIANT));
    r.satt("stannar_vid_dump");
    r.satt("startar_inte");
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("svarar inte efteråt") && text.contains("slå av och på strömmen") && text.contains("Ingenting har skrivits"), "{text}");
    assert_eq!((aterstalld, kan, r.anrop().as_str()), (None, false, "dumpa starta"));
}

#[test]
fn okant_tillval_slapper_aldrig_igenom_nagot() {
    // Före skrivningen: robotd kan se nödstoppet men har inte hunnit höra något än.
    let r = Rigg::ny("okant-fore", Some(VARIANT));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    r.bild.lock().unwrap().nodstopp.hors = None;
    r.tills(|s| s.sparrar.iter().any(|s| s.contains("inte hunnit se om styrkortet rapporterar nödstopp")));
    assert!(r.u.uppdatera("30.33").unwrap_err().contains("inte hunnit se"));
    assert_eq!(r.anrop(), "");

    // Efter skrivningen: robotd vet ännu inte vad den nya firmwaren rapporterar (kortet har
    // just startat om). Det får inte räknas som att nödstoppet finns: utan en rapport från
    // den nya firmwaren skrivs den förra tillbaka.
    let r = Rigg::ny("okant-efter", Some(VARIANT));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    let (bild, d) = (r.bild.clone(), r.d.clone());
    std::thread::spawn(move || loop {
        let nytt = std::fs::read_to_string(d.join("version")).is_ok_and(|v| v.trim() == "30.33");
        bild.lock().unwrap().nodstopp.hors = if nytt { None } else { Some(true) };
        std::thread::sleep(Duration::from_millis(10));
    });
    let (text, aterstalld, _) = misslyckad(&r.vanta(fore));
    assert!(text.contains("rapporterar inte nödstoppet") && text.contains("är tillbaka"), "{text}");
    assert_eq!((aterstalld, r.kortversion().as_str(), r.anrop().as_str()), (Some(true), "30.32", "dumpa skriv aterstall"));
}

#[test]
fn utan_insyn_i_nodstoppet_hoppas_bara_den_kontrollen_over() {
    // Som Robotstyrnings robotd: nödstoppets rapporter går inte att se (Car_Client skickar
    // egna), så varianten jämförs med märkfilerna i stället. Uppdateringen går igenom.
    let r = Rigg::ny("utan-insyn", Some(VARIANT));
    r.bild.lock().unwrap().nodstopp = Tillval::default();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(r.uppdatera(), FirmwareLage::Klar { version: "30.33".into(), fran: "30.32".into(), anmarkningar: vec![] });
}

#[test]
fn ny_firmware_utan_lagesknapp_aterstalls() {
    let variant = "mp101-robant-nodstopp-lagesknapp";
    let r = Rigg::ny("lagesknapp", Some(variant));
    r.publicera(11, "30.33", variant, &elf("30.33"), "");
    r.bild.lock().unwrap().lagesknapp.hors = Some(true);
    r.tills(|s| s.sparrar.is_empty());
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    // "Den nya firmwaren" skickar inte lägesknappens paket.
    let (bild, d) = (r.bild.clone(), r.d.clone());
    std::thread::spawn(move || loop {
        let nytt = std::fs::read_to_string(d.join("version")).is_ok_and(|v| v.trim() == "30.33");
        bild.lock().unwrap().lagesknapp.hors = if nytt { None } else { Some(true) };
        std::thread::sleep(Duration::from_millis(10));
    });
    let (text, aterstalld, _) = misslyckad(&r.vanta(fore));
    assert!(text.contains("rapporterar inte lägesknappen på DI2"), "{text}");
    assert_eq!((aterstalld, r.kortversion().as_str()), (Some(true), "30.32"));
    // Med en firmware som rapporterar båda tillvalen går samma uppdatering igenom.
    let r = Rigg::ny("lagesknapp-ok", Some(variant));
    r.publicera(11, "30.33", variant, &elf("30.33"), "");
    r.bild.lock().unwrap().lagesknapp.hors = Some(true);
    r.tills(|s| s.sparrar.is_empty());
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    assert!(matches!(r.vanta(fore), FirmwareLage::Klar { .. }));
}

#[test]
fn fel_version_efter_skrivningen_aterstalls() {
    let r = Rigg::ny("felversion", Some(VARIANT));
    r.satt("fel_version");
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("uppger version 30.1") && text.contains("Den förra firmwaren (30.32) är tillbaka"), "{text}");
    assert_eq!((aterstalld, kan, r.kortversion().as_str(), r.anrop().as_str()), (Some(true), false, "30.32", "dumpa skriv aterstall"));
}

#[test]
fn misslyckad_skrivning_och_tyst_kort_aterstalls() {
    let r = Rigg::ny("skrivfel", Some(VARIANT));
    r.satt("fel_skriv");
    let (text, aterstalld, _) = misslyckad(&r.uppdatera());
    assert!(text.contains("verify failed"), "{text}");
    assert_eq!(aterstalld, Some(true));
    // Kortet startar inte efter skrivningen.
    let r = Rigg::ny("tyst", Some(VARIANT));
    r.satt("dott_kort");
    let (text, aterstalld, _) = misslyckad(&r.uppdatera());
    assert!(text.contains("svarar inte efter skrivningen") && text.contains("är tillbaka"), "{text}");
    assert_eq!((aterstalld, r.kortversion().as_str()), (Some(true), "30.32"));
}

#[test]
fn ny_firmware_utan_nodstopp_aterstalls() {
    let r = Rigg::ny("nodstopp", Some(VARIANT));
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
    let fore = r.u.klara_order();
    r.u.uppdatera("30.33").unwrap();
    // "Den nya firmwaren" rapporterar inte nödstoppet: robotd ser det när kortet startat.
    let (bild, d) = (r.bild.clone(), r.d.clone());
    std::thread::spawn(move || loop {
        let nytt = std::fs::read_to_string(d.join("version")).is_ok_and(|v| v.trim() == "30.33");
        bild.lock().unwrap().nodstopp.hors = Some(!nytt);
        std::thread::sleep(Duration::from_millis(10));
    });
    let (text, aterstalld, _) = misslyckad(&r.vanta(fore));
    assert!(text.contains("rapporterar inte nödstoppet"), "{text}");
    assert_eq!((aterstalld, r.kortversion().as_str()), (Some(true), "30.32"));
}

#[test]
fn misslyckad_aterstallning_gar_att_gora_om() {
    let r = Rigg::ny("aterstall", Some(VARIANT));
    r.satt("fel_version");
    r.satt("fel_aterstall");
    let (text, aterstalld, kan) = misslyckad(&r.uppdatera());
    assert!(text.contains("Återställningen misslyckades"), "{text}");
    assert_eq!((aterstalld, kan, r.kortversion().as_str()), (Some(false), true, "30.1"));
    // Beskedet står kvar: en sökning får inte ta bort möjligheten att återställa.
    assert!(matches!(r.sok(), FirmwareLage::Misslyckad { kan_aterstallas: true, .. }));
    // robotd startar om: läget och säkerhetskopian finns kvar på disken.
    assert!(matches!(ladda(&r.d.join("tillstand")).lage, FirmwareLage::Misslyckad { kan_aterstallas: true, .. }));
    // Med en spärr görs ingenting; utan den skrivs säkerhetskopian tillbaka.
    r.bild.lock().unwrap().sparrar = vec!["Ett uppdrag kör.".into()];
    r.tills(|s| !s.sparrar.is_empty());
    assert!(r.u.aterstall().unwrap_err().contains("uppdrag"));
    r.bild.lock().unwrap().sparrar.clear();
    std::fs::remove_file(r.d.join("fel_aterstall")).unwrap();
    r.tills(|s| s.sparrar.is_empty());
    let fore = r.u.klara_order();
    r.u.aterstall().unwrap();
    let (text, aterstalld, kan) = misslyckad(&r.vanta(fore));
    assert!(text.contains("är tillbaka"), "{text}");
    assert_eq!((aterstalld, kan, r.kortversion().as_str()), (Some(true), false, "30.32"));
    assert!(r.u.aterstall().is_err());
    assert!(matches!(r.sok(), FirmwareLage::NyFinns { .. }));
}

#[test]
fn omstart_mitt_i_uppdateringen_syns_efterat() {
    let d = std::env::temp_dir().join(format!("robotuppdatering-prov-{}-omstart", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    let dump = d.join("flash.bin");
    std::fs::write(&dump, flash::prov::dump()).unwrap();
    let pagar = |steg: u8| Sparat { lage: FirmwareLage::Pagar { version: "30.33".into(), steg, steg_antal: STEG_ANTAL, text: "Skriver".into(), procent: None }, dump: Some(dump.clone()), fran: Some("30.32".into()), listans_datum: String::new() };
    // Före skrivningen: ingenting hände med kortet. Under skrivningen: okänt, och kopian finns.
    spara(&d, &pagar(5));
    let (text, aterstalld, kan) = misslyckad(&ladda(&d).lage);
    assert!(text.contains("Ingenting hade skrivits"), "{text}");
    assert_eq!((aterstalld, kan), (None, false));
    spara(&d, &pagar(6));
    let (text, aterstalld, kan) = misslyckad(&ladda(&d).lage);
    assert!(text.contains("inte säkert att styrkortet fungerar"), "{text}");
    assert_eq!((aterstalld, kan), (None, true));
    // Utan kopia går det inte att återställa härifrån.
    std::fs::remove_file(&dump).unwrap();
    spara(&d, &pagar(7));
    assert_eq!(misslyckad(&ladda(&d).lage).2, false);
    // Ett sökresultat överlever inte en omstart; ett slutresultat gör det.
    spara(&d, &Sparat { lage: FirmwareLage::NyFinns { version: "30.33".into(), nyheter: String::new() }, ..Default::default() });
    assert_eq!(ladda(&d).lage, FirmwareLage::Vilar);
    std::fs::write(tillstandsfil(&d), b"trasig fil").unwrap();
    assert_eq!(ladda(&d).lage, FirmwareLage::Vilar);
}

#[test]
fn komponenternas_versioner() {
    let d = std::env::temp_dir().join(format!("robotuppdatering-prov-{}-komponenter", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    let inst = Installning { mapp: d.join("tillstand"), rot: d.join("rot"), ..Default::default() };
    assert!(installerat(&inst).values().all(|v| *v == Installerat::Saknas));
    std::fs::create_dir_all(d.join("rot/usr/local/bin")).unwrap();
    std::fs::write(d.join("rot/usr/local/bin/robotd"), b"").unwrap();
    registrera_komponent(&inst.mapp, "sentinant-robotd", "0.1.0+202611262000");
    let i = installerat(&inst);
    assert_eq!(i["robotd"], Installerat::Okand);
    assert_eq!(i["sentinant-robotd"], Installerat::Version("0.1.0+202611262000".into()));
    assert_eq!(i["car_client"], Installerat::Saknas);
}
