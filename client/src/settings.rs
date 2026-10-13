//! Settings-vyn: VESC-rollmappning. Skannar CAN-bussen (via robotd, inte
//! direkt) och listar alla svarande VESC-ID. Varje ID tilldelas en roll:
//! Drift vänster, Drift höger, Styrning, eller Annat (fritt namn + valfri
//! kontrollindata + min/max-gränser). Sparas som profil på Pi:n — se
//! PROJECT_SPEC.md §5.
//!
//! Det här är UI-lagret; själva skanningen/sparandet pratar med `robotd`
//! över samma reläanslutning som styrkommandona (ett nytt litet
//! begäran/svar-par läggs till i `relay-protocol` när det kopplas ihop
//! mot riktig hårdvara — se TODO nedan).

use eframe::egui;
use relay_protocol::{ActuatorConfig, ActuatorSlot};

use crate::net::ActuatorReply;

/// Aktiviteter på styrkortet, samma nummer som RControlStation:s "controls"
/// (dosan skickar dem i CMD_RC_CONTROL_ADV). robotd kör med 10 och 11.
const ACTIVITIES: &[(u16, &str)] = &[
    (10, "Fart (Speed Control)"),
    (11, "Styrning (Steering Control)"),
    (7, "Främre lyft (Front Lift)"),
    (8, "Bakre lyft (Rear Lift)"),
    (9, "Redskapsposition (Implement Position)"),
    (12, "Nödstopp (Emergency Stop)"),
];

/// Lägen som firmware kör för VESC (motor_set_vesc_value).
const MODES: &[(u16, &str)] = &[(0, "Duty"), (1, "Ström"), (3, "RPM")];

const KINDS: &[(u16, &str)] = &[(0, "VESC"), (1, "Hydraulik")];

fn name_of(list: &[(u16, &str)], v: u16) -> String {
    list.iter()
        .find(|(n, _)| *n == v)
        .map(|(_, s)| s.to_string())
        .unwrap_or_else(|| format!("Annan ({v})"))
}

fn combo(ui: &mut egui::Ui, id: String, list: &[(u16, &str)], value: &mut u16) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_source(id)
        .selected_text(name_of(list, *value))
        .show_ui(ui, |ui| {
            for (n, label) in list {
                if ui.selectable_label(*value == *n, *label).clicked() {
                    *value = *n;
                    changed = true;
                }
            }
        });
    changed
}

#[derive(Debug, Clone, PartialEq)]
pub enum VescRole {
    DriftVanster,
    DriftHoger,
    Styrning,
    Annat {
        namn: String,
        input: AccessoryInput,
        min: f32,
        max: f32,
    },
}

impl VescRole {
    fn label(&self) -> &'static str {
        match self {
            VescRole::DriftVanster => "Drift vänster",
            VescRole::DriftHoger => "Drift höger",
            VescRole::Styrning => "Styrning",
            VescRole::Annat { .. } => "Annat",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessoryInput {
    L1L2,
    R1R2,
}

impl AccessoryInput {
    fn label(&self) -> &'static str {
        match self {
            AccessoryInput::L1L2 => "L1 / L2",
            AccessoryInput::R1R2 => "R1 / R2",
        }
    }
}

#[derive(Debug, Clone)]
pub struct VescEntry {
    pub can_id: u8,
    pub role: Option<VescRole>,
    /// Senast sedda status (svarar den på bussen just nu?).
    pub last_seen_ms_ago: Option<u32>,
}

pub struct SettingsView {
    pub vescs: Vec<VescEntry>,
    pub scanning: bool,
    editing_other_name: String,
    dirty: bool,
    /// Styrkortets aktuatorer som de redigeras här.
    actuators: Option<ActuatorConfig>,
    /// Det som senast lästes från / skrevs till styrkortet.
    actuators_on_board: Option<ActuatorConfig>,
    actuator_busy: bool,
    confirm_write: bool,
    /// (lyckades, text) efter senaste läsning/skrivning.
    actuator_msg: Option<(bool, String)>,
    /// VESC som väntar på "Är maskinen upphissad?" innan provsnurr.
    confirm_spin: Option<u8>,
    /// Provsnurr skickat, väntar på svar.
    spin_busy: bool,
    /// (lyckades, text) efter senaste provsnurr.
    spin_msg: Option<(bool, String)>,
}

impl SettingsView {
    pub fn new() -> Self {
        Self {
            vescs: Vec::new(),
            scanning: false,
            editing_other_name: String::new(),
            dirty: false,
            actuators: None,
            actuators_on_board: None,
            actuator_busy: false,
            confirm_write: false,
            actuator_msg: None,
            confirm_spin: None,
            spin_busy: false,
            spin_msg: None,
        }
    }

    /// Kallas när en ny skanning startas. `net` skickar begäran vidare via
    /// reläet till robotd; svaret plockas upp i `poll_net()` nedan.
    pub fn start_scan(&mut self, net: &crate::net::NetLink) {
        self.scanning = true;
        net.request_vesc_scan();
    }

    /// Kallas varje bildruta från main.rs — plockar upp svar som kommit in
    /// via net.rs sedan sist.
    pub fn poll_net(&mut self, net: &mut crate::net::NetLink) {
        if let Some(result) = net.vesc_scan_result.take() {
            self.apply_scan_result(result.iter().map(|s| (s.can_id, s.responding)).collect());
        }
        if let Some((_, ok, msg)) = net.test_spin_reply.take() {
            self.spin_busy = false;
            self.spin_msg = Some((ok, msg));
        }
        if net.vesc_profile_saved {
            net.vesc_profile_saved = false;
            self.dirty = false;
        }
        if let Some(reply) = net.actuator_reply.take() {
            self.actuator_busy = false;
            match reply {
                ActuatorReply::Read(c) => {
                    self.actuator_msg = Some((true, "Läst från styrkortet.".to_string()));
                    self.actuators = Some(c.clone());
                    self.actuators_on_board = Some(c);
                }
                ActuatorReply::Written(c) => {
                    self.actuator_msg =
                        Some((true, "Skrivet till styrkortet och kontrolläst. Sparat i kortets minne.".to_string()));
                    self.actuators = Some(c.clone());
                    self.actuators_on_board = Some(c);
                }
                ActuatorReply::Error(e) => self.actuator_msg = Some((false, e)),
            }
        }
    }

    /// Kallas när svaret kommer in från robotd.
    /// `(can_id, svarade)`. Kända men tysta VESC läggs också till, som
    /// "Svarar inte", så att roller går att sätta ändå.
    pub fn apply_scan_result(&mut self, sightings: Vec<(u8, bool)>) {
        self.scanning = false;
        for (id, responding) in sightings {
            let seen = if responding { Some(0) } else { None };
            match self.vescs.iter_mut().find(|v| v.can_id == id) {
                Some(v) => v.last_seen_ms_ago = seen,
                None => self.vescs.push(VescEntry {
                    can_id: id,
                    role: None,
                    last_seen_ms_ago: seen,
                }),
            }
        }
    }

    pub fn draw(&mut self, ctx: &egui::Context, net: &mut crate::net::NetLink, on_close: &mut dyn FnMut()) {
        self.poll_net(net);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Inställningar — VESC-mappning");
                if ui.button("Stäng").clicked() {
                    on_close();
                }
            });
            ui.separator();

            ui.horizontal(|ui| {
                let btn_label = if self.scanning { "Skannar..." } else { "Skanna CAN-bussen" };
                if ui.add_enabled(!self.scanning, egui::Button::new(btn_label)).clicked() {
                    self.start_scan(net);
                }
                if ui.add_enabled(self.dirty, egui::Button::new("Spara")).clicked() {
                    self.save(net);
                }
                if self.dirty {
                    ui.colored_label(egui::Color32::YELLOW, "Osparade ändringar");
                }
            });
            ui.add_space(8.0);

            if let Some((ok, msg)) = &self.spin_msg {
                let color = if *ok { egui::Color32::from_rgb(60, 200, 90) } else { egui::Color32::from_rgb(230, 80, 80) };
                ui.colored_label(color, msg);
            }

            if self.vescs.is_empty() && !self.scanning {
                ui.label("Inga VESC hittade än — tryck Skanna CAN-bussen.");
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                let mut ids: Vec<u8> = self.vescs.iter().map(|v| v.can_id).collect();
                ids.sort_unstable();
                for can_id in ids {
                    self.draw_vesc_row(ui, can_id, net);
                    ui.separator();
                }

                ui.add_space(16.0);
                self.draw_actuators(ui, net);
            });
        });
    }

    /// Styrkortets aktuatorer: vilken VESC som gör vad. Samma som
    /// Confcommon-fliken i RControlStation (Read / Write).
    fn draw_actuators(&mut self, ui: &mut egui::Ui, net: &crate::net::NetLink) {
        ui.heading("Styrkortets aktuatorer");
        ui.label(
            "Vilken VESC som styrs av vad. Samma som Confcommon i RControlStation. \
             Sparas på styrkortet och behövs en gång för ett nytt kort.",
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let read_label = if self.actuator_busy { "Arbetar…" } else { "Läs från styrkortet" };
            if ui.add_enabled(!self.actuator_busy, egui::Button::new(read_label)).clicked() {
                self.actuator_busy = true;
                self.confirm_write = false;
                self.actuator_msg = None;
                net.read_actuators();
            }
            let changed = self.actuators.is_some() && self.actuators != self.actuators_on_board;
            if ui
                .add_enabled(changed && !self.actuator_busy, egui::Button::new("Skriv till styrkortet"))
                .clicked()
            {
                self.confirm_write = true;
            }
            if changed {
                ui.colored_label(egui::Color32::YELLOW, "Ändrat, inte skrivet");
            }
        });

        if self.confirm_write {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 170, 90),
                    "Skriva ändringarna till styrkortet? Roboten får inte vara aktiverad.",
                );
                if ui.button("Ja, skriv").clicked() {
                    if let Some(c) = &self.actuators {
                        self.actuator_busy = true;
                        self.actuator_msg = None;
                        net.write_actuators(c.clone());
                    }
                    self.confirm_write = false;
                }
                if ui.button("Avbryt").clicked() {
                    self.confirm_write = false;
                }
            });
        }

        if let Some((ok, msg)) = &self.actuator_msg {
            let color = if *ok { egui::Color32::from_rgb(60, 200, 90) } else { egui::Color32::from_rgb(230, 80, 80) };
            ui.colored_label(color, msg);
        }

        let Some(cfg) = &mut self.actuators else {
            ui.label("Tryck \"Läs från styrkortet\" för att se inställningarna.");
            return;
        };
        cfg.slots.resize(4, ActuatorSlot::default());

        ui.horizontal(|ui| {
            ui.label("Antal aktuatorer som används:");
            ui.add(egui::DragValue::new(&mut cfg.count).clamp_range(0..=4));
        });

        egui::Grid::new("actuator_grid").striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.strong("#");
            ui.strong("Typ");
            ui.strong("VESC-ID");
            ui.strong("Aktivitet");
            ui.strong("Läge");
            ui.end_row();
            let count = cfg.count as usize;
            for (i, slot) in cfg.slots.iter_mut().enumerate() {
                let used = i < count;
                ui.label(if used { format!("{}", i + 1) } else { format!("{} (används ej)", i + 1) });
                ui.add_enabled_ui(used, |ui| combo(ui, format!("act_kind_{i}"), KINDS, &mut slot.kind));
                ui.add_enabled_ui(used, |ui| ui.add(egui::DragValue::new(&mut slot.vesc_id).clamp_range(0..=254)));
                ui.add_enabled_ui(used, |ui| combo(ui, format!("act_activity_{i}"), ACTIVITIES, &mut slot.activity));
                ui.add_enabled_ui(used, |ui| combo(ui, format!("act_mode_{i}"), MODES, &mut slot.mode));
                ui.end_row();
            }
        });
    }

    fn draw_vesc_row(&mut self, ui: &mut egui::Ui, can_id: u8, net: &crate::net::NetLink) {
        let idx = self.vescs.iter().position(|v| v.can_id == can_id).unwrap();

        ui.horizontal(|ui| {
            ui.strong(format!("VESC {can_id}"));

            let seen = self.vescs[idx].last_seen_ms_ago;
            let status = match seen {
                Some(ms) if ms < 500 => ("● Svarar", egui::Color32::from_rgb(60, 200, 90)),
                Some(_) => ("● Senast sedd", egui::Color32::from_rgb(230, 190, 40)),
                None => ("● Svarar inte", egui::Color32::from_rgb(220, 60, 60)),
            };
            ui.colored_label(status.1, status.0);

            let current_label = self.vescs[idx]
                .role
                .as_ref()
                .map(|r| r.label())
                .unwrap_or("Ingen roll vald");

            egui::ComboBox::from_id_source(format!("role_{can_id}"))
                .selected_text(current_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(false, "Drift vänster").clicked() {
                        self.vescs[idx].role = Some(VescRole::DriftVanster);
                        self.dirty = true;
                    }
                    if ui.selectable_label(false, "Drift höger").clicked() {
                        self.vescs[idx].role = Some(VescRole::DriftHoger);
                        self.dirty = true;
                    }
                    if ui.selectable_label(false, "Styrning").clicked() {
                        self.vescs[idx].role = Some(VescRole::Styrning);
                        self.dirty = true;
                    }
                    if ui.selectable_label(false, "Annat...").clicked() {
                        self.vescs[idx].role = Some(VescRole::Annat {
                            namn: "Lastarm".to_string(),
                            input: AccessoryInput::L1L2,
                            min: -1.0,
                            max: 1.0,
                        });
                        self.dirty = true;
                    }
                });

            // Provsnurr: se vilken motor det är (som i VESC Tool).
            let spin_label = if self.spin_busy { "Snurrar…" } else { "Snurra" };
            if ui
                .add_enabled(!self.spin_busy, egui::Button::new(spin_label))
                .on_hover_text("Kör motorn 30 % i 0,5 s. Maskinen måste vara upphissad.")
                .clicked()
            {
                self.confirm_spin = Some(can_id);
                self.spin_msg = None;
            }
        });

        if self.confirm_spin == Some(can_id) {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(255, 170, 90),
                    format!("Är maskinen upphissad? VESC {can_id} snurrar 30 % i 0,5 s."),
                );
                if ui.button("Ja, snurra").clicked() {
                    self.spin_busy = true;
                    self.confirm_spin = None;
                    net.test_spin_vesc(can_id);
                }
                if ui.button("Avbryt").clicked() {
                    self.confirm_spin = None;
                }
            });
        }

        // Extra fält om rollen är "Annat".
        if let Some(VescRole::Annat { namn, input, min, max }) = &mut self.vescs[idx].role {
            ui.indent(format!("annat_{can_id}"), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Namn:");
                    if ui.text_edit_singleline(namn).changed() {
                        self.dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Styrs av:");
                    egui::ComboBox::from_id_source(format!("input_{can_id}"))
                        .selected_text(input.label())
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(*input == AccessoryInput::L1L2, "L1 / L2").clicked() {
                                *input = AccessoryInput::L1L2;
                                self.dirty = true;
                            }
                            if ui.selectable_label(*input == AccessoryInput::R1R2, "R1 / R2").clicked() {
                                *input = AccessoryInput::R1R2;
                                self.dirty = true;
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Min:");
                    if ui.add(egui::DragValue::new(min).speed(0.05)).changed() {
                        self.dirty = true;
                    }
                    ui.label("Max:");
                    if ui.add(egui::DragValue::new(max).speed(0.05)).changed() {
                        self.dirty = true;
                    }
                });
            });
        }
    }

    /// Kallas när användaren trycker "Spara" — skickar hela profilen till
    /// robotd (via net.rs/reläet) för att sparas lokalt på Pi:n.
    pub fn save(&mut self, net: &crate::net::NetLink) {
        use relay_protocol::VescRoleMsg;
        use std::collections::HashMap;

        let mut roller = HashMap::new();
        for v in &self.vescs {
            let Some(role) = &v.role else { continue };
            let msg = match role {
                VescRole::DriftVanster => VescRoleMsg::DriftVanster,
                VescRole::DriftHoger => VescRoleMsg::DriftHoger,
                VescRole::Styrning => VescRoleMsg::Styrning,
                VescRole::Annat { namn, input, min, max } => VescRoleMsg::Annat {
                    namn: namn.clone(),
                    input: match input {
                        AccessoryInput::L1L2 => "l1_l2".to_string(),
                        AccessoryInput::R1R2 => "r1_r2".to_string(),
                    },
                    min: *min,
                    max: *max,
                },
            };
            roller.insert(v.can_id, msg);
        }
        net.save_vesc_profile(relay_protocol::VescProfileMsg { roller });
        // `dirty` rensas när VescProfileSaved kommer tillbaka (se poll_net).
    }
}
