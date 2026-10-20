//! Körhjälp i kamerabilden (2026-10-14): en röd linje som visar ungefär vart
//! roboten är på väg med nuvarande styrvinkel, som backkameran i en bil. Vid
//! bandvagnsvändning (O) visas en rotationssymbol i stället (2026-10-19; vyn
//! ovanifrån togs bort samtidigt, styrvinkeln står redan i statusraden).
//!
//! RobAnt styr med bakbenen: framhjulen står still och vridningscentrum ligger
//! på framaxelns linje. Framaxelns mitt går då i en cirkel med radien
//! R = axelavstånd / tan(styrvinkel). Kamerans placering och lutning är inte
//! inmätta, så perspektivet är en uppskattning — linjen visar riktningen, inte
//! exakt var hjulen hamnar.

use eframe::egui::{self, Color32, Pos2, Stroke};
use relay_protocol::StatusUpdate;

/// Avstånd mellan fram- och bakaxel (m). Uppskattat för RobAnt.
const WHEELBASE_M: f32 = 1.2;
/// Avståndet mellan linjerna för vänster och höger hjul (m): RobAnt är 1,35 m bred,
/// linjerna ska gå vid däcken (önskemål 2026-10-20, förut 1,0).
const TRACK_M: f32 = 1.3;
/// Största styrvinkel (grader) när bara procent finns (äldre robotd).
const MAX_STEER_DEG: f32 = 30.0;
/// Styrutslag (procent) under vilket linjerna visas raka.
const STRAIGHT_PERCENT: f32 = 9.0;
/// ...och under så här många grader, eftersom styrningen stannar inom ±1° från
/// spakens läge (firmwarens dödband) och 1° kan vara 4 % på en robot med litet utslag.
const STRAIGHT_DEG: f32 = 2.7;
/// Hur långt fram linjen ritas (m).
const PATH_LEN_M: f32 = 4.0;
/// Horisonten i bilden, som andel av höjden uppifrån.
const HORIZON: f32 = 0.42;
/// Kamerans höjd över marken i bildens skala (större = linjerna börjar längre ner).
const CAM_HEIGHT: f32 = 1.0;

const PATH: Color32 = Color32::from_rgb(255, 60, 50);
const PATH_FAINT: Color32 = Color32::from_rgba_premultiplied(160, 30, 25, 160);

/// Styrvinkel i grader (+ = höger) från status: givarens grader i första hand,
/// annars procent av största utslag. `None` om roboten inte skickar någon.
pub fn steering_deg(s: Option<&StatusUpdate>) -> Option<f32> {
    let s = s?;
    let deg = s.steering_deg.or_else(|| s.steering_percent.map(|p| p / 100.0 * MAX_STEER_DEG))?;
    // Under 9 % utslag (ca 2,7°) är det i praktiken rakt: visa raka linjer i stället för
    // att de vickar för varje grad givaren rör sig (önskemål 2026-10-15).
    let pct = s.steering_percent.unwrap_or(deg / MAX_STEER_DEG * 100.0);
    Some(if pct.abs() < STRAIGHT_PERCENT || deg.abs() < STRAIGHT_DEG { 0.0 } else { deg })
}

/// Punkt på marken (x åt höger, z framåt, meter, från framaxelns mitt) längs en
/// bana med avståndet `lateral` från mitten, `s` meter fram, vid styrvinkeln `deg`.
pub fn path_point(deg: f32, lateral: f32, s: f32) -> (f32, f32) {
    let k = deg.to_radians().tan() / WHEELBASE_M; // krökning (1/m), + = höger
    if k.abs() < 1e-4 {
        return (lateral, s);
    }
    let r = 1.0 / k; // vridningscentrum i (r, 0)
    let rr = r - lateral; // det här hjulets radie (med tecken)
    let a = s * k; // vinkeln runt centrum
    (r - rr * a.cos(), rr * a.sin())
}

/// Markpunkt → bildpunkt med ett enkelt nålhålsperspektiv inom `dest`.
fn project(dest: egui::Rect, x: f32, z: f32) -> Option<Pos2> {
    let z = z + 1.2; // kameran sitter en bit bakom bildens nederkant
    if z < 0.3 {
        return None;
    }
    let f = dest.width() * 0.55;
    let horizon = dest.top() + dest.height() * HORIZON;
    let y = horizon + f * CAM_HEIGHT / z;
    let px = dest.center().x + f * x / z;
    (y <= dest.bottom() + 40.0).then(|| Pos2::new(px, y))
}

/// Röda färdlinjer över kamerabilden `dest`.
pub fn draw_path(painter: &egui::Painter, dest: egui::Rect, deg: f32) {
    let painter = painter.with_clip_rect(dest);
    let steps = 40;
    // I skarpa svängar ritas bara de första ~60° av kurvan, annars blir det en krok.
    let k = deg.to_radians().tan().abs() / WHEELBASE_M;
    let len = if k > 1e-4 { PATH_LEN_M.min(std::f32::consts::FRAC_PI_3 / k) } else { PATH_LEN_M };
    for (lateral, stroke) in [
        (-TRACK_M / 2.0, Stroke::new(7.0_f32, PATH)),
        (TRACK_M / 2.0, Stroke::new(7.0_f32, PATH)),
        (0.0, Stroke::new(3.0_f32, PATH_FAINT)),
    ] {
        let pts: Vec<Pos2> = (0..=steps)
            .filter_map(|i| {
                let s = len * i as f32 / steps as f32;
                let (x, z) = path_point(deg, lateral, s);
                project(dest, x, z)
            })
            .collect();
        if pts.len() >= 2 {
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
    // Tvärstreck varje meter, som avståndsmarkering.
    for m in [1.0_f32, 2.0, 3.0, 4.0].into_iter().filter(|m| *m <= len) {
        let a = path_point(deg, -TRACK_M / 2.0, m);
        let b = path_point(deg, TRACK_M / 2.0, m);
        if let (Some(a), Some(b)) = (project(dest, a.0, a.1), project(dest, b.0, b.1)) {
            painter.line_segment([a, b], Stroke::new(3.0_f32, PATH_FAINT));
        }
    }
}

/// Rotationssymbol mitt i kamerabilden `dest` vid bandvagnsvändning: en nästan
/// hel cirkel med pilspets. `dir` > 0 = medurs (höger), < 0 = moturs, 0 = ingen spets.
pub fn draw_pivot(painter: &egui::Painter, dest: egui::Rect, dir: f32) {
    let painter = painter.with_clip_rect(dest);
    let c = dest.center();
    let r = dest.height().min(dest.width()) * 0.14;
    let stroke = Stroke::new(9.0_f32, PATH);
    // Bågen från 30° till 330° (0° = uppåt, medurs), öppningen överst.
    let at = |deg: f32| {
        let (sin, cos) = deg.to_radians().sin_cos();
        c + egui::vec2(sin, -cos) * r
    };
    let pts: Vec<Pos2> = (0..=60).map(|i| at(30.0 + 300.0 * i as f32 / 60.0)).collect();
    painter.add(egui::Shape::line(pts, stroke));
    if dir != 0.0 {
        // Spetsen i bågens ände i färdriktningen, pekar längs tangenten.
        let end = if dir > 0.0 { 330.0_f32 } else { 30.0 };
        let tip = at(end);
        let (sin, cos) = end.to_radians().sin_cos();
        let tangent = egui::vec2(cos, sin) * dir.signum(); // medurs tangent
        let normal = egui::vec2(sin, -cos);
        let base = tip - tangent * 26.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip + tangent * 8.0, base + normal * 20.0, base - normal * 20.0],
            PATH,
            Stroke::NONE,
        ));
    }
    painter.text(
        c,
        egui::Align2::CENTER_CENTER,
        "VÄNDNING",
        egui::FontId::proportional(r * 0.28),
        PATH,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rakt_fram_ger_rak_linje() {
        let (x, z) = path_point(0.0, 0.5, 4.0);
        assert!((x - 0.5).abs() < 1e-5 && (z - 4.0).abs() < 1e-5);
    }

    #[test]
    fn hogersvang_drar_at_hoger() {
        let (x, z) = path_point(20.0, 0.0, 3.0);
        assert!(x > 0.3, "x = {x}");
        assert!(z > 0.0 && z < 3.0);
        let (xv, _) = path_point(-20.0, 0.0, 3.0);
        assert!((xv + x).abs() < 1e-4, "vänster ska spegla höger");
    }

    #[test]
    fn inre_hjulet_har_mindre_radie() {
        // I en högersväng ligger högerhjulet på insidan och hinner kortare åt sidan.
        let (xh, zh) = path_point(25.0, 0.5, 3.0);
        let (xv, zv) = path_point(25.0, -0.5, 3.0);
        let rh = ((xh - 0.5).powi(2) + zh.powi(2)).sqrt();
        let rv = ((xv + 0.5).powi(2) + zv.powi(2)).sqrt();
        assert!(rh < rv);
    }

    #[test]
    fn grader_i_forsta_hand_annars_procent() {
        let mut s = StatusUpdate { steering_percent: Some(50.0), ..Default::default() };
        assert_eq!(steering_deg(Some(&s)), Some(15.0));
        s.steering_deg = Some(-12.0);
        assert_eq!(steering_deg(Some(&s)), Some(-12.0));
        assert_eq!(steering_deg(None), None);
    }

    #[test]
    fn litet_utslag_visas_rakt() {
        let s = StatusUpdate { steering_deg: Some(0.6), steering_percent: Some(2.0), ..Default::default() };
        assert_eq!(steering_deg(Some(&s)), Some(0.0));
        let s = StatusUpdate { steering_deg: Some(1.2), steering_percent: Some(4.8), ..Default::default() };
        assert_eq!(steering_deg(Some(&s)), Some(0.0), "1° är rakt även om det är 4 %");
        let s = StatusUpdate { steering_deg: Some(2.0), steering_percent: Some(6.7), ..Default::default() };
        assert_eq!(steering_deg(Some(&s)), Some(0.0), "2° (skärmbild 2026-10-15) ska vara rakt");
        let s = StatusUpdate { steering_deg: Some(3.0), steering_percent: Some(10.0), ..Default::default() };
        assert_eq!(steering_deg(Some(&s)), Some(3.0));
        assert_eq!(steering_deg(None), None);
    }
}
