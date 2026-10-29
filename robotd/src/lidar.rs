//! Lidarstoppet och lidarns vy, för Robotstyrning.
//!
//! Själva stoppet sitter i Car_Client (alla körkommandon går den vägen), som skriver sitt
//! läge till `/tmp/lidarstopp_status`: `<gasfaktor 0..1> <unix-s> <fram m eller -> <text>`.
//! Vyn ovanifrån skriver lidarenheten (autorobot) till `/run/lidar_vy` (JSON, se
//! [`relay_protocol::LidarVy`]). robotd läser bara filerna och skickar vidare i statusen.

use relay_protocol::LidarVy;
use std::time::{SystemTime, UNIX_EPOCH};

pub const STOPP_FIL: &str = "/tmp/lidarstopp_status";
pub const VY_FIL: &str = "/run/lidar_vy";

/// Äldre än så här: Car_Client respektive lidarenheten hänger eller är borta.
const MAX_ALDER_S: f64 = 2.0;

fn nu() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// (gasfaktor, text). `None` = inget lidarstopp på roboten. Är filen gammal svarar
/// Car_Client inte, och då står det så i texten.
pub fn read_stopp() -> Option<(f32, Option<String>)> {
    parse_stopp(&std::fs::read_to_string(STOPP_FIL).ok()?, nu())
}

fn parse_stopp(text: &str, nu: f64) -> Option<(f32, Option<String>)> {
    let mut d = text.trim_end().splitn(4, ' ');
    let gas: f32 = d.next()?.parse().ok()?;
    let tid: f64 = d.next()?.parse().ok()?;
    let _fram = d.next();
    if nu - tid > MAX_ALDER_S {
        return Some((0.0, Some("Car_Client svarar inte".into())));
    }
    let t = d.next().map(str::trim).filter(|t| !t.is_empty()).map(|t| t.replace('.', ","));
    Some((gas.clamp(0.0, 1.0), t))
}

/// Vyn, om lidarenheten skrivit den nyss.
pub fn read_vy() -> Option<LidarVy> {
    parse_vy(&std::fs::read_to_string(VY_FIL).ok()?, nu())
}

fn parse_vy(text: &str, nu: f64) -> Option<LidarVy> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    if nu - v.get("unix")?.as_f64()? > MAX_ALDER_S {
        return None;
    }
    serde_json::from_value(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stoppet() {
        assert_eq!(parse_stopp("1.00 100.0 - \n", 100.5), Some((1.0, None)));
        assert_eq!(
            parse_stopp("0.00 100.0 0.80 något 0.8 m framför roboten\n", 100.5),
            Some((0.0, Some("något 0,8 m framför roboten".into())))
        );
        assert_eq!(parse_stopp("0.40 100.0 2.20 något 2.2 m framför roboten\n", 101.0).unwrap().0, 0.4);
        assert_eq!(parse_stopp("1.00 100.0 - \n", 105.0), Some((0.0, Some("Car_Client svarar inte".into()))));
        assert_eq!(parse_stopp("skräp", 0.0), None);
    }

    #[test]
    fn vyn() {
        let t = r##"{"unix":100.0,"cell":0.25,"x0":-2.5,"x1":7.5,"y":6.0,"robot":[0.23,1.65,0.675],"fram":null,"sikt":1.0,"rader":[" .#"]}"##;
        let v = parse_vy(t, 100.5).unwrap();
        assert_eq!((v.cell, v.robot[1], v.fram), (0.25, 1.65, None));
        assert_eq!(v.rader, vec![" .#".to_string()]);
        assert_eq!(parse_vy(t, 103.0), None);
    }
}
