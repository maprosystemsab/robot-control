//! Den signerade uppdateringsbeskrivningen (`uppdatering.json`): signatur, innehåll,
//! färskhet och kontroll av hämtade filer. Används av programuppdateringen i klienten och
//! av firmwareuppdateringen på roboten.
//!
//! Beskrivningens form (det som signeras är texten i `manifest`, tecken för tecken):
//! ```json
//! { "manifest": "<JSON-texten nedan>", "signaturer": ["<128 hex>", "..."] }
//!
//! { "format": 1,
//!   "lopnummer": 1795790000,
//!   "skapad": "2026-11-26T14:27:00Z",
//!   "paket": {
//!     "sentinant": {
//!       "version": "0.1.0+202611261256",
//!       "nyheter": "Valfri text som visas vid knappen",
//!       "filer": {
//!         "windows-x86_64": { "fil": "sentinant-…-windows-x86_64.exe", "storlek": 123, "sha256": "<64 hex>" },
//!         "linux-x86_64":   { "fil": "sentinant-…-linux-x86_64",       "storlek": 123, "sha256": "<64 hex>" }
//!       } },
//!     "firmware": {
//!       "version": "30.33",
//!       "nyheter": "…",
//!       "kraver": { "car_client": "30.33" },
//!       "filer": {
//!         "mp101-robant-nodstopp": { "fil": "firmware-30.33-mp101-robant-nodstopp.elf", "storlek": 123, "sha256": "<64 hex>" }
//!       } } } }
//! ```
//! En post per komponent, och i den en fil per plattform (program) eller kortvariant
//! (firmware). Okända fält och poster hoppas över. Flera signaturer får finnas, så att
//! nyckeln kan bytas: det räcker att en stämmer med någon av de inbyggda nycklarna.
//!
//! Färskhet: signaturen hindrar förfalskning, men inte att någon som svarar i serverns
//! ställe visar en gammal, äkta beskrivning om och om igen. Därför har beskrivningen ett
//! signerat `lopnummer` som bara växer. Den som läser minns det högsta den sett och avvisar
//! en beskrivning med lägre nummer.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use serde::Deserialize;

/// Vår server i tunneln. Mappen innehåller `uppdatering.json` och filerna den nämner.
pub const STANDARDKALLA: &str = "http://192.168.200.1:8088/sentinant/";
pub const MANIFESTFIL: &str = "uppdatering.json";
/// Den form på beskrivningen som koden förstår.
pub const FORMAT: u32 = 1;
/// Publika nycklar (ed25519, 32 byte som hex) som en beskrivning får vara signerad med.
/// Den privata halvan ligger utanför git (se dokument/uppdatering.md).
pub const PUBLIKA_NYCKLAR: &[&str] = &["a25b48745b8670e89083a17daf3a8a0fcb4a6523b49e3c0969b974e30f628a87"];
/// Provnyckeln (fröet är sha256 av en känd text, så vem som helst kan signera med den).
/// Godkänns bara i ett bygge med `provnyckel` påslaget, se Cargo.toml.
pub const PROVNYCKEL: &str = "aa269d84e15bbc359b65679c0be1fdf3ff46215a9d7b231a07ad85badd7c1d30";

pub const STORSTA_BESKRIVNING: u64 = 256 * 1024;
pub const STORSTA_FIL: u64 = 400 * 1024 * 1024;

/// Varför en beskrivning eller en fil inte godkändes.
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
    /// Beskrivningen har en nyare form än koden förstår.
    Format(u32),
    /// Den hämtade filen har fel storlek.
    Storlek,
    /// Den hämtade filen har fel SHA-256.
    Kontrollsumma,
    /// Filen gick inte att läsa eller spara.
    Fil(String),
    /// Beskrivningen är äldre än en som redan setts (lägre löpnummer).
    Aterspelad { sett: u64, fick: u64 },
}

#[derive(Deserialize)]
struct Kuvert {
    manifest: String,
    #[serde(default)]
    signaturer: Vec<String>,
}

/// Uppdateringsbeskrivningen, efter att signaturen kontrollerats.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Manifest {
    pub format: u32,
    /// Växer med varje publicering; en beskrivning med lägre nummer än det högsta som
    /// setts avvisas ([`granska_farskhet`]).
    pub lopnummer: u64,
    /// När beskrivningen gjordes (RFC 3339).
    #[serde(default)]
    pub skapad: String,
    /// Valfritt: efter den här tiden är listan gammal (en varning, inget stopp).
    #[serde(default)]
    pub giltig_till: Option<String>,
    /// Komponent ("sentinant", "firmware" …) -> det som finns att hämta.
    #[serde(default)]
    pub paket: BTreeMap<String, Paket>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Paket {
    pub version: String,
    #[serde(default)]
    pub nyheter: String,
    /// Plattform ("windows-x86_64") eller kortvariant ("mp101-robant-nodstopp") -> fil.
    #[serde(default)]
    pub filer: BTreeMap<String, Fil>,
    /// Det som måste finnas på mottagaren: komponent -> lägsta version.
    #[serde(default)]
    pub kraver: BTreeMap<String, String>,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Fil {
    pub fil: String,
    pub storlek: u64,
    pub sha256: String,
}

/// En nyare version som går att hämta, med versionen i mottagarens egen form.
#[derive(Debug, Clone, PartialEq)]
pub struct Erbjudande<V> {
    pub version: V,
    pub fil: String,
    pub storlek: u64,
    pub sha256: [u8; 32],
    pub nyheter: String,
    pub kraver: BTreeMap<String, String>,
}

// `is_multiple_of` finns inte i de äldre Rust-versionerna på robotarna.
#[allow(unknown_lints, clippy::manual_is_multiple_of)]
pub fn hex_till_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// De inbyggda publika nycklarna (och provnyckeln i ett provbygge).
pub fn nycklar() -> Vec<[u8; 32]> {
    let prov: &[&str] = if cfg!(feature = "provnyckel") { &[PROVNYCKEL] } else { &[] };
    PUBLIKA_NYCKLAR.iter().chain(prov).filter_map(|h| hex_till_bytes(h)?.try_into().ok()).collect()
}

/// Kontrollerar signaturen och tolkar sedan beskrivningen. Innehållet i `manifest` läses
/// inte förrän minst en signatur stämmer med någon av `nycklar`.
pub fn oppna_kuvert(kuvert: &[u8], nycklar: &[[u8; 32]]) -> Result<Manifest, Fel> {
    let kuvert: Kuvert = serde_json::from_slice(kuvert).map_err(|_| Fel::Kuvert)?;
    let text = kuvert.manifest.as_bytes();
    let signerad = kuvert.signaturer.iter().filter_map(|s| hex_till_bytes(s)).filter(|s| s.len() == 64).any(|signatur| {
        nycklar.iter().any(|nyckel| ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, nyckel).verify(text, &signatur).is_ok())
    });
    if !signerad {
        return Err(Fel::Signatur);
    }
    #[derive(Deserialize)]
    struct BaraFormat {
        format: u32,
    }
    let format = serde_json::from_slice::<BaraFormat>(text).map_err(|e| Fel::Manifest(e.to_string()))?.format;
    if format != FORMAT {
        return Err(Fel::Format(format));
    }
    serde_json::from_slice(text).map_err(|e| Fel::Manifest(e.to_string()))
}

/// Ett filnamn i beskrivningen får bara vara ett namn: inga mappar, ingen adress.
pub fn giltigt_filnamn(namn: &str) -> bool {
    !namn.is_empty() && namn.len() <= 120 && !namn.starts_with('.') && namn.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
}

impl Fil {
    /// Filens storlek och SHA-256, om namnet och värdena är rimliga.
    pub fn kontrollerad(&self) -> Result<(u64, [u8; 32]), Fel> {
        if !giltigt_filnamn(&self.fil) {
            return Err(Fel::Manifest(format!("otillåtet filnamn: {}", self.fil)));
        }
        if self.storlek == 0 || self.storlek > STORSTA_FIL {
            return Err(Fel::Manifest(format!("orimlig storlek: {}", self.storlek)));
        }
        let sha256 = hex_till_bytes(&self.sha256).and_then(|b| <[u8; 32]>::try_from(b).ok()).ok_or_else(|| Fel::Manifest("sha256 ska vara 64 hextecken".into()))?;
        Ok((self.storlek, sha256))
    }
}

impl Manifest {
    /// Finns en version av `komponent` för `plattform` som är nyare än `har`? En äldre
    /// eller samma version är aldrig ett erbjudande: ingenting går bakåt. `tolka` gör
    /// versionstexten till mottagarens egen form (som ska gå att jämföra).
    pub fn nyare<V: PartialOrd + Copy>(&self, komponent: &str, plattform: &str, har: V, tolka: impl Fn(&str) -> Option<V>) -> Result<Option<Erbjudande<V>>, Fel> {
        let Some(paket) = self.paket.get(komponent) else { return Ok(None) };
        let Some(fil) = paket.filer.get(plattform) else { return Ok(None) };
        let version = tolka(&paket.version).ok_or_else(|| Fel::Manifest(format!("obegriplig version: {}", paket.version)))?;
        if version.partial_cmp(&har) != Some(std::cmp::Ordering::Greater) {
            return Ok(None);
        }
        let (storlek, sha256) = fil.kontrollerad()?;
        Ok(Some(Erbjudande { version, fil: fil.fil.clone(), storlek, sha256, nyheter: paket.nyheter.trim().to_string(), kraver: paket.kraver.clone() }))
    }
}

/// Det som är känt om serverns lista när den godkänts.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Farskhet {
    /// Dagen listan gjordes (ÅÅÅÅ-MM-DD), tom om den inte står där.
    pub skapad: String,
    /// `giltig_till` (ÅÅÅÅ-MM-DD) om den tiden har passerats: listan borde ha förnyats.
    pub utgangen: Option<String>,
}

/// Det högsta löpnummer som setts, sparat i `fil` (0 om filen saknas).
pub fn las_hogsta_lopnummer(fil: &Path) -> u64 {
    #[derive(Deserialize)]
    struct Sett {
        hogsta_lopnummer: u64,
    }
    std::fs::read(fil).ok().and_then(|b| serde_json::from_slice::<Sett>(&b).ok()).map_or(0, |s| s.hogsta_lopnummer)
}

/// Avvisar en beskrivning med lägre löpnummer än det högsta som setts (sparat i `minne`),
/// och sparar numret när det är högre. Ser också efter om `giltig_till` har passerats;
/// det ger bara en uppgift tillbaka, aldrig ett fel.
pub fn granska_farskhet(manifest: &Manifest, minne: Option<&Path>, nu: chrono::DateTime<chrono::Utc>) -> Result<Farskhet, Fel> {
    let sett = minne.map_or(0, las_hogsta_lopnummer);
    if manifest.lopnummer < sett {
        return Err(Fel::Aterspelad { sett, fick: manifest.lopnummer });
    }
    let utgangen = match &manifest.giltig_till {
        Some(t) => {
            let till = chrono::DateTime::parse_from_rfc3339(t).map_err(|_| Fel::Manifest(format!("obegriplig giltig_till: {t}")))?;
            (till < nu).then(|| till.format("%Y-%m-%d").to_string())
        }
        None => None,
    };
    if let (Some(fil), true) = (minne, manifest.lopnummer > sett) {
        if let Some(mapp) = fil.parent() {
            let _ = std::fs::create_dir_all(mapp);
        }
        let _ = std::fs::write(fil, format!("{{\"hogsta_lopnummer\":{}}}\n", manifest.lopnummer));
    }
    let skapad = chrono::DateTime::parse_from_rfc3339(&manifest.skapad).map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default();
    Ok(Farskhet { skapad, utgangen })
}

/// Stämmer filen på disken med beskrivningen? Storleken först, sedan SHA-256.
pub fn kontrollera_fil(fil: &Path, storlek: u64, sha256: &[u8; 32]) -> Result<(), Fel> {
    let mut f = std::fs::File::open(fil).map_err(|e| Fel::Fil(e.to_string()))?;
    if f.metadata().map_err(|e| Fel::Fil(e.to_string()))?.len() != storlek {
        return Err(Fel::Storlek);
    }
    let mut summa = ring::digest::Context::new(&ring::digest::SHA256);
    let mut bit = vec![0u8; 64 * 1024];
    let mut last = 0u64;
    loop {
        let n = f.read(&mut bit).map_err(|e| Fel::Fil(e.to_string()))?;
        if n == 0 {
            break;
        }
        last += n as u64;
        summa.update(&bit[..n]);
    }
    if last != storlek {
        return Err(Fel::Storlek);
    }
    if summa.finish().as_ref() != sha256 {
        return Err(Fel::Kontrollsumma);
    }
    Ok(())
}

/// Jämför två versioner skrivna med siffror, punkter och ett valfritt `+bygge`
/// ("30.33", "0.1.0+202611261256"). Siffra för siffra från vänster; saknade delar är 0.
/// `None` om någon av dem inte är en sådan version.
pub fn jamfor_versioner(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    fn delar(s: &str) -> Option<Vec<u64>> {
        if s.is_empty() || s.len() > 64 {
            return None;
        }
        s.split(['.', '+']).map(|d| (!d.is_empty() && d.len() <= 18 && d.bytes().all(|c| c.is_ascii_digit())).then(|| d.parse().ok()).flatten()).collect()
    }
    let (a, b) = (delar(a)?, delar(b)?);
    let n = a.len().max(b.len());
    let pa = a.iter().copied().chain(std::iter::repeat(0)).take(n);
    let pb = b.iter().copied().chain(std::iter::repeat(0)).take(n);
    Some(pa.cmp(pb))
}

#[cfg(test)]
pub(crate) mod prov {
    //! Hjälp för testerna: signera en beskrivning med en nyckel ur ett frö.
    use ring::signature::KeyPair;

    pub fn nyckel(fro: u8) -> (ring::signature::Ed25519KeyPair, [u8; 32]) {
        let par = ring::signature::Ed25519KeyPair::from_seed_unchecked(&[fro; 32]).unwrap();
        let publik: [u8; 32] = par.public_key().as_ref().try_into().unwrap();
        (par, publik)
    }

    pub fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    pub fn signerat(manifest: &str, par: &ring::signature::Ed25519KeyPair) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({ "manifest": manifest, "signaturer": [hex(par.sign(manifest.as_bytes()).as_ref())] })).unwrap()
    }

    pub fn sha256(b: &[u8]) -> [u8; 32] {
        ring::digest::digest(&ring::digest::SHA256, b).as_ref().try_into().unwrap()
    }
}

#[cfg(test)]
mod tester {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn versioner_jamfors_som_tal() {
        assert_eq!(jamfor_versioner("30.33", "30.4"), Some(Greater));
        assert_eq!(jamfor_versioner("30.4", "30.33"), Some(Less));
        assert_eq!(jamfor_versioner("30.33", "30.33"), Some(Equal));
        assert_eq!(jamfor_versioner("30.33", "30.33.0"), Some(Equal));
        assert_eq!(jamfor_versioner("0.1.0+202611261256", "0.1.0+202611261255"), Some(Greater));
        assert_eq!(jamfor_versioner("0.1.0", "0.1.0+1"), Some(Less));
        for skrap in ["", "30.", ".33", "30.x", "v30.33", "30,33", " 30.33", "30.33-rc1", "-1", "99999999999999999999"] {
            assert_eq!(jamfor_versioner(skrap, "30.33"), None, "{skrap:?}");
            assert_eq!(jamfor_versioner("30.33", skrap), None, "{skrap:?}");
        }
    }

    #[test]
    fn signaturen_avgor_och_kraven_foljer_med() {
        let (par, publik) = prov::nyckel(1);
        let text = r#"{"format":1,"lopnummer":5,"paket":{"firmware":{"version":"30.33","kraver":{"car_client":"30.33"},"filer":{"mp101-robant":{"fil":"firmware-30.33-mp101-robant.elf","storlek":4,"sha256":"9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"}}}}}"#;
        let m = oppna_kuvert(&prov::signerat(text, &par), &[publik]).unwrap();
        assert_eq!(m.paket["firmware"].kraver["car_client"], "30.33");
        assert_eq!(oppna_kuvert(&prov::signerat(text, &par), &[prov::nyckel(2).1]), Err(Fel::Signatur));
        assert_eq!(oppna_kuvert(&prov::signerat(text, &par), &[]), Err(Fel::Signatur));
        // Den riktiga nyckeln är alltid med; provnyckeln bara i ett provbygge.
        assert_eq!(nycklar().len(), PUBLIKA_NYCKLAR.len() + usize::from(crate::PROVBYGGE));
    }
}
