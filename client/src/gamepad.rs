//! PS4-kontroll via gilrs. Styrschema (beslutat):
//!   - Vänster spak upp/ner -> gas/broms (samma som RControlStation)
//!   - Höger spak vänster/höger -> styrning
//!   - L1/L2 -> tillval A (t.ex. lastarm)
//!   - R1/R2 -> tillval B (t.ex. tilt)
//!   - O (håll in) -> bandvagnsvändning: höger spak vänder på stället
//! Klienten skickar en generell signal; robotd översätter till rätt VESC
//! utifrån sparad rollprofil, så klienten aldrig behöver känna till VESC-ID:n.

use gilrs::{Axis, Button, Gilrs};

/// Under det här värdet räknas spaken som "i neutralläge". Utan detta
/// visade riktiga tester (2026-09-20, PS4-dosa över Bluetooth) att
/// spakarna aldrig går exakt till 0.0 — resterande brus/drift på
/// ±0.12–0.16 syntes i loggen även med släppt spak. Två konkreta problem
/// det orsakar utan dödzon: krypning på riktig hydraulik i "vila", och —
/// allvarligare — att 5-minuters-idle-timeouten i klienten (som bara
/// räknar |värde| > 0.02 som aktivitet) troligen ALDRIG skulle lösa ut,
/// eftersom bruset ensamt redan ligger över den tröskeln.
const STICK_DEADZONE: f32 = 0.12;

/// Nollställ värden under dödzonen, skala om resten linjärt så att
/// utslaget fortfarande går hela vägen till ±1.0 strax utanför dödzonen
/// (annars skulle t.ex. 0.13 plötsligt vara "nästan fullt utslag").
fn apply_deadzone(value: f32) -> f32 {
    let abs = value.abs();
    if abs < STICK_DEADZONE {
        0.0
    } else {
        let sign = value.signum();
        sign * ((abs - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)).min(1.0)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GamepadState {
    pub throttle: f32,   // -1.0..=1.0, vänster spak Y (som i RControlStation)
    pub steering: f32,   // -1.0..=1.0, höger spak X
    pub accessory_a: f32, // L1/L2, -1.0..=1.0
    pub accessory_b: f32, // R1/R2, -1.0..=1.0
    pub pivot: bool,      // O hålls in: vända på stället
    pub connected: bool,
}

/// Hur troligt det är att enheten är själva dosan (högst vinner).
fn dosa_poang(namn: &str, har_vanster_spak: bool) -> i32 {
    let n = namn.to_lowercase();
    let mut p = 0;
    if n.contains("motion sensor") || n.contains("touchpad") || n.contains("accelerometer") {
        p -= 100;
    }
    if har_vanster_spak {
        p += 10;
    }
    p
}

pub struct GamepadReader {
    gilrs: Gilrs,
}

impl GamepadReader {
    pub fn new() -> Result<Self, String> {
        Gilrs::new()
            .map(|gilrs| Self { gilrs })
            .map_err(|e| format!("kunde inte starta gilrs: {e}"))
    }

    /// Anropas varje bildruta/tick. Pumpar events och returnerar senaste läge
    /// för första anslutna kontroll (bara en förare i taget, så en kontroll
    /// räcker).
    pub fn poll(&mut self) -> GamepadState {
        while self.gilrs.next_event().is_some() {}

        // En PS4-dosa på USB syns som flera enheter i Linux ("Wireless Controller",
        // "... Motion Sensors", "... Touchpad"), och en HP-dator kan ha en
        // accelerometer som "joystick". Förut togs den första: blev det
        // rörelsesensorn räknades dosan som ansluten men spakarna var alltid 0
        // (kunden med RobAnt 3, 2026-10-16). Välj en riktig dosa med spakar.
        let Some((_id, gp)) = self
            .gilrs
            .gamepads()
            .filter(|(_, g)| g.is_connected())
            .max_by_key(|(_, g)| dosa_poang(g.name(), g.axis_data(Axis::LeftStickY).is_some() || g.axis_code(Axis::LeftStickY).is_some()))
        else {
            return GamepadState {
                connected: false,
                ..Default::default()
            };
        };

        let throttle = apply_deadzone(gp.axis_data(Axis::LeftStickY).map(|a| a.value()).unwrap_or(0.0));
        let steering = apply_deadzone(gp.axis_data(Axis::RightStickX).map(|a| a.value()).unwrap_or(0.0));

        let l2 = gp.button_data(Button::LeftTrigger2).map(|b| b.value()).unwrap_or(0.0);
        let l1 = if gp.is_pressed(Button::LeftTrigger) { 1.0 } else { 0.0 };
        let accessory_a = l1 - l2;

        let r2 = gp.button_data(Button::RightTrigger2).map(|b| b.value()).unwrap_or(0.0);
        let r1 = if gp.is_pressed(Button::RightTrigger) { 1.0 } else { 0.0 };
        let accessory_b = r1 - r2;

        // O på PS4-dosan (östra knappen), samma som i RControlStation.
        let pivot = gp.is_pressed(Button::East);

        GamepadState {
            throttle,
            steering,
            accessory_a,
            accessory_b,
            pivot,
            connected: gp.is_connected(),
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dosa_valjs_fore_rorelsesensorn() {
        use super::dosa_poang;
        assert!(dosa_poang("Wireless Controller", true) > dosa_poang("Wireless Controller Motion Sensors", false));
        assert!(dosa_poang("Sony Interactive Entertainment Wireless Controller", true)
            > dosa_poang("Sony Interactive Entertainment Wireless Controller Touchpad", true));
        assert!(dosa_poang("Xbox Wireless Controller", true) > dosa_poang("ST LIS3LV02DL Accelerometer", false));
    }

    use super::*;

    #[test]
    fn brus_inom_dodzonen_blir_noll() {
        assert_eq!(apply_deadzone(0.0), 0.0);
        assert_eq!(apply_deadzone(0.1), 0.0);
        assert_eq!(apply_deadzone(-0.1), 0.0);
        // Just under gränsen ska fortfarande nollas.
        assert_eq!(apply_deadzone(0.119), 0.0);
    }

    #[test]
    fn full_utslag_forblir_helt() {
        assert!((apply_deadzone(1.0) - 1.0).abs() < 1e-6);
        assert!((apply_deadzone(-1.0) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn strax_utanfor_dodzonen_ar_nara_noll_inte_fullt_utslag() {
        let v = apply_deadzone(0.13);
        assert!(v > 0.0 && v < 0.05, "förväntade nästan noll, fick {v}");
    }
}
