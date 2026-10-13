//! VESC:s öppna CAN-protokoll (samma som används rakt av av bl.a. rise_sdvp).
//! CAN-ID:t kodar VESC:ens ID + vilket kommando det gäller (standard-mönstret:
//! `(command_id << 8) | vesc_id`).

pub const CAN_PACKET_SET_DUTY: u8 = 0;
pub const CAN_PACKET_SET_CURRENT: u8 = 1;
pub const CAN_PACKET_SET_RPM: u8 = 3;
pub const CAN_PACKET_STATUS: u8 = 9; // rpm, current, duty
pub const CAN_PACKET_STATUS_5: u8 = 27; // temp, input_voltage

#[derive(Debug, Clone, Copy)]
pub struct VescStatus {
    pub vesc_id: u8,
    pub rpm: f32,
    pub current: f32,
    pub duty: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct VescStatus5 {
    pub vesc_id: u8,
    pub temp_c: f32,
    pub input_voltage: f32,
}

pub fn make_set_rpm_frame(vesc_id: u8, rpm: f32) -> (u32, [u8; 8]) {
    let can_id = ((CAN_PACKET_SET_RPM as u32) << 8) | vesc_id as u32;
    let mut data = [0u8; 8];
    data[0..4].copy_from_slice(&(rpm as i32).to_be_bytes());
    (can_id | 0x8000_0000, data) // extended frame
}

pub fn make_set_current_frame(vesc_id: u8, current_amps: f32) -> (u32, [u8; 8]) {
    let can_id = ((CAN_PACKET_SET_CURRENT as u32) << 8) | vesc_id as u32;
    let mut data = [0u8; 8];
    data[0..4].copy_from_slice(&((current_amps * 1000.0) as i32).to_be_bytes());
    (can_id | 0x8000_0000, data)
}

pub fn parse_status(can_id: u32, data: &[u8]) -> Option<VescStatus> {
    let cmd = (can_id >> 8) as u8;
    if cmd != CAN_PACKET_STATUS || data.len() < 8 {
        return None;
    }
    let vesc_id = (can_id & 0xff) as u8;
    let rpm = i32::from_be_bytes(data[0..4].try_into().ok()?) as f32;
    let current = i16::from_be_bytes(data[4..6].try_into().ok()?) as f32 / 10.0;
    let duty = i16::from_be_bytes(data[6..8].try_into().ok()?) as f32 / 1000.0;
    Some(VescStatus {
        vesc_id,
        rpm,
        current,
        duty,
    })
}

pub fn parse_status5(can_id: u32, data: &[u8]) -> Option<VescStatus5> {
    let cmd = (can_id >> 8) as u8;
    if cmd != CAN_PACKET_STATUS_5 || data.len() < 8 {
        return None;
    }
    let vesc_id = (can_id & 0xff) as u8;
    let temp_c = i16::from_be_bytes(data[0..2].try_into().ok()?) as f32 / 10.0;
    let input_voltage = i16::from_be_bytes(data[4..6].try_into().ok()?) as f32 / 10.0;
    Some(VescStatus5 {
        vesc_id,
        temp_c,
        input_voltage,
    })
}

/// Uppskatta batteriprocent från spänning över 4x 12V litiumblock i serie
/// (~48V nominellt). Grov, linjär uppskattning tills något bättre finns —
/// litiumcellers urladdningskurva är inte linjär, men det räcker för en
/// ungefärlig indikator i hörnet av skärmen.
pub fn estimate_battery_percent(input_voltage: f32) -> f32 {
    const EMPTY_V: f32 = 42.0; // ungefärlig tom-spänning för 4x12V litium i serie
    const FULL_V: f32 = 54.4; // ungefärlig fulladdad spänning
    ((input_voltage - EMPTY_V) / (FULL_V - EMPTY_V) * 100.0).clamp(0.0, 100.0)
}

/// `CMD_GET_VESC_STATUS` (140): nytt kommando i CarController-firmware
/// (`firmware/0001-cmd-get-vesc-status.patch`). Kortet svarar med de VESC som
/// det hört CAN-status från. Bara en läsfråga, inget skickas på CAN-bussen.
/// (Den äldre vägen via `CMD_VESC_FWD`/`COMM_PING_CAN` fungerar inte: kortet
/// skickar bara vidare till ett redan valt VESC-ID.)
pub const CMD_GET_VESC_STATUS: u8 = 140;
/// Längd på en post i svaret: id(1) + ålder ms(2) + rpm(4) + ström*10(2) + duty*1000(2).
const VESC_STATUS_ENTRY_LEN: usize = 11;
/// Äldre än så räknas VESC:en som tyst. VESC skickar status många gånger per
/// sekund när CAN-status är påslagen.
pub const VESC_STATUS_FRESH_MS: u16 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VescStatusEntry {
    pub can_id: u8,
    pub age_ms: u16,
    pub rpm: i32,
    pub current_a: f32,
    pub duty: f32,
}

impl VescStatusEntry {
    pub fn is_fresh(&self) -> bool {
        self.age_ms < VESC_STATUS_FRESH_MS
    }
}

pub fn make_vesc_status_request(car_id: u8) -> Vec<u8> {
    vec![car_id, CMD_GET_VESC_STATUS]
}

/// Tolka ett paket från Car_Client som svar på `CMD_GET_VESC_STATUS`. Andra
/// paket (tillstånd, NMEA…) ger `None`.
pub fn parse_vesc_status_reply(payload: &[u8]) -> Option<Vec<VescStatusEntry>> {
    let [_id, CMD_GET_VESC_STATUS, rest @ ..] = payload else {
        return None;
    };
    if rest.len() % VESC_STATUS_ENTRY_LEN != 0 {
        return None; // trasigt eller okänt format
    }
    Some(
        rest.chunks_exact(VESC_STATUS_ENTRY_LEN)
            .map(|c| VescStatusEntry {
                can_id: c[0],
                age_ms: u16::from_be_bytes([c[1], c[2]]),
                rpm: i32::from_be_bytes([c[3], c[4], c[5], c[6]]),
                current_a: i16::from_be_bytes([c[7], c[8]]) as f32 / 10.0,
                duty: i16::from_be_bytes([c[9], c[10]]) as f32 / 1000.0,
            })
            .collect(),
    )
}

/// `CMD_PING_VESC_CAN` (141, firmware 30.4): kortet pingar ID 0–253 på
/// CAN-bussen som VESC Tool gör och svarar med de som svarade. Hittar alla
/// VESC, även de som inte har CAN-status påslagen. Kortet är upptaget ~0,1 s.
pub const CMD_PING_VESC_CAN: u8 = 141;

pub fn make_vesc_ping_request(car_id: u8) -> Vec<u8> {
    vec![car_id, CMD_PING_VESC_CAN]
}

/// Svar på `CMD_PING_VESC_CAN`: `[id][141]` följt av ett VESC-ID per byte.
pub fn parse_vesc_ping_reply(payload: &[u8]) -> Option<Vec<u8>> {
    match payload {
        [_id, CMD_PING_VESC_CAN, ids @ ..] => Some(ids.to_vec()),
        _ => None,
    }
}

/// `CMD_TEST_SPIN_VESC` (142, firmware 30.4): kortet kör en VESC på 30 % duty
/// i 1 s och stoppar sedan själv. Svar `[id][142][vesc][1 = startat, 0 = upptaget]`.
pub const CMD_TEST_SPIN_VESC: u8 = 142;

pub fn make_test_spin_request(car_id: u8, vesc_id: u8) -> Vec<u8> {
    vec![car_id, CMD_TEST_SPIN_VESC, vesc_id]
}

pub fn parse_test_spin_reply(payload: &[u8]) -> Option<(u8, bool)> {
    match payload {
        [_id, CMD_TEST_SPIN_VESC, vesc, started, ..] => Some((*vesc, *started != 0)),
        _ => None,
    }
}

/// `CMD_GET_STATE` (120): styrkortets tillstånd, samma fråga som RControlStation
/// pollar. Vi läser bara fart, batterispänning, temperatur och felkod.
pub const CMD_GET_STATE: u8 = 120;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardState {
    /// Lutning åt sidan, framåt/bakåt och kurs (grader), från styrkortets IMU.
    pub roll_deg: f32,
    pub pitch_deg: f32,
    pub yaw_deg: f32,
    /// m/s, från styrkortets positionsberäkning.
    pub speed_ms: f32,
    /// Batterispänning (VESC:ns v_in).
    pub v_in: f32,
    pub temp_mos: f32,
    pub fault_code: u8,
}

pub fn make_state_request(car_id: u8) -> Vec<u8> {
    vec![car_id, CMD_GET_STATE]
}

/// Svarsformat enligt `commands.c` (CMD_GET_STATE): `[id][120][fw maj][fw min]`
/// och sedan float32 som i32 big-endian (värde*skala): roll, pitch, yaw,
/// accel×3, gyro×3, mag×3, px, py, speed (1e6), v_in (1e6), temp_mos (1e6),
/// felkod (u8), … Resten av paketet bryr vi oss inte om.
pub fn parse_state_reply(payload: &[u8]) -> Option<BoardState> {
    if payload.len() < 73 || payload[1] != CMD_GET_STATE {
        return None;
    }
    let f = |at: usize, scale: f32| {
        i32::from_be_bytes([payload[at], payload[at + 1], payload[at + 2], payload[at + 3]]) as f32 / scale
    };
    Some(BoardState {
        roll_deg: f(4, 1e6),
        pitch_deg: f(8, 1e6),
        yaw_deg: f(12, 1e6),
        speed_ms: f(60, 1e6),
        v_in: f(64, 1e6),
        temp_mos: f(68, 1e6),
        fault_code: payload[72],
    })
}

/// VESC:ns felkod (`mc_fault_code` i firmwarens datatypes.h) i klartext.
pub fn fault_text(code: u8) -> Option<String> {
    let s = match code {
        0 => return None,
        1 => "Överspänning – batterispänningen är för hög",
        2 => "Underspänning – batteriet är för lågt (ladda)",
        3 => "Fel i motordrivaren (DRV)",
        4 => "För hög motorström",
        5 => "VESC överhettad (transistorerna)",
        6 => "Motorn överhettad",
        n => return Some(format!("VESC-fel, kod {n}")),
    };
    Some(s.to_string())
}

/// `CMD_TERMINAL_CMD` (1): skicka ett terminalkommando till styrkortet, som
/// terminalen i RControlStation. Svaret kommer som `CMD_PRINTF` (0).
pub const CMD_TERMINAL_CMD: u8 = 1;
pub const CMD_PRINTF: u8 = 0;

pub fn make_terminal_cmd(car_id: u8, cmd: &str) -> Vec<u8> {
    let mut v = vec![car_id, CMD_TERMINAL_CMD];
    v.extend(cmd.as_bytes());
    v
}

/// Vinkelgivarens svar på terminalkommandot `vinkel` (RobAnt-firmware, adconv.c):
/// "Vinkelgivare: 2500 mV, vinkel 3.2 grader, OK". Ger (vinkel, givaren OK).
/// Andra utskrifter ger `None`.
pub fn parse_angle_printf(payload: &[u8]) -> Option<(f32, bool)> {
    let [_id, CMD_PRINTF, text @ ..] = payload else { return None };
    let text = String::from_utf8_lossy(text);
    let rest = text.split("Vinkelgivare:").nth(1)?;
    let after = rest.split("vinkel").nth(1)?;
    let deg: f32 = after.split_whitespace().next()?.trim_end_matches(',').parse().ok()?;
    Some((deg, rest.contains(", OK")))
}

/// `CMD_RC_CONTROL_ADV` (125): samma kommando som RControlStation skickar när
/// dosan styr (`PacketInterface::setRcControlAdvanced`). Kortet slår upp alla
/// aktuatorer med den aktiviteten (sparade på kortet via Confcommon → Write)
/// och skickar värdet till deras VESC i aktuatorns läge (duty/ström/rpm).
/// Bekräftat mot `commands.c` och körning på RobAnt 2026-09-23.
pub const CMD_RC_CONTROL_ADV: u8 = 125;

/// `[bil-ID][125][aktivitet][värde*1e4 som i32 big-endian]`, byte för byte som
/// `buffer_append_double32(value, 1e4)` i RControlStation.
pub fn make_rc_control_adv(car_id: u8, activity: u8, value: f32) -> Vec<u8> {
    let raw = (value as f64 * 1e4) as i32;
    let mut v = vec![car_id, CMD_RC_CONTROL_ADV, activity];
    v.extend(raw.to_be_bytes());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pingsvar_tolkas_och_andra_paket_ignoreras() {
        assert_eq!(parse_vesc_ping_reply(&[6, 141, 2, 16, 94, 105, 125]), Some(vec![2, 16, 94, 105, 125]));
        assert_eq!(parse_vesc_ping_reply(&[6, 141]), Some(vec![]));
        assert_eq!(parse_vesc_ping_reply(&[6, 140, 2]), None);
        assert_eq!(make_vesc_ping_request(6), vec![6, 141]);
    }

    #[test]
    fn provsnurr_fraga_och_svar() {
        assert_eq!(make_test_spin_request(6, 105), vec![6, 142, 105]);
        assert_eq!(parse_test_spin_reply(&[6, 142, 105, 1]), Some((105, true)));
        assert_eq!(parse_test_spin_reply(&[6, 142, 105, 0]), Some((105, false)));
        assert_eq!(parse_test_spin_reply(&[6, 142, 105]), None);
        assert_eq!(parse_test_spin_reply(&[6, 141, 105, 1]), None);
    }

    /// Bygg ett svar som firmware skulle göra: [id][140] + poster.
    fn entry_bytes(id: u8, age: u16, rpm: i32, cur10: i16, duty1000: i16) -> Vec<u8> {
        let mut v = vec![id];
        v.extend(age.to_be_bytes());
        v.extend(rpm.to_be_bytes());
        v.extend(cur10.to_be_bytes());
        v.extend(duty1000.to_be_bytes());
        v
    }

    #[test]
    fn statusfraga_har_ratt_form() {
        assert_eq!(make_vesc_status_request(4), vec![4, 140]);
    }

    #[test]
    fn vinkelsvaret_tolkas() {
        let mut p = vec![4, CMD_PRINTF];
        p.extend(b"Vinkelgivare: 2350 mV, vinkel -10.8 grader, OK\n");
        assert_eq!(parse_angle_printf(&p), Some((-10.8, true)));
        let mut p = vec![4, CMD_PRINTF];
        p.extend(b"Vinkelgivare: 40 mV, vinkel 0.0 grader, FEL (ingen magnet/kabel?)\n");
        assert_eq!(parse_angle_printf(&p), Some((0.0, false)));
        let mut p = vec![4, CMD_PRINTF];
        p.extend(b"Activity 10 -> 2 actuator(s)");
        assert_eq!(parse_angle_printf(&p), None);
        assert_eq!(make_terminal_cmd(4, "vinkel"), vec![4, 1, b'v', b'i', b'n', b'k', b'e', b'l']);
    }

    #[test]
    fn felkoder_i_klartext() {
        assert_eq!(fault_text(0), None);
        assert!(fault_text(2).unwrap().contains("Underspänning"));
        assert_eq!(fault_text(42).unwrap(), "VESC-fel, kod 42");
    }

    #[test]
    fn statussvar_med_tva_vesc_tolkas() {
        let mut p = vec![4, 140];
        p.extend(entry_bytes(28, 120, -1500, -35, 250));
        p.extend(entry_bytes(36, 5_000, 800, 12, -10));
        let r = parse_vesc_status_reply(&p).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].can_id, 28);
        assert_eq!(r[0].rpm, -1500);
        assert!((r[0].current_a - (-3.5)).abs() < 1e-6);
        assert!((r[0].duty - 0.25).abs() < 1e-6);
        assert!(r[0].is_fresh());
        assert_eq!(r[1].can_id, 36);
        assert!(!r[1].is_fresh()); // 5 s gammal = tyst
    }

    #[test]
    fn statussvar_utan_vesc_ar_tomt_men_giltigt() {
        assert_eq!(parse_vesc_status_reply(&[4, 140]), Some(vec![]));
    }

    #[test]
    fn andra_paket_och_trasiga_svar_ignoreras() {
        assert_eq!(parse_vesc_status_reply(&[4, 120, 0, 0]), None); // tillståndssvar
        assert_eq!(parse_vesc_status_reply(&[4, 63, 1, 2, 3]), None); // NMEA
        assert_eq!(parse_vesc_status_reply(&[4]), None);
        assert_eq!(parse_vesc_status_reply(&[]), None);
        assert_eq!(parse_vesc_status_reply(&[4, 140, 28, 0, 1]), None); // trunkerad post
    }

    #[test]
    fn styrpaket_matchar_rcontrolstation() {
        // 0.15 * 1e4 = 1500 = 0x05DC
        assert_eq!(make_rc_control_adv(4, 10, 0.15), vec![4, 125, 10, 0, 0, 0x05, 0xDC]);
        // -0.15 -> -1500 = 0xFFFFFA24
        assert_eq!(make_rc_control_adv(4, 11, -0.15), vec![4, 125, 11, 0xFF, 0xFF, 0xFA, 0x24]);
        assert_eq!(make_rc_control_adv(4, 10, 0.0), vec![4, 125, 10, 0, 0, 0, 0]);
    }

    #[test]
    fn tillstandssvar_tolkas() {
        let mut p = vec![4, CMD_GET_STATE, 10, 5];
        p.extend(3_500_000i32.to_be_bytes()); // roll 3.5°
        p.extend((-2_000_000i32).to_be_bytes()); // pitch -2°
        p.extend(123_000_000i32.to_be_bytes()); // yaw 123°
        for _ in 0..11 {
            p.extend(0i32.to_be_bytes()); // accel, gyro, mag, px, py
        }
        p.extend(1_250_000i32.to_be_bytes()); // speed 1.25 m/s
        p.extend(52_600_000i32.to_be_bytes()); // v_in 52.6 V
        p.extend(31_500_000i32.to_be_bytes()); // temp 31.5 °C
        p.push(0); // felkod
        p.extend([0u8; 40]); // resten
        let st = parse_state_reply(&p).unwrap();
        assert!((st.speed_ms - 1.25).abs() < 1e-4);
        assert!((st.v_in - 52.6).abs() < 1e-4);
        assert!((st.temp_mos - 31.5).abs() < 1e-4);
        assert_eq!(st.fault_code, 0);
        assert!((st.roll_deg - 3.5).abs() < 1e-4);
        assert!((st.pitch_deg + 2.0).abs() < 1e-4);
        assert!((st.yaw_deg - 123.0).abs() < 1e-4);
        assert_eq!(parse_state_reply(&p[..60]), None); // för kort
        assert_eq!(parse_state_reply(&[4, 140]), None);
    }

    #[test]
    fn battery_estimate_clampar() {
        assert_eq!(estimate_battery_percent(30.0), 0.0);
        assert_eq!(estimate_battery_percent(60.0), 100.0);
    }
}
