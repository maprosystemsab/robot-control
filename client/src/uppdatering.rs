//! Uppdatering av programmet med en knapp på anslutningsskärmen (Windows och Linux).
//!
//! Samma uppdatering som i SentinAnt (autorobot, `client/src/uppdatering.rs`): den här
//! filen är den, med SentinAnt Lites namn. Kontrollen av beskrivningen (signatur,
//! löpnummer, storlek och SHA-256) är gemensam kod i craten `robotuppdatering`. Rättas
//! något här ska det rättas där också.
//!
//! Programmet hämtar en uppdateringsbeskrivning (`uppdatering.json`) från vår server i
//! tunneln, i bakgrunden vid start och när man trycker "Sök uppdatering". Finns en nyare
//! version blir knappen grön ("Uppdatera nu"); ett tryck hämtar den nya programfilen,
//! kontrollerar den, byter ut programmet och startar om det. Annan källa (för prov):
//! miljövariabeln `SENTINANT_UPPDATERING=http://adress/mapp/`.
//!
//! Säkerheten: hämtningen går över vanlig HTTP, så ingenting litas på förrän det är
//! kontrollerat. Beskrivningen är signerad (ed25519) med en nyckel som bara vi har, och
//! den publika halvan är inbyggd här. I beskrivningen står varje fils storlek och
//! SHA-256. En fil installeras bara om signaturen stämmer, versionen är nyare än den
//! som kör (aldrig bakåt), storleken och kontrollsumman stämmer och den nya programfilen
//! går att starta och själv uppger rätt version.
//!
//! Beskrivningens form (det som signeras är texten i `manifest`, tecken för tecken):
//! ```json
//! { "manifest": "<JSON-texten nedan>", "signaturer": ["<128 hex>", "..."] }
//!
//! { "format": 1,
//!   "lopnummer": 1795790000,
//!   "skapad": "2026-11-26T14:27:00Z",
//!   "paket": {
//!     "sentinant-lite": {
//!       "version": "0.1.0+202611261256",
//!       "nyheter": "Valfri text som visas vid knappen",
//!       "filer": {
//!         "windows-x86_64": { "fil": "sentinant-lite-…-windows-x86_64.exe", "storlek": 123, "sha256": "<64 hex>" },
//!         "linux-x86_64":   { "fil": "sentinant-lite-…-linux-x86_64",       "storlek": 123, "sha256": "<64 hex>" }
//!       } } } }
//! ```
//! En post per komponent och plattform. Listan är gemensam med SentinAnt (posten
//! `sentinant`) och styrkortets firmware (`firmware`): varje program läser bara sin egen
//! post. Okända fält och poster hoppas över. Flera signaturer får finnas, så att nyckeln
//! kan bytas: det räcker att en stämmer med någon av de inbyggda nycklarna.
//!
//! Färskhet: signaturen hindrar förfalskning, men inte att någon som svarar i serverns
//! ställe visar en gammal, äkta beskrivning om och om igen. Därför har beskrivningen ett
//! signerat `lopnummer` som bara växer (publiceringsskriptet sätter det). Programmet minns
//! det högsta det sett (`uppdatering.json` i inställningsmappen) och avvisar en
//! beskrivning med lägre nummer, med en varning på skärmen. `skapad` visas vid knappen, så
//! att det syns hur gammal serverns lista är. Ett valfritt `giltig_till` (RFC 3339) ger en
//! varning när det passerats men stoppar ingenting: en dator med fel klocka eller en lista
//! som ingen hunnit förnya ska inte stänga av uppdateringen. En dator som aldrig fått se
//! en nyare beskrivning kan inte upptäcka att den hålls tillbaka; då är datumet vid
//! knappen och vårt mejl om den nya versionen det som avslöjar det.
//!
//! Var programmet ligger avgör hur det byts ([`Installation`]): installerat per användare
//! (Windows-installationen, eller en egen mapp) byts programfilen på plats; Linux-paketet
//! i /usr/bin kan inte skrivas över utan lösenord, så där läggs den nya versionen i
//! användarens mapp (`~/.local/share/sentinant-lite/program/`) och programmet i /usr/bin
//! lämnar över till den vid start. Ett program som ligger i cargos `target` uppdateras
//! aldrig härifrån (utvecklingsdatorn). En kund som installerat från källkod
//! (`scripts/install_client.sh`) har programmet kopierat till en egen mapp
//! (`~/.local/share/sentinant-lite/`) och får knappen som alla andra.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;

use crate::version::{self, Version};

use robotuppdatering::beskrivning::{self as karna, MANIFESTFIL, STANDARDKALLA, STORSTA_BESKRIVNING};
pub use robotuppdatering::beskrivning::{Farskhet, Manifest};
#[cfg(test)]
use robotuppdatering::beskrivning::PUBLIKA_NYCKLAR;

/// Det här programmets post i beskrivningen.
const KOMPONENT: &str = "sentinant-lite";
/// Ny kontroll när servern inte svarade, och när den svarade (programmet kan stå på i dagar).
const VANTAN_EFTER_FEL: Duration = Duration::from_secs(120);
const VANTAN_EFTER_SVAR: Duration = Duration::from_secs(6 * 3600);

/// Varför en uppdatering inte gick att söka eller installera.
#[derive(Debug, Clone, PartialEq)]
pub enum Fel {
    /// Servern svarade inte (inget nät, tunneln nere) eller svarade med fel.
    Nat(String),
    /// Beskrivningen gick inte att läsa alls.
    Kuvert,
    /// Ingen signatur stämmer med någon av våra nycklar.
    Signatur,
    /// Signaturen stämmer men innehållet är fel (skrivfel vid publiceringen).
    Manifest(String),
    /// Beskrivningen har en nyare form än programmet förstår.
    Format(u32),
    /// Den hämtade filen har fel storlek.
    Storlek,
    /// Den hämtade filen har fel SHA-256.
    Kontrollsumma,
    /// Den nya programfilen startar inte, eller uppger fel version.
    Provstart(String),
    /// Filen gick inte att spara eller byta.
    Fil(String),
    /// Beskrivningen är äldre än en som den här datorn redan sett (lägre löpnummer).
    Aterspelad { sett: u64, fick: u64 },
    /// Föraren lämnade anslutningsskärmen.
    Avbruten,
}

impl Fel {
    /// Fel som kan betyda att någon lägger sig i uppdateringen: visas som varning.
    fn varning(&self) -> bool {
        matches!(self, Fel::Kuvert | Fel::Signatur | Fel::Aterspelad { .. })
    }
}

impl std::fmt::Display for Fel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fel::Nat(_) => write!(f, "Uppdateringsservern svarar inte. Kontrollera att VPN:et (WireGuard) är igång."),
            Fel::Kuvert | Fel::Signatur => write!(f, "Uppdateringen från servern gick inte att kontrollera (signaturen stämmer inte). Ingenting har installerats."),
            Fel::Manifest(t) => write!(f, "Uppdateringen på servern är felaktigt beskriven ({t}). Ingenting har installerats."),
            Fel::Format(_) => write!(f, "Den här versionen är för gammal för att uppdatera sig själv. Installera den senaste installationsfilen."),
            Fel::Storlek | Fel::Kontrollsumma => write!(f, "Den hämtade filen stämmer inte med beskrivningen och har kastats. Försök igen."),
            Fel::Provstart(t) => write!(f, "Den nya versionen gick inte att starta på den här datorn ({t}). Ingenting har ändrats."),
            Fel::Fil(t) => write!(f, "Den nya versionen gick inte att spara: {t}"),
            Fel::Aterspelad { .. } => write!(f, "Servern visade en äldre uppdateringslista än den här datorn redan har sett. Ingenting har installerats. Hör av dig till oss om det står kvar."),
            Fel::Avbruten => write!(f, "Uppdateringen avbröts."),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Beskrivningen: signatur och innehåll (koden ligger i `robotuppdatering::beskrivning`,
// gemensam med robotens firmwareuppdatering)
// ---------------------------------------------------------------------------------------

impl From<karna::Fel> for Fel {
    fn from(f: karna::Fel) -> Fel {
        match f {
            karna::Fel::Nat(t) => Fel::Nat(t),
            karna::Fel::Kuvert => Fel::Kuvert,
            karna::Fel::Signatur => Fel::Signatur,
            karna::Fel::Manifest(t) => Fel::Manifest(t),
            karna::Fel::Format(n) => Fel::Format(n),
            karna::Fel::Storlek => Fel::Storlek,
            karna::Fel::Kontrollsumma => Fel::Kontrollsumma,
            karna::Fel::Fil(t) => Fel::Fil(t),
            karna::Fel::Aterspelad { sett, fick } => Fel::Aterspelad { sett, fick },
        }
    }
}

/// En nyare version av programmet som går att hämta.
pub type Erbjudande = karna::Erbjudande<Version>;

/// De inbyggda publika nycklarna.
fn nycklar() -> Vec<[u8; 32]> {
    karna::nycklar()
}

/// Kontrollerar signaturen och tolkar sedan beskrivningen ([`karna::oppna_kuvert`]).
pub fn oppna_kuvert(kuvert: &[u8], nycklar: &[[u8; 32]]) -> Result<Manifest, Fel> {
    Ok(karna::oppna_kuvert(kuvert, nycklar)?)
}

/// Programmets erbjudande ur beskrivningen.
pub trait Programlista {
    /// Finns en version av `komponent` för `plattform` som är nyare än `har`? En äldre
    /// eller samma version är aldrig ett erbjudande: programmet går aldrig bakåt.
    fn erbjudande(&self, komponent: &str, plattform: &str, har: Version) -> Result<Option<Erbjudande>, Fel>;
}

impl Programlista for Manifest {
    fn erbjudande(&self, komponent: &str, plattform: &str, har: Version) -> Result<Option<Erbjudande>, Fel> {
        Ok(self.nyare(komponent, plattform, har, Version::tolka)?)
    }
}

/// Avvisar en beskrivning med lägre löpnummer än det högsta som setts ([`karna::granska_farskhet`]).
pub fn granska_farskhet(manifest: &Manifest, minne: Option<&Path>, nu: chrono::DateTime<chrono::Utc>) -> Result<Farskhet, Fel> {
    Ok(karna::granska_farskhet(manifest, minne, nu)?)
}

/// Där det högsta sedda löpnumret sparas.
fn farskhetsminne() -> Option<PathBuf> {
    crate::config_dir().map(|d| d.join("uppdatering.json"))
}

// ---------------------------------------------------------------------------------------
// Hämtning och kontroll av filer
// ---------------------------------------------------------------------------------------

fn http() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(20))
        .user_agent(&format!("SentinAnt-Lite/{}", version::DENNA))
        .build()
}

fn natfel(e: ureq::Error) -> Fel {
    match e {
        ureq::Error::Status(kod, _) => Fel::Nat(format!("HTTP {kod}")),
        e => Fel::Nat(e.to_string()),
    }
}

/// Källans adress, alltid med snedstreck sist.
fn kalla() -> String {
    let k = std::env::var("SENTINANT_UPPDATERING").ok().filter(|k| !k.trim().is_empty()).unwrap_or_else(|| STANDARDKALLA.to_string());
    format!("{}/", k.trim().trim_end_matches('/'))
}

fn hamta_beskrivning(kalla: &str) -> Result<Vec<u8>, Fel> {
    let svar = http().get(&format!("{kalla}{MANIFESTFIL}")).call().map_err(natfel)?;
    let mut b = Vec::new();
    svar.into_reader().take(STORSTA_BESKRIVNING + 1).read_to_end(&mut b).map_err(|e| Fel::Nat(e.to_string()))?;
    if b.len() as u64 > STORSTA_BESKRIVNING {
        return Err(Fel::Kuvert);
    }
    Ok(b)
}

/// Frågar källan om det finns en nyare version för den här datorn.
fn sok(kalla: &str, plattform: &str) -> Result<(Option<Erbjudande>, Farskhet), Fel> {
    let kuvert = hamta_beskrivning(kalla)?;
    let manifest = oppna_kuvert(&kuvert, &nycklar())?;
    let farskhet = granska_farskhet(&manifest, farskhetsminne().as_deref(), chrono::Utc::now())?;
    Ok((manifest.erbjudande(KOMPONENT, plattform, Version::denna())?, farskhet))
}

/// Hämtar `url` till filen `mal` (som skapas körbar). Tar aldrig emot mer än `storlek`
/// byte. `fortsatt` frågas mellan bitarna; svarar den nej avbryts hämtningen.
fn hamta_fil(url: &str, mal: &Path, storlek: u64, hamtat: &AtomicU64, fortsatt: &dyn Fn() -> bool) -> Result<(), Fel> {
    let svar = http().get(url).call().map_err(natfel)?;
    let mut las = svar.into_reader();
    let mut val = std::fs::OpenOptions::new();
    val.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut val, 0o755);
    let mut fil = val.open(mal).map_err(|e| Fel::Fil(e.to_string()))?;
    let mut bit = vec![0u8; 64 * 1024];
    let mut summa = 0u64;
    hamtat.store(0, Ordering::Relaxed);
    loop {
        if !fortsatt() {
            return Err(Fel::Avbruten);
        }
        let n = las.read(&mut bit).map_err(|e| Fel::Nat(e.to_string()))?;
        if n == 0 {
            break;
        }
        summa += n as u64;
        if summa > storlek {
            return Err(Fel::Storlek);
        }
        fil.write_all(&bit[..n]).map_err(|e| Fel::Fil(e.to_string()))?;
        hamtat.store(summa, Ordering::Relaxed);
    }
    fil.sync_all().map_err(|e| Fel::Fil(e.to_string()))
}

/// Stämmer filen på disken med beskrivningen? Storleken först, sedan SHA-256.
pub fn kontrollera_fil(fil: &Path, storlek: u64, sha256: &[u8; 32]) -> Result<(), Fel> {
    Ok(karna::kontrollera_fil(fil, storlek, sha256)?)
}

/// Startar `fil --version` och väntar på svaret (högst `langst`).
fn fraga_version(fil: &Path, langst: Duration) -> Result<Version, String> {
    let mut kommando = Command::new(fil);
    kommando.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut kommando, 0x0800_0000); // inget konsolfönster
    let mut barn = kommando.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    loop {
        match barn.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > langst => {
                let _ = barn.kill();
                let _ = barn.wait();
                return Err("den svarade inte".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let mut ut = String::new();
    if let Some(mut o) = barn.stdout.take() {
        let _ = o.read_to_string(&mut ut);
    }
    Version::tolka(ut.trim()).ok_or_else(|| "den uppgav ingen version".to_string())
}

/// Sista kontrollen före bytet: den nya programfilen ska gå att starta på den här datorn
/// och själv uppge den version beskrivningen lovar.
fn provstarta(fil: &Path, vantad: Version) -> Result<(), Fel> {
    match fraga_version(fil, Duration::from_secs(15)) {
        Ok(v) if v == vantad => Ok(()),
        Ok(v) => Err(Fel::Provstart(format!("den uppger version {v}, inte {vantad}"))),
        Err(t) => Err(Fel::Provstart(t)),
    }
}

// ---------------------------------------------------------------------------------------
// Var programmet ligger och hur det byts
// ---------------------------------------------------------------------------------------

/// Hur den här installationen uppdateras.
#[derive(Debug, Clone, PartialEq)]
pub enum Installation {
    /// Byggt från källkod (ligger i cargos `target`): uppdateras med git, aldrig härifrån.
    Utveckling,
    /// Programfilen ligger där användaren får skriva (installerat per användare): byts på plats.
    PaPlats(PathBuf),
    /// Linux-paketet (/usr/bin): den nya versionen läggs i användarens mapp, och programmet
    /// i /usr/bin lämnar över till den vid start ([`fore_start`]).
    Anvandarkopia(PathBuf),
    /// Går inte att uppdatera här.
    Ingen,
}

/// Användarens egen kopia av programmet (bara Linux).
fn anvandarkopia() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    Some(directories::ProjectDirs::from("se", "mapro", "sentinant-lite")?.data_local_dir().join("program").join("sentinant-lite"))
}

fn skrivbar(mapp: &Path) -> bool {
    let prov = mapp.join(format!(".sentinant-lite-skrivprov-{}", std::process::id()));
    let gick = std::fs::write(&prov, b"").is_ok();
    let _ = std::fs::remove_file(&prov);
    gick
}

fn installation_for(program: &Path, kopia: Option<PathBuf>) -> Installation {
    let Some(mapp) = program.parent() else { return Installation::Ingen };
    // Cargos byggmapp (target/release, target/debug) känns igen på sina egna filer.
    if mapp.join(".fingerprint").is_dir() || mapp.join(".cargo-lock").is_file() {
        return Installation::Utveckling;
    }
    if skrivbar(mapp) {
        return Installation::PaPlats(program.to_path_buf());
    }
    match kopia {
        Some(k) if k != program => Installation::Anvandarkopia(k),
        _ => Installation::Ingen,
    }
}

/// Programfilen som kör, med länkar utredda (`~/.local/bin/sentinant-lite` är en länk).
fn programfil() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?;
    Some(std::fs::canonicalize(&p).unwrap_or(p))
}

fn installation() -> Installation {
    match programfil() {
        Some(p) => installation_for(&p, anvandarkopia()),
        None => Installation::Ingen,
    }
}

/// Plattformens namn i beskrivningen. `None` där det inte finns något färdigt program att
/// hämta; då visas ingen knapp.
pub fn plattform() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some("linux-aarch64")
    } else {
        None
    }
}

const NY_PREFIX: &str = "sentinant-lite-ny";
const GAMMAL_PREFIX: &str = "sentinant-lite-gammal";

fn bredvid(mal: &Path, prefix: &str) -> PathBuf {
    let andelse = if cfg!(windows) { ".exe" } else { "" };
    mal.with_file_name(format!("{prefix}-{}{andelse}", std::process::id()))
}

/// Byter programfilen `mal` mot `ny`. Den gamla sparas som `gammal` tills den nya har
/// startat (på Windows går en programfil som kör inte att skriva över, men den går att
/// byta namn på). Går bytet inte ligger den gamla kvar orörd.
fn byt(ny: &Path, mal: &Path, gammal: &Path) -> std::io::Result<()> {
    let _ = std::fs::remove_file(gammal);
    if mal.exists() {
        if cfg!(windows) {
            std::fs::rename(mal, gammal)?;
        } else {
            // Unix: `rename` nedan ersätter filen i ett enda steg; en hård länk behåller den gamla.
            let _ = std::fs::hard_link(mal, gammal);
        }
    }
    std::fs::rename(ny, mal).inspect_err(|_| {
        if cfg!(windows) {
            let _ = std::fs::rename(gammal, mal);
        }
    })
}

/// Ångrar [`byt`] (när den nya versionen inte gick att starta).
fn byt_tillbaka(mal: &Path, gammal: &Path) {
    if gammal.exists() {
        let _ = std::fs::remove_file(mal);
        let _ = std::fs::rename(gammal, mal);
    }
}

/// Tar bort det en tidigare uppdatering lämnat kvar bredvid programmet. På Windows kan den
/// gamla versionen fortfarande hålla på att avslutas, så det prövas några gånger.
fn stada(mapp: PathBuf) {
    std::thread::spawn(move || {
        for _ in 0..10 {
            let kvar: Vec<PathBuf> = std::fs::read_dir(&mapp)
                .map(|l| l.flatten().map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(NY_PREFIX) || n.starts_with(GAMMAL_PREFIX))).collect())
                .unwrap_or_default();
            if kvar.iter().filter(|p| std::fs::remove_file(p).is_err()).count() == 0 {
                return;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

/// Windows: versionen som visas under "Installerade appar" (om programmet är installerat
/// med installationsfilen).
#[cfg(windows)]
fn registrera_version(v: Version) {
    const NYCKEL: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\SentinAnt Lite";
    let reg = std::env::var_os("SystemRoot").map(|r| PathBuf::from(r).join("System32").join("reg.exe")).unwrap_or_else(|| "reg.exe".into());
    let kor = |arg: &[&str]| {
        let mut k = Command::new(&reg);
        k.args(arg).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        std::os::windows::process::CommandExt::creation_flags(&mut k, 0x0800_0000);
        k.status().map(|s| s.success()).unwrap_or(false)
    };
    if kor(&["query", NYCKEL]) {
        kor(&["add", NYCKEL, "/v", "DisplayVersion", "/t", "REG_SZ", "/d", &v.to_string(), "/f"]);
    }
}

#[cfg(not(windows))]
fn registrera_version(_v: Version) {}

/// Körs först av allt i `main`, innan fönstret öppnas:
/// - `sentinant-lite --version` skriver versionen och avslutar (så provstartas en ny version).
/// - `sentinant-lite --kontrollera-uppdatering MAPP` kontrollerar en uppdateringsmapp på disken
///   precis som programmet gör vid en uppdatering (används av publiceringsskriptet).
/// - Rester efter en tidigare uppdatering städas bort.
/// - Linux-paketet i /usr/bin lämnar över till användarens nyare kopia, om den finns.
pub fn fore_start() {
    let mut arg = std::env::args().skip(1);
    match arg.next().as_deref() {
        Some("--version") => {
            // Ett provbygge (godkänner provnyckeln) uppger ingen ren version, så det kan
            // aldrig bli installerat av uppdateringen eller godkänt av publiceringsskripten.
            println!("{}{}", version::DENNA, if robotuppdatering::PROVBYGGE { " PROVBYGGE" } else { "" });
            std::process::exit(0);
        }
        Some("--kontrollera-uppdatering") => {
            let mapp = arg.next().unwrap_or_else(|| ".".into());
            if robotuppdatering::PROVBYGGE {
                println!("PROVBYGGE: godkänner också provnyckeln. Kontrollera en riktig uppdatering med ett vanligt bygge.");
            }
            std::process::exit(if kontrollera_mapp(Path::new(&mapp)) { 0 } else { 1 });
        }
        _ => {}
    }
    match installation() {
        Installation::PaPlats(p) => {
            if let Some(mapp) = p.parent() {
                stada(mapp.to_path_buf());
            }
        }
        Installation::Anvandarkopia(kopia) => lamna_over(&kopia),
        Installation::Utveckling | Installation::Ingen => {}
    }
}

/// Programmet i /usr/bin: finns en nyare version i användarens mapp startas den i stället
/// (den här processen blir den). En kopia som är äldre (paketet har uppdaterats sedan
/// dess) eller inte går att starta tas bort.
fn lamna_over(kopia: &Path) {
    if !kopia.is_file() || programfil().as_deref() == Some(kopia) {
        return;
    }
    match fraga_version(kopia, Duration::from_secs(5)) {
        Ok(v) if v > Version::denna() => {
            #[cfg(unix)]
            {
                let fel = std::os::unix::process::CommandExt::exec(Command::new(kopia).args(std::env::args_os().skip(1)));
                eprintln!("sentinant-lite: kunde inte starta {}: {fel}", kopia.display());
            }
        }
        _ => {
            let _ = std::fs::remove_file(kopia);
        }
    }
}

/// Märket som bara finns i ett provbygge (byggt med `--features provnyckel`). Det visas på
/// skärmen, och skripten som paketerar och installerar letar efter texten i programfilen
/// och vägrar när den finns.
#[cfg(feature = "provnyckel")]
fn provmarke() -> Option<&'static str> {
    Some(robotuppdatering::PROVMARKE)
}

#[cfg(not(feature = "provnyckel"))]
fn provmarke() -> Option<&'static str> {
    None
}

/// Kontrollerar en uppdateringsmapp på disken med programmets egna nycklar och regler:
/// signaturen, och för varje fil i beskrivningen namn, storlek och SHA-256. Skriver vad den
/// ser. Sant om allt stämmer.
fn kontrollera_mapp(mapp: &Path) -> bool {
    let manifest = match std::fs::read(mapp.join(MANIFESTFIL)).map_err(|e| Fel::Fil(e.to_string())).and_then(|k| oppna_kuvert(&k, &nycklar())) {
        Ok(m) => m,
        Err(e) => {
            println!("FEL: {} går inte att godkänna: {e:?}", mapp.join(MANIFESTFIL).display());
            return false;
        }
    };
    println!("Signaturen stämmer med programmets inbyggda nyckel.");
    println!("Löpnummer {}, skapad {}, giltig till {}.", manifest.lopnummer, manifest.skapad, manifest.giltig_till.as_deref().unwrap_or("(ingen gräns)"));
    if let Err(e) = granska_farskhet(&manifest, None, chrono::Utc::now()) {
        println!("FEL: {e:?}");
        return false;
    }
    let mut bra = true;
    for (komponent, paket) in &manifest.paket {
        // Styrkortets firmware har sin egen versionsform (X.Y) och en fil per kortvariant.
        let firmware = komponent == robotuppdatering::firmware::KOMPONENT;
        let version = Version::tolka(&paket.version);
        if firmware {
            if robotuppdatering::firmware::FwVersion::tolka(&paket.version).is_none() {
                println!("FEL: firmware: obegriplig version {}", paket.version);
                bra = false;
            }
            for (krav, lagst) in &paket.kraver {
                println!("     firmware kräver {krav} minst {lagst}");
            }
        } else if version.is_none() {
            println!("FEL: {komponent}: obegriplig version {}", paket.version);
            bra = false;
        }
        for (plattform, fil) in &paket.filer {
            let mut resultat = fil.kontrollerad().map_err(Fel::from).and_then(|(storlek, sha256)| kontrollera_fil(&mapp.join(&fil.fil), storlek, &sha256));
            if firmware && resultat.is_ok() {
                if robotuppdatering::firmware::Variant::tolka(plattform).is_none() {
                    resultat = Err(Fel::Manifest(format!("okänd kortvariant: {plattform}")));
                } else if let Err(t) = robotuppdatering::firmware::kontrollera_elf(&mapp.join(&fil.fil)) {
                    resultat = Err(Fel::Manifest(t));
                }
            }
            match resultat {
                Ok(()) => println!("OK   {komponent} {} {plattform}: {} ({} byte)", paket.version, fil.fil, fil.storlek),
                Err(e) => {
                    println!("FEL: {komponent} {plattform}: {}: {e:?}", fil.fil);
                    bra = false;
                }
            }
        }
        if komponent == KOMPONENT {
            match version {
                Some(v) if v == Version::denna() => println!("     {KOMPONENT}: samma version som det här programmet."),
                Some(v) if v > Version::denna() => println!("     {KOMPONENT}: nyare än det här programmet ({}).", version::DENNA),
                Some(_) => println!("     OBS: {KOMPONENT} i mappen är ÄLDRE än det här programmet ({}); ingen dator med den här versionen tar emot den.", version::DENNA),
                None => {}
            }
        }
    }
    bra
}

// ---------------------------------------------------------------------------------------
// Knappen på anslutningsskärmen
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Lage {
    Soker,
    /// Kontrollerat: ingen nyare version finns.
    Aktuell(Farskhet),
    /// Kunde inte kontrollera.
    Onabar(Fel),
    NyFinns(Erbjudande),
    Hamtar(Erbjudande),
    /// Hämtad, kontrollerad och provstartad: nästa bildruta byts programmet och startas om.
    Klar(Erbjudande),
    Misslyckad(Erbjudande, Fel),
}

enum Order {
    Sok,
    /// Hämta erbjudandet; numret är omgången (se `Delat::omgang`).
    Hamta(Erbjudande, u64),
}

struct Delat {
    lage: Mutex<Lage>,
    hamtat: AtomicU64,
    /// Räknas upp vid varje ny hämtning och varje avbrott. En hämtning som inte längre är
    /// den senaste omgången avbryter sig själv och får aldrig sätta läget.
    omgang: AtomicU64,
}

/// Uppdateringsknappen och det som pågår bakom den.
pub struct Uppdatering {
    installation: Installation,
    delat: Arc<Delat>,
    /// Finns bara där programmet kan uppdatera sig självt.
    order: Option<mpsc::Sender<Order>>,
    senast_sokt: Instant,
}

impl Uppdatering {
    /// Tar reda på hur installationen uppdateras och söker efter en ny version i bakgrunden.
    pub fn starta() -> Self {
        // Där det inte finns något färdigt program att hämta undersöks ingenting.
        let installation = if plattform().is_some() { installation() } else { Installation::Ingen };
        let delat = Arc::new(Delat { lage: Mutex::new(Lage::Soker), hamtat: AtomicU64::new(0), omgang: AtomicU64::new(0) });
        let mal = match &installation {
            Installation::PaPlats(p) | Installation::Anvandarkopia(p) => Some(p.clone()),
            Installation::Utveckling | Installation::Ingen => None,
        };
        let order = match (plattform(), mal) {
            (Some(plattform), Some(mal)) => {
                let (tx, rx) = mpsc::channel();
                let delat = delat.clone();
                let kalla = kalla();
                std::thread::spawn(move || arbetare(rx, &delat, &kalla, plattform, &mal));
                let _ = tx.send(Order::Sok);
                Some(tx)
            }
            _ => None,
        };
        Self { installation, delat, order, senast_sokt: Instant::now() }
    }

    /// Ska raden med versionen och knappen visas alls?
    pub fn visas(&self) -> bool {
        plattform().is_some()
    }

    fn sok(&mut self) {
        if let Some(tx) = &self.order {
            let mut lage = self.delat.lage.lock().unwrap();
            // En redan känd ny version står kvar (grön knapp) medan det söks på nytt.
            if !matches!(*lage, Lage::NyFinns(_)) {
                *lage = Lage::Soker;
            }
            self.senast_sokt = Instant::now();
            let _ = tx.send(Order::Sok);
        }
    }

    fn hamta(&mut self, e: Erbjudande) {
        if let Some(tx) = &self.order {
            let mut lage = self.delat.lage.lock().unwrap();
            let omgang = self.delat.omgang.fetch_add(1, Ordering::SeqCst) + 1;
            self.delat.hamtat.store(0, Ordering::Relaxed);
            *lage = Lage::Hamtar(e.clone());
            let _ = tx.send(Order::Hamta(e, omgang));
        }
    }

    /// Föraren lämnar anslutningsskärmen (ansluter till en robot): en pågående hämtning
    /// avbryts, och programmet startas aldrig om medan en robot är ansluten.
    pub fn avbryt(&mut self) {
        let mut lage = self.delat.lage.lock().unwrap();
        if let Lage::Hamtar(e) | Lage::Klar(e) = &*lage {
            self.delat.omgang.fetch_add(1, Ordering::SeqCst);
            *lage = Lage::NyFinns(e.clone());
        }
    }

    /// Byter programmet mot den hämtade versionen och startar om. Kommer bara tillbaka om
    /// det inte gick.
    fn byt_och_starta_om(&self, e: &Erbjudande) -> Fel {
        let (Installation::PaPlats(mal) | Installation::Anvandarkopia(mal)) = &self.installation else {
            return Fel::Fil("programmet kan inte uppdateras här".into());
        };
        let (ny, gammal) = (bredvid(mal, NY_PREFIX), bredvid(mal, GAMMAL_PREFIX));
        // Aldrig bakåt, och filen på disken ska fortfarande vara den som kontrollerades.
        if e.version <= Version::denna() {
            return Fel::Manifest("versionen är inte nyare än den som kör".into());
        }
        if let Err(fel) = kontrollera_fil(&ny, e.storlek, &e.sha256) {
            return fel;
        }
        if let Err(fel) = byt(&ny, mal, &gammal) {
            return Fel::Fil(fel.to_string());
        }
        match Command::new(mal).args(std::env::args_os().skip(1)).spawn() {
            Ok(_) => {
                registrera_version(e.version);
                std::process::exit(0);
            }
            Err(fel) => {
                byt_tillbaka(mal, &gammal);
                Fel::Provstart(fel.to_string())
            }
        }
    }

    /// Ritar versionen och knappen (längst ned på anslutningsskärmen).
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let gra = egui::Color32::GRAY;
        let varning = egui::Color32::from_rgb(255, 170, 90);
        let denna = Version::denna().visning();
        let liten = |t: String, farg: egui::Color32| egui::RichText::new(t).size(12.0).color(farg);
        if let Some(marke) = provmarke() {
            ui.label(egui::RichText::new(format!("PROVBYGGE: godkänner provsignerade uppdateringar och får inte delas ut ({marke})")).size(13.0).strong().color(egui::Color32::from_rgb(235, 60, 60)));
        }
        if self.order.is_none() {
            let text = match self.installation {
                Installation::Utveckling => format!("Utvecklingsversion {denna} (byggd från källkod, uppdateras inte härifrån)"),
                _ => format!("Version {denna} (kan inte uppdateras härifrån: installera med installationsfilen)"),
            };
            ui.label(liten(text, gra));
            return;
        }
        let lage = self.delat.lage.lock().unwrap().clone();
        // Ny kontroll av sig själv med jämna mellanrum, bara här på anslutningsskärmen.
        let vantan = match &lage {
            Lage::Onabar(_) => Some(VANTAN_EFTER_FEL),
            Lage::Aktuell(_) | Lage::NyFinns(_) => Some(VANTAN_EFTER_SVAR),
            _ => None,
        };
        if vantan.is_some_and(|v| self.senast_sokt.elapsed() >= v) {
            self.sok();
        }
        let knapp = egui::vec2(240.0, 34.0);
        let gron = |text: &str| egui::Button::new(egui::RichText::new(text).size(17.0).strong().color(egui::Color32::WHITE)).fill(egui::Color32::from_rgb(30, 150, 60)).min_size(knapp);
        match lage {
            Lage::Soker => {
                ui.add_enabled(false, egui::Button::new("Söker uppdatering…").min_size(knapp));
                ui.label(liten(format!("Version {denna}"), gra));
            }
            Lage::Aktuell(farskhet) => {
                if ui.add(egui::Button::new("Sök uppdatering").min_size(knapp)).clicked() {
                    self.sok();
                }
                ui.label(liten(format!("Version {denna} – du har den senaste versionen"), gra));
                if let Some(till) = &farskhet.utgangen {
                    // Listan borde ha förnyats: det kan finnas en nyare version som inte når hit.
                    let text = format!("Serverns uppdateringslista är gammal (skulle ha förnyats senast {till}). Kontrollera datorns klocka, och hör av dig till oss om det står kvar.");
                    ui.label(liten(text, varning));
                } else if !farskhet.skapad.is_empty() {
                    ui.label(liten(format!("Serverns uppdateringslista är från {}", farskhet.skapad), gra));
                }
            }
            Lage::Onabar(fel) => {
                if ui.add(egui::Button::new("Sök uppdatering").min_size(knapp)).clicked() {
                    self.sok();
                }
                ui.label(liten(format!("Version {denna}"), gra));
                ui.label(liten(fel.to_string(), if fel.varning() { varning } else { gra }));
            }
            Lage::NyFinns(e) => {
                if ui.add(gron("Uppdatera nu")).on_hover_text("Hämtar den nya versionen, installerar den och startar om programmet").clicked() {
                    self.hamta(e.clone());
                }
                ui.label(liten(format!("Ny version finns: {} (du har {denna})", e.version.visning()), egui::Color32::from_rgb(120, 220, 140)));
                if !e.nyheter.is_empty() {
                    ui.label(liten(e.nyheter.clone(), gra));
                }
            }
            Lage::Hamtar(e) => {
                let andel = self.delat.hamtat.load(Ordering::Relaxed) as f32 / e.storlek.max(1) as f32;
                let text = if andel >= 1.0 { "Kontrollerar den nya versionen…".to_string() } else { format!("Hämtar den nya versionen… {:.0} %", andel * 100.0) };
                ui.add(egui::ProgressBar::new(andel.min(1.0)).desired_width(320.0).text(text));
                ui.label(liten(format!("Version {} – programmet startar om när det är klart", e.version.visning()), gra));
            }
            Lage::Klar(e) => {
                let fel = self.byt_och_starta_om(&e);
                *self.delat.lage.lock().unwrap() = Lage::Misslyckad(e, fel);
            }
            Lage::Misslyckad(e, fel) => {
                if ui.add(gron("Försök igen")).clicked() {
                    self.hamta(e.clone());
                }
                ui.label(liten(fel.to_string(), varning));
                ui.label(liten(format!("Version {denna}"), gra));
            }
        }
    }
}

/// Bakgrundstråden: söker och hämtar när knappen ber om det.
fn arbetare(rx: mpsc::Receiver<Order>, delat: &Delat, kalla: &str, plattform: &str, mal: &Path) {
    while let Ok(order) = rx.recv() {
        match order {
            Order::Sok => {
                let svar = match sok(kalla, plattform) {
                    Ok((Some(e), _)) => Lage::NyFinns(e),
                    Ok((None, farskhet)) => Lage::Aktuell(farskhet),
                    Err(fel) => Lage::Onabar(fel),
                };
                let mut lage = delat.lage.lock().unwrap();
                // En hämtning som startats under tiden går före. En känd ny version byts
                // bara mot ett nytt besked från servern, inte mot "servern svarar inte".
                let byt = match &*lage {
                    Lage::Soker => true,
                    Lage::NyFinns(_) => !matches!(svar, Lage::Onabar(_)),
                    _ => false,
                };
                if byt {
                    *lage = svar;
                }
            }
            Order::Hamta(e, omgang) => {
                let fortsatt = || delat.omgang.load(Ordering::SeqCst) == omgang;
                let ny = bredvid(mal, NY_PREFIX);
                let resultat = (|| {
                    if let Some(mapp) = ny.parent() {
                        std::fs::create_dir_all(mapp).map_err(|e| Fel::Fil(e.to_string()))?;
                    }
                    hamta_fil(&format!("{kalla}{}", e.fil), &ny, e.storlek, &delat.hamtat, &fortsatt)?;
                    kontrollera_fil(&ny, e.storlek, &e.sha256)?;
                    provstarta(&ny, e.version)
                })();
                if resultat.is_err() {
                    let _ = std::fs::remove_file(&ny);
                }
                let mut lage = delat.lage.lock().unwrap();
                if fortsatt() {
                    *lage = match resultat {
                        Ok(()) => Lage::Klar(e),
                        Err(fel) => Lage::Misslyckad(e, fel),
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tester {
    use super::*;
    use ring::signature::KeyPair;
    use robotuppdatering::beskrivning::{giltigt_filnamn, hex_till_bytes, las_hogsta_lopnummer};

    /// Provnyckel (inte den riktiga): nyckelpar ur ett fast frö.
    fn provnyckel(fro: u8) -> (ring::signature::Ed25519KeyPair, [u8; 32]) {
        let par = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[fro; 32]).unwrap();
        let publik: [u8; 32] = par.public_key().as_ref().try_into().unwrap();
        (par, publik)
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn kuvert(manifest: &str, signaturer: &[String]) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "manifest": manifest, "signaturer": signaturer })).unwrap()
    }

    fn signerat(manifest: &str, par: &ring::signature::Ed25519KeyPair) -> Vec<u8> {
        kuvert(manifest, &[hex(par.sign(manifest.as_bytes()).as_ref())])
    }

    const SHA: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"; // sha256("test")

    fn manifest(version: &str) -> String {
        format!(
            r#"{{"format":1,"lopnummer":20,"skapad":"2026-11-26T14:27:00Z","paket":{{
              "sentinant-lite":{{"version":"{version}","nyheter":" Nya knappar ","filer":{{
                "linux-x86_64":{{"fil":"sentinant-lite-linux","storlek":4,"sha256":"{SHA}"}},
                "windows-x86_64":{{"fil":"sentinant-lite-windows.exe","storlek":5,"sha256":"{SHA}","okant_falt":1}}}}}},
              "robotd":{{"version":"9.9.9","filer":{{"linux-aarch64":{{"fil":"robotd","storlek":7,"sha256":"{SHA}"}}}}}}}}}}"#
        )
    }

    fn v(s: &str) -> Version {
        Version::tolka(s).unwrap()
    }

    fn provmapp(namn: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sentinant-lite-uppdateringsprov-{}-{namn}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn inbyggda_nycklar_ar_riktiga_nycklar() {
        assert_eq!(nycklar().len(), PUBLIKA_NYCKLAR.len() + usize::from(robotuppdatering::PROVBYGGE));
        assert!(!nycklar().is_empty());
    }

    #[test]
    fn signerad_beskrivning_godkanns() {
        let (par, publik) = provnyckel(1);
        let m = oppna_kuvert(&signerat(&manifest("0.1.0+202611270000"), &par), &[publik]).unwrap();
        let e = m.erbjudande(KOMPONENT, "windows-x86_64", v("0.1.0+202611261256")).unwrap().unwrap();
        assert_eq!((e.version, e.fil.as_str(), e.storlek, e.nyheter.as_str()), (v("0.1.0+202611270000"), "sentinant-lite-windows.exe", 5, "Nya knappar"));
        assert_eq!(hex(&e.sha256), SHA);
        // Andra komponenter ligger i samma beskrivning utan att störa.
        assert_eq!(m.erbjudande("robotd", "linux-aarch64", v("1.0.0")).unwrap().unwrap().fil, "robotd");
    }

    #[test]
    fn signatur_gjord_med_openssl_godkanns() {
        // openssl genpkey -algorithm ed25519; openssl pkeyutl -sign -rawin (så signerar publiceringsskriptet).
        let publik: [u8; 32] = hex_till_bytes("01b7e7d573708bbc2fdfc262d5df772f6efe4d325ae722d2287e5898b7820a87").unwrap().try_into().unwrap();
        let signatur = "347752cfa79246337bc4d55637bd09f3a359819fa53972e75d447f3a038c7e3a8e207017ecd400edc1154e5451301ed2653c3d70cd52cccb1c30bb82cec3d007";
        let m = oppna_kuvert(&kuvert(r#"{"format":1,"lopnummer":7,"paket":{}}"#, &[signatur.to_string()]), &[publik]).unwrap();
        assert!(m.paket.is_empty() && m.lopnummer == 7);
        assert_eq!(oppna_kuvert(&kuvert(r#"{"format":1,"lopnummer":7,"paket":{} }"#, &[signatur.to_string()]), &[publik]), Err(Fel::Signatur));
    }

    #[test]
    fn manipulerad_beskrivning_avvisas() {
        let (par, publik) = provnyckel(1);
        let (annan, annan_publik) = provnyckel(2);
        let text = manifest("0.1.0+202611270000");
        let signatur = hex(par.sign(text.as_bytes()).as_ref());
        let ratt = std::slice::from_ref(&signatur);
        // Ett enda ändrat tecken i texten (annan kontrollsumma, annan version).
        assert_eq!(oppna_kuvert(&kuvert(&text.replace("9f86", "9f87"), ratt), &[publik]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&kuvert(&text.replace("202611270000", "202611270001"), ratt), &[publik]), Err(Fel::Signatur));
        // Ändrad signatur, ingen signatur, skräp, fel längd.
        let mut andrad = signatur.clone().into_bytes();
        andrad[0] = if andrad[0] == b'0' { b'1' } else { b'0' };
        assert_eq!(oppna_kuvert(&kuvert(&text, &[String::from_utf8(andrad).unwrap()]), &[publik]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&kuvert(&text, &[]), &[publik]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&kuvert(&text, &["zz".into(), String::new(), signatur[..126].to_string()]), &[publik]), Err(Fel::Signatur));
        // Rätt signerad, men med en nyckel programmet inte litar på.
        assert_eq!(oppna_kuvert(&signerat(&text, &annan), &[publik]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&kuvert(&text, ratt), &[annan_publik]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&kuvert(&text, ratt), &[]), Err(Fel::Signatur));
        // Inget kuvert alls, eller manifestet direkt utan kuvert.
        assert_eq!(oppna_kuvert(b"<html>404</html>", &[publik]), Err(Fel::Kuvert));
        assert_eq!(oppna_kuvert(text.as_bytes(), &[publik]), Err(Fel::Kuvert));
        // Oförändrat godkänns (motprov).
        assert!(oppna_kuvert(&kuvert(&text, ratt), &[publik]).is_ok());
    }

    #[test]
    fn nyckelbyte_en_av_flera_signaturer_racker() {
        let (gammal, gammal_publik) = provnyckel(1);
        let (ny, ny_publik) = provnyckel(2);
        let text = manifest("0.1.0+202611270000");
        let bada = kuvert(&text, &[hex(ny.sign(text.as_bytes()).as_ref()), hex(gammal.sign(text.as_bytes()).as_ref())]);
        // Ett program med bara den gamla nyckeln, ett med bara den nya, ett med båda.
        assert!(oppna_kuvert(&bada, &[gammal_publik]).is_ok());
        assert!(oppna_kuvert(&bada, &[ny_publik]).is_ok());
        assert!(oppna_kuvert(&bada, &[ny_publik, gammal_publik]).is_ok());
    }

    #[test]
    fn nedgradering_och_samma_version_ar_inget_erbjudande() {
        let (par, publik) = provnyckel(1);
        let m = oppna_kuvert(&signerat(&manifest("0.1.0+202611261256"), &par), &[publik]).unwrap();
        let fragar = |har: &str| m.erbjudande(KOMPONENT, "linux-x86_64", v(har)).unwrap();
        assert!(fragar("0.1.0+202611261255").is_some()); // nyare än min: ja
        assert_eq!(fragar("0.1.0+202611261256"), None); // samma
        assert_eq!(fragar("0.1.0+202611261257"), None); // äldre än min: aldrig bakåt
        assert_eq!(fragar("0.2.0"), None);
        assert_eq!(fragar("1.0.0+1"), None);
        // Ingen post för komponenten eller plattformen: ingenting att hämta, inget fel.
        assert_eq!(m.erbjudande(KOMPONENT, "android-arm64", v("0.0.1")).unwrap(), None);
        assert_eq!(m.erbjudande("klocka", "linux-x86_64", v("0.0.1")).unwrap(), None);
    }

    /// Listan är gemensam med SentinAnt och styrkortets firmware: Lite tar bara sin egen post.
    #[test]
    fn andra_programs_poster_i_samma_lista_ror_inte_lite() {
        let (par, publik) = provnyckel(1);
        let andra = format!(
            r#""sentinant":{{"version":"9.0.0+209901010000","filer":{{"linux-x86_64":{{"fil":"sentinant-9-linux","storlek":4,"sha256":"{SHA}"}}}}}},
              "firmware":{{"version":"30.33","kraver":{{"car_client":"30.33"}},"filer":{{"mp101-robant":{{"fil":"firmware-30.33-mp101-robant.elf","storlek":4,"sha256":"{SHA}"}}}}}}"#
        );
        // Bara de andras poster: ingenting för Lite, hur ny SentinAnt än är.
        let utan = format!(r#"{{"format":1,"lopnummer":21,"paket":{{{andra}}}}}"#);
        let m = oppna_kuvert(&signerat(&utan, &par), &[publik]).unwrap();
        assert_eq!(m.erbjudande(KOMPONENT, "linux-x86_64", v("0.1.0")).unwrap(), None);
        // Alla tre: Lite får sin egen fil och version, inte SentinAnts.
        let med = format!(
            r#"{{"format":1,"lopnummer":22,"paket":{{{andra},
              "sentinant-lite":{{"version":"0.1.0+202611270900","filer":{{"linux-x86_64":{{"fil":"sentinant-lite-linux","storlek":4,"sha256":"{SHA}"}}}}}}}}}}"#
        );
        let m = oppna_kuvert(&signerat(&med, &par), &[publik]).unwrap();
        let e = m.erbjudande(KOMPONENT, "linux-x86_64", v("0.1.0+202611270859")).unwrap().unwrap();
        assert_eq!((e.version, e.fil.as_str()), (v("0.1.0+202611270900"), "sentinant-lite-linux"));
        assert_eq!(m.erbjudande(KOMPONENT, "linux-x86_64", v("0.1.0+202611270900")).unwrap(), None);
    }

    #[test]
    fn felaktigt_innehall_avvisas_fast_signaturen_stammer() {
        let (par, publik) = provnyckel(1);
        let bra = manifest("0.1.0+202611270000");
        let oppna = |text: &str| oppna_kuvert(&signerat(text, &par), &[publik]);
        let erbjud = |text: &str| oppna(text).and_then(|m| m.erbjudande(KOMPONENT, "linux-x86_64", v("0.1.0")));
        assert!(erbjud(&bra).unwrap().is_some());
        // Filnamn som pekar ut ur mappen eller till en annan adress.
        for namn in ["../sentinant-lite", "a/b", "http://ond.example/x", ".dold", "", "med mellanslag", "c:\\\\x.exe", "x?y=1"] {
            let fel = erbjud(&bra.replace("sentinant-lite-linux", namn));
            assert!(matches!(fel, Err(Fel::Manifest(_))), "{namn:?}: {fel:?}");
        }
        assert!(matches!(erbjud(&bra.replacen(SHA, "abc", 1)), Err(Fel::Manifest(_))));
        assert!(matches!(erbjud(&bra.replace("\"storlek\":4", "\"storlek\":0")), Err(Fel::Manifest(_))));
        assert!(matches!(erbjud(&bra.replace("\"storlek\":4", "\"storlek\":99999999999")), Err(Fel::Manifest(_))));
        assert!(matches!(erbjud(&bra.replace("0.1.0+202611270000", "senaste")), Err(Fel::Manifest(_))));
        // En nyare form på beskrivningen än programmet förstår, och ingen form alls.
        assert_eq!(oppna(&bra.replace("\"format\":1", "\"format\":2")), Err(Fel::Format(2)));
        assert!(matches!(oppna("{}"), Err(Fel::Manifest(_))));
        // Utan löpnummer finns ingen färskhet att kontrollera: avvisas.
        assert!(matches!(oppna(&bra.replace("\"lopnummer\":20,", "")), Err(Fel::Manifest(_))));
        assert!(matches!(oppna("inte json"), Err(Fel::Manifest(_))));
    }

    #[test]
    fn aldre_beskrivning_an_den_redan_sedda_avvisas() {
        let (par, publik) = provnyckel(1);
        let d = provmapp("farskhet");
        let minne = d.join("installningar").join("uppdatering.json");
        let nu = chrono::DateTime::parse_from_rfc3339("2026-12-01T12:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let med_nummer = |n: u64| oppna_kuvert(&signerat(&manifest("0.1.0+202611270000").replace("\"lopnummer\":20", &format!("\"lopnummer\":{n}")), &par), &[publik]).unwrap();
        let granska = |n: u64| granska_farskhet(&med_nummer(n), Some(&minne), nu);
        // Första gången finns inget minne: allt tas emot, och numret sparas.
        assert_eq!(granska(20), Ok(Farskhet { skapad: "2026-11-26".into(), utgangen: None }));
        assert_eq!(las_hogsta_lopnummer(&minne), 20);
        // En äldre, äkta signerad beskrivning visas igen (återspelning): avvisas, minnet står kvar.
        assert_eq!(granska(19), Err(Fel::Aterspelad { sett: 20, fick: 19 }));
        assert_eq!(granska(0), Err(Fel::Aterspelad { sett: 20, fick: 0 }));
        assert_eq!(las_hogsta_lopnummer(&minne), 20);
        // Samma igen och nyare tas emot; minnet följer med uppåt.
        assert!(granska(20).is_ok());
        assert!(granska(21).is_ok());
        assert_eq!(las_hogsta_lopnummer(&minne), 21);
        assert_eq!(granska(20), Err(Fel::Aterspelad { sett: 21, fick: 20 }));
        // Ett trasigt minne räknas som inget minne (hellre det än att uppdateringen låser sig).
        std::fs::write(&minne, b"skrap").unwrap();
        assert!(granska(5).is_ok());
        assert_eq!(las_hogsta_lopnummer(&minne), 5);
        // Utan minne (publiceringsskriptets kontroll) sparas ingenting.
        assert!(granska_farskhet(&med_nummer(99), None, nu).is_ok());
        assert_eq!(las_hogsta_lopnummer(&minne), 5);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn utgangen_beskrivning_varnar_men_stoppar_inte() {
        let (par, publik) = provnyckel(1);
        let nu = chrono::DateTime::parse_from_rfc3339("2026-12-01T12:00:00Z").unwrap().with_timezone(&chrono::Utc);
        let med = |till: &str| oppna_kuvert(&signerat(&manifest("0.1.0+202611270000").replace("\"paket\"", &format!("\"giltig_till\":\"{till}\",\"paket\"")), &par), &[publik]).unwrap();
        // Inte passerad: ingen varning. Ingen gräns alls: ingen varning.
        assert_eq!(granska_farskhet(&med("2026-12-31T00:00:00Z"), None, nu).unwrap().utgangen, None);
        assert_eq!(granska_farskhet(&oppna_kuvert(&signerat(&manifest("0.1.0+202611270000"), &par), &[publik]).unwrap(), None, nu).unwrap().utgangen, None);
        // Passerad: varning med datumet, men erbjudandet i beskrivningen gäller fortfarande.
        let gammal = med("2026-11-30T00:00:00+01:00");
        assert_eq!(granska_farskhet(&gammal, None, nu).unwrap().utgangen.as_deref(), Some("2026-11-30"));
        assert!(gammal.erbjudande(KOMPONENT, "linux-x86_64", v("0.1.0")).unwrap().is_some());
        // Obegripligt datum är ett fel i beskrivningen.
        assert!(matches!(granska_farskhet(&med("snart"), None, nu), Err(Fel::Manifest(_))));
    }

    #[test]
    fn manipulerad_fil_avvisas() {
        let d = provmapp("fil");
        let fil = d.join("ny");
        let sha: [u8; 32] = hex_till_bytes(SHA).unwrap().try_into().unwrap();
        std::fs::write(&fil, b"test").unwrap();
        assert_eq!(kontrollera_fil(&fil, 4, &sha), Ok(()));
        std::fs::write(&fil, b"tesT").unwrap(); // samma storlek, ett tecken ändrat
        assert_eq!(kontrollera_fil(&fil, 4, &sha), Err(Fel::Kontrollsumma));
        std::fs::write(&fil, b"tes").unwrap(); // avkortad
        assert_eq!(kontrollera_fil(&fil, 4, &sha), Err(Fel::Storlek));
        std::fs::write(&fil, b"test+tillagg").unwrap(); // något tillagt efter
        assert_eq!(kontrollera_fil(&fil, 4, &sha), Err(Fel::Storlek));
        assert!(matches!(kontrollera_fil(&d.join("saknas"), 4, &sha), Err(Fel::Fil(_))));
        let _ = std::fs::remove_dir_all(d);
    }

    /// En liten HTTP-server som svarar på en enda fråga med `innehall`.
    fn server(innehall: Vec<u8>) -> String {
        let lyssnare = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let adress = lyssnare.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = lyssnare.accept() {
                let mut b = [0u8; 2048];
                let _ = s.read(&mut b);
                let _ = s.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", innehall.len()).as_bytes());
                let _ = s.write_all(&innehall);
            }
        });
        format!("http://{adress}/fil")
    }

    #[test]
    fn hamtning_tar_aldrig_emot_mer_an_utlovat() {
        let d = provmapp("hamta");
        let fil = d.join("ny");
        let hamtat = AtomicU64::new(0);
        let innehall = vec![7u8; 200_000];
        assert_eq!(hamta_fil(&server(innehall.clone()), &fil, 200_000, &hamtat, &|| true), Ok(()));
        assert_eq!(std::fs::read(&fil).unwrap(), innehall);
        assert_eq!(hamtat.load(Ordering::Relaxed), 200_000);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&fil).unwrap().permissions()) & 0o777, 0o755);
        // Servern skickar mer än beskrivningen säger: avbryts.
        assert_eq!(hamta_fil(&server(innehall.clone()), &fil, 150_000, &hamtat, &|| true), Err(Fel::Storlek));
        // Föraren lämnar anslutningsskärmen.
        assert_eq!(hamta_fil(&server(innehall), &fil, 200_000, &hamtat, &|| false), Err(Fel::Avbruten));
        // Ingen server alls.
        let ledig = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert!(matches!(hamta_fil(&format!("http://{ledig}/fil"), &fil, 1, &hamtat, &|| true), Err(Fel::Nat(_))));
        let _ = std::fs::remove_dir_all(d);
    }

    #[cfg(unix)]
    fn skript(fil: &Path, innehall: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(fil, format!("#!/bin/sh\n{innehall}\n")).unwrap();
        std::fs::set_permissions(fil, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn provstarten_kraver_ratt_version() {
        let d = provmapp("provstart");
        let fil = d.join("program");
        skript(&fil, "[ \"$1\" = --version ] && echo 0.1.0+202611270000");
        assert_eq!(provstarta(&fil, v("0.1.0+202611270000")), Ok(()));
        // Filen är en äldre version än beskrivningen lovar (fel fil publicerad).
        assert!(matches!(provstarta(&fil, v("0.1.0+202611280000")), Err(Fel::Provstart(_))));
        skript(&fil, "echo hej");
        assert!(matches!(provstarta(&fil, v("0.1.0+202611270000")), Err(Fel::Provstart(_))));
        skript(&fil, "exit 3");
        assert!(matches!(provstarta(&fil, v("0.1.0+202611270000")), Err(Fel::Provstart(_))));
        std::fs::write(&fil, b"inte ett program").unwrap();
        assert!(matches!(provstarta(&fil, v("0.1.0+202611270000")), Err(Fel::Provstart(_))));
        // Hänger sig: avbryts efter väntetiden.
        skript(&fil, "sleep 30");
        assert_eq!(fraga_version(&fil, Duration::from_millis(300)), Err("den svarade inte".to_string()));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn bytet_ersatter_programmet_och_gar_att_angra() {
        let d = provmapp("byt");
        let (mal, ny, gammal) = (d.join("sentinant-lite"), d.join("sentinant-lite-ny-1"), d.join("sentinant-lite-gammal-1"));
        std::fs::write(&mal, b"gammal version").unwrap();
        std::fs::write(&ny, b"ny version").unwrap();
        byt(&ny, &mal, &gammal).unwrap();
        assert_eq!(std::fs::read(&mal).unwrap(), b"ny version");
        assert_eq!(std::fs::read(&gammal).unwrap(), b"gammal version");
        assert!(!ny.exists());
        byt_tillbaka(&mal, &gammal);
        assert_eq!(std::fs::read(&mal).unwrap(), b"gammal version");
        assert!(!gammal.exists());
        // Första gången finns inget program på platsen (användarens kopia på Linux).
        let (mal2, ny2) = (d.join("kopia"), d.join("kopia-ny"));
        std::fs::write(&ny2, b"ny version").unwrap();
        byt(&ny2, &mal2, &d.join("kopia-gammal")).unwrap();
        assert_eq!(std::fs::read(&mal2).unwrap(), b"ny version");
        // Saknas den nya filen ligger den gamla kvar orörd.
        assert!(byt(&d.join("finns-inte"), &mal, &gammal).is_err());
        assert_eq!(std::fs::read(&mal).unwrap(), b"gammal version");
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn installationens_slag() {
        let d = provmapp("installation");
        // Cargos byggmapp: utvecklingsversion, uppdateras aldrig.
        let bygg = d.join("target").join("release");
        std::fs::create_dir_all(bygg.join(".fingerprint")).unwrap();
        assert_eq!(installation_for(&bygg.join("sentinant-lite"), Some(d.join("kopia"))), Installation::Utveckling);
        // Egen skrivbar mapp: byts på plats.
        let egen = d.join("program");
        std::fs::create_dir_all(&egen).unwrap();
        assert_eq!(installation_for(&egen.join("sentinant-lite"), Some(d.join("kopia"))), Installation::PaPlats(egen.join("sentinant-lite")));
        // Skrivskyddad mapp (som /usr/bin): användarens kopia, eller ingen uppdatering alls.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let skyddad = d.join("usr-bin");
            std::fs::create_dir_all(&skyddad).unwrap();
            std::fs::set_permissions(&skyddad, std::fs::Permissions::from_mode(0o555)).unwrap();
            assert_eq!(installation_for(&skyddad.join("sentinant-lite"), Some(d.join("kopia"))), Installation::Anvandarkopia(d.join("kopia")));
            assert_eq!(installation_for(&skyddad.join("sentinant-lite"), None), Installation::Ingen);
            assert_eq!(installation_for(&skyddad.join("sentinant-lite"), Some(skyddad.join("sentinant-lite"))), Installation::Ingen);
            std::fs::set_permissions(&skyddad, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn kallans_adress_far_snedstreck() {
        assert!(STANDARDKALLA.ends_with('/'));
        assert!(giltigt_filnamn("sentinant-lite-0.1.0-202611261256-windows-x86_64.exe"));
    }
}
