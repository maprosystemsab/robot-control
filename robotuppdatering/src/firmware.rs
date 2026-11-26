//! Styrkortets firmware i uppdateringsbeskrivningen: version, kortvariant och krav.

use std::collections::BTreeMap;
use std::fmt;

use crate::beskrivning::{jamfor_versioner, Erbjudande, Fel, Manifest};

/// Firmwarens post i beskrivningen.
pub const KOMPONENT: &str = "firmware";

/// Firmwarens version som kortet uppger den: två tal, "30.33".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct FwVersion {
    pub huvud: u8,
    pub under: u8,
}

impl FwVersion {
    /// Tolkar "30.33". Bara siffror och exakt en punkt.
    pub fn tolka(text: &str) -> Option<FwVersion> {
        fn tal(s: &str) -> Option<u8> {
            (!s.is_empty() && s.len() <= 3 && s.bytes().all(|c| c.is_ascii_digit()) && (s == "0" || !s.starts_with('0'))).then(|| s.parse().ok()).flatten()
        }
        let (h, u) = text.split_once('.')?;
        Some(FwVersion { huvud: tal(h)?, under: tal(u)? })
    }
}

impl fmt::Display for FwVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.huvud, self.under)
    }
}

/// Vilket styrkort roboten har. Det avgör både filen och hur den skrivs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kort {
    /// Det äldre styrkortet (Carcontroller), skrivs med en ST-Link på USB.
    Gamla,
    /// MP101 (Upwis ROV_MCU), skrivs via CM5:ans egna ben.
    Mp101,
}

/// Kortvarianten: allt som avgör vilken firmwarefil som är rätt för roboten. Fel variant
/// kan ge en robot utan fungerande nödstopp, så den gissas aldrig: den ska stå uttryckligen
/// i robotens inställningar, i exakt den form som [`Variant::nyckel`] skriver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    pub kort: Kort,
    /// Maskinens profil: `robant`, `mactrac` eller `drangen`.
    pub profil: String,
    /// Nödstoppet sitter på styrkortets DI1 (bara MP101).
    pub nodstopp: bool,
    /// Lägesknappen sitter på DI2 och lampan på DI3 (bara MP101).
    pub lagesknapp: bool,
}

pub const PROFILER: &[&str] = &["robant", "mactrac", "drangen"];

impl Variant {
    /// Tolkar `<kort>-<profil>[-nodstopp][-lagesknapp]`, t.ex. `mp101-robant-nodstopp`.
    /// Bara exakt den formen godtas (tillvalen i den ordningen, och bara på MP101).
    pub fn tolka(text: &str) -> Option<Variant> {
        let mut delar = text.split('-');
        let kort = match delar.next()? {
            "gamla" => Kort::Gamla,
            "mp101" => Kort::Mp101,
            _ => return None,
        };
        let profil = delar.next().filter(|p| PROFILER.contains(p))?.to_string();
        let mut v = Variant { kort, profil, nodstopp: false, lagesknapp: false };
        let mut nasta = delar.next();
        if nasta == Some("nodstopp") {
            v.nodstopp = true;
            nasta = delar.next();
        }
        if nasta == Some("lagesknapp") {
            v.lagesknapp = true;
            nasta = delar.next();
        }
        if nasta.is_some() || (kort == Kort::Gamla && (v.nodstopp || v.lagesknapp)) {
            return None;
        }
        Some(v)
    }

    /// Variantens namn i beskrivningen (nyckeln under `filer`).
    pub fn nyckel(&self) -> String {
        format!(
            "{}-{}{}{}",
            match self.kort {
                Kort::Gamla => "gamla",
                Kort::Mp101 => "mp101",
            },
            self.profil,
            if self.nodstopp { "-nodstopp" } else { "" },
            if self.lagesknapp { "-lagesknapp" } else { "" }
        )
    }
}

/// Det servern har för ett kort.
#[derive(Debug, Clone, PartialEq)]
pub enum Utbud {
    /// Listan har ingen firmware alls.
    Inget,
    /// Listan har firmware, men ingen fil för den här kortvarianten.
    VariantSaknas { version: FwVersion },
    /// Kortet har redan den version som finns (eller en nyare).
    Aktuell { version: FwVersion },
    Nyare(Erbjudande<FwVersion>),
}

/// Vad finns i beskrivningen för kortvarianten `variant` som nu har version `har`?
pub fn utbud(manifest: &Manifest, variant: &Variant, har: FwVersion) -> Result<Utbud, Fel> {
    let Some(paket) = manifest.paket.get(KOMPONENT) else { return Ok(Utbud::Inget) };
    let version = FwVersion::tolka(&paket.version).ok_or_else(|| Fel::Manifest(format!("obegriplig firmwareversion: {}", paket.version)))?;
    let nyckel = variant.nyckel();
    // Varje variantnamn i listan ska gå att tolka: ett skrivfel där ska inte kunna se ut
    // som "ingen fil för den här roboten".
    if let Some(fel) = paket.filer.keys().find(|k| Variant::tolka(k).is_none()) {
        return Err(Fel::Manifest(format!("okänd kortvariant i listan: {fel}")));
    }
    if !paket.filer.contains_key(&nyckel) {
        return Ok(Utbud::VariantSaknas { version });
    }
    Ok(match manifest.nyare(KOMPONENT, &nyckel, har, FwVersion::tolka)? {
        Some(e) => Utbud::Nyare(e),
        None => Utbud::Aktuell { version },
    })
}

/// Det roboten vet om en komponent som ett krav kan gälla.
#[derive(Debug, Clone, PartialEq)]
pub enum Installerat {
    Version(String),
    /// Komponenten finns på roboten men versionen är inte känd.
    Okand,
    /// Komponenten finns inte på den här roboten (kravet gäller då inte).
    Saknas,
}

/// Kontrollerar firmwarens krav mot det som finns på roboten. Ger en text per krav som
/// inte är uppfyllt. Ett krav på en komponent som roboten inte känner till alls räknas
/// som ouppfyllt (robotens programvara är då äldre än kravet).
pub fn ouppfyllda_krav(kraver: &BTreeMap<String, String>, installerat: &BTreeMap<String, Installerat>) -> Vec<String> {
    let mut ut = Vec::new();
    for (komponent, lagst) in kraver {
        let text = match installerat.get(komponent) {
            Some(Installerat::Saknas) => continue,
            Some(Installerat::Version(v)) => match jamfor_versioner(v, lagst) {
                Some(std::cmp::Ordering::Less) => format!("{komponent} på roboten är version {v}, den nya firmwaren kräver minst {lagst}"),
                Some(_) => continue,
                None => format!("{komponent} har en version som inte går att jämföra ({v} mot {lagst})"),
            },
            Some(Installerat::Okand) => format!("den nya firmwaren kräver {komponent} minst {lagst}, och robotens version är inte känd"),
            None => format!("den nya firmwaren kräver {komponent} minst {lagst}, som den här roboten inte känner till"),
        };
        ut.push(text);
    }
    ut
}

/// En firmwarefil ska vara en ELF-fil för 32-bitars ARM (STM32). Fångar en fil för fel
/// maskin eller en .bin-fil, som skulle skriva över kortets inställningar.
pub fn kontrollera_elf(fil: &std::path::Path) -> Result<(), String> {
    let mut huvud = [0u8; 20];
    let last = std::fs::File::open(fil).and_then(|mut f| std::io::Read::read_exact(&mut f, &mut huvud));
    if last.is_err() || huvud[..4] != [0x7f, b'E', b'L', b'F'] || huvud[4] != 1 || huvud[5] != 1 || u16::from_le_bytes([huvud[18], huvud[19]]) != 40 {
        return Err("den hämtade filen är ingen firmwarefil för styrkortet (ELF för ARM)".into());
    }
    Ok(())
}

#[cfg(test)]
mod tester {
    use super::*;
    use crate::beskrivning::{oppna_kuvert, prov};

    #[test]
    fn firmwareversioner() {
        let v = |s| FwVersion::tolka(s).unwrap();
        assert_eq!(v("30.33").to_string(), "30.33");
        assert!(v("30.33") > v("30.4"));
        assert!(v("31.0") > v("30.99"));
        assert_eq!(v("30.0"), FwVersion { huvud: 30, under: 0 });
        for skrap in ["", "30", "30.", ".33", "30.33.1", "30.x", "v30.33", "30,33", " 30.33", "30.33 ", "256.1", "30.256", "30.033", "030.33", "+30.33", "30.-1"] {
            assert_eq!(FwVersion::tolka(skrap), None, "{skrap:?}");
        }
    }

    #[test]
    fn varianten_ska_vara_exakt() {
        for bra in ["gamla-robant", "gamla-mactrac", "gamla-drangen", "mp101-robant", "mp101-robant-nodstopp", "mp101-robant-lagesknapp", "mp101-robant-nodstopp-lagesknapp", "mp101-mactrac-nodstopp"] {
            let v = Variant::tolka(bra).unwrap_or_else(|| panic!("{bra}"));
            assert_eq!(v.nyckel(), bra);
        }
        let v = Variant::tolka("mp101-robant-nodstopp").unwrap();
        assert!(v.nodstopp && !v.lagesknapp && v.kort == Kort::Mp101 && v.profil == "robant");
        for fel in [
            "", "robant", "mp101", "mp101-", "mp101-robant-", "mp101-robant-lagesknapp-nodstopp", "mp101-robant-nodstopp-nodstopp", "gamla-robant-nodstopp",
            "gamla-robant-lagesknapp", "MP101-robant", "mp101-Robant", "mp101-robant-nödstopp", "mp101-robant nodstopp", "rovmcu-robant", "mp101-robant-nodstopp-lagesknapp-x",
            "mp101_robant", " mp101-robant", "mp101-robant\n",
        ] {
            assert_eq!(Variant::tolka(fel), None, "{fel:?}");
        }
    }

    fn lista(version: &str, varianter: &[&str]) -> Manifest {
        let (par, publik) = prov::nyckel(3);
        let filer: Vec<String> = varianter
            .iter()
            .map(|v| format!(r#""{v}":{{"fil":"firmware-{version}-{v}.elf","storlek":4,"sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}}"#))
            .collect();
        let text = format!(
            r#"{{"format":1,"lopnummer":5,"paket":{{"sentinant":{{"version":"0.1.0+1","filer":{{}}}},"firmware":{{"version":"{version}","nyheter":" Bättre broms ","kraver":{{"car_client":"30.33"}},"filer":{{{}}}}}}}}}"#,
            filer.join(",")
        );
        oppna_kuvert(&prov::signerat(&text, &par), &[publik]).unwrap()
    }

    #[test]
    fn utbudet_for_ett_kort() {
        let v = |s| FwVersion::tolka(s).unwrap();
        let mk1 = Variant::tolka("mp101-robant-nodstopp-lagesknapp").unwrap();
        let m = lista("30.33", &["gamla-robant", "mp101-robant-nodstopp-lagesknapp"]);
        match utbud(&m, &mk1, v("30.32")).unwrap() {
            Utbud::Nyare(e) => {
                assert_eq!((e.version, e.fil.as_str(), e.nyheter.as_str()), (v("30.33"), "firmware-30.33-mp101-robant-nodstopp-lagesknapp.elf", "Bättre broms"));
                assert_eq!(e.kraver["car_client"], "30.33");
            }
            annat => panic!("{annat:?}"),
        }
        // Samma eller nyare på kortet: aldrig bakåt.
        assert_eq!(utbud(&m, &mk1, v("30.33")).unwrap(), Utbud::Aktuell { version: v("30.33") });
        assert_eq!(utbud(&m, &mk1, v("30.40")).unwrap(), Utbud::Aktuell { version: v("30.33") });
        assert_eq!(utbud(&m, &mk1, v("31.0")).unwrap(), Utbud::Aktuell { version: v("30.33") });
        // En närliggande variant duger inte: nödstoppet får inte försvinna.
        for annan in ["mp101-robant", "mp101-robant-nodstopp", "mp101-robant-lagesknapp", "mp101-mactrac-nodstopp-lagesknapp", "gamla-mactrac"] {
            assert_eq!(utbud(&m, &Variant::tolka(annan).unwrap(), v("30.1")).unwrap(), Utbud::VariantSaknas { version: v("30.33") }, "{annan}");
        }
        // Ingen firmware i listan alls, och skräp i listan.
        let (par, publik) = prov::nyckel(3);
        let tom = oppna_kuvert(&prov::signerat(r#"{"format":1,"lopnummer":5,"paket":{}}"#, &par), &[publik]).unwrap();
        assert_eq!(utbud(&tom, &mk1, v("30.32")).unwrap(), Utbud::Inget);
        assert!(matches!(utbud(&lista("senaste", &["gamla-robant"]), &mk1, v("30.32")), Err(Fel::Manifest(_))));
        assert!(matches!(utbud(&lista("30.33", &["gamla-robant", "mp101-robant-nodstop"]), &mk1, v("30.32")), Err(Fel::Manifest(_))));
    }

    #[test]
    fn kraven() {
        use Installerat::*;
        let krav: BTreeMap<String, String> = [("car_client".to_string(), "30.33".to_string()), ("robotd".to_string(), "0.1.0+202611270000".to_string())].into();
        let har = |cc: Option<Installerat>, rd: Option<Installerat>| -> BTreeMap<String, Installerat> {
            [("car_client", cc), ("robotd", rd)].into_iter().filter_map(|(k, v)| Some((k.to_string(), v?))).collect()
        };
        assert!(ouppfyllda_krav(&krav, &har(Some(Version("30.33".into())), Some(Version("0.1.0+202611270000".into())))).is_empty());
        assert!(ouppfyllda_krav(&krav, &har(Some(Version("30.40".into())), Some(Version("0.2.0".into())))).is_empty());
        // Komponenten finns inte på roboten: kravet gäller inte.
        assert!(ouppfyllda_krav(&krav, &har(Some(Saknas), Some(Saknas))).is_empty());
        // För gammal, okänd version, och en komponent roboten aldrig hört talas om.
        assert_eq!(ouppfyllda_krav(&krav, &har(Some(Version("30.32".into())), Some(Saknas))).len(), 1);
        assert_eq!(ouppfyllda_krav(&krav, &har(Some(Okand), Some(Version("0.1.0+202611260000".into())))).len(), 2);
        assert_eq!(ouppfyllda_krav(&krav, &har(None, None)).len(), 2);
        assert_eq!(ouppfyllda_krav(&krav, &har(Some(Version("skräp".into())), Some(Saknas))).len(), 1);
        assert!(ouppfyllda_krav(&BTreeMap::new(), &har(None, None)).is_empty());
    }
}
