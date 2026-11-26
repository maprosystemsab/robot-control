//! Inställningar → Robotens firmware: sök ny firmware till styrkortet och uppdatera roboten.
//!
//! Klienten visar bara vad roboten säger och skickar tre kommandon: sök, uppdatera,
//! återställ. Roboten hämtar, kontrollerar (signatur, kontrollsumma), säkerhetskopierar
//! och skriver själv, och håller sina egna spärrar (stilla, inte aktiverad, inget uppdrag).
//! Före en uppdatering måste föraren läsa varningen, bocka i tre rutor och hålla in en knapp.
//!
//! Samma fil finns i Robotstyrning (robot-control, client/src/firmware.rs).

use std::time::{Duration, Instant};

use eframe::egui;
use relay_protocol::firmware::{FirmwareKommando, FirmwareLage, FirmwareStatus};

const ROD: egui::Color32 = egui::Color32::from_rgb(235, 60, 60);
const GRON: egui::Color32 = egui::Color32::from_rgb(60, 200, 90);
const ORANGE: egui::Color32 = egui::Color32::from_rgb(255, 170, 90);
/// Så länge ska knappen hållas in.
const HALL_IN: Duration = Duration::from_millis(2000);

/// Det föraren ska intyga före en uppdatering.
const INTYG: [&str; 3] = ["Roboten står stilla och är parkerad på en öppen yta", "Jag har roboten under uppsikt hela tiden", "Jag når nödstoppet"];

#[derive(Debug, Clone, PartialEq)]
enum Fraga {
    Uppdatera(String),
    Aterstall,
}

#[derive(Default)]
pub struct FirmwareVy {
    /// Varningsrutan är öppen.
    fraga: Option<Fraga>,
    intyg: [bool; 3],
    hall_sedan: Option<Instant>,
}

/// Raden om kortets version.
fn versionsrad(s: &FirmwareStatus) -> String {
    match &s.kortversion {
        Some(v) => format!("Styrkortet har firmware {v}."),
        None => "Styrkortet svarar inte just nu.".into(),
    }
}

/// Texten för läget, färgen, och om "Sök ny firmware" ska gå att trycka på.
fn lagesrad(s: &FirmwareStatus) -> (String, egui::Color32, bool) {
    let datum = if s.listans_datum.is_empty() { String::new() } else { format!(" Listan på servern är från {}.", s.listans_datum) };
    match &s.lage {
        FirmwareLage::Vilar => ("Tryck på Sök ny firmware för att fråga servern.".into(), egui::Color32::GRAY, true),
        FirmwareLage::Soker => ("Roboten frågar servern …".into(), egui::Color32::GRAY, false),
        FirmwareLage::Aktuell => (format!("Styrkortet har den senaste firmwaren.{datum}"), GRON, true),
        FirmwareLage::NyFinns { version, nyheter } => {
            let har = s.kortversion.as_deref().map(|v| format!(" (styrkortet har {v})")).unwrap_or_default();
            let nytt = if nyheter.is_empty() { String::new() } else { format!("\n{nyheter}") };
            (format!("Ny firmware finns: {version}{har}.{nytt}"), GRON, true)
        }
        FirmwareLage::Hinder { text } => (text.clone(), ORANGE, true),
        FirmwareLage::Pagar { version, steg, steg_antal, text, procent } => {
            let p = procent.map(|p| format!(" ({p} %)")).unwrap_or_default();
            (format!("Uppdaterar styrkortet till {version} – steg {steg} av {steg_antal}: {text}{p}"), ORANGE, false)
        }
        FirmwareLage::Klar { version, fran, .. } => {
            let fran = if fran.is_empty() { String::new() } else { format!(" från {fran}") };
            (format!("Klart. Styrkortet är uppdaterat{fran} till {version}."), GRON, true)
        }
        FirmwareLage::Misslyckad { version, text, kan_aterstallas, .. } => (format!("Uppdateringen till {version} blev inte gjord. {text}"), ROD, !kan_aterstallas),
    }
}

impl FirmwareVy {
    /// Ritar delen under Inställningar. `status` är robotens besked (`None` = roboten kan
    /// inte uppdateras härifrån), `forare` om den här klienten har kontrollen. Ger kommandot
    /// som ska skickas till roboten, om föraren bad om något.
    pub fn draw(&mut self, ui: &mut egui::Ui, status: Option<&FirmwareStatus>, forare: bool) -> Option<FirmwareKommando> {
        ui.heading("Robotens firmware");
        ui.label(egui::RichText::new("Programmet i robotens styrkort. Roboten hämtar ny firmware från vår server och kontrollerar den själv innan något skrivs.").small().color(egui::Color32::GRAY));
        let Some(s) = status else {
            self.fraga = None;
            ui.colored_label(egui::Color32::GRAY, "Roboten stöder inte uppdatering härifrån ännu (robotens program behöver uppdateras först).");
            return None;
        };
        if s.provbygge {
            ui.colored_label(ROD, "PROVBYGGE: robotens program godkänner provsignerade listor. Det får inte användas på en riktig robot.");
        }
        ui.label(versionsrad(s));
        if let Some(v) = &s.variant {
            ui.label(egui::RichText::new(format!("Kortvariant: {v}")).small().color(egui::Color32::GRAY));
        }
        let (text, farg, kan_soka) = lagesrad(s);
        let mut kommando = None;
        if let FirmwareLage::Pagar { steg, steg_antal, .. } = &s.lage {
            self.fraga = None;
            ui.add_space(6.0);
            ui.label(egui::RichText::new(text).size(18.0).strong().color(farg));
            ui.add(egui::ProgressBar::new(*steg as f32 / (*steg_antal).max(1) as f32).desired_width(420.0).animate(true));
            ui.label(egui::RichText::new("BRYT INTE STRÖMMEN TILL ROBOTEN OCH STÄNG INTE PROGRAMMET.").size(18.0).strong().color(ROD));
            ui.label("Roboten går inte att köra under tiden. Tappas anslutningen gör roboten klart själv, och resultatet visas här när du ansluter igen.");
            ui.ctx().request_repaint_after(Duration::from_millis(250));
            return None;
        }
        ui.colored_label(farg, text);
        if let FirmwareLage::Klar { anmarkningar, .. } = &s.lage {
            for a in anmarkningar {
                ui.colored_label(ORANGE, format!("OBS: {a}"));
            }
        }
        let erbjuden = match &s.lage {
            FirmwareLage::NyFinns { version, .. } => Some(version.clone()),
            _ => None,
        };
        let aterstallbar = matches!(s.lage, FirmwareLage::Misslyckad { kan_aterstallas: true, .. });
        if (erbjuden.is_some() || aterstallbar) && !s.sparrar.is_empty() {
            ui.label("Det här hindrar just nu:");
            for sp in &s.sparrar {
                ui.colored_label(ORANGE, format!("  •  {sp}"));
            }
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(forare && kan_soka, egui::Button::new("Sök ny firmware"))
                .on_disabled_hover_text(if forare { "Vänta tills roboten är klar." } else { "Bara den som kör roboten kan söka och uppdatera." })
                .clicked()
            {
                kommando = Some(FirmwareKommando::Sok);
            }
            let fritt = forare && s.sparrar.is_empty();
            if let Some(version) = &erbjuden {
                let knapp = egui::Button::new(egui::RichText::new("Uppdatera roboten").color(egui::Color32::WHITE).strong()).fill(if fritt { egui::Color32::from_rgb(34, 150, 60) } else { egui::Color32::from_gray(70) });
                if ui.add_enabled(fritt, knapp).on_disabled_hover_text(if forare { "Roboten är inte redo, se ovan." } else { "Bara den som kör roboten kan uppdatera." }).clicked() {
                    self.oppna(Fraga::Uppdatera(version.clone()));
                }
            }
            if aterstallbar && ui.add_enabled(fritt, egui::Button::new("Skriv tillbaka den förra firmwaren")).on_disabled_hover_text("Roboten är inte redo, se ovan.").clicked() {
                self.oppna(Fraga::Aterstall);
            }
        });
        // Rutan stängs om det den gällde inte längre finns (ny sökning, annan version).
        let galler = match &self.fraga {
            Some(Fraga::Uppdatera(v)) => erbjuden.as_ref() == Some(v),
            Some(Fraga::Aterstall) => aterstallbar,
            None => false,
        };
        if !galler {
            self.fraga = None;
        }
        if let Some(fraga) = self.fraga.clone() {
            if let Some(k) = self.varningsruta(ui.ctx(), &fraga, s, forare) {
                kommando = Some(k);
            }
        }
        kommando
    }

    fn oppna(&mut self, fraga: Fraga) {
        self.fraga = Some(fraga);
        self.intyg = [false; 3];
        self.hall_sedan = None;
    }

    /// Varningen före skrivningen: stor röd text, tre rutor att bocka i och en knapp att hålla in.
    fn varningsruta(&mut self, ctx: &egui::Context, fraga: &Fraga, s: &FirmwareStatus, forare: bool) -> Option<FirmwareKommando> {
        let mut kommando = None;
        let mut stang = false;
        let rubrik = match fraga {
            Fraga::Uppdatera(v) => format!("Uppdatera styrkortet till {v}"),
            Fraga::Aterstall => "Skriv tillbaka den förra firmwaren".to_string(),
        };
        egui::Window::new(rubrik).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_max_width(620.0);
            ui.label(egui::RichText::new("VARNING – ROBOTENS STYRKORT SKRIVS OM").size(26.0).strong().color(ROD));
            ui.add_space(6.0);
            for rad in ["Roboten ska stå helt stilla, helst parkerad på en öppen yta.", "Du ska ha roboten under uppsikt hela tiden.", "Du ska kunna trycka på nödstoppet."] {
                ui.label(egui::RichText::new(rad).size(20.0).strong().color(ROD));
            }
            ui.add_space(8.0);
            ui.label("Styrkortet startar om under uppdateringen och roboten går inte att köra förrän den är klar. Det tar några minuter.");
            ui.label(egui::RichText::new("Bryt inte strömmen till roboten och stäng inte programmet.").strong());
            ui.label("Tappas anslutningen gör roboten klart själv, och resultatet visas när du ansluter igen. Kortets inställningar sparas först och läggs tillbaka om något går fel.");
            ui.add_space(8.0);
            for (i, text) in INTYG.iter().enumerate() {
                ui.checkbox(&mut self.intyg[i], egui::RichText::new(*text).size(17.0));
            }
            if !s.sparrar.is_empty() {
                ui.add_space(4.0);
                for sp in &s.sparrar {
                    ui.colored_label(ORANGE, format!("Roboten är inte redo: {sp}"));
                }
            }
            ui.add_space(10.0);
            let klart = forare && s.sparrar.is_empty() && self.intyg.iter().all(|b| *b);
            ui.horizontal(|ui| {
                let text = match fraga {
                    Fraga::Uppdatera(_) => "Håll in för att uppdatera roboten",
                    Fraga::Aterstall => "Håll in för att skriva tillbaka",
                };
                let knapp = egui::Button::new(egui::RichText::new(text).size(18.0).color(egui::Color32::WHITE).strong())
                    .min_size(egui::vec2(340.0, 46.0))
                    // click_and_drag: med bara click() släpper egui en knapp som hållits 0,8 s.
                    .sense(egui::Sense::click_and_drag())
                    .fill(if klart { egui::Color32::from_rgb(170, 40, 40) } else { egui::Color32::from_gray(70) });
                let svar = ui.add_enabled(klart, knapp).on_disabled_hover_text("Bocka i de tre rutorna först.");
                if klart && svar.is_pointer_button_down_on() {
                    let sedan = *self.hall_sedan.get_or_insert_with(Instant::now);
                    let del = (sedan.elapsed().as_secs_f32() / HALL_IN.as_secs_f32()).min(1.0);
                    let mut fylld = svar.rect;
                    fylld.set_width(svar.rect.width() * del);
                    ui.painter().rect_filled(fylld, 3.0, egui::Color32::from_rgba_unmultiplied(255, 255, 255, 70));
                    ui.ctx().request_repaint();
                    if del >= 1.0 {
                        kommando = Some(match fraga {
                            Fraga::Uppdatera(v) => FirmwareKommando::Uppdatera { version: v.clone() },
                            Fraga::Aterstall => FirmwareKommando::Aterstall,
                        });
                        stang = true;
                    }
                } else {
                    self.hall_sedan = None;
                    // Trycket och pekarens läge kan komma i samma bild: rita en gång till
                    // medan en knapp är nere, så att hållningen börjar räknas.
                    if ui.input(|i| i.pointer.any_down()) {
                        ui.ctx().request_repaint();
                    }
                }
                if ui.add(egui::Button::new(egui::RichText::new("Avbryt").size(18.0)).min_size(egui::vec2(120.0, 46.0))).clicked() {
                    stang = true;
                }
            });
        });
        if stang {
            self.fraga = None;
            self.hall_sedan = None;
        }
        kommando
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn status(lage: FirmwareLage) -> FirmwareStatus {
        FirmwareStatus { kortversion: Some("30.32".into()), variant: Some("mp101-robant".into()), lage, sparrar: vec![], listans_datum: "2026-11-26".into(), provbygge: false }
    }

    #[test]
    fn raderna_sager_vad_som_galler() {
        assert_eq!(versionsrad(&status(FirmwareLage::Vilar)), "Styrkortet har firmware 30.32.");
        assert_eq!(versionsrad(&FirmwareStatus::default()), "Styrkortet svarar inte just nu.");
        let (t, _, sok) = lagesrad(&status(FirmwareLage::NyFinns { version: "30.33".into(), nyheter: "Bättre broms".into() }));
        assert_eq!((t.as_str(), sok), ("Ny firmware finns: 30.33 (styrkortet har 30.32).\nBättre broms", true));
        let (t, farg, sok) = lagesrad(&status(FirmwareLage::Aktuell));
        assert_eq!((t.as_str(), farg, sok), ("Styrkortet har den senaste firmwaren. Listan på servern är från 2026-11-26.", GRON, true));
        // Under en uppdatering går det inte att söka, och inte heller medan en misslyckad
        // uppdatering väntar på att den förra firmwaren skrivs tillbaka.
        let (t, _, sok) = lagesrad(&status(FirmwareLage::Pagar { version: "30.33".into(), steg: 2, steg_antal: 8, text: "Hämtar".into(), procent: Some(40) }));
        assert_eq!((t.as_str(), sok), ("Uppdaterar styrkortet till 30.33 – steg 2 av 8: Hämtar (40 %)", false));
        let misslyckad = |kan| FirmwareLage::Misslyckad { version: "30.33".into(), text: "Fel.".into(), aterstalld: Some(false), kan_aterstallas: kan };
        assert_eq!(lagesrad(&status(misslyckad(true))).2, false);
        assert_eq!(lagesrad(&status(misslyckad(false))), ("Uppdateringen till 30.33 blev inte gjord. Fel.".to_string(), ROD, true));
        let (t, ..) = lagesrad(&status(FirmwareLage::Klar { version: "30.33".into(), fran: "30.32".into(), anmarkningar: vec![] }));
        assert_eq!(t, "Klart. Styrkortet är uppdaterat från 30.32 till 30.33.");
    }
}
