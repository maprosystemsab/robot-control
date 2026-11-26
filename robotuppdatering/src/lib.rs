//! Gemensam kod för uppdateringarna i SentinAnt och Robotstyrning.
//!
//! - [`protokoll`]: det klienten och robotd säger till varandra om styrkortets firmware
//!   (alltid med, bara serde).
//! - [`beskrivning`] (`kontroll`): den signerade uppdateringsbeskrivningen, samma som
//!   programuppdateringen använder (signatur, löpnummer, storlek och SHA-256).
//! - [`firmware`] (`kontroll`): firmwarens version, kortvariant och krav.
//! - [`robot`] (`robot`): det robotd gör: söker, hämtar, kontrollerar, säkerhetskopierar,
//!   flashar med openocd och återställer om det går fel.
//!
//! ORIGINALET ligger i autorobot (SentinAnt), `robotuppdatering/`. robot-control har en
//! ordagrann kopia. Se Cargo.toml.

pub mod protokoll;

#[cfg(feature = "kontroll")]
pub mod beskrivning;
#[cfg(feature = "kontroll")]
pub mod firmware;

#[cfg(feature = "robot")]
pub mod robot;

/// Sant i ett bygge som godkänner provnyckeln. Ett sådant får aldrig installeras.
pub const PROVBYGGE: bool = cfg!(feature = "provnyckel");

/// Märket som bara finns i programfilen på ett provbygge. Installationsskripten letar efter
/// texten i filen (utan att starta den) och vägrar installera ett program som har den.
#[cfg(feature = "provnyckel")]
#[used]
pub static PROVMARKE: &str = "ROBOTUPPDATERING-PROVBYGGE-MED-PROVNYCKEL";
