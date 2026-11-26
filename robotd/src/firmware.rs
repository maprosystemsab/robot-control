//! Uppdateringen av styrkortets firmware från Robotstyrning (Inställningar → Robotens firmware).
//!
//! Själva arbetet (listan från servern, signaturen, hämtningen, säkerhetskopian, openocd,
//! återställningen) gör den gemensamma `robotuppdatering::robot`. Här finns det bara robotd
//! kan: läsa och skriva kortets inställningar över länken till Car_Client, och säga vad som
//! hindrar en uppdatering just nu (spärrarna).
//!
//! Varianten (`firmware.variant` i config.json) ska stå uttryckligen: den avgör vilken
//! firmwarefil som är rätt, och fel fil kan ge en robot utan fungerande nödstopp. Utan den
//! går det inte att uppdatera härifrån. Den jämförs också med märkfilerna som
//! `uppdatera_pi.sh` bygger efter (`/etc/robotd/nodstopp-di1`, `/etc/robotd/lagesknapp-di2`):
//! säger de olika saker görs ingenting.

use std::path::{Path, PathBuf};
use std::time::Duration;

use robotuppdatering::firmware::{FwVersion, Variant};
use robotuppdatering::robot::{Installning, Installningar, Kortet, Lagesbild, Tillval, Uppdaterare};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use crate::car_client_link::ConfigRequest;

/// robotd:s version (paketets version + commit-tiden, se build.rs).
pub const VERSION: &str = env!("ROBOTD_VERSION");
/// robotd:s namn i firmwarens krav.
pub const KOMPONENT: &str = "robotd";
/// Märkfilerna som säger hur MP101:s firmware är byggd (se `scripts/uppdatera_pi.sh`).
const MARKE_NODSTOPP: &str = "etc/robotd/nodstopp-di1";
const MARKE_LAGESKNAPP: &str = "etc/robotd/lagesknapp-di2";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FirmwareConfig {
    /// Robotens kortvariant, t.ex. "mp101-robant-nodstopp" eller "gamla-robant" (se
    /// robot-control/UPPDATERING_FIRMWARE.md). Skrivs av installationen. `None` = roboten
    /// kan inte uppdateras från Robotstyrning.
    pub variant: Option<String>,
    /// Annan källa än vår server (bara för prov; signaturen kontrolleras alltid).
    pub kalla: Option<String>,
    /// Eget kommando i stället för openocd (prov utan hårdvara).
    pub flashkommando: Option<PathBuf>,
    /// Annan mapp för tillståndet och säkerhetskopiorna (prov utan hårdvara).
    pub mapp: Option<PathBuf>,
}

/// Efter skrivningen får frågor om inställningarna vänta på kortet (det har just startat
/// om); före skrivningen ska det svara på en gång.
const FORSOK_EFTER: u32 = 10;
const FORSOK_FORE: u32 = 2;
const MELLAN_FORSOK: Duration = Duration::from_secs(3);
const FILNAMN: &str = "installningar.bin";

/// Kortets inställningar över länken till Car_Client. Anropen kommer från uppdateringens
/// egen tråd (inte från tokio), därför de blockerande sändningarna.
struct Lank {
    config: mpsc::Sender<ConfigRequest>,
}

impl Lank {
    fn las(&self, antal: u32) -> Result<Vec<u8>, String> {
        let mut sista = String::new();
        for forsok in 0..antal {
            if forsok > 0 {
                std::thread::sleep(MELLAN_FORSOK);
            }
            let (tx, rx) = oneshot::channel();
            if self.config.blocking_send(ConfigRequest::ReadRaw(tx)).is_err() {
                return Err("länken till styrkortet har avslutats".into());
            }
            match rx.blocking_recv() {
                Ok(Ok(b)) => return Ok(b),
                Ok(Err(e)) => sista = e,
                Err(_) => sista = "länken till styrkortet avbröt".into(),
            }
        }
        Err(sista)
    }

    fn skriv(&self, body: Vec<u8>) -> Result<Vec<u8>, String> {
        let (tx, rx) = oneshot::channel();
        self.config.blocking_send(ConfigRequest::WriteRaw(body, tx)).map_err(|_| "länken till styrkortet har avslutats".to_string())?;
        rx.blocking_recv().unwrap_or_else(|_| Err("länken till styrkortet avbröt".into()))
    }
}

impl Kortet for Lank {
    fn spara_installningar(&mut self, mapp: &Path) -> Result<(), String> {
        // Två läsningar ska ge samma svar, så att kopian inte är en halvläst inställning.
        let (a, b) = (self.las(FORSOK_FORE)?, self.las(FORSOK_FORE)?);
        if a != b || a.len() < 100 {
            return Err("två läsningar av inställningarna gav olika svar".into());
        }
        std::fs::write(mapp.join(FILNAMN), &a).map_err(|e| format!("kopian gick inte att spara: {e}"))?;
        tracing::info!("Styrkortets inställningar sparade ({} byte) i {}.", a.len(), mapp.display());
        Ok(())
    }

    fn kontrollera_installningar(&mut self, mapp: &Path) -> Result<Installningar, String> {
        let fore = std::fs::read(mapp.join(FILNAMN)).map_err(|e| format!("kopian går inte att läsa: {e}"))?;
        let nu = self.las(FORSOK_EFTER)?;
        if nu == fore {
            return Ok(Installningar::Lika);
        }
        if nu.len() != fore.len() {
            return Ok(Installningar::Skiljer(format!("inställningsblocket är {} byte i den nya versionen, {} i den gamla", nu.len(), fore.len())));
        }
        tracing::warn!("Styrkortets inställningar ändrades av uppdateringen: de sparade skrivs tillbaka.");
        match self.skriv(fore.clone()) {
            Ok(efter) if efter == fore => Ok(Installningar::Aterstallda),
            Ok(_) => Ok(Installningar::Skiljer("kortet kvitterade skrivningen men visar något annat efteråt".into())),
            Err(e) => Ok(Installningar::Skiljer(e)),
        }
    }
}

/// Startar uppdateraren och skriver in robotd:s version i robotens register (för kraven).
pub fn starta(cfg: &FirmwareConfig, config: mpsc::Sender<ConfigRequest>) -> Uppdaterare {
    let mut inst = Installning { variant: cfg.variant.clone(), flashkommando: cfg.flashkommando.clone(), ..Default::default() };
    if let Some(k) = cfg.kalla.as_deref().filter(|k| !k.trim().is_empty()) {
        tracing::warn!("Firmwareuppdateringen hämtar från {k} (inte den vanliga servern). Signaturen kontrolleras ändå.");
        inst.kalla = k.to_string();
    }
    if let Some(m) = &cfg.mapp {
        inst.mapp = m.clone();
    }
    if robotuppdatering::PROVBYGGE {
        tracing::error!("PROVBYGGE: den här robotd godkänner uppdateringslistor signerade med provnyckeln. Den får inte användas på en riktig robot.");
    }
    match &cfg.variant {
        Some(v) => tracing::info!("Styrkortets firmware kan uppdateras från Robotstyrning (kortvariant {v})."),
        None => tracing::info!("Ingen kortvariant angiven (firmware.variant): styrkortet kan inte uppdateras från Robotstyrning."),
    }
    robotuppdatering::robot::registrera_komponent(&inst.mapp, KOMPONENT, VERSION);
    Uppdaterare::starta(inst, Box::new(Lank { config }))
}

/// Det robotd ser som avgör om kortet får uppdateras nu.
#[derive(Debug, Clone, Default)]
pub struct Lage {
    pub aktiverad: bool,
    /// Gas, styrning eller vändning skickas till kortet.
    pub styr: bool,
    pub fart_ms: Option<f32>,
    /// Så länge har roboten stått still (ingen fart, inga styrvärden). `None` = den rör sig.
    pub stilla: Option<Duration>,
    /// Anslutna utöver föraren (tittare, telefonen).
    pub andra_anslutna: usize,
    pub batteri_procent: Option<f32>,
    /// Varianten och märkfilerna på roboten säger olika saker (texten säger vad).
    pub markfel: Option<String>,
}

/// Så länge ska roboten ha stått still innan kortet får skrivas.
pub const STILLA_MINST: Duration = Duration::from_secs(3);
/// Under den här batterinivån uppdateras inte kortet (strömmen får inte ta slut mitt i).
pub const BATTERI_MINST: f32 = 20.0;

/// Spärrarna i klartext. Tom lista = roboten är redo.
pub fn sparrar(l: &Lage) -> Vec<String> {
    let mut s = Vec::new();
    if l.aktiverad {
        s.push("AKTIVERA är på. Stäng av den.".to_string());
    }
    if l.styr || l.fart_ms.is_some_and(|v| v.abs() > 0.05) {
        s.push("Roboten rör sig. Den ska stå helt still.".into());
    } else if l.stilla.is_none_or(|t| t < STILLA_MINST) {
        s.push("Roboten har inte stått still tillräckligt länge än.".into());
    }
    if l.andra_anslutna > 0 {
        s.push(format!("{} till är anslutna till roboten. Bara den som kör får vara ansluten.", l.andra_anslutna));
    }
    if l.batteri_procent.is_some_and(|p| p < BATTERI_MINST) {
        s.push(format!("Batteriet är under {BATTERI_MINST:.0} %. Ladda först."));
    }
    s.extend(l.markfel.clone());
    s
}

/// Stämmer den angivna varianten med märkfilerna under `rot` ("/")? `Some(text)` när de
/// säger olika saker om nödstoppet på DI1 eller lägesknappen på DI2 (bara MP101 har dem).
pub fn markfel(variant: Option<&str>, rot: &Path) -> Option<String> {
    let v = Variant::tolka(variant?.trim())?;
    let fel = |namn: &str, angiven: bool, marke: &str| {
        let finns = rot.join(marke).exists();
        (finns != angiven).then(|| {
            format!(
                "Robotens kortvariant säger {} {namn}, men märkfilen /{marke} {}. Det ena är fel: rätta det innan roboten kan uppdateras (kontakta Mapro).",
                if angiven { "med" } else { "utan" },
                if finns { "finns" } else { "finns inte" }
            )
        })
    };
    fel("nödstopp på DI1", v.nodstopp, MARKE_NODSTOPP).or_else(|| fel("lägesknapp på DI2", v.lagesknapp, MARKE_LAGESKNAPP))
}

/// Lägesbilden till uppdateraren. `fw` är versionen och när kortets svar kom.
///
/// Nödstoppets paket säger ingenting här (Car_Client skickar ett eget för nödstoppet på
/// Pi:n), så det tillvalet är inte synligt och varianten jämförs med märkfilerna i stället
/// ([`markfel`]). Lägesknappens paket kommer bara från en firmware byggd med lägesknapp,
/// och Car_Client skickar det vidare: `lagesknapp_senast` är när det senast kom, `moget` om
/// kortet har svarat så länge att ett uteblivet paket betyder att firmwaren saknar knappen
/// (före det är svaret okänt, och okänt släpper aldrig igenom något).
pub fn lagesbild(fw: Option<((u8, u8), std::time::Instant)>, sparrar: Vec<String>, lagesknapp_senast: Option<std::time::Instant>, moget: bool) -> Lagesbild {
    let hors = match lagesknapp_senast {
        Some(nar) if nar.elapsed() < Duration::from_secs(4) => Some(true),
        _ if moget => Some(false),
        _ => None,
    };
    Lagesbild {
        kortversion: fw.map(|((huvud, under), _)| FwVersion { huvud, under }),
        kort_svarade: fw.map(|(_, nar)| nar),
        sparrar,
        nodstopp: Tillval::default(),
        lagesknapp: Tillval { synligt: true, hors, senast: lagesknapp_senast },
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn redo() -> Lage {
        Lage { stilla: Some(Duration::from_secs(10)), fart_ms: Some(0.0), batteri_procent: Some(80.0), ..Default::default() }
    }

    #[test]
    fn bara_en_stillastaende_robot_utan_annat_pa_gang_ar_redo() {
        assert!(sparrar(&redo()).is_empty());
        let fall: Vec<(&str, Box<dyn Fn(&mut Lage)>)> = vec![
            ("AKTIVERA", Box::new(|l| l.aktiverad = true)),
            ("rör sig", Box::new(|l| l.styr = true)),
            ("rör sig", Box::new(|l| l.fart_ms = Some(-0.2))),
            ("tillräckligt länge", Box::new(|l| l.stilla = Some(Duration::from_secs(1)))),
            ("tillräckligt länge", Box::new(|l| l.stilla = None)),
            ("anslutna", Box::new(|l| l.andra_anslutna = 1)),
            ("Batteriet", Box::new(|l| l.batteri_procent = Some(12.0))),
            ("märkfilen", Box::new(|l| l.markfel = Some("… märkfilen …".into()))),
        ];
        for (del, andra) in fall {
            let mut l = redo();
            andra(&mut l);
            let s = sparrar(&l);
            assert!(s.len() == 1 && s[0].contains(del), "{del}: {s:?}");
        }
        assert!(sparrar(&Lage { fart_ms: None, batteri_procent: None, ..redo() }).is_empty());
    }

    #[test]
    fn varianten_ska_stamma_med_markfilerna() {
        let rot = std::env::temp_dir().join(format!("robotd-markfiler-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&rot);
        std::fs::create_dir_all(rot.join("etc/robotd")).unwrap();
        // Inga märkfiler: bara varianter utan tillval stämmer.
        assert_eq!(markfel(Some("mp101-robant"), &rot), None);
        assert_eq!(markfel(Some("gamla-robant"), &rot), None);
        assert!(markfel(Some("mp101-robant-nodstopp"), &rot).unwrap().contains("med nödstopp på DI1"));
        assert!(markfel(Some("mp101-robant-lagesknapp"), &rot).unwrap().contains("med lägesknapp på DI2"));
        // Nödstoppets märkfil finns: en variant utan nödstopp får inte gå igenom.
        std::fs::write(rot.join(MARKE_NODSTOPP), b"").unwrap();
        assert!(markfel(Some("mp101-robant"), &rot).unwrap().contains("utan nödstopp på DI1"));
        assert_eq!(markfel(Some("mp101-robant-nodstopp"), &rot), None);
        assert!(markfel(Some("mp101-robant-nodstopp-lagesknapp"), &rot).unwrap().contains("lägesknapp"));
        std::fs::write(rot.join(MARKE_LAGESKNAPP), b"").unwrap();
        assert_eq!(markfel(Some("mp101-robant-nodstopp-lagesknapp"), &rot), None);
        // Ingen eller obegriplig variant: det säger uppdateraren själv ifrån om.
        assert_eq!(markfel(None, &rot), None);
        assert_eq!(markfel(Some("skräp"), &rot), None);
    }

    #[test]
    fn lagesknappen_ar_okand_tills_kortet_svarat_en_stund_och_nodstoppet_syns_inte() {
        let nyss = Some(std::time::Instant::now());
        let b = lagesbild(Some(((30, 32), std::time::Instant::now())), vec![], nyss, false);
        assert_eq!((b.lagesknapp.synligt, b.lagesknapp.hors, b.nodstopp.synligt), (true, Some(true), false));
        assert_eq!(lagesbild(None, vec![], None, false).lagesknapp.hors, None);
        assert_eq!(lagesbild(None, vec![], None, true).lagesknapp.hors, Some(false));
        let lange_sedan = std::time::Instant::now().checked_sub(Duration::from_secs(30));
        assert_eq!(lagesbild(None, vec![], lange_sedan, false).lagesknapp.hors, None);
    }

    #[test]
    fn versionen_gar_att_jamfora_med_ett_krav() {
        assert!(robotuppdatering::beskrivning::jamfor_versioner(VERSION, "0.1.0").is_some(), "{VERSION}");
    }
}
