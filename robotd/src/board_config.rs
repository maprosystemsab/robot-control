//! Styrkortets aktuatorinställningar (vilken VESC som gör vad), samma sak som
//! Confcommon-fliken i RControlStation: Read → ändra → Write.
//!
//! Aktuatorerna ligger mitt i styrkortets huvudinställning (`MAIN_CONFIG`),
//! som bara går att läsa (`CMD_GET_MAIN_CONFIG`) och skriva
//! (`CMD_SET_MAIN_CONFIG`) i sin helhet. Båda använder exakt samma byteformat
//! (`commands.c`), så robotd läser hela inställningen, byter BARA
//! aktuatorbytena och skriver tillbaka resten orört, byte för byte. Inget
//! annat fält behöver tolkas.
//!
//! Format fram till aktuatorerna (efter `[bil-ID][kommando]`), från `commands.c`:
//! 108 byte fasta fält (magnetometer, GPS, UWB, autopilot, loggfrekvens/-på),
//! loggnamnet (nollterminerad text), `log_mode_ext` (1), `log_uart_baud` (4),
//! fordonsflaggor och -tal (9 + 44 + 20). Sedan `actuators` (u16) och 4 ×
//! `{type, motorid, activity, mode}` (u16 var), därefter sensorer (2 + 32) och
//! reglerloopar (2 + 4 × 31).

use relay_protocol::{ActuatorConfig, ActuatorSlot};

pub const CMD_SET_MAIN_CONFIG: u8 = 77;
pub const CMD_GET_MAIN_CONFIG: u8 = 78;

/// Byte före loggnamnet.
const BEFORE_LOG_NAME: usize = 108;
/// Byte mellan loggnamnets nollbyte och aktuatorerna.
const LOG_NAME_TO_ACTUATORS: usize = 1 + 4 + 9 + 44 + 20;
const ACTUATOR_BLOCK: usize = 2 + 4 * 8;
/// Byte efter aktuatorerna: sensorer + reglerloopar.
const AFTER_ACTUATORS: usize = (2 + 4 * 8) + (2 + 4 * 31);
pub const SLOTS: usize = 4;

/// Hela inställningen som den kom (utan `[id][kommando]`), utan att tolka den. Används
/// för säkerhetskopian före en firmwareuppdatering.
pub fn parse_raw_read_reply(payload: &[u8]) -> Option<Vec<u8>> {
    (payload.len() > 2 && payload[1] == CMD_GET_MAIN_CONFIG).then(|| payload[2..].to_vec())
}

/// Skriver tillbaka en inställning precis som den lästes (`body` från [`parse_raw_read_reply`]).
pub fn make_raw_write_request(car_id: u8, body: &[u8]) -> Vec<u8> {
    let mut p = vec![car_id, CMD_SET_MAIN_CONFIG];
    p.extend_from_slice(body);
    p
}

pub fn make_read_request(car_id: u8) -> Vec<u8> {
    vec![car_id, CMD_GET_MAIN_CONFIG]
}

/// Var aktuatorblocket börjar i inställningen (utan `[id][kommando]`), och
/// kontroll att längden stämmer exakt med formatet. Stämmer den inte är det
/// en annan firmwareversion — då rör vi ingenting.
fn actuator_offset(body: &[u8]) -> Result<usize, String> {
    let name_len = body
        .get(BEFORE_LOG_NAME..)
        .and_then(|rest| rest.iter().position(|&b| b == 0))
        .ok_or("styrkortets inställning är för kort (hittar inte loggnamnet)")?;
    let offset = BEFORE_LOG_NAME + name_len + 1 + LOG_NAME_TO_ACTUATORS;
    let expected = offset + ACTUATOR_BLOCK + AFTER_ACTUATORS;
    if body.len() != expected {
        return Err(format!(
            "styrkortets inställning har oväntad längd ({} byte, väntade {expected}) — \
             annan firmwareversion? Ingenting ändrat.",
            body.len()
        ));
    }
    Ok(offset)
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_be_bytes([b[at], b[at + 1]])
}

/// Tolka svaret på `CMD_GET_MAIN_CONFIG` (hela paketet, med `[id][78]`).
/// Ger inställningens byte (att skriva tillbaka) och aktuatorerna.
pub fn parse_read_reply(payload: &[u8]) -> Option<Result<(Vec<u8>, ActuatorConfig), String>> {
    if payload.len() < 2 || payload[1] != CMD_GET_MAIN_CONFIG {
        return None;
    }
    let body = payload[2..].to_vec();
    Some(actuator_offset(&body).map(|at| {
        let slots = (0..SLOTS)
            .map(|i| {
                let s = at + 2 + i * 8;
                ActuatorSlot {
                    kind: u16_at(&body, s),
                    vesc_id: u16_at(&body, s + 2),
                    activity: u16_at(&body, s + 4),
                    mode: u16_at(&body, s + 6),
                }
            })
            .collect();
        let cfg = ActuatorConfig { count: u16_at(&body, at), slots };
        (body, cfg)
    }))
}

/// Är skrivsvaret (kvittens `[id][77]`) det här paketet?
pub fn is_write_ack(payload: &[u8]) -> bool {
    payload.len() == 2 && payload[1] == CMD_SET_MAIN_CONFIG
}

/// Bygg `CMD_SET_MAIN_CONFIG` av en nyss läst inställning där bara
/// aktuatorbytena byts ut.
pub fn make_write_request(car_id: u8, read_body: &[u8], cfg: &ActuatorConfig) -> Result<Vec<u8>, String> {
    validate(cfg)?;
    let at = actuator_offset(read_body)?;
    let mut body = read_body.to_vec();
    body[at..at + 2].copy_from_slice(&cfg.count.to_be_bytes());
    for (i, slot) in cfg.slots.iter().enumerate() {
        let s = at + 2 + i * 8;
        body[s..s + 2].copy_from_slice(&slot.kind.to_be_bytes());
        body[s + 2..s + 4].copy_from_slice(&slot.vesc_id.to_be_bytes());
        body[s + 4..s + 6].copy_from_slice(&slot.activity.to_be_bytes());
        body[s + 6..s + 8].copy_from_slice(&slot.mode.to_be_bytes());
    }
    let mut out = vec![car_id, CMD_SET_MAIN_CONFIG];
    out.extend(body);
    Ok(out)
}

/// Rimlighetskontroll innan något skrivs till styrkortet.
pub fn validate(cfg: &ActuatorConfig) -> Result<(), String> {
    if cfg.slots.len() != SLOTS {
        return Err(format!("det ska vara {SLOTS} aktuatorplatser"));
    }
    if cfg.count as usize > SLOTS {
        return Err(format!("antal aktuatorer får vara högst {SLOTS}"));
    }
    for (i, s) in cfg.slots.iter().take(cfg.count as usize).enumerate() {
        let n = i + 1;
        if s.kind > 1 {
            return Err(format!("aktuator {n}: okänd typ {}", s.kind));
        }
        if s.kind == 0 && s.vesc_id > 254 {
            return Err(format!("aktuator {n}: VESC-ID {} är inte ett giltigt CAN-ID (0–254)", s.vesc_id));
        }
        // Firmware kör bara duty (0), ström (1) och rpm (3) — se motor_set_vesc_value.
        if s.kind == 0 && ![0, 1, 3].contains(&s.mode) {
            return Err(format!("aktuator {n}: läge {} stöds inte (duty, ström eller rpm)", s.mode));
        }
    }
    Ok(())
}

/// Testhjälp: en inställning byggd som firmware gör, med igenkännbara byte
/// runt aktuatorerna. Används också av länkuppgiftens tester.
#[cfg(test)]
pub fn fake_body(log_name: &str, slots: &[(u16, u16, u16, u16)], count: u16) -> Vec<u8> {
    tests::fake_body(log_name, slots, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bygger en inställning precis som firmware (`CMD_GET_MAIN_CONFIG`), med
    /// igenkännbara byte i alla fält runt aktuatorerna.
    pub fn fake_body(log_name: &str, slots: &[(u16, u16, u16, u16)], count: u16) -> Vec<u8> {
        let mut b: Vec<u8> = (0..BEFORE_LOG_NAME as u32).map(|i| (i % 200 + 1) as u8).collect();
        b.extend(log_name.as_bytes());
        b.push(0);
        b.extend((0..LOG_NAME_TO_ACTUATORS as u32).map(|i| (i % 7 + 30) as u8));
        b.extend(count.to_be_bytes());
        for &(t, id, act, mode) in slots {
            for v in [t, id, act, mode] {
                b.extend(v.to_be_bytes());
            }
        }
        b.extend((0..AFTER_ACTUATORS as u32).map(|i| (i % 11 + 100) as u8));
        b
    }

    fn robant_slots() -> Vec<(u16, u16, u16, u16)> {
        vec![(0, 28, 10, 0), (0, 36, 10, 0), (0, 76, 11, 0), (0, 0, 0, 0)]
    }

    fn reply(body: &[u8]) -> Vec<u8> {
        let mut p = vec![4, CMD_GET_MAIN_CONFIG];
        p.extend_from_slice(body);
        p
    }

    #[test]
    fn aktuatorer_hittas_oavsett_loggnamnets_langd() {
        for name in ["", "log", "ett_mycket_langre_loggnamn"] {
            let body = fake_body(name, &robant_slots(), 3);
            let (_, cfg) = parse_read_reply(&reply(&body)).unwrap().unwrap();
            assert_eq!(cfg.count, 3, "loggnamn {name:?}");
            assert_eq!(cfg.slots[0], ActuatorSlot { kind: 0, vesc_id: 28, activity: 10, mode: 0 });
            assert_eq!(cfg.slots[2].vesc_id, 76);
            assert_eq!(cfg.slots[2].activity, 11);
        }
    }

    #[test]
    fn skrivning_andrar_bara_aktuatorbytena() {
        let body = fake_body("log", &robant_slots(), 3);
        let (read_body, mut cfg) = parse_read_reply(&reply(&body)).unwrap().unwrap();
        cfg.slots[2].vesc_id = 77;
        cfg.slots[3] = ActuatorSlot { kind: 0, vesc_id: 90, activity: 7, mode: 1 };
        cfg.count = 4;
        let out = make_write_request(4, &read_body, &cfg).unwrap();
        assert_eq!(&out[..2], &[4, CMD_SET_MAIN_CONFIG]);
        let new_body = &out[2..];
        assert_eq!(new_body.len(), body.len());

        // Allt utom aktuatorblocket är orört.
        let at = actuator_offset(&body).unwrap();
        assert_eq!(&new_body[..at], &body[..at]);
        assert_eq!(&new_body[at + ACTUATOR_BLOCK..], &body[at + ACTUATOR_BLOCK..]);

        // Och det nya går att läsa tillbaka.
        let (_, back) = parse_read_reply(&reply(new_body)).unwrap().unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn fel_langd_ror_ingenting() {
        let mut body = fake_body("log", &robant_slots(), 3);
        body.push(0); // en byte för mycket = annat format
        assert!(parse_read_reply(&reply(&body)).unwrap().is_err());
        let cfg = ActuatorConfig { count: 0, slots: vec![ActuatorSlot::default(); 4] };
        assert!(make_write_request(4, &body, &cfg).is_err());
        assert!(parse_read_reply(&reply(&body[..50])).unwrap().is_err());
    }

    #[test]
    fn andra_paket_ar_inte_inställningssvar() {
        assert!(parse_read_reply(&[4, 120, 1, 2]).is_none());
        assert!(is_write_ack(&[4, CMD_SET_MAIN_CONFIG]));
        assert!(!is_write_ack(&[4, CMD_SET_MAIN_CONFIG, 0]));
    }

    #[test]
    fn orimliga_varden_stoppas() {
        let ok = ActuatorSlot { kind: 0, vesc_id: 28, activity: 10, mode: 0 };
        let mk = |s: ActuatorSlot, count| ActuatorConfig { count, slots: vec![s, ok, ok, ok] };
        assert!(validate(&mk(ok, 4)).is_ok());
        assert!(validate(&mk(ok, 5)).is_err());
        assert!(validate(&mk(ActuatorSlot { vesc_id: 300, ..ok }, 1)).is_err());
        assert!(validate(&mk(ActuatorSlot { mode: 2, ..ok }, 1)).is_err());
        assert!(validate(&mk(ActuatorSlot { kind: 5, ..ok }, 1)).is_err());
        // Oanvända platser (efter count) kontrolleras inte.
        assert!(validate(&mk(ActuatorSlot { vesc_id: 300, ..ok }, 0)).is_ok());
        let short = ActuatorConfig { count: 1, slots: vec![ok] };
        assert!(validate(&short).is_err());
    }
}
