//! Skrivningen till styrkortet: openocd med ST-Link (det äldre kortet) eller via CM5:ans
//! egna ben (MP101), eller ett utbytbart kommando (simulatorn och prov).
//!
//! Tre saker görs med kortet: hela flashminnet läses ut till en fil (säkerhetskopian),
//! ELF-filen skrivs (bara programmets sektorer, så kortets sparade inställningar i
//! EEPROM-emuleringen på 0x08004000–0x0800BFFF blir kvar), och vid fel skrivs
//! säkerhetskopian tillbaka (hela minnet, alltså också inställningarna som de var).
//!
//! openocd-anropen är desamma som i robot-control:s `flash_styrkort.sh` (ST-Link:
//! `board/stm32f4discovery.cfg`; MP101: linuxgpiod med SWCLK GPIO11, SWDIO GPIO8,
//! NRST GPIO17 och BOOT0 GPIO16 hållen låg).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::firmware::Kort;

/// Flashminnets början på STM32F4.
const FLASHSTART: u32 = 0x0800_0000;
/// Längsta tid för ett steg mot kortet (utläsning via CM5:ans ben är långsam).
const LANGST: Duration = Duration::from_secs(15 * 60);

/// ST-Link på USB: STMicroelectronics (0483) och adapterns produktnummer.
const STLINK_TILLVERKARE: &str = "0483";
const STLINK_PRODUKTER: &[&str] = &["3744", "3748", "374a", "374b", "374d", "374e", "374f", "3752", "3753", "3754"];

#[derive(Debug, Clone)]
pub enum Flashare {
    /// openocd på roboten. `rot` är filsystemets rot ("/"; annat bara i tester) och
    /// `arbetsmapp` där openocd:s inställningsfil och logg läggs.
    Openocd { kort: Kort, rot: PathBuf, arbetsmapp: PathBuf },
    /// Ett eget kommando: `KOMMANDO dumpa FIL`, `KOMMANDO skriv ELF`, `KOMMANDO aterstall FIL`
    /// och `KOMMANDO starta` (starta kortet efter en utläsning som gått fel).
    /// Slutkod 0 = gick bra. Används av simulatorn och vid prov.
    Kommando { program: PathBuf, arbetsmapp: PathBuf },
}

fn finns_i_path(namn: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(namn).is_file()))
}

/// Sitter en ST-Link på USB?
pub fn stlink_finns(rot: &Path) -> bool {
    let Ok(enheter) = std::fs::read_dir(rot.join("sys/bus/usb/devices")) else { return false };
    enheter.flatten().any(|e| {
        let las = |f: &str| std::fs::read_to_string(e.path().join(f)).unwrap_or_default().trim().to_ascii_lowercase();
        las("idVendor") == STLINK_TILLVERKARE && STLINK_PRODUKTER.contains(&las("idProduct").as_str())
    })
}

/// Är den här datorn en CM5 (som sitter ihop med MP101)?
pub fn ar_cm5(rot: &Path) -> bool {
    std::fs::read(rot.join("proc/device-tree/model")).is_ok_and(|b| String::from_utf8_lossy(&b).contains("Compute Module 5"))
}

/// Kör robotd som root? openocd, gpioset och pinctrl behöver det (flashskripten kör dem
/// med sudo). `status` är innehållet i /proc/self/status.
fn ar_root(status: &str) -> bool {
    status.lines().find_map(|r| r.strip_prefix("Uid:")).and_then(|r| r.split_whitespace().nth(1)).is_some_and(|euid| euid == "0")
}

/// Kör ett kommando med all utskrift till `logg`. `Err` har de sista raderna ur loggen.
fn kor(kommando: &mut Command, logg: &Path, langst: Duration) -> Result<(), String> {
    let ut = std::fs::OpenOptions::new().create(true).append(true).open(logg).map_err(|e| format!("loggen {} går inte att skriva: {e}", logg.display()))?;
    // Loggen fylls på av varje körning: felet ska bara citera det den här körningen skrev.
    let fore = ut.metadata().map_or(0, |m| m.len()) as usize;
    let fel = ut.try_clone().map_err(|e| e.to_string())?;
    let namn = kommando.get_program().to_string_lossy().to_string();
    let mut barn = kommando.stdin(Stdio::null()).stdout(ut).stderr(fel).spawn().map_err(|e| format!("{namn} gick inte att starta: {e}"))?;
    let start = Instant::now();
    let kod = loop {
        match barn.try_wait() {
            Ok(Some(kod)) => break kod,
            Ok(None) if start.elapsed() > langst => {
                let _ = barn.kill();
                let _ = barn.wait();
                return Err(format!("{namn} blev inte klar på {} minuter och avbröts", langst.as_secs() / 60));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("{namn}: {e}")),
        }
    };
    if kod.success() {
        return Ok(());
    }
    let allt = std::fs::read(logg).unwrap_or_default();
    let text = String::from_utf8_lossy(allt.get(fore..).unwrap_or_default()).to_string();
    let rader: Vec<&str> = text.lines().filter(|r| !r.trim().is_empty()).collect();
    let slut = rader[rader.len().saturating_sub(4)..].join(" | ");
    Err(format!("{namn} misslyckades ({kod}): {slut}"))
}

/// CM5:ans GPIO-krets (`gpiodetect`: raden med `[pinctrl-rp1]`).
fn rp1_krets() -> Result<String, String> {
    let ut = Command::new("gpiodetect").output().map_err(|e| format!("gpiodetect gick inte att starta: {e}"))?;
    let text = String::from_utf8_lossy(&ut.stdout);
    text.lines()
        .find(|r| r.contains("[pinctrl-rp1]"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|k| k.strip_prefix("gpiochip"))
        .filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .ok_or_else(|| "hittar inte CM5:ans GPIO-krets (pinctrl-rp1)".to_string())
}

impl Flashare {
    fn logg(&self) -> PathBuf {
        match self {
            Flashare::Openocd { arbetsmapp, .. } | Flashare::Kommando { arbetsmapp, .. } => arbetsmapp.join("flash.log"),
        }
    }

    /// Det som saknas för att kunna skriva till kortet (tom = allt finns).
    pub fn forutsattningar(&self) -> Vec<String> {
        let mut saknas = Vec::new();
        match self {
            Flashare::Kommando { program, .. } => {
                if !program.is_file() {
                    saknas.push(format!("flashkommandot {} finns inte", program.display()));
                }
            }
            Flashare::Openocd { kort, rot, arbetsmapp } => {
                // Rättigheterna: robotd installeras som root (systemd-tjänsten), och det
                // behövs för openocd mot ST-Linken och för CM5:ans ben. Är det inte så ska det
                // synas här, inte upptäckas mitt i säkerhetskopieringen.
                if !ar_root(&std::fs::read_to_string("/proc/self/status").unwrap_or_default()) {
                    saknas.push("robotens program kör inte som root och får inte skriva till styrkortet (kontakta Mapro)".into());
                }
                let prov = arbetsmapp.join(".skrivprov");
                if std::fs::create_dir_all(arbetsmapp).and_then(|()| std::fs::write(&prov, b"")).is_err() {
                    saknas.push(format!("robotens program kan inte skriva i {} (kontakta Mapro)", arbetsmapp.display()));
                }
                let _ = std::fs::remove_file(&prov);
                let verktyg: &[&str] = match kort {
                    Kort::Gamla => &["openocd"],
                    Kort::Mp101 => &["openocd", "gpiodetect", "gpioset", "pinctrl"],
                };
                for v in verktyg.iter().filter(|v| !finns_i_path(v)) {
                    saknas.push(format!("verktyget {v} är inte installerat på roboten (kontakta Mapro)"));
                }
                match kort {
                    Kort::Gamla if ar_cm5(rot) => saknas.push("robotens kortvariant säger det äldre styrkortet, men datorn är en CM5 (MP101). Kontakta Mapro.".into()),
                    Kort::Gamla if !stlink_finns(rot) => saknas.push("det sitter ingen ST-Link i roboten. Det här styrkortet går bara att uppdatera med en ST-Link ansluten (kontakta Mapro).".into()),
                    Kort::Mp101 if !ar_cm5(rot) => saknas.push("robotens kortvariant säger MP101, men datorn är ingen CM5. Kontakta Mapro.".into()),
                    _ => {}
                }
            }
        }
        saknas
    }

    /// Kör openocd med `kommandon`. På MP101 hålls BOOT0 låg under tiden och benen ställs
    /// tillbaka efteråt (NRST hög, annars kan STM32 fastna i reset).
    fn openocd(&self, kort: Kort, arbetsmapp: &Path, kommandon: &[String]) -> Result<(), String> {
        let logg = self.logg();
        let mut oc = Command::new("openocd");
        match kort {
            Kort::Gamla => {
                oc.args(["-f", "board/stm32f4discovery.cfg", "-c", "reset_config trst_only combined"]);
                for k in kommandon {
                    oc.args(["-c", k]);
                }
                kor(&mut oc, &logg, LANGST)
            }
            Kort::Mp101 => {
                let krets = rp1_krets()?;
                let cfg = arbetsmapp.join("mp101_openocd.cfg");
                let text = format!(
                    "adapter driver linuxgpiod\nadapter gpio swclk 11 -chip {krets}\nadapter gpio swdio 8 -chip {krets}\nadapter gpio srst 17 -chip {krets}\n\
                     transport select swd\nadapter speed 1000\nreset_config srst_only srst_push_pull\nsource [find target/stm32f4x.cfg]\n"
                );
                std::fs::write(&cfg, text).map_err(|e| format!("{} går inte att skriva: {e}", cfg.display()))?;
                oc.arg("-f").arg(&cfg);
                for k in kommandon {
                    oc.args(["-c", k]);
                }
                let mut boot0 = Command::new("gpioset")
                    .args(["--chip", &format!("gpiochip{krets}"), "--consumer", "mp101-boot0", "16=0"])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .map_err(|e| format!("gpioset gick inte att starta: {e}"))?;
                std::thread::sleep(Duration::from_millis(300));
                let resultat = kor(&mut oc, &logg, LANGST);
                let _ = boot0.kill();
                let _ = boot0.wait();
                // Fasta nivåer när ingen flashar: BOOT0 låg (starta från flash), NRST hög (kör).
                for ben in [["set", "16", "op", "dl"], ["set", "17", "op", "dh"]] {
                    let _ = kor(Command::new("pinctrl").args(ben), &logg, Duration::from_secs(10));
                }
                let _ = std::fs::remove_file(&cfg);
                resultat
            }
        }
    }

    /// Startar kortet (`reset run`) utan att läsa eller skriva något. Används efter en
    /// utläsning som gått fel: den stannar processorn först (`reset halt`), och avbryts
    /// openocd sedan står kortet kvar stoppat fast ingenting har skrivits. På MP101 ställs
    /// benen också tillbaka (NRST hög), som efter varje körning.
    pub fn starta_kortet(&self) -> Result<(), String> {
        let logg = self.logg();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&logg) {
            let _ = std::io::Write::write_all(&mut f, "--- försöker starta styrkortet igen (reset run) ---\n".as_bytes());
        }
        match self {
            Flashare::Kommando { program, .. } => kor(Command::new(program).arg("starta"), &logg, Duration::from_secs(60)),
            Flashare::Openocd { kort, arbetsmapp, .. } => self.openocd(*kort, arbetsmapp, &["init".to_string(), "reset run".to_string(), "shutdown".to_string()]),
        }
    }

    /// Läser ut hela flashminnet till `mal` och kontrollerar att det ser ut som ett program.
    /// Går utläsningen fel kan kortet stå kvar stoppat: den som anropar ska då försöka
    /// [`Self::starta_kortet`].
    pub fn dumpa(&self, mal: &Path) -> Result<(), String> {
        let _ = std::fs::remove_file(mal);
        match self {
            Flashare::Kommando { program, .. } => kor(Command::new(program).arg("dumpa").arg(mal), &self.logg(), LANGST)?,
            Flashare::Openocd { kort, arbetsmapp, .. } => {
                let k = ["init".to_string(), "reset halt".to_string(), format!("flash read_bank 0 {{{}}}", mal.display()), "reset run".to_string(), "shutdown".to_string()];
                self.openocd(*kort, arbetsmapp, &k)?
            }
        }
        kontrollera_dump(mal)
    }

    /// Skriver ELF-filen och låter openocd kontrolläsa den. Kortet startar om.
    pub fn skriv(&self, elf: &Path) -> Result<(), String> {
        match self {
            Flashare::Kommando { program, .. } => kor(Command::new(program).arg("skriv").arg(elf), &self.logg(), LANGST),
            Flashare::Openocd { kort, arbetsmapp, .. } => self.openocd(*kort, arbetsmapp, &[format!("program {{{}}} verify reset exit", elf.display())]),
        }
    }

    /// Skriver tillbaka hela flashminnet från säkerhetskopian. Kortet startar om.
    pub fn aterstall(&self, dump: &Path) -> Result<(), String> {
        kontrollera_dump(dump)?;
        match self {
            Flashare::Kommando { program, .. } => kor(Command::new(program).arg("aterstall").arg(dump), &self.logg(), LANGST),
            Flashare::Openocd { kort, arbetsmapp, .. } => self.openocd(*kort, arbetsmapp, &[format!("program {{{}}} verify reset exit {FLASHSTART:#010x}", dump.display())]),
        }
    }
}

/// En utläsning av flashminnet ska börja med en rimlig vektortabell: stackpekaren i RAM
/// och startadressen i flashminnet (udda = Thumb). En fil med bara nollor eller 0xFF
/// (kortet svarade inte på riktigt) godkänns alltså inte som säkerhetskopia.
pub fn kontrollera_dump(fil: &Path) -> Result<(), String> {
    let b = std::fs::read(fil).map_err(|e| format!("säkerhetskopian {} går inte att läsa: {e}", fil.display()))?;
    if b.len() < 256 * 1024 || b.len() > 16 * 1024 * 1024 {
        return Err(format!("säkerhetskopian av kortets minne har orimlig storlek ({} byte)", b.len()));
    }
    let ord = |at: usize| u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]);
    let (stack, start) = (ord(0), ord(4));
    let slut = FLASHSTART + b.len() as u32;
    if !(0x2000_0000..=0x2010_0000).contains(&stack) || !(FLASHSTART..slut).contains(&start) || start % 2 == 0 {
        return Err("säkerhetskopian av kortets minne ser inte ut som ett program (kortet svarade inte rätt vid utläsningen)".into());
    }
    Ok(())
}

pub use crate::firmware::kontrollera_elf;

#[cfg(test)]
pub(crate) mod prov {
    //! Låtsasfiler för testerna.

    /// Ett flashminne som ser ut som ett program (1 MiB).
    pub fn dump() -> Vec<u8> {
        let mut b = vec![0xffu8; 1024 * 1024];
        b[0..4].copy_from_slice(&0x2002_0000u32.to_le_bytes());
        b[4..8].copy_from_slice(&0x0800_c1a5u32.to_le_bytes());
        b
    }

    /// Början på en ELF-fil för ARM, följd av `resten`.
    pub fn elf(resten: &[u8]) -> Vec<u8> {
        let mut b = vec![0x7f, b'E', b'L', b'F', 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 40, 0];
        b.extend_from_slice(resten);
        b
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn mapp(namn: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("robotuppdatering-flash-{}-{namn}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dumpen_ska_se_ut_som_ett_program() {
        let d = mapp("dump");
        let f = d.join("flash.bin");
        std::fs::write(&f, prov::dump()).unwrap();
        assert_eq!(kontrollera_dump(&f), Ok(()));
        for (namn, innehall) in [("nollor", vec![0u8; 1024 * 1024]), ("ff", vec![0xffu8; 1024 * 1024]), ("kort", prov::dump()[..1000].to_vec()), ("tom", vec![])] {
            std::fs::write(&f, innehall).unwrap();
            assert!(kontrollera_dump(&f).is_err(), "{namn}");
        }
        // Startadressen ska ligga i flashminnet och vara udda (Thumb).
        let mut b = prov::dump();
        b[4..8].copy_from_slice(&0x0800_c1a4u32.to_le_bytes());
        std::fs::write(&f, &b).unwrap();
        assert!(kontrollera_dump(&f).is_err());
        b[4..8].copy_from_slice(&0x2000_0001u32.to_le_bytes());
        std::fs::write(&f, &b).unwrap();
        assert!(kontrollera_dump(&f).is_err());
        assert!(kontrollera_dump(&d.join("saknas")).is_err());
    }

    #[test]
    fn bara_elf_for_arm_godkanns() {
        let d = mapp("elf");
        let f = d.join("fw.elf");
        std::fs::write(&f, prov::elf(b"resten")).unwrap();
        assert_eq!(kontrollera_elf(&f), Ok(()));
        let mut x86 = prov::elf(b"");
        x86[18] = 62;
        for (namn, innehall) in [("bin", prov::dump()[..64].to_vec()), ("x86", x86), ("kort", vec![0x7f, b'E', b'L', b'F']), ("text", b"<html>404</html> och lite till".to_vec())] {
            std::fs::write(&f, innehall).unwrap();
            assert!(kontrollera_elf(&f).is_err(), "{namn}");
        }
    }

    #[test]
    fn root_kanns_igen_pa_effektivt_uid() {
        assert!(ar_root("Name:\trobotd\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n"));
        assert!(ar_root("Uid:\t1000\t0\t0\t0\n"));
        assert!(!ar_root("Name:\trobotd\nUid:\t1000\t1000\t1000\t1000\n"));
        assert!(!ar_root("Uid:\t0\t1000\t1000\t1000\n"));
        assert!(!ar_root(""));
    }

    #[test]
    fn stlink_och_cm5_kanns_igen() {
        let rot = mapp("rot");
        assert!(!stlink_finns(&rot) && !ar_cm5(&rot));
        let usb = rot.join("sys/bus/usb/devices/1-1.2");
        std::fs::create_dir_all(&usb).unwrap();
        std::fs::write(usb.join("idVendor"), "0483\n").unwrap();
        std::fs::write(usb.join("idProduct"), "5740\n").unwrap(); // styrkortets egen USB-port, ingen ST-Link
        assert!(!stlink_finns(&rot));
        std::fs::write(usb.join("idProduct"), "3748\n").unwrap();
        assert!(stlink_finns(&rot));
        std::fs::create_dir_all(rot.join("proc/device-tree")).unwrap();
        std::fs::write(rot.join("proc/device-tree/model"), b"Raspberry Pi Compute Module 5 Rev 1.0\0").unwrap();
        assert!(ar_cm5(&rot));
        std::fs::write(rot.join("proc/device-tree/model"), b"Raspberry Pi 4 Model B Rev 1.4\0").unwrap();
        assert!(!ar_cm5(&rot));
        // Det äldre kortet utan ST-Link, och en variant som inte stämmer med datorn, stoppas.
        let utan = mapp("utan");
        let f = |kort, rot: &Path| Flashare::Openocd { kort, rot: rot.to_path_buf(), arbetsmapp: utan.clone() }.forutsattningar().join(" / ");
        assert!(f(Kort::Gamla, &utan).contains("ST-Link"));
        assert!(!f(Kort::Gamla, &rot).contains("ST-Link"));
        assert!(f(Kort::Mp101, &rot).contains("ingen CM5"));
        std::fs::write(rot.join("proc/device-tree/model"), b"Raspberry Pi Compute Module 5 Rev 1.0\0").unwrap();
        assert!(f(Kort::Gamla, &rot).contains("CM5"));
        assert!(!f(Kort::Mp101, &rot).contains("CM5"));
    }
}
