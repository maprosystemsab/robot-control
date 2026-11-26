//! Det klienten och robotd säger till varandra om styrkortets firmware.
//!
//! Klienten skickar bara ett [`FirmwareKommando`]. Allt innehåll (vilken version som
//! finns, filen, kontrollsumman) hämtar och kontrollerar roboten själv. robotd skickar
//! [`FirmwareStatus`] i sin vanliga status, så en klient som ansluter igen mitt i eller
//! efter en uppdatering ser hur det gick.

use serde::{Deserialize, Serialize};

/// Från klienten (bara den som har kontrollen över roboten).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FirmwareKommando {
    /// Fråga servern om det finns ny firmware för den här robotens kort.
    Sok,
    /// Uppdatera till `version`. Den ska vara den version roboten själv erbjöd; har listan
    /// ändrats sedan dess gör roboten ingenting.
    Uppdatera { version: String },
    /// Skriv tillbaka kortets förra innehåll (säkerhetskopian från den senaste,
    /// misslyckade uppdateringen).
    Aterstall,
}

/// Läget på roboten. Texterna är färdiga att visa.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FirmwareStatus {
    /// Styrkortets version just nu, t.ex. "30.32". `None` = kortet svarar inte.
    #[serde(default)]
    pub kortversion: Option<String>,
    /// Robotens kortvariant (det som avgör vilken fil som passar), om den är angiven.
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub lage: FirmwareLage,
    /// Det som hindrar en uppdatering just nu (roboten är aktiverad, rör sig …). Tom = fritt.
    #[serde(default)]
    pub sparrar: Vec<String>,
    /// Dagen serverns lista gjordes (ÅÅÅÅ-MM-DD), tom om ingen lista lästs.
    #[serde(default)]
    pub listans_datum: String,
    /// robotd är ett provbygge som godkänner provnyckeln. Ska aldrig synas på en riktig robot.
    #[serde(default)]
    pub provbygge: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum FirmwareLage {
    /// Ingen har sökt sedan robotd startade.
    #[default]
    Vilar,
    Soker,
    /// Kortet har den senaste versionen som finns för det.
    Aktuell,
    NyFinns {
        version: String,
        #[serde(default)]
        nyheter: String,
    },
    /// Det går inte att söka eller uppdatera, med orsaken (servern svarar inte, varianten
    /// är inte angiven, ett krav är inte uppfyllt …).
    Hinder { text: String },
    /// Uppdateringen pågår. `steg` av `steg_antal` (från 1), `text` säger vad som görs.
    Pagar {
        version: String,
        steg: u8,
        steg_antal: u8,
        text: String,
        #[serde(default)]
        procent: Option<u8>,
    },
    Klar {
        version: String,
        /// Versionen före.
        #[serde(default)]
        fran: String,
        /// Sådant föraren ska veta eller göra efteråt.
        #[serde(default)]
        anmarkningar: Vec<String>,
    },
    Misslyckad {
        version: String,
        text: String,
        /// `Some(true)` = kortets förra innehåll skrevs tillbaka och kortet svarar,
        /// `Some(false)` = återställningen misslyckades, `None` = ingenting hann skrivas.
        #[serde(default)]
        aterstalld: Option<bool>,
        /// Det finns en säkerhetskopia av kortets minne som går att skriva tillbaka.
        #[serde(default)]
        kan_aterstallas: bool,
    },
}

impl FirmwareLage {
    pub fn pagar(&self) -> bool {
        matches!(self, FirmwareLage::Pagar { .. })
    }
}

/// Texten robotd svarar med när någon försöker köra medan styrkortet uppdateras.
pub const TEXT_UPPDATERING_PAGAR: &str = "Styrkortet uppdateras. Roboten går inte att köra förrän det är klart.";

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn aldre_meddelanden_utan_falten_gar_att_lasa() {
        let s: FirmwareStatus = serde_json::from_str("{}").unwrap();
        assert_eq!(s, FirmwareStatus::default());
        let s: FirmwareStatus = serde_json::from_str(r#"{"kortversion":"30.32","lage":{"NyFinns":{"version":"30.33"}}}"#).unwrap();
        assert_eq!(s.lage, FirmwareLage::NyFinns { version: "30.33".into(), nyheter: String::new() });
        let k: FirmwareKommando = serde_json::from_str(r#"{"Uppdatera":{"version":"30.33"}}"#).unwrap();
        assert_eq!(k, FirmwareKommando::Uppdatera { version: "30.33".into() });
        assert_eq!(serde_json::to_string(&FirmwareKommando::Sok).unwrap(), "\"Sok\"");
    }
}
