//! Robotsidan av firmwareuppdateringen: det robotd gör när föraren trycker "Sök ny
//! firmware" och "Uppdatera roboten".
//!
//! Roboten hämtar och kontrollerar allt själv. Klienten skickar bara "sök" och
//! "uppdatera till version X"; listan, filen och kontrollsumman kommer från vår server och
//! godkänns bara med rätt signatur. Ordningen i en uppdatering ([`Uppdaterare`]):
//!
//! 1. Spärrarna: roboten ska stå still och vara redo (robotd säger vad som hindrar), kortet
//!    ska svara, varianten ska vara angiven och stämma, kraven ska vara uppfyllda och
//!    verktygen ska finnas. Listan hämtas och kontrolleras på nytt.
//! 2. Filen hämtas och 3. kontrolleras (storlek, SHA-256, ELF för ARM).
//! 4. Kortets inställningar sparas och 5. hela flashminnet läses ut till en fil. Går
//!    något av det inte skrivs ingenting.
//! 6. Firmwaren skrivs. 7. Kortet startar om. 8. Det ska svara med exakt den utlovade
//!    versionen, och inställningarna ska vara som före (annars skrivs de tillbaka).
//!
//! Går 6–8 fel skrivs säkerhetskopian av flashminnet tillbaka. Läget sparas på disk
//! efter varje steg, så en klient som ansluter igen (eller robotd efter en omstart) vet
//! hur det gick.

pub mod flash;

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::beskrivning::{self, Erbjudande, Fel, Manifest, MANIFESTFIL};
use crate::firmware::{self, FwVersion, Installerat, Kort, Utbud, Variant};
use crate::protokoll::{FirmwareLage, FirmwareStatus};
use flash::Flashare;

/// Där tillståndet, säkerhetskopiorna och komponenternas versioner ligger på roboten.
pub const STANDARDMAPP: &str = "/var/lib/robotuppdatering";
const STEG_ANTAL: u8 = 8;
/// Så gammal får robotd:s senaste lägesbild vara när ett beslut tas.
const FARSK: Duration = Duration::from_secs(5);
/// Så länge väntas på att kortet ska svara efter en skrivning.
const VANTA_PA_KORTET: Duration = Duration::from_secs(if cfg!(test) { 2 } else { 120 });
/// Så länge får kortet på sig att börja rapportera nödstoppet efter start.
const VANTA_PA_NODSTOPP: Duration = Duration::from_secs(if cfg!(test) { 1 } else { 20 });
/// Så ofta ses det efter om verktygen och ST-Linken finns (för statusen).
const FORUTSATTNINGAR_GALLER: Duration = Duration::from_secs(3);
/// Så många säkerhetskopior sparas.
const SPARADE_KOPIOR: usize = 3;

/// Filen som finns medan kortet uppdateras (under `rot`, alltså `/run/…`). Lägesbytet på
/// robotar med båda lägena (`byt_lage.sh`) vägrar byta medan den finns: bytet skulle stoppa
/// robotd, och openocd med den, mitt i skrivningen. /run töms vid omstart.
pub const PAGARFIL: &str = "run/robotuppdatering-pagar";

/// Komponenter som krav kan gälla, och filen som visar att komponenten finns på roboten.
const KANDA_KOMPONENTER: &[(&str, &str)] = &[
    ("robotd", "usr/local/bin/robotd"),
    ("sentinant-robotd", "usr/local/bin/sentinant-robotd"),
    ("car_client", "etc/systemd/system/car_client.service"),
];

/// Robotens inställningar för uppdateringen (ur robotd:s inställningsfil).
#[derive(Debug, Clone)]
pub struct Installning {
    /// Mappen på servern (med `uppdatering.json`). Signaturen kontrolleras alltid, också
    /// när en annan källa anges för prov.
    pub kalla: String,
    /// Robotens kortvariant, uttryckligen angiven (se [`Variant`]). `None` = ingen uppdatering.
    pub variant: Option<String>,
    pub mapp: PathBuf,
    /// Ett eget kommando i stället för openocd (simulatorn och prov).
    pub flashkommando: Option<PathBuf>,
    /// Filsystemets rot ("/"); annat bara i tester.
    pub rot: PathBuf,
}

impl Default for Installning {
    fn default() -> Self {
        Installning { kalla: beskrivning::STANDARDKALLA.into(), variant: None, mapp: STANDARDMAPP.into(), flashkommando: None, rot: "/".into() }
    }
}

/// Det robotd vet om roboten just nu. Skickas in ofta ([`Uppdaterare::lagesbild`]).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Lagesbild {
    /// Styrkortets version, `None` när kortet inte svarar.
    pub kortversion: Option<FwVersion>,
    /// När kortets svar med versionen kom. Efter en skrivning räknas bara ett svar som
    /// kommit efter att skrivningen var klar: robotd:s senaste uppgift kan annars vara
    /// den gamla firmwarens, från strax innan kortet startade om.
    pub kort_svarade: Option<Instant>,
    /// Det som hindrar en uppdatering just nu, i klartext (aktiverad, rör sig, uppdrag kör,
    /// någon annan ansluten, lågt batteri …). Att kortet inte svarar ska inte stå här.
    pub sparrar: Vec<String>,
    /// Nödstoppet på DI1, som robotd ser det på kortets egna rapporter.
    pub nodstopp: Tillval,
    /// Lägesknappen på DI2, på samma sätt.
    pub lagesknapp: Tillval,
}

/// Det robotd ser av ett tillval i firmwaren (nödstopp på DI1, lägesknapp på DI2): en
/// firmware byggd med tillvalet rapporterar det av sig själv, en utan gör det inte.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Tillval {
    /// robotd kan över huvud taget se om kortet rapporterar tillvalet. `false` = det går
    /// inte att kontrollera här (Robotstyrnings robotd och nödstoppet: Car_Client skickar
    /// ett eget paket för nödstoppet på Pi:n). Bara då hoppas kontrollerna över.
    pub synligt: bool,
    /// Rapporterar kortet tillvalet? `None` = robotd vet inte än (kortet har just börjat
    /// svara). Okänt räknas aldrig som "stämmer".
    pub hors: Option<bool>,
    /// När kortets senaste rapport kom.
    pub senast: Option<Instant>,
}

/// Hur det gick med kortets inställningar efter skrivningen.
#[derive(Debug, Clone, PartialEq)]
pub enum Installningar {
    Lika,
    /// De skilde sig och de sparade har skrivits tillbaka.
    Aterstallda,
    /// De skiljer sig och gick inte att skriva tillbaka (t.ex. annan uppbyggnad i den nya
    /// versionen), med förklaringen.
    Skiljer(String),
}

/// Det bara robotd kan göra med kortet. Anropen får ta tid (de körs i uppdateringens egen tråd).
pub trait Kortet: Send + 'static {
    /// Läser kortets inställningar och sparar dem i `mapp`. `Err` = det gick inte; då
    /// skrivs ingenting till kortet.
    fn spara_installningar(&mut self, mapp: &Path) -> Result<(), String>;
    /// Jämför kortets inställningar med de sparade i `mapp`, och skriver tillbaka de
    /// sparade om de skiljer sig.
    fn kontrollera_installningar(&mut self, mapp: &Path) -> Result<Installningar, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Sparat {
    lage: FirmwareLage,
    /// Säkerhetskopian av flashminnet från den senaste uppdateringen.
    #[serde(default)]
    dump: Option<PathBuf>,
    /// Versionen före den senaste uppdateringen.
    #[serde(default)]
    fran: Option<String>,
    #[serde(default)]
    listans_datum: String,
}

struct Inre {
    sparat: Sparat,
    bild: Lagesbild,
    bild_nar: Option<Instant>,
    /// Det senaste erbjudandet (efter en sökning).
    erbjudande: Option<Erbjudande<FwVersion>>,
    /// Antal order (sök, uppdatera, återställ) som trådens arbete är klart med.
    klara: u64,
}

enum Order {
    Sok,
    Uppdatera(String),
    Aterstall,
}

struct Delat {
    inre: Mutex<Inre>,
    inst: Installning,
    /// Senaste kontrollen av verktyg och ST-Link (den frågas efter i varje status).
    forutsattningar: Mutex<Option<(Instant, Vec<String>)>>,
}

/// Nycklarna en lista får vara signerad med.
#[cfg(not(test))]
fn godkanda_nycklar() -> Vec<[u8; 32]> {
    beskrivning::nycklar()
}

/// I den här cratens egna tester signeras listorna med en nyckel ur ett frö.
#[cfg(test)]
fn godkanda_nycklar() -> Vec<[u8; 32]> {
    vec![beskrivning::prov::nyckel(tester::FRO).1]
}

/// Uppdateringen av styrkortet. Skapas en gång av robotd.
pub struct Uppdaterare {
    delat: Arc<Delat>,
    tx: mpsc::Sender<Order>,
}

fn tillstandsfil(mapp: &Path) -> PathBuf {
    mapp.join("tillstand.json")
}

fn spara(mapp: &Path, s: &Sparat) {
    let _ = std::fs::create_dir_all(mapp);
    let tmp = mapp.join("tillstand.json.ny");
    let skrivet = serde_json::to_vec_pretty(s).map_err(|e| e.to_string()).and_then(|b| {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(&b).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
    });
    if skrivet.is_ok() {
        let _ = std::fs::rename(&tmp, tillstandsfil(mapp));
    }
}

/// Läser det sparade läget. En uppdatering som pågick när robotd slutade blir "misslyckad",
/// med besked om säkerhetskopian finns; lägen som bara gäller en sökning glöms.
fn ladda(mapp: &Path) -> Sparat {
    let mut s: Sparat = std::fs::read(tillstandsfil(mapp)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    s.lage = match s.lage {
        FirmwareLage::Pagar { version, steg, text, .. } => {
            let kopia = s.dump.as_deref().is_some_and(|d| flash::kontrollera_dump(d).is_ok());
            let skrivet = steg >= 6;
            FirmwareLage::Misslyckad {
                version,
                text: if skrivet {
                    format!("Robotens program startade om mitt i uppdateringen ({text}). Det är inte säkert att styrkortet fungerar.")
                } else {
                    format!("Robotens program startade om under uppdateringen ({text}). Ingenting hade skrivits till styrkortet.")
                },
                aterstalld: None,
                kan_aterstallas: skrivet && kopia,
            }
        }
        l @ (FirmwareLage::Klar { .. } | FirmwareLage::Misslyckad { .. }) => l,
        _ => FirmwareLage::Vilar,
    };
    s
}

/// Skriver in den här komponentens version i robotens register (för firmwarens krav).
pub fn registrera_komponent(mapp: &Path, namn: &str, version: &str) {
    let d = mapp.join("komponenter");
    let _ = std::fs::create_dir_all(&d);
    let _ = std::fs::write(d.join(namn), format!("{version}\n"));
}

/// Det som finns på roboten av de komponenter ett krav kan gälla.
pub fn installerat(inst: &Installning) -> BTreeMap<String, Installerat> {
    KANDA_KOMPONENTER
        .iter()
        .map(|(namn, tecken)| {
            let registrerad = std::fs::read_to_string(inst.mapp.join("komponenter").join(namn)).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
            let lage = match registrerad {
                Some(v) => Installerat::Version(v),
                None if inst.rot.join(tecken).exists() => Installerat::Okand,
                None => Installerat::Saknas,
            };
            (namn.to_string(), lage)
        })
        .collect()
}

fn http() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(8)).timeout_read(Duration::from_secs(30)).user_agent("robotuppdatering").build()
}

fn natfel(e: ureq::Error) -> Fel {
    match e {
        ureq::Error::Status(kod, _) => Fel::Nat(format!("HTTP {kod}")),
        e => Fel::Nat(e.to_string()),
    }
}

fn felfras(fel: &Fel) -> String {
    match fel {
        Fel::Nat(_) => "Roboten når inte uppdateringsservern. Kontrollera robotens nät (VPN).".into(),
        Fel::Kuvert | Fel::Signatur => "Uppdateringslistan från servern gick inte att kontrollera (signaturen stämmer inte). Ingenting har ändrats.".into(),
        Fel::Manifest(t) => format!("Uppdateringslistan på servern är felaktigt beskriven ({t}). Ingenting har ändrats."),
        Fel::Format(_) => "Robotens program är för gammalt för den här uppdateringslistan. Kontakta Mapro.".into(),
        Fel::Storlek | Fel::Kontrollsumma => "Den hämtade filen stämmer inte med beskrivningen och har kastats. Ingenting har ändrats.".into(),
        Fel::Fil(t) => format!("Filen gick inte att spara på roboten: {t}"),
        Fel::Aterspelad { .. } => "Servern visade en äldre uppdateringslista än roboten redan har sett. Ingenting har ändrats. Kontakta Mapro om det står kvar.".into(),
    }
}

impl Delat {
    fn kalla(&self) -> String {
        format!("{}/", self.inst.kalla.trim().trim_end_matches('/'))
    }

    fn variant(&self) -> Result<Variant, String> {
        match self.inst.variant.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            None => Err("Robotens kortvariant är inte angiven, så roboten vet inte vilken firmware som är rätt. Kontakta Mapro.".into()),
            Some(v) => Variant::tolka(v).ok_or_else(|| format!("Robotens kortvariant \"{v}\" är inte en giltig variant. Kontakta Mapro.")),
        }
    }

    fn flashare(&self, kort: Kort) -> Flashare {
        match &self.inst.flashkommando {
            Some(program) => Flashare::Kommando { program: program.clone(), arbetsmapp: self.inst.mapp.clone() },
            None => Flashare::Openocd { kort, rot: self.inst.rot.clone(), arbetsmapp: self.inst.mapp.clone() },
        }
    }

    /// Sätter eller tar bort [`PAGARFIL`].
    fn pagarfil(&self, pa: bool) {
        let fil = self.inst.rot.join(PAGARFIL);
        if pa {
            if let Some(mapp) = fil.parent() {
                let _ = std::fs::create_dir_all(mapp);
            }
            let _ = std::fs::write(&fil, b"");
        } else {
            let _ = std::fs::remove_file(&fil);
        }
    }

    fn satt_lage(&self, lage: FirmwareLage) {
        let mut i = self.inre.lock().unwrap();
        i.sparat.lage = lage;
        spara(&self.inst.mapp, &i.sparat);
    }

    fn steg(&self, version: FwVersion, steg: u8, text: &str, procent: Option<u8>) {
        self.satt_lage(FirmwareLage::Pagar { version: version.to_string(), steg, steg_antal: STEG_ANTAL, text: text.to_string(), procent });
    }

    /// Den senaste lägesbilden om den är färsk.
    fn bild(&self) -> Option<Lagesbild> {
        let i = self.inre.lock().unwrap();
        i.bild_nar.filter(|n| n.elapsed() <= FARSK).map(|_| i.bild.clone())
    }

    /// Hämtar listan, kontrollerar signatur och färskhet.
    fn hamta_lista(&self) -> Result<(Manifest, String), Fel> {
        let svar = http().get(&format!("{}{MANIFESTFIL}", self.kalla())).call().map_err(natfel)?;
        let mut b = Vec::new();
        svar.into_reader().take(beskrivning::STORSTA_BESKRIVNING + 1).read_to_end(&mut b).map_err(|e| Fel::Nat(e.to_string()))?;
        if b.len() as u64 > beskrivning::STORSTA_BESKRIVNING {
            return Err(Fel::Kuvert);
        }
        let manifest = beskrivning::oppna_kuvert(&b, &godkanda_nycklar())?;
        let farskhet = beskrivning::granska_farskhet(&manifest, Some(&self.inst.mapp.join("sett.json")), chrono::Utc::now())?;
        Ok((manifest, farskhet.skapad))
    }

    /// Det som hindrar en uppdatering utöver robotd:s egna spärrar: variant och verktyg,
    /// och att varianten stämmer med det kortet visar.
    fn fasta_hinder(&self, bild: Option<&Lagesbild>) -> Vec<String> {
        let variant = match self.variant() {
            Ok(v) => v,
            Err(t) => return vec![t],
        };
        let mut hinder = {
            let mut f = self.forutsattningar.lock().unwrap();
            match &*f {
                Some((nar, h)) if nar.elapsed() < FORUTSATTNINGAR_GALLER => h.clone(),
                _ => {
                    let h = self.flashare(variant.kort).forutsattningar();
                    *f = Some((Instant::now(), h.clone()));
                    h
                }
            }
        };
        if let Some(b) = bild {
            // Varianten ska stämma med det kortet visar. Kan robotd se tillvalet men vet inte
            // än (kortet har just börjat svara) är det också ett hinder: okänt får aldrig
            // räknas som att det stämmer.
            let stam = |namn: &str, angiven: bool, t: &Tillval| match t.hors {
                _ if !t.synligt => None,
                None => Some(format!("Roboten har inte hunnit se om styrkortet rapporterar {namn}. Försök igen om några sekunder.")),
                Some(h) if h != angiven => Some(format!(
                    "Robotens kortvariant säger {} {namn}, men styrkortet {} det. Varianten måste rättas innan roboten kan uppdateras (kontakta Mapro).",
                    if angiven { "med" } else { "utan" },
                    if h { "rapporterar" } else { "rapporterar inte" }
                )),
                Some(_) => None,
            };
            hinder.extend(stam("nödstopp på DI1", variant.nodstopp, &b.nodstopp));
            hinder.extend(stam("lägesknapp på DI2", variant.lagesknapp, &b.lagesknapp));
        }
        hinder
    }

    /// Allt som hindrar just nu: robotd:s spärrar, de fasta hindren och ett tyst kort.
    fn alla_hinder(&self, kravs_kort: bool) -> Vec<String> {
        let Some(bild) = self.bild() else { return vec!["Roboten har inget färskt besked om sitt läge. Försök igen om en stund.".into()] };
        let mut hinder = bild.sparrar.clone();
        if kravs_kort && bild.kortversion.is_none() {
            hinder.push("Styrkortet svarar inte.".into());
        }
        hinder.extend(self.fasta_hinder(Some(&bild)));
        hinder
    }

    fn sok(&self) {
        {
            let mut i = self.inre.lock().unwrap();
            // Under en uppdatering söks inte, och inte heller medan en misslyckad uppdatering
            // väntar på att säkerhetskopian skrivs tillbaka (då ska det beskedet stå kvar).
            if i.sparat.lage.pagar() || matches!(i.sparat.lage, FirmwareLage::Misslyckad { kan_aterstallas: true, .. }) {
                return;
            }
            i.sparat.lage = FirmwareLage::Soker;
            i.erbjudande = None;
        }
        let (lage, erbjudande, datum) = self.sok_inre();
        let mut i = self.inre.lock().unwrap();
        i.erbjudande = erbjudande;
        if let Some(d) = datum {
            i.sparat.listans_datum = d;
        }
        i.sparat.lage = lage;
        spara(&self.inst.mapp, &i.sparat);
    }

    fn sok_inre(&self) -> (FirmwareLage, Option<Erbjudande<FwVersion>>, Option<String>) {
        let hinder = |text: String| (FirmwareLage::Hinder { text }, None, None);
        let variant = match self.variant() {
            Ok(v) => v,
            Err(t) => return hinder(t),
        };
        let Some(har) = self.bild().and_then(|b| b.kortversion) else { return hinder("Styrkortet svarar inte, så roboten vet inte vilken firmware det har.".into()) };
        let (manifest, datum) = match self.hamta_lista() {
            Ok(m) => m,
            Err(e) => return hinder(felfras(&e)),
        };
        let datum = Some(datum);
        match firmware::utbud(&manifest, &variant, har) {
            Err(e) => (FirmwareLage::Hinder { text: felfras(&e) }, None, datum),
            Ok(Utbud::Inget) => (FirmwareLage::Aktuell, None, datum),
            Ok(Utbud::Aktuell { .. }) => (FirmwareLage::Aktuell, None, datum),
            Ok(Utbud::VariantSaknas { version }) if version > har => (
                FirmwareLage::Hinder { text: format!("Det finns firmware {version} på servern, men ingen fil för den här robotens kort ({}). Kontakta Mapro.", variant.nyckel()) },
                None,
                datum,
            ),
            Ok(Utbud::VariantSaknas { .. }) => (FirmwareLage::Aktuell, None, datum),
            Ok(Utbud::Nyare(e)) => {
                let krav = firmware::ouppfyllda_krav(&e.kraver, &installerat(&self.inst));
                if krav.is_empty() {
                    (FirmwareLage::NyFinns { version: e.version.to_string(), nyheter: e.nyheter.clone() }, Some(e), datum)
                } else {
                    (FirmwareLage::Hinder { text: format!("Firmware {} finns, men roboten måste först uppdateras av Mapro: {}.", e.version, krav.join("; ")) }, None, datum)
                }
            }
        }
    }

    fn hamta_fil(&self, e: &Erbjudande<FwVersion>, mal: &Path) -> Result<(), Fel> {
        let svar = http().get(&format!("{}{}", self.kalla(), e.fil)).call().map_err(natfel)?;
        let mut las = svar.into_reader();
        let mut fil = std::fs::File::create(mal).map_err(|e| Fel::Fil(e.to_string()))?;
        let mut bit = vec![0u8; 64 * 1024];
        let (mut summa, mut visat) = (0u64, 101u8);
        loop {
            let n = las.read(&mut bit).map_err(|e| Fel::Nat(e.to_string()))?;
            if n == 0 {
                break;
            }
            summa += n as u64;
            if summa > e.storlek {
                return Err(Fel::Storlek);
            }
            fil.write_all(&bit[..n]).map_err(|e| Fel::Fil(e.to_string()))?;
            let procent = (summa * 100 / e.storlek.max(1)) as u8;
            if procent != visat {
                visat = procent;
                self.steg(e.version, 2, "Hämtar den nya firmwaren från servern", Some(procent));
            }
        }
        fil.sync_all().map_err(|e| Fel::Fil(e.to_string()))
    }

    /// Väntar på en lägesbild där kortet har svarat efter `efter` (ett äldre svar kan
    /// vara från före omstarten).
    fn vanta_pa_kortet(&self, efter: Instant, langst: Duration) -> Option<Lagesbild> {
        let slut = Instant::now() + langst;
        while Instant::now() < slut {
            {
                let i = self.inre.lock().unwrap();
                if i.bild.kort_svarade.is_some_and(|n| n > efter) && i.bild.kortversion.is_some() {
                    return Some(i.bild.clone());
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        None
    }

    /// Som [`Self::vanta_pa_kortet`], men väntar också tills robotd vet om kortet rapporterar
    /// de tillval som går att se (efter ett uppehåll tar det några sekunder). Används före
    /// skrivningen, så att spärrarna inte faller på att uppgiften bara inte hunnit komma.
    fn vanta_pa_hela_bilden(&self, efter: Instant, langst: Duration) -> Option<Lagesbild> {
        let slut = Instant::now() + langst;
        let mut svarat = None;
        while Instant::now() < slut {
            if let Some(b) = self.vanta_pa_kortet(efter, Duration::from_millis(400)) {
                let kand = |t: &Tillval| !t.synligt || t.hors.is_some();
                if kand(&b.nodstopp) && kand(&b.lagesknapp) {
                    return Some(b);
                }
                svarat = Some(b);
            }
        }
        // Kortet svarar men tillvalen blev aldrig kända: spärrarna säger då varför.
        svarat
    }

    /// Försöker starta kortet efter en utläsning som gått fel eller lämnat det tyst, och
    /// säger hur det gick: tom text när kortet svarar, annars vad föraren ska göra.
    fn starta_efter_utlasning(&self, flashare: &Flashare) -> String {
        let fore = Instant::now();
        // Går försöket fel står det i flash.log; kortet kan ha startat ändå, så svaret avgör.
        let _ = flashare.starta_kortet();
        match self.vanta_pa_kortet(fore, VANTA_PA_KORTET) {
            Some(_) => String::new(),
            None => " Styrkortet svarar inte efteråt: slå av och på strömmen till roboten.".into(),
        }
    }

    /// Skriver tillbaka säkerhetskopian och ser om kortet svarar med den gamla versionen.
    fn skriv_tillbaka(&self, flashare: &Flashare, dump: &Path, fran: Option<FwVersion>) -> (bool, String) {
        if let Err(e) = flashare.aterstall(dump) {
            return (false, format!("Återställningen misslyckades: {e}."));
        }
        match self.vanta_pa_kortet(Instant::now(), VANTA_PA_KORTET).and_then(|b| b.kortversion) {
            Some(v) if fran.is_none() || fran == Some(v) => (true, format!("Den förra firmwaren ({v}) är tillbaka och styrkortet svarar.")),
            Some(v) => (false, format!("Säkerhetskopian skrevs tillbaka, men styrkortet uppger version {v}.")),
            None => (false, "Säkerhetskopian skrevs tillbaka, men styrkortet svarar inte.".into()),
        }
    }

    fn uppdatera(&self, onskad: &str, kort: &mut dyn Kortet) {
        let version = {
            let mut i = self.inre.lock().unwrap();
            if i.sparat.lage.pagar() {
                return;
            }
            let Some(v) = FwVersion::tolka(onskad) else { return };
            i.sparat.lage = FirmwareLage::Pagar { version: v.to_string(), steg: 1, steg_antal: STEG_ANTAL, text: "Kontrollerar att roboten står still och är redo".into(), procent: None };
            spara(&self.inst.mapp, &i.sparat);
            v
        };
        self.pagarfil(true);
        let utfall = self.uppdatera_inre(version, kort);
        let mut i = self.inre.lock().unwrap();
        i.erbjudande = None;
        i.sparat.lage = match utfall {
            Ok((fran, anmarkningar)) => FirmwareLage::Klar { version: version.to_string(), fran, anmarkningar },
            Err(Avbrott { text, aterstalld }) => {
                let kan = aterstalld == Some(false) && i.sparat.dump.as_deref().is_some_and(|d| flash::kontrollera_dump(d).is_ok());
                FirmwareLage::Misslyckad { version: version.to_string(), text, aterstalld, kan_aterstallas: kan }
            }
        };
        spara(&self.inst.mapp, &i.sparat);
        self.pagarfil(false);
    }

    fn uppdatera_inre(&self, version: FwVersion, kort: &mut dyn Kortet) -> Result<(String, Vec<String>), Avbrott> {
        let orort = |text: String| Avbrott { text: format!("{text} Ingenting har skrivits till styrkortet."), aterstalld: None };
        let spart = |d: &Delat| -> Result<Lagesbild, Avbrott> {
            let hinder = d.alla_hinder(true);
            if !hinder.is_empty() {
                return Err(orort(format!("Roboten är inte redo: {}", hinder.join(" "))));
            }
            d.bild().ok_or_else(|| orort("Roboten har inget färskt besked om sitt läge.".into()))
        };

        // 1. Spärrarna, och listan en gång till (den kan ha bytts sedan sökningen).
        let bild = spart(self)?;
        let variant = self.variant().map_err(&orort)?;
        let har = bild.kortversion.ok_or_else(|| orort("Styrkortet svarar inte.".into()))?;
        let (manifest, datum) = self.hamta_lista().map_err(|e| orort(felfras(&e)))?;
        self.inre.lock().unwrap().sparat.listans_datum = datum;
        let e = match firmware::utbud(&manifest, &variant, har).map_err(|e| orort(felfras(&e)))? {
            Utbud::Nyare(e) if e.version == version => e,
            Utbud::Nyare(e) => return Err(orort(format!("Servern erbjuder nu version {}, inte {version}. Sök igen.", e.version))),
            _ => return Err(orort(format!("Servern erbjuder inte längre version {version} för den här roboten. Sök igen."))),
        };
        if version <= har {
            return Err(orort(format!("Styrkortet har redan version {har}.")));
        }
        let krav = firmware::ouppfyllda_krav(&e.kraver, &installerat(&self.inst));
        if !krav.is_empty() {
            return Err(orort(format!("Roboten måste först uppdateras av Mapro: {}.", krav.join("; "))));
        }
        let flashare = self.flashare(variant.kort);

        // 2–3. Filen.
        let hamtat = self.inst.mapp.join("hamtat");
        std::fs::create_dir_all(&hamtat).map_err(|e| orort(format!("Mappen {} går inte att skapa: {e}.", hamtat.display())))?;
        let elf = hamtat.join(&e.fil);
        self.steg(version, 2, "Hämtar den nya firmwaren från servern", Some(0));
        let kontroll = self.hamta_fil(&e, &elf).and_then(|()| {
            self.steg(version, 3, "Kontrollerar filen", None);
            beskrivning::kontrollera_fil(&elf, e.storlek, &e.sha256)
        });
        if let Err(fel) = kontroll.map_err(|f| felfras(&f)).and_then(|()| flash::kontrollera_elf(&elf)) {
            let _ = std::fs::remove_file(&elf);
            return Err(orort(fel));
        }

        // 4–5. Säkerhetskopiorna. Roboten ska fortfarande stå still.
        spart(self)?;
        let kopia = self.inst.mapp.join("backup").join(chrono::Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string());
        std::fs::create_dir_all(&kopia).map_err(|e| orort(format!("Mappen {} går inte att skapa: {e}.", kopia.display())))?;
        self.steg(version, 4, "Sparar styrkortets inställningar", None);
        kort.spara_installningar(&kopia).map_err(|t| orort(format!("Styrkortets inställningar gick inte att spara ({t}).")))?;
        self.steg(version, 5, "Säkerhetskopierar styrkortets minne", None);
        let dump = kopia.join("flash.bin");
        if let Err(t) = flashare.dumpa(&dump) {
            // Utläsningen stannar processorn. Gick den fel kan kortet stå kvar stoppat fast
            // ingenting skrivits: försök starta det, och säg till om det inte svarar.
            let efterat = self.starta_efter_utlasning(&flashare);
            return Err(orort(format!("Styrkortets minne gick inte att säkerhetskopiera ({t}).{efterat}")));
        }
        {
            let mut i = self.inre.lock().unwrap();
            i.sparat.dump = Some(dump.clone());
            i.sparat.fran = Some(har.to_string());
        }
        // Utläsningen stannade kortet en stund: vänta på att det svarar igen (och att robotd
        // åter vet vad kortet rapporterar), och se att roboten fortfarande står still, innan
        // något skrivs. Svarar det inte görs ett försök att starta det.
        let efter_utlasning = Instant::now();
        if self.vanta_pa_hela_bilden(efter_utlasning, VANTA_PA_KORTET).is_none() {
            let efterat = self.starta_efter_utlasning(&flashare);
            if !efterat.is_empty() {
                return Err(orort(format!("Styrkortet svarar inte efter säkerhetskopieringen.{efterat}")));
            }
            if self.vanta_pa_hela_bilden(efter_utlasning, VANTA_PA_KORTET).is_none() {
                return Err(orort("Styrkortet svarar inte efter säkerhetskopieringen.".into()));
            }
        }
        // Den här bilden (kortet har svarat en stund) avgör vad som ska krävas av den nya
        // firmwaren efter skrivningen: bilden direkt efter omstarten gör det inte, för då
        // vet robotd ännu inte vad kortet rapporterar.
        let fore = spart(self)?;

        // 6. Skrivningen. Härifrån skrivs säkerhetskopian tillbaka om något går fel.
        self.steg(version, 6, "Skriver den nya firmwaren – bryt inte strömmen", None);
        let tillbaka = |d: &Delat, text: String| -> Avbrott {
            d.steg(version, 6, "Återställer den förra firmwaren – bryt inte strömmen", None);
            let (gick, besked) = d.skriv_tillbaka(&flashare, &dump, Some(har));
            Avbrott { text: format!("{text} {besked}"), aterstalld: Some(gick) }
        };
        if let Err(t) = flashare.skriv(&elf) {
            return Err(tillbaka(self, format!("Skrivningen till styrkortet misslyckades ({t}).")));
        }
        let skrivet = Instant::now();

        // 7. Kortet startar om.
        self.steg(version, 7, "Väntar på att styrkortet startar", None);
        let Some(efter) = self.vanta_pa_kortet(skrivet, VANTA_PA_KORTET) else {
            return Err(tillbaka(self, "Styrkortet svarar inte efter skrivningen.".into()));
        };

        // 8. Rätt version, rätt variant, samma inställningar.
        self.steg(version, 8, "Kontrollerar version och inställningar", None);
        if efter.kortversion != Some(version) {
            let fick = efter.kortversion.map(|v| v.to_string()).unwrap_or_default();
            return Err(tillbaka(self, format!("Styrkortet uppger version {fick} efter skrivningen, inte {version}.")));
        }
        // Tillvalen: har varianten nödstoppet på DI1 (eller lägesknappen på DI2) och robotd kan
        // se kortets rapporter, ska en rapport komma från den NYA firmwaren, alltså efter
        // skrivningen. Vad robotd kan se avgjordes före skrivningen (`fore`); att robotd inte
        // hunnit höra något direkt efter omstarten släpper aldrig igenom något.
        type Senast = fn(&Lagesbild) -> Option<Instant>;
        let tillval: [(&str, bool, bool, Senast); 2] = [
            ("nödstoppet på DI1", variant.nodstopp, fore.nodstopp.synligt, |b| b.nodstopp.senast),
            ("lägesknappen på DI2", variant.lagesknapp, fore.lagesknapp.synligt, |b| b.lagesknapp.senast),
        ];
        for (namn, angiven, kunde_ses, valj) in tillval {
            if !(angiven && kunde_ses) {
                continue;
            }
            let slut = Instant::now() + VANTA_PA_NODSTOPP;
            let hord = |d: &Delat| d.bild().is_some_and(|b| valj(&b).is_some_and(|n| n > skrivet));
            while !hord(self) && Instant::now() < slut {
                std::thread::sleep(Duration::from_millis(300));
            }
            if !hord(self) {
                return Err(tillbaka(self, format!("Den nya firmwaren rapporterar inte {namn}.")));
            }
        }
        let mut anmarkningar = Vec::new();
        match kort.kontrollera_installningar(&kopia) {
            Ok(Installningar::Lika) => {}
            Ok(Installningar::Aterstallda) => anmarkningar.push("Styrkortets inställningar hade ändrats av uppdateringen och har skrivits tillbaka som de var.".into()),
            Ok(Installningar::Skiljer(t)) => anmarkningar.push(format!("Styrkortets inställningar skiljer sig från före uppdateringen och gick inte att skriva tillbaka ({t}). Kontakta Mapro innan roboten körs.")),
            Err(t) => anmarkningar.push(format!("Styrkortets inställningar gick inte att kontrollera efter uppdateringen ({t}). Kontakta Mapro innan roboten körs.")),
        }
        let _ = std::fs::remove_file(&elf);
        stada_kopior(&self.inst.mapp.join("backup"));
        Ok((har.to_string(), anmarkningar))
    }

    /// Föraren ber om att säkerhetskopian från den senaste, misslyckade uppdateringen skrivs tillbaka.
    fn aterstall(&self) {
        let (version, dump, fran) = {
            let mut i = self.inre.lock().unwrap();
            let FirmwareLage::Misslyckad { version, kan_aterstallas: true, .. } = i.sparat.lage.clone() else { return };
            let Some(dump) = i.sparat.dump.clone() else { return };
            let fran = i.sparat.fran.as_deref().and_then(FwVersion::tolka);
            let v = FwVersion::tolka(&version).unwrap_or(FwVersion { huvud: 0, under: 0 });
            i.sparat.lage = FirmwareLage::Pagar { version: version.clone(), steg: 6, steg_antal: STEG_ANTAL, text: "Återställer den förra firmwaren – bryt inte strömmen".into(), procent: None };
            spara(&self.inst.mapp, &i.sparat);
            (v, dump, fran)
        };
        self.pagarfil(true);
        let hinder = self.alla_hinder(false);
        let lage = if !hinder.is_empty() {
            FirmwareLage::Misslyckad { version: version.to_string(), text: format!("Återställningen gjordes inte, roboten är inte redo: {}", hinder.join(" ")), aterstalld: Some(false), kan_aterstallas: true }
        } else {
            let kort = self.variant().map(|v| v.kort).unwrap_or(Kort::Gamla);
            let (gick, besked) = self.skriv_tillbaka(&self.flashare(kort), &dump, fran);
            FirmwareLage::Misslyckad { version: version.to_string(), text: besked, aterstalld: Some(gick), kan_aterstallas: !gick }
        };
        self.satt_lage(lage);
        self.pagarfil(false);
    }
}

struct Avbrott {
    text: String,
    /// Se [`FirmwareLage::Misslyckad`].
    aterstalld: Option<bool>,
}

/// Behåller de senaste säkerhetskopiorna (mapparna heter efter tiden) och tar bort äldre.
fn stada_kopior(mapp: &Path) {
    let Ok(lista) = std::fs::read_dir(mapp) else { return };
    let mut mappar: Vec<PathBuf> = lista.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    mappar.sort();
    let bort = mappar.len().saturating_sub(SPARADE_KOPIOR);
    for m in &mappar[..bort] {
        let _ = std::fs::remove_dir_all(m);
    }
}

impl Uppdaterare {
    /// Läser det sparade läget och startar uppdateringens tråd.
    pub fn starta(inst: Installning, mut kort: Box<dyn Kortet>) -> Uppdaterare {
        let sparat = ladda(&inst.mapp);
        spara(&inst.mapp, &sparat);
        let delat = Arc::new(Delat { inre: Mutex::new(Inre { sparat, bild: Lagesbild::default(), bild_nar: None, erbjudande: None, klara: 0 }), inst, forutsattningar: Mutex::new(None) });
        // En fil som blivit kvar efter att robotd avbrutits mitt i gäller inte längre.
        delat.pagarfil(false);
        let (tx, rx) = mpsc::channel();
        let d = delat.clone();
        std::thread::Builder::new()
            .name("firmwareuppdatering".into())
            .spawn(move || {
                while let Ok(order) = rx.recv() {
                    match order {
                        Order::Sok => d.sok(),
                        Order::Uppdatera(version) => d.uppdatera(&version, kort.as_mut()),
                        Order::Aterstall => d.aterstall(),
                    }
                    d.inre.lock().unwrap().klara += 1;
                }
            })
            .expect("tråden för firmwareuppdateringen gick inte att starta");
        Uppdaterare { delat, tx }
    }

    /// robotd:s senaste besked om roboten. Anropas ofta (varje statusvarv).
    pub fn lagesbild(&self, bild: Lagesbild) {
        let mut i = self.delat.inre.lock().unwrap();
        i.bild = bild;
        i.bild_nar = Some(Instant::now());
    }

    /// Antal order (sök, uppdatera, återställ) som är färdigbehandlade sedan start. Den som
    /// vill vänta på ett besked ser när talet växer.
    pub fn klara_order(&self) -> u64 {
        self.delat.inre.lock().unwrap().klara
    }

    /// Pågår en uppdatering? Då ska robotd vägra aktivering och körkommandon.
    pub fn pagar(&self) -> bool {
        self.delat.inre.lock().unwrap().sparat.lage.pagar()
    }

    /// Läget, för robotd:s status till klienterna.
    pub fn status(&self) -> FirmwareStatus {
        let (lage, bild, datum) = {
            let i = self.delat.inre.lock().unwrap();
            (i.sparat.lage.clone(), i.bild_nar.filter(|n| n.elapsed() <= FARSK).map(|_| i.bild.clone()), i.sparat.listans_datum.clone())
        };
        let mut sparrar = bild.as_ref().map(|b| b.sparrar.clone()).unwrap_or_default();
        if !lage.pagar() {
            sparrar.extend(self.delat.fasta_hinder(bild.as_ref()));
        }
        FirmwareStatus {
            kortversion: bild.as_ref().and_then(|b| b.kortversion).map(|v| v.to_string()),
            variant: self.delat.variant().ok().map(|v| v.nyckel()),
            lage,
            sparrar,
            listans_datum: datum,
            provbygge: crate::PROVBYGGE,
        }
    }

    pub fn sok(&self) {
        let _ = self.tx.send(Order::Sok);
    }

    /// Börjar uppdateringen till `version`. `Err` = den började inte, med orsaken.
    pub fn uppdatera(&self, version: &str) -> Result<(), String> {
        {
            let i = self.delat.inre.lock().unwrap();
            if i.sparat.lage.pagar() {
                return Err("En uppdatering av styrkortet pågår redan.".into());
            }
            match &i.erbjudande {
                Some(e) if e.version.to_string() == version => {}
                _ => return Err("Den versionen är inte den roboten erbjöd. Sök ny firmware igen.".into()),
            }
        }
        let hinder = self.delat.alla_hinder(true);
        if !hinder.is_empty() {
            return Err(format!("Roboten är inte redo: {}", hinder.join(" ")));
        }
        self.tx.send(Order::Uppdatera(version.to_string())).map_err(|_| "uppdateringens tråd har slutat".to_string())
    }

    /// Ber om att säkerhetskopian skrivs tillbaka (efter en misslyckad uppdatering).
    pub fn aterstall(&self) -> Result<(), String> {
        {
            let i = self.delat.inre.lock().unwrap();
            if !matches!(i.sparat.lage, FirmwareLage::Misslyckad { kan_aterstallas: true, .. }) {
                return Err("Det finns ingen misslyckad uppdatering att återställa.".into());
            }
        }
        let hinder = self.delat.alla_hinder(false);
        if !hinder.is_empty() {
            return Err(format!("Roboten är inte redo: {}", hinder.join(" ")));
        }
        self.tx.send(Order::Aterstall).map_err(|_| "uppdateringens tråd har slutat".to_string())
    }
}

#[cfg(test)]
mod tester;
