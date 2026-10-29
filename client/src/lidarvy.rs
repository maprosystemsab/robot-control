//! Lidarn i Robotstyrning: vyn ovanifrån (nedre vänstra hörnet) och texten när
//! lidarstoppet tar bort gasen framåt. Data från robotd (`StatusUpdate::lidar_*`).

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use relay_protocol::{LidarVy, StatusUpdate};

const MARK: Color32 = Color32::from_rgb(70, 90, 80);
const HINDER: Color32 = Color32::from_rgb(255, 90, 60);
const ROBOT: Color32 = Color32::from_rgb(120, 200, 255);

/// Skylten när lidarstoppet begränsar gasen: (text, röd = ingen gas framåt alls).
pub fn stopp_text(s: &StatusUpdate) -> Option<(String, bool)> {
    let gas = s.lidar_gas?;
    if gas >= 1.0 {
        return None;
    }
    let orsak = s.lidar_text.clone().unwrap_or_default();
    Some(if gas <= 0.0 {
        (format!("LIDARSTOPP – ingen gas framåt: {orsak}. Backa eller styr undan."), true)
    } else {
        (format!("Lidarn saktar in ({:.0} % gas): {orsak}", gas * 100.0), false)
    })
}

/// Storleken som vyn ritas i: `skala` pixlar per cell.
pub fn size(v: &LidarVy, skala: f32) -> egui::Vec2 {
    let kol = v.rader.first().map_or(0, |r| r.chars().count());
    egui::vec2(kol as f32 * skala, v.rader.len() as f32 * skala)
}

/// Ritar vyn i `rect` (framåt uppåt). `stopp` = röd ram.
pub fn draw(p: &egui::Painter, rect: Rect, v: &LidarVy, stopp: bool) {
    p.rect_filled(rect, 4.0, Color32::from_rgba_unmultiplied(0, 0, 0, 190));
    let kol = v.rader.first().map_or(0, |r| r.chars().count());
    if kol == 0 || v.cell <= 0.0 {
        return;
    }
    let cw = rect.width() / kol as f32;
    let ch = rect.height() / v.rader.len() as f32;
    for (r, rad) in v.rader.iter().enumerate() {
        for (k, c) in rad.chars().enumerate() {
            let farg = match c {
                '#' => HINDER,
                '.' => MARK,
                _ => continue,
            };
            let min = Pos2::new(rect.left() + k as f32 * cw, rect.top() + r as f32 * ch);
            p.rect_filled(Rect::from_min_size(min, egui::vec2(cw.max(1.0), ch.max(1.0))), 0.0, farg);
        }
    }
    // Robotens kropp: x framåt = uppåt, y åt vänster = åt vänster.
    let till = |x: f32, y: f32| {
        Pos2::new(
            rect.left() + (v.y - y) / v.cell * cw,
            rect.top() + (v.x1 - x) / v.cell * ch,
        )
    };
    let [fram, bak, hw] = v.robot;
    let kropp = Rect::from_two_pos(till(fram, hw), till(-bak, -hw)).intersect(rect);
    p.rect_stroke(kropp, 2.0, Stroke::new(1.5, ROBOT));
    // Nosen: en liten pil framåt.
    let nos = till(fram, 0.0);
    p.line_segment([nos, nos - egui::vec2(0.0, 5.0)], Stroke::new(1.5, ROBOT));
    if let Some(f) = v.fram {
        p.text(
            rect.right_top() + egui::vec2(-4.0, 3.0),
            egui::Align2::RIGHT_TOP,
            format!("{f:.1} m").replace('.', ","),
            egui::FontId::proportional(12.0),
            Color32::WHITE,
        );
    }
    p.text(
        rect.left_top() + egui::vec2(4.0, 3.0),
        egui::Align2::LEFT_TOP,
        "Lidar",
        egui::FontId::proportional(11.0),
        Color32::from_gray(170),
    );
    let ram = if stopp { Stroke::new(2.5, Color32::from_rgb(230, 30, 30)) } else { Stroke::new(1.0, Color32::from_gray(90)) };
    p.rect_stroke(rect, 4.0, ram);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skylten() {
        let mut s = StatusUpdate::default();
        assert_eq!(stopp_text(&s), None);
        s.lidar_gas = Some(1.0);
        assert_eq!(stopp_text(&s), None);
        s.lidar_gas = Some(0.4);
        s.lidar_text = Some("något 2,2 m framför roboten".into());
        let (t, rod) = stopp_text(&s).unwrap();
        assert!(!rod && t.contains("40 %"), "{t}");
        s.lidar_gas = Some(0.0);
        s.lidar_text = Some("lidarn svarar inte".into());
        let (t, rod) = stopp_text(&s).unwrap();
        assert!(rod && t.contains("LIDARSTOPP") && t.contains("svarar inte"), "{t}");
    }

    #[test]
    fn storleken() {
        let v = LidarVy { cell: 0.25, x0: -2.5, x1: 7.5, y: 6.0, robot: [0.2, 1.6, 0.7], fram: None, rader: vec![" ".repeat(48); 40] };
        assert_eq!(size(&v, 3.0), egui::vec2(144.0, 120.0));
    }
}
