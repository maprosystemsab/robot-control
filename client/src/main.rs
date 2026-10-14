//! Robotstyrning — klientprogrammet.
//!
//! Flöde (beslutat i spec): svart anslutningsskärm med ett textfält i
//! mitten (robotnamn eller IP) -> helskärm med kamerabild -> AKTIVERA-knapp
//! ovanpå bilden -> färgad ram runt hela fönstret (grön/gul/röd, lila vid
//! dosa-tapp) -> statusinfo i hörnet (hastighet, signal, ping, batteri).
//! Kan minimeras/flyttas som ett vanligt fönster, tvingar inte helskärm.
//! Språk: svenska.

mod gamepad;
mod hud;
mod net;
mod settings;
mod video;
mod guide;

use std::time::{Duration, Instant};

use eframe::egui;
use gamepad::GamepadReader;
use net::{ConnectionState, NetLink};
use relay_protocol::{ControlCommand, LinkQuality};

const ACTIVATE_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn main() -> eframe::Result<()> {
    let rt = tokio::runtime::Runtime::new().expect("kunde inte starta tokio-runtime");
    let rt_handle = rt.handle().clone();
    // Håll runtimen vid liv i bakgrunden så länge appen kör.
    std::thread::spawn(move || rt.block_on(std::future::pending::<()>()));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Robotstyrning",
        options,
        Box::new(move |_cc| {
            let mut app = App::new(rt_handle);
            // `robotstyrning <adress>` ansluter direkt (t.ex. 127.0.0.1 mot mock-robotd).
            if let Some(target) = std::env::args().nth(1) {
                app.connect_to(target);
            }
            Box::new(app)
        }),
    )
}

enum Screen {
    Connect,
    Driving,
    Settings,
}

/// Styr bara vilka krav som gäller för att AKTIVERA ska gå att trycka —
/// INTE vad som visas på skärmen. Kamerabild (eller en framtida
/// VR-ström) renderas oavsett läge; se draw_driving_screen. VESC-kravet
/// (drift/styr svarar på bussen) gäller i båda lägena — det handlar om
/// att roboten mekaniskt går att styra alls, inte om föraren ser den.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrivingMode {
    /// Kräver kamerabild för att AKTIVERA ska gå att trycka.
    Cam,
    /// "Line of sight" — föraren ser roboten direkt, kamerakravet släpps.
    Los,
}

impl Default for DrivingMode {
    fn default() -> Self {
        DrivingMode::Cam
    }
}

struct App {
    screen: Screen,
    name_input: String,
    known_robots: Vec<String>, // laddas/sparas via `directories`-mappen
    net: NetLink,
    gamepad: Option<GamepadReader>,
    activated: bool,
    last_activity: Instant,
    gamepad_connected: bool,
    settings: settings::SettingsView,
    lights_on: bool,
    rt_handle: tokio::runtime::Handle,
    /// Standardporten robotd lyssnar på (se robotd:s config.json, `bind_addr`).
    robot_port: u16,
    /// Sätts när ett anslutningsförsök pågår; flyttas till `known_robots`
    /// bara vid lyckad anslutning (Fel 6, testrapport 2026-09-19 — sparades
    /// tidigare direkt vid försök, så felstavningar hamnade i listan).
    pending_target: Option<String>,
    /// Visas på anslutningsskärmen efter en nekad/avbruten anslutning
    /// (Fel 5 — syntes tidigare ingenstans i UI:t).
    connect_error: Option<String>,
    driving_mode: DrivingMode,
    /// Kameraströmmen (egen TCP-anslutning + egen tråd, se video.rs).
    video: video::VideoReceiver,
    /// robotd:s videoport (`video.port` i robotd:s config.json).
    video_port: u16,
    video_texture: Option<egui::TextureHandle>,
    /// Röd färdlinje i kamerabilden och bakhjulsvyn (knappen "Färdlinje").
    show_guide: bool,
    /// Förarens Max (0..1) som i RControlStation, sparas mellan körningar.
    max_output: f32,
    /// Körloggsfönstret: öppet, listan från roboten och senaste meddelande.
    logs_open: bool,
    logs: Vec<relay_protocol::DriveLogInfo>,
    logs_message: String,
    /// För tester (`ROBOTSTYRNING_SKARMBILD`): när appen startade och om skärmbilden begärts.
    started: Instant,
    screenshot_requested: bool,
}

impl App {
    fn new(rt_handle: tokio::runtime::Handle) -> Self {
        Self {
            screen: Screen::Connect,
            name_input: String::new(),
            known_robots: load_known_robots(),
            net: NetLink::new(),
            gamepad: GamepadReader::new().ok(),
            activated: false,
            last_activity: Instant::now(),
            gamepad_connected: true,
            settings: settings::SettingsView::new(),
            lights_on: false,
            rt_handle,
            robot_port: 9000,
            pending_target: None,
            connect_error: None,
            driving_mode: DrivingMode::Cam,
            video: video::VideoReceiver::new(),
            video_port: 9001,
            video_texture: None,
            show_guide: true,
            max_output: load_max_output(),
            logs_open: false,
            logs: Vec::new(),
            logs_message: String::new(),
            started: Instant::now(),
            screenshot_requested: false,
        }
    }

    fn border_color(&self) -> egui::Color32 {
        if !self.gamepad_connected {
            return egui::Color32::from_rgb(180, 80, 220); // lila: dosa frånkopplad
        }
        match self.net.link_quality(Instant::now()) {
            LinkQuality::Green => egui::Color32::from_rgb(60, 200, 90),
            LinkQuality::Yellow => egui::Color32::from_rgb(230, 190, 40),
            LinkQuality::Red => egui::Color32::from_rgb(220, 60, 60),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.test_screenshot(ctx);
        // Fel 1 (testrapport 2026-09-19): måste köras HÄR, inte bara i
        // draw_driving_screen, annars slutar klienten läsa nätverket så
        // fort en annan skärm (t.ex. Settings) visas — kön fylls och hela
        // nätverksuppgiften fastnar på `send().await`.
        self.net.poll_updates();

        match self.net.state {
            ConnectionState::ConnectedAsDriver | ConnectionState::ConnectedAsViewer => {
                // Fel 6: robotnamnet sparas i listan först nu, vid bekräftat
                // lyckad anslutning — inte redan när användaren bara försökte.
                if let Some(target) = self.pending_target.take() {
                    if !self.known_robots.contains(&target) {
                        self.known_robots.push(target);
                        save_known_robots(&self.known_robots);
                    }
                }
                self.connect_error = None;
            }
            ConnectionState::Denied | ConnectionState::Disconnected => {
                // Fel 5: nekad/avbruten anslutning gav tidigare ingen
                // återkoppling i UI:t alls — fastnade på körskärmen utan
                // text. Nu: tillbaka till anslutningsskärmen med orsaken.
                if !matches!(self.screen, Screen::Connect) {
                    self.video.stop();
                    self.video_texture = None;
                    self.connect_error = Some(
                        self.net
                            .deny_reason
                            .clone()
                            .unwrap_or_else(|| "Anslutningen bröts.".to_string()),
                    );
                    self.screen = Screen::Connect;
                    self.pending_target = None;
                }
            }
            ConnectionState::Connecting => {}
        }

        match self.screen {
            Screen::Connect => self.draw_connect_screen(ctx),
            Screen::Driving => self.draw_driving_screen(ctx),
            Screen::Settings => {
                let mut close = false;
                self.settings.draw(ctx, &mut self.net, &mut || close = true);
                if close {
                    self.screen = Screen::Driving;
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(33)); // ~30fps UI-loop
    }
}

impl App {
    fn draw_connect_screen(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() / 2.0 - 60.0);

                    let field = egui::TextEdit::singleline(&mut self.name_input)
                        .hint_text("Robotnamn eller IP-adress")
                        .desired_width(320.0)
                        .text_color(egui::Color32::WHITE);
                    let response = ui.add(field);

                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.connect_to(self.name_input.clone());
                    }

                    if let Some(err) = &self.connect_error {
                        ui.add_space(12.0);
                        ui.colored_label(egui::Color32::from_rgb(220, 90, 90), err);
                    }

                    ui.add_space(20.0);

                    if !self.known_robots.is_empty() {
                        ui.label(
                            egui::RichText::new("Tidigare anslutna").color(egui::Color32::GRAY),
                        );
                        for name in self.known_robots.clone() {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(&name).color(egui::Color32::WHITE),
                                ))
                                .clicked()
                            {
                                self.connect_to(name);
                            }
                        }
                    }
                });
            });
    }

    fn connect_to(&mut self, target: String) {
        if target.trim().is_empty() {
            return;
        }
        self.connect_error = None;
        self.pending_target = Some(target.clone());
        // Alltid CAM som startläge vid varje ny anslutning — medvetet
        // vald standard, minns inte senast använda läget (spec-diskussion
        // 2026-09-19).
        self.driving_mode = DrivingMode::Cam;
        // Direktanslutning över WireGuard-tunneln — inget relä, ingen
        // parkopplingskod. `target` är robotens VPN-IP eller ett namn som
        // löses via klientens /etc/hosts (se PROJECT_SPEC.md §14).
        self.net.connect(&self.rt_handle, target.clone(), self.robot_port, false, None);
        // Videon på en egen anslutning till samma värd, så en tung bildruta
        // aldrig kan fördröja styrkommandon på styr-anslutningen.
        self.video_texture = None;
        self.video.start(target, self.video_port);
        self.screen = Screen::Driving;
    }

    fn draw_driving_screen(&mut self, ctx: &egui::Context) {
        // Poll gamepad varje bildruta.
        let mut gp_state = gamepad::GamepadState::default();
        if let Some(gp) = &mut self.gamepad {
            gp_state = gp.poll();
            self.gamepad_connected = gp_state.connected;
            if gp_state.connected
                && (gp_state.throttle.abs() > 0.02 || gp_state.steering.abs() > 0.02)
            {
                self.last_activity = Instant::now();
            }
        }

        // (poll_updates() körs nu centralt i App::update(), oavsett skärm —
        // se Fel 1 i testrapporten 2026-09-19: den fick tidigare bara köras
        // härifrån, vilket gjorde att nätverket slutade läsas så fort
        // Settings-vyn var öppen.)

        if self.activated && self.last_activity.elapsed() > ACTIVATE_IDLE_TIMEOUT {
            self.activated = false;
        }
        if !self.gamepad_connected {
            self.activated = false;
        }
        // Nödstopp på roboten: lås, föraren måste aktivera igen när knappen är ute.
        if self
            .net
            .fresh_status()
            .is_some_and(|st| hud::estop_text(st.estop.as_deref()).is_some())
        {
            self.activated = false;
        }
        // Esc låser också, snabbare än att sikta på knappen.
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.activated = false;
        }

        // Skicka styrkommando varje bildruta. Gas/styr nollställs om vi
        // inte är aktiverade (dosan låst), men tillval (lastarm/tilt) och
        // belysning skickas alltid, precis som beslutat (accessoarer är
        // inte låsta bakom AKTIVERA).
        let cmd = ControlCommand {
            throttle: if self.activated { gp_state.throttle } else { 0.0 },
            steering: if self.activated { gp_state.steering } else { 0.0 },
            accessory_a: gp_state.accessory_a,
            accessory_b: gp_state.accessory_b,
            lights: self.lights_on,
            activated: self.activated,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            max_output: Some(self.max_output),
        };
        self.net.send_control(cmd);

        let border = self.border_color();

        // Ny kamerabild -> textur. Bara den senaste bilden uppdateras; om
        // avkodningen ligger före UI:t visas alltid den färskaste.
        if let Some(f) = self.video.take_new_frame() {
            let img = egui::ColorImage::from_rgb([f.width, f.height], &f.rgb);
            match &mut self.video_texture {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => {
                    self.video_texture =
                        Some(ctx.load_texture("kamera", img, egui::TextureOptions::LINEAR))
                }
            }
        }
        let video_live = self.video.is_live();

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_gray(20)))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(10));
                match &self.video_texture {
                    Some(tex) => {
                        // Rita bilden så stor som möjligt utan att förvränga den.
                        let tex_size = tex.size_vec2();
                        let scale = (rect.width() / tex_size.x).min(rect.height() / tex_size.y);
                        let dest = egui::Rect::from_center_size(rect.center(), tex_size * scale);
                        ui.painter().image(
                            tex.id(),
                            dest,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                        if self.show_guide {
                            if let Some(deg) = guide::steering_deg(self.net.fresh_status()) {
                                guide::draw_path(ui.painter(), dest, deg);
                            }
                        }
                        if !video_live {
                            // En stillastående bild ser levande ut — säg det
                            // rakt ut när strömmen fryst eller brutits.
                            ui.painter().rect_filled(
                                dest,
                                0.0,
                                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140),
                            );
                            ui.painter().text(
                                rect.center() + egui::vec2(0.0, 90.0),
                                egui::Align2::CENTER_CENTER,
                                "BILDEN FRUSEN – ingen ny bild från roboten",
                                egui::FontId::proportional(26.0),
                                egui::Color32::from_rgb(255, 120, 120),
                            );
                        }
                    }
                    None => {
                        ui.painter().text(
                            rect.center() + egui::vec2(0.0, 90.0),
                            egui::Align2::CENTER_CENTER,
                            format!("Ingen bild ännu ({})", self.video.status_line()),
                            egui::FontId::proportional(24.0),
                            egui::Color32::GRAY,
                        );
                    }
                }

                // Statusinfo, övre vänstra hörnet.
                if self.driving_mode == DrivingMode::Los {
                    // Ren visuell markör om aktivt läge — påverkar inte
                    // kamerabilden/videoströmmen alls, den renderas oavsett
                    // (spec-diskussion 2026-09-19: kameran ska kunna
                    // strömmas/spelas in även när man själv kör LOS, t.ex.
                    // med VR-glasögon på).
                    egui::Area::new("los_badge".into())
                        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 16.0))
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_unmultiplied(180, 80, 220, 220))
                                .inner_margin(egui::Margin::symmetric(14.0, 6.0))
                                .rounding(6.0)
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new("LOS-läge").color(egui::Color32::WHITE).strong());
                                });
                        });
                }

                // Ljusgrön text på mörk, halvgenomskinlig botten — läsbar även
                // när kamerabilden bakom är ljus (dagsljus, himmel).
                egui::Area::new("status".into())
                    .fixed_pos(rect.left_top() + egui::vec2(16.0, 16.0))
                    .show(ctx, |ui| {
                        egui::Frame::none()
                            .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 175))
                            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                            .rounding(8.0)
                            .show(ui, |ui| {
                                // Blinkar ungefär en gång per sekund vid låg batterinivå.
                                let blink = (ui.input(|i| i.time) * 2.0) as i64 % 2 == 0;
                                hud::show(
                                    ui,
                                    self.net.fresh_status(),
                                    self.net.last_ping_ms,
                                    &self.video.status_line(),
                                    blink,
                                );
                            });
                    });

                // Belysningsknapp + körläge + inställningar, övre högra hörnet.
                // Förankrad i övre högra hörnet: raden växer åt vänster när
                // Lås-knappen dyker upp, kugghjulet ligger alltid ytterst.
                egui::Area::new("lighting".into())
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            // Lås = slå av AKTIVERA direkt. Gas/styr nollas i nästa
                            // kommando och robotd rampar ner; AKTIVERA krävs igen.
                            if self.activated
                                && ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("🔒 Lås").color(egui::Color32::WHITE).strong(),
                                        )
                                        .fill(egui::Color32::from_rgb(170, 40, 40)),
                                    )
                                    .on_hover_text("Lås körningen (samma som att AKTIVERA slås av). Tangent: Esc")
                                    .clicked()
                            {
                                self.activated = false;
                            }
                            let lights_label = if self.lights_on { "💡 Belysning PÅ" } else { "💡 Belysning" };
                            if ui.button(lights_label).clicked() {
                                self.lights_on = !self.lights_on;
                            }

                            // Läget låst medan AKTIVERAD — ska inte gå att
                            // byta mitt i körning, bara innan/efter.
                            let mode_label = match self.driving_mode {
                                DrivingMode::Cam => "📷 CAM",
                                DrivingMode::Los => "👁 LOS",
                            };
                            if ui
                                .add_enabled(!self.activated, egui::Button::new(mode_label))
                                .on_hover_text("Växla mellan CAM (kräver kamerabild) och LOS (fri sikt, inget kamerakrav)")
                                .clicked()
                            {
                                self.driving_mode = match self.driving_mode {
                                    DrivingMode::Cam => DrivingMode::Los,
                                    DrivingMode::Los => DrivingMode::Cam,
                                };
                            }

                            let resp = ui
                                .add(
                                    egui::DragValue::new(&mut self.max_output)
                                        .clamp_range(0.05..=1.0)
                                        .speed(0.005)
                                        .fixed_decimals(2)
                                        .prefix("Max: "),
                                )
                                .on_hover_text(
                                    "Farten vid fullt spakutslag (som Max i RControlStation). Styrningen går \
                                     alltid på 0,8. Dra eller dubbelklicka och skriv. Gäller direkt, även under körning.",
                                );
                            if resp.changed() {
                                save_max_output(self.max_output);
                            }

                            if ui
                                .selectable_label(self.show_guide, "Färdlinje")
                                .on_hover_text("Röd linje i bilden som visar vart roboten är på väg, och hur bakhjulen står")
                                .clicked()
                            {
                                self.show_guide = !self.show_guide;
                            }

                            if ui.button("Loggar").on_hover_text("Körloggar: hämta loggar från roboten").clicked() {
                                self.logs_open = true;
                                self.logs_message = "Hämtar listan…".to_string();
                                self.net.list_drive_logs();
                            }
                            if ui.button("⚙").on_hover_text("Inställningar").clicked() {
                                self.screen = Screen::Settings;
                            }
                        });
                    });

                // Bakhjulen ovanifrån, nedre högra hörnet.
                if self.show_guide {
                    let deg = guide::steering_deg(self.net.fresh_status());
                    egui::Area::new("topview".into())
                        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
                        .show(ctx, |ui| {
                            egui::Frame::none()
                                .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 175))
                                .inner_margin(egui::Margin::symmetric(8.0, 8.0))
                                .rounding(8.0)
                                .show(ui, |ui| guide::draw_topview(ui, deg));
                        });
                }

                if !self.activated {
                    egui::Area::new("activate".into())
                        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                        .show(ctx, |ui| {
                            if !self.gamepad_connected {
                                // Fix efter regressionstest 2026-09-19 (Dator
                                // 2): utan det här visade knappen bara
                                // AKTIVERA, gick att trycka, men föll
                                // omedelbart tillbaka varje bildruta eftersom
                                // `activated` nollställs utan dosa (spec §6)
                                // — kändes trasigt istället för avsiktligt.
                                ui.vertical_centered(|ui| {
                                    let frame = egui::Frame::none()
                                        .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180))
                                        .inner_margin(egui::Margin::symmetric(24.0, 16.0))
                                        .rounding(8.0);
                                    frame.show(ui, |ui| {
                                        ui.colored_label(
                                            egui::Color32::from_rgb(200, 140, 220),
                                            egui::RichText::new("Ingen dosa ansluten").size(20.0).strong(),
                                        );
                                    });
                                });
                                return;
                            }
                            let btn = egui::Button::new(
                                egui::RichText::new("AKTIVERA").size(32.0).strong(),
                            )
                            .fill(egui::Color32::from_rgb(40, 140, 60))
                            .min_size(egui::vec2(220.0, 70.0));
                            if ui.add_enabled(self.can_activate(), btn).clicked() {
                                self.activated = true;
                                self.last_activity = Instant::now();
                            }
                            if let Some(why) = self.activate_blocker() {
                                ui.colored_label(egui::Color32::from_rgb(255, 170, 90), why);
                            }
                        });
                }
            });

        self.draw_logs_window(ctx);

        // Rita ramen SIST, på ett eget lager ovanpå allt annat innehåll.
        // Tidigare låg den som Frame-stroke bakom kamerabildens
        // rect_filled, så den syntes aldrig (Fel 2, testrapport 2026-09-19).
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("border")))
            .rect_stroke(ctx.screen_rect(), 0.0, egui::Stroke::new(6.0_f32, border));
    }

    /// För tester: `ROBOTSTYRNING_SKARMBILD=fil.ppm` sparar en skärmbild av fönstret
    /// efter 10 s (eller `ROBOTSTYRNING_SKARMBILD_S`) och avslutar programmet.
    /// `ROBOTSTYRNING_LOGGFONSTER=1` öppnar körloggsfönstret direkt.
    fn test_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("ROBOTSTYRNING_SKARMBILD") else { return };
        let secs = std::env::var("ROBOTSTYRNING_SKARMBILD_S").ok().and_then(|v| v.parse().ok()).unwrap_or(10u64);
        if std::env::var_os("ROBOTSTYRNING_LOGGFONSTER").is_some()
            && !self.logs_open
            && self.started.elapsed() > Duration::from_secs(secs / 2)
            && matches!(self.screen, Screen::Driving)
        {
            self.logs_open = true;
            self.net.list_drive_logs();
        }
        let image = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let mut ppm = format!("P6\n{} {}\n255\n", img.size[0], img.size[1]).into_bytes();
            for p in &img.pixels {
                ppm.extend([p.r(), p.g(), p.b()]);
            }
            let _ = std::fs::write(path, ppm);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if !self.screenshot_requested && self.started.elapsed() > Duration::from_secs(secs) {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    /// Körloggarna på roboten: lista, hämta och spara som CSV i Hämtningar.
    fn draw_logs_window(&mut self, ctx: &egui::Context) {
        if let Some(reply) = self.net.drive_log_reply.take() {
            match reply {
                net::DriveLogReply::List(l) => {
                    self.logs_message = if l.is_empty() {
                        "Inga körloggar på roboten ännu.".to_string()
                    } else {
                        format!("{} körloggar (nyaste först).", l.len())
                    };
                    self.logs = l;
                }
                net::DriveLogReply::File { name, csv } => {
                    self.logs_message = match save_drive_log(&name, &csv) {
                        Ok(path) => format!("Sparad: {}", path.display()),
                        Err(e) => format!("Kunde inte spara {name}: {e}"),
                    };
                }
                net::DriveLogReply::Error(e) => self.logs_message = format!("Fel: {e}"),
            }
        }
        if !self.logs_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Körloggar")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                ui.label("En logg per körning, en rad per sekund (fart, batteri, styrning, lutning, fel, signal).");
                ui.horizontal(|ui| {
                    if ui.button("Uppdatera").clicked() {
                        self.logs_message = "Hämtar listan…".to_string();
                        self.net.list_drive_logs();
                    }
                    if ui.button("Öppna mappen").clicked() {
                        if let Some(dir) = drive_log_dir() {
                            let _ = std::fs::create_dir_all(&dir);
                            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
                        }
                    }
                });
                ui.label(egui::RichText::new(&self.logs_message).weak());
                ui.separator();
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    let mut fetch: Option<String> = None;
                    for log in &self.logs {
                        ui.horizontal(|ui| {
                            ui.monospace(&log.name);
                            ui.label(egui::RichText::new(format!("{:.0} kB", log.bytes as f64 / 1024.0)).weak());
                            if ui.button("Spara").clicked() {
                                fetch = Some(log.name.clone());
                            }
                        });
                    }
                    if let Some(name) = fetch {
                        self.logs_message = format!("Hämtar {name}…");
                        self.net.get_drive_log(&name);
                    }
                });
            });
        self.logs_open = open;
    }

    /// Kamerabild + drift-/styr-VESC måste vara OK innan Aktivera går att
    /// trycka. GPS/temp/övrigt varnar bara, blockerar inte.
    /// VESC-kravet (drift/styr svarar) gäller ALLTID, oavsett läge — det
    /// handlar om att roboten mekaniskt går att styra, inte om föraren ser
    /// den. Kamerakravet gäller bara i CAM-läge; i LOS-läge släpps det,
    /// eftersom föraren då litar på egen sikt istället för skärmen.
    fn can_activate(&self) -> bool {
        self.activate_blocker().is_none()
    }

    /// Varför AKTIVERA inte går att trycka, eller `None` om allt är OK.
    fn activate_blocker(&self) -> Option<String> {
        // VESC-kravet gäller i båda lägena: alla VESC som ska finnas på
        // roboten måste ha skickat CAN-status nyss (enligt färsk status från robotd).
        let Some(status) = self.net.fresh_status() else {
            return Some("Väntar på status från roboten…".to_string());
        };
        if let Some(text) = hud::estop_text(status.estop.as_deref()) {
            return Some(text.to_string());
        }
        if let Some(err) = &status.last_error {
            if status.vescs_responding.is_empty() {
                return Some(err.clone());
            }
        }
        let missing: Vec<u8> = status
            .vescs_expected
            .iter()
            .copied()
            .filter(|id| !status.vescs_responding.contains(id))
            .collect();
        if !missing.is_empty() {
            return Some(format!("VESC svarar inte: {missing:?}"));
        }
        if status.vescs_expected.is_empty() && status.vescs_responding.is_empty() {
            return Some("Ingen VESC svarar".to_string());
        }
        // Kamerakravet gäller bara i CAM-läge: en NY bild måste ha kommit nyligen.
        if self.driving_mode == DrivingMode::Cam && !self.video.is_live() {
            return Some("Ingen kamerabild – byt till LOS för att köra utan".to_string());
        }
        None
    }
}

/// Mappen där hämtade körloggar sparas: Hämtningar/Robotstyrning-körloggar.
fn drive_log_dir() -> Option<std::path::PathBuf> {
    let base = directories::UserDirs::new()
        .and_then(|u| u.download_dir().map(|d| d.to_path_buf()))
        .or_else(|| directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()))?;
    Some(base.join("Robotstyrning-körloggar"))
}

fn save_drive_log(name: &str, csv: &str) -> Result<std::path::PathBuf, String> {
    // Namnet kommer från roboten: bara filnamnet, aldrig en sökväg.
    let file = std::path::Path::new(name).file_name().ok_or("ogiltigt namn")?;
    let dir = drive_log_dir().ok_or("hittar ingen Hämtningar-mapp")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(file);
    std::fs::write(&path, csv).map_err(|e| e.to_string())?;
    Ok(path)
}

fn max_output_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("se", "robot-control", "robotstyrning")
        .map(|d| d.config_dir().join("max.json"))
}

/// Förarens Max (bara farten), 0.30 om inget sparats.
fn load_max_output() -> f32 {
    max_output_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<f32>(&s).ok())
        .filter(|v| v.is_finite())
        .map(|v| v.clamp(0.05, 1.0))
        .unwrap_or(0.30)
}

fn save_max_output(v: f32) {
    if let Some(path) = max_output_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, format!("{v:.2}"));
    }
}

fn known_robots_path() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("se", "robot-control", "robotstyrning")
        .map(|d| d.config_dir().join("kanda_robotar.json"))
}

fn load_known_robots() -> Vec<String> {
    known_robots_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_known_robots(names: &[String]) {
    if let Some(path) = known_robots_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(names) {
            let _ = std::fs::write(path, json);
        }
    }
}
