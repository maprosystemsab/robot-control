//! Kartan i LOS-läge: satellitbild med roboten som en pil och ett spår efter den,
//! i stället för en svart skärm när det inte finns någon kamerabild.
//!
//! Kartbilderna är samma som i RControlStation (Esri World Imagery). De hämtas i
//! bakgrunden och sparas på datorn, så en plats man varit på förut visas även
//! utan internet. Positionen kommer i robotens statusmeddelande (`lat`/`lon`,
//! robotd från 2026-11-26); en robot med äldre programvara ger ingen position.
//!
//! Musen: dra för att flytta kartan, rulla för att zooma, dubbelklicka (eller
//! knappen Följ roboten) för att låta kartan följa roboten igen.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use relay_protocol::StatusUpdate;
use serde::{Deserialize, Serialize};

/// Samma kartserver som RControlStation (`mapwidget.cpp`): …/tile/{zoom}/{y}/{x}.
const KARTSERVER: &str = "https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/";
const RUTA_PUNKTER: f64 = 256.0;
/// Finaste nivån som hämtas; närmare än så förstoras bilderna.
const MAX_RUTZOOM: u8 = 19;
const MIN_ZOOM: f32 = 3.0;
const MAX_ZOOM: f32 = 21.0;
const STANDARDZOOM: f32 = 18.0;
/// Kartbilder i minnet (à 0,26 MB). Äldst använda slängs först.
const MAX_RUTOR_I_MINNET: usize = 250;
const HAMTARE: usize = 3;
/// En bild som inte gick att hämta provas igen efter så här lång tid.
const FORSOK_IGEN: Duration = Duration::from_secs(30);
const SPAR_MAX: usize = 20_000;
const SPAR_STEG_M: f64 = 0.25;

/// Zoomnivå, x, y.
type Nyckel = (u8, u32, u32);

/// Web Mercator: latitud/longitud (grader) -> läge i världen, 0..1 åt båda håll
/// (x österut från 180° väst, y söderut från ca 85° nord).
pub fn till_varld(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.clamp(-85.0511, 85.0511).to_radians();
    let x = (lon + 180.0) / 360.0;
    let y = (1.0 - lat.tan().asinh() / std::f64::consts::PI) / 2.0;
    (x, y)
}

pub fn fran_varld(x: f64, y: f64) -> (f64, f64) {
    let lon = x * 360.0 - 180.0;
    let lat = (std::f64::consts::PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees();
    (lat, lon)
}

/// Meter på marken per skärmpunkt vid en viss latitud och zoom.
pub fn meter_per_punkt(lat: f64, zoom: f32) -> f64 {
    40_075_016.686 * lat.to_radians().cos() / (RUTA_PUNKTER * 2f64.powf(zoom as f64))
}

/// Avstånd i meter mellan två närliggande punkter (lat, lon).
fn avstand_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let dy = (b.0 - a.0) * 111_320.0;
    let dx = (b.1 - a.1) * 111_320.0 * a.0.to_radians().cos();
    (dx * dx + dy * dy).sqrt()
}

/// Skalstreckets längd: det jämnaste värdet (1, 2, 5, 10 … m) som blir högst
/// `max_punkter` brett. Ger (meter, bredd i punkter).
pub fn skalstreck(meter_per_punkt: f64, max_punkter: f64) -> (f64, f64) {
    let max_m = meter_per_punkt * max_punkter;
    let mut bast = 0.1;
    let mut tiotal = 0.1;
    while tiotal <= max_m {
        for steg in [1.0, 2.0, 5.0] {
            if tiotal * steg <= max_m {
                bast = tiotal * steg;
            }
        }
        tiotal *= 10.0;
    }
    (bast, bast / meter_per_punkt)
}

pub fn skaltext(meter: f64) -> String {
    if meter >= 1000.0 {
        format!("{:.0} km", meter / 1000.0)
    } else if meter >= 1.0 {
        format!("{meter:.0} m")
    } else {
        format!("{:.0} cm", meter * 100.0)
    }
}

/// Robotens läge ur statusmeddelandet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub lat: f64,
    pub lon: f64,
    /// GGA-kvalitet: 1 = GPS, 2 = DGPS, 4 = RTK Fix, 5 = RTK Float.
    pub kvalitet: Option<u8>,
    pub satelliter: Option<u8>,
    /// Kompasskurs, 0 = norr.
    pub kurs: Option<f32>,
}

impl Position {
    pub fn fran_status(s: &StatusUpdate) -> Option<Self> {
        let (lat, lon) = (s.lat?, s.lon?);
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) || (lat == 0.0 && lon == 0.0) {
            return None;
        }
        Some(Self { lat, lon, kvalitet: s.gps_quality, satelliter: s.gps_sats, kurs: s.heading_deg })
    }

    /// Text och färg för hur säker positionen är.
    pub fn kvalitetstext(&self) -> (String, Color32) {
        let sat = self.satelliter.map(|n| format!(" · {n} satelliter")).unwrap_or_default();
        match self.kvalitet {
            Some(4) => (format!("RTK Fix{sat}"), Color32::from_rgb(90, 220, 110)),
            Some(5) => (format!("RTK Float – några decimeter fel{sat}"), Color32::from_rgb(240, 210, 60)),
            _ => (format!("GPS utan RTK – flera meter fel{sat}"), Color32::from_rgb(255, 150, 60)),
        }
    }
}

enum Ruta {
    Klar { bild: egui::TextureHandle, anvand: u64 },
    Misslyckad(Instant),
}

#[derive(Default)]
struct Ko {
    /// Bilder som syns just nu och saknas, närmast mitten först. Skrivs över varje
    /// bildruta, så det som rullat ur bild hämtas aldrig i onödan.
    vill: Vec<Nyckel>,
    pagar: HashSet<Nyckel>,
    stang: bool,
}

type DeladKo = Arc<(Mutex<Ko>, Condvar)>;
type Svar = (Nyckel, Option<egui::ColorImage>);

#[derive(Serialize, Deserialize)]
struct SparadVy {
    lat: f64,
    lon: f64,
    zoom: f32,
}

pub struct Karta {
    rutor: HashMap<Nyckel, Ruta>,
    ko: DeladKo,
    klara: Option<mpsc::Receiver<Svar>>,
    bildruta: u64,
    zoom: f32,
    /// Kartans mittpunkt i världen (0..1).
    mitt: (f64, f64),
    /// Kartan följer roboten (slås av när man drar i kartan).
    folj: bool,
    /// Var roboten har kört sedan anslutningen (lat, lon).
    spar: VecDeque<(f64, f64)>,
    /// Senast kända position, även efter att den försvunnit.
    senast: Option<Position>,
    vy_sparad: Instant,
    vy_andrad: bool,
    hamtfel: bool,
}

impl Default for Karta {
    fn default() -> Self {
        // Börja där kartan stod sist; första gången visas hela Sverige.
        let vy = ladda_vy().unwrap_or(SparadVy { lat: 62.0, lon: 16.0, zoom: 5.0 });
        Self {
            rutor: HashMap::new(),
            ko: Arc::default(),
            klara: None,
            bildruta: 0,
            zoom: vy.zoom.clamp(MIN_ZOOM, MAX_ZOOM),
            mitt: till_varld(vy.lat, vy.lon),
            folj: true,
            spar: VecDeque::new(),
            senast: None,
            vy_sparad: Instant::now(),
            vy_andrad: false,
            hamtfel: false,
        }
    }
}

impl Drop for Karta {
    fn drop(&mut self) {
        self.spara_vy();
        self.ko.0.lock().unwrap().stang = true;
        self.ko.1.notify_all();
    }
}

impl Karta {
    /// Ny anslutning: spåret börjar om och kartan följer roboten.
    pub fn ny_anslutning(&mut self) {
        self.spar.clear();
        self.senast = None;
        self.folj = true;
    }

    fn starta_hamtare(&mut self, ctx: &egui::Context) {
        if self.klara.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.klara = Some(rx);
        let mapp = bildmapp();
        for i in 0..HAMTARE {
            let (ko, tx, ctx, mapp) = (self.ko.clone(), tx.clone(), ctx.clone(), mapp.clone());
            let _ = std::thread::Builder::new()
                .name(format!("kartbilder-{i}"))
                .spawn(move || hamtare(ko, tx, ctx, mapp));
        }
    }

    fn satt_zoom(&mut self, zoom: f32) {
        let zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if zoom != self.zoom {
            self.zoom = zoom;
            self.vy_andrad = true;
        }
    }

    fn spara_vy(&mut self) {
        if !self.vy_andrad {
            return;
        }
        self.vy_andrad = false;
        self.vy_sparad = Instant::now();
        let (lat, lon) = fran_varld(self.mitt.0, self.mitt.1);
        if let (Some(p), Ok(json)) = (vyfil(), serde_json::to_string(&SparadVy { lat, lon, zoom: self.zoom })) {
            if let Some(d) = p.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(p, json);
        }
    }

    /// Ritar kartan i `rect`. `pos` är robotens position just nu, om den finns.
    pub fn visa(&mut self, ui: &mut egui::Ui, rect: Rect, pos: Option<Position>) {
        self.starta_hamtare(ui.ctx());
        self.bildruta += 1;

        // Nyhämtade bilder blir texturer.
        if let Some(rx) = &self.klara {
            while let Ok(((z, x, y), bild)) = rx.try_recv() {
                let ruta = match bild {
                    Some(b) => Ruta::Klar {
                        bild: ui.ctx().load_texture(format!("karta-{z}-{x}-{y}"), b, egui::TextureOptions::LINEAR),
                        anvand: self.bildruta,
                    },
                    None => Ruta::Misslyckad(Instant::now()),
                };
                self.rutor.insert((z, x, y), ruta);
            }
        }

        if let Some(p) = pos {
            if self.senast.is_none() && self.zoom < 15.0 {
                // Första positionen och kartan står långt utzoomad: gå nära.
                self.zoom = STANDARDZOOM;
            }
            if self.spar.back().map_or(true, |s| avstand_m(*s, (p.lat, p.lon)) >= SPAR_STEG_M) {
                self.spar.push_back((p.lat, p.lon));
                if self.spar.len() > SPAR_MAX {
                    self.spar.pop_front();
                }
            }
            self.senast = Some(p);
            if self.folj {
                let ny = till_varld(p.lat, p.lon);
                if ny != self.mitt {
                    self.mitt = ny;
                    self.vy_andrad = true;
                }
            }
        }

        // Musen: dra = flytta, rulla = zooma, dubbelklick = följ roboten igen.
        let svar = ui.interact(rect, ui.id().with("karta"), egui::Sense::click_and_drag());
        let skala = RUTA_PUNKTER * 2f64.powf(self.zoom as f64); // punkter per värld
        if svar.dragged() {
            let d = svar.drag_delta();
            if d != egui::Vec2::ZERO {
                self.mitt.0 = (self.mitt.0 - d.x as f64 / skala).clamp(0.0, 1.0);
                self.mitt.1 = (self.mitt.1 - d.y as f64 / skala).clamp(0.0, 1.0);
                self.folj = false;
                self.vy_andrad = true;
            }
        }
        if svar.double_clicked() {
            self.folj = true;
        }
        if svar.hovered() {
            let (rull, nyp) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if rull != 0.0 {
                self.satt_zoom(self.zoom + rull / 100.0);
            }
            if nyp != 1.0 {
                self.satt_zoom(self.zoom + nyp.log2());
            }
        }
        if self.vy_andrad && self.vy_sparad.elapsed() > Duration::from_secs(20) {
            self.spara_vy();
        }

        let skala = RUTA_PUNKTER * 2f64.powf(self.zoom as f64);
        let mitt = self.mitt;
        // Kartans mittpunkt (roboten, när kartan följer den) ritas en bit under
        // skärmens mitt, så att den inte hamnar bakom AKTIVERA-knappen.
        let c = rect.center() + egui::vec2(0.0, (rect.height() * 0.17).min(130.0));
        let till_skarm = |v: (f64, f64)| -> Pos2 {
            Pos2::new(c.x + ((v.0 - mitt.0) * skala) as f32, c.y + ((v.1 - mitt.1) * skala) as f32)
        };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_gray(24));

        // --- Kartbilderna.
        let zt = (self.zoom.round() as u8).min(MAX_RUTZOOM);
        let n = (1u32 << zt) as f64;
        let index = |v: f64| (v * n).floor().clamp(0.0, n - 1.0) as u32;
        let (x0, x1) = (
            index(mitt.0 - (c.x - rect.left()) as f64 / skala),
            index(mitt.0 + (rect.right() - c.x) as f64 / skala),
        );
        let (y0, y1) = (
            index(mitt.1 - (c.y - rect.top()) as f64 / skala),
            index(mitt.1 + (rect.bottom() - c.y) as f64 / skala),
        );
        let hel = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        let mut saknas: Vec<(f64, Nyckel)> = Vec::new();
        self.hamtfel = false;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dest = Rect::from_min_max(
                    till_skarm((x as f64 / n, y as f64 / n)),
                    till_skarm(((x + 1) as f64 / n, (y + 1) as f64 / n)),
                );
                let nyckel = (zt, x, y);
                match self.rutor.get_mut(&nyckel) {
                    Some(Ruta::Klar { bild, anvand }) => {
                        *anvand = self.bildruta;
                        painter.image(bild.id(), dest, hel, Color32::WHITE);
                        continue;
                    }
                    Some(Ruta::Misslyckad(nar)) if nar.elapsed() < FORSOK_IGEN => self.hamtfel = true,
                    _ => {
                        let avst = (x as f64 + 0.5 - mitt.0 * n).hypot(y as f64 + 0.5 - mitt.1 * n);
                        saknas.push((avst, nyckel));
                    }
                }
                // Bilden finns inte (än): visa en grövre bild över samma yta om en sådan finns.
                for d in 1..=6u8.min(zt) {
                    if let Some(Ruta::Klar { bild, anvand }) = self.rutor.get_mut(&(zt - d, x >> d, y >> d)) {
                        *anvand = self.bildruta;
                        let delar = (1u32 << d) as f32;
                        let (ux, uy) = ((x & ((1 << d) - 1)) as f32 / delar, (y & ((1 << d) - 1)) as f32 / delar);
                        let uv = Rect::from_min_max(Pos2::new(ux, uy), Pos2::new(ux + 1.0 / delar, uy + 1.0 / delar));
                        painter.image(bild.id(), dest, uv, Color32::WHITE);
                        break;
                    }
                }
            }
        }
        saknas.sort_by(|a, b| a.0.total_cmp(&b.0));
        {
            let mut ko = self.ko.0.lock().unwrap();
            ko.vill = saknas.into_iter().map(|(_, k)| k).collect();
            if !ko.vill.is_empty() {
                self.ko.1.notify_all();
            }
        }
        if self.rutor.len() > MAX_RUTOR_I_MINNET {
            // Släng det som inte ritats på längst, ner till tre fjärdedelar.
            let mut alder: Vec<(u64, Nyckel)> = self
                .rutor
                .iter()
                .map(|(k, r)| (if let Ruta::Klar { anvand, .. } = r { *anvand } else { 0 }, *k))
                .collect();
            alder.sort_unstable();
            for (_, k) in alder.into_iter().take(self.rutor.len() - MAX_RUTOR_I_MINNET * 3 / 4) {
                self.rutor.remove(&k);
            }
        }

        // --- Spåret.
        if self.spar.len() >= 2 {
            let punkter: Vec<Pos2> = self.spar.iter().map(|(lat, lon)| till_skarm(till_varld(*lat, *lon))).collect();
            painter.add(egui::Shape::line(punkter.clone(), Stroke::new(5.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 110))));
            painter.add(egui::Shape::line(punkter, Stroke::new(2.5_f32, Color32::from_rgb(90, 190, 255))));
        }

        // --- Roboten: pil med färg efter hur säker positionen är. Utan aktuell
        // position ritas den senast kända platsen grå.
        if let Some(p) = pos.or(self.senast) {
            let farg = if pos.is_some() { p.kvalitetstext().1 } else { Color32::from_gray(150) };
            let m = till_skarm(till_varld(p.lat, p.lon));
            let kant = Stroke::new(2.0_f32, Color32::BLACK);
            match p.kurs {
                Some(kurs) => {
                    let k = kurs.to_radians();
                    let fram = egui::vec2(k.sin(), -k.cos());
                    let sida = egui::vec2(-fram.y, fram.x);
                    let (spets, bak) = (m + fram * 18.0, m - fram * 5.0);
                    let (v, h) = (m - fram * 13.0 - sida * 12.0, m - fram * 13.0 + sida * 12.0);
                    painter.add(egui::Shape::convex_polygon(vec![spets, h, bak], farg, Stroke::NONE));
                    painter.add(egui::Shape::convex_polygon(vec![spets, bak, v], farg, Stroke::NONE));
                    painter.add(egui::Shape::closed_line(vec![spets, h, bak, v], kant));
                }
                None => {
                    painter.circle(m, 9.0, farg, kant);
                }
            }
        }
    }

    /// Knappar, positionens kvalitet och skalstreck: nedre högra hörnet.
    pub fn visa_reglage(&mut self, ctx: &egui::Context, pos: Option<Position>) {
        egui::Area::new("kartreglage".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                egui::Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(0, 0, 0, 175))
                    .inner_margin(egui::Margin::symmetric(12.0, 10.0))
                    .rounding(8.0)
                    .show(ui, |ui| {
                        ui.set_max_width(330.0);
                        ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                            let (text, farg) = match (pos, self.senast) {
                                (Some(p), _) => p.kvalitetstext(),
                                (None, Some(_)) => (
                                    "Ingen position just nu – senast kända plats visas".to_string(),
                                    Color32::from_rgb(255, 120, 120),
                                ),
                                (None, None) => (
                                    "Ingen position från roboten".to_string(),
                                    Color32::from_rgb(255, 120, 120),
                                ),
                            };
                            ui.label(egui::RichText::new(text).color(farg).strong());
                            if pos.is_none() && self.senast.is_none() {
                                ui.label(
                                    egui::RichText::new("(ingen GPS-position, eller äldre programvara i roboten)")
                                        .color(Color32::GRAY)
                                        .small(),
                                );
                            }
                            if self.hamtfel {
                                ui.label(
                                    egui::RichText::new("Kartbilder kunde inte hämtas – inget internet?")
                                        .color(Color32::from_rgb(240, 210, 60))
                                        .small(),
                                );
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                let knapp = |t: &str| egui::Button::new(egui::RichText::new(t).size(18.0)).min_size(egui::vec2(34.0, 30.0));
                                // Raden fylls från höger: + längst ut, sedan –, sedan Följ.
                                if ui.add(knapp("+")).on_hover_text("Zooma in (eller rulla med musen)").clicked() {
                                    self.satt_zoom((self.zoom + 1.0).round());
                                }
                                if ui.add(knapp("–")).on_hover_text("Zooma ut (eller rulla med musen)").clicked() {
                                    self.satt_zoom((self.zoom - 1.0).round());
                                }
                                if !self.folj
                                    && ui
                                        .add(egui::Button::new("Följ roboten").min_size(egui::vec2(0.0, 30.0)))
                                        .on_hover_text("Låt kartan följa roboten igen (eller dubbelklicka i kartan)")
                                        .clicked()
                                {
                                    self.folj = true;
                                }
                            });
                            ui.add_space(6.0);
                            // Skalstreck.
                            let lat = fran_varld(self.mitt.0, self.mitt.1).0;
                            let (meter, bredd) = skalstreck(meter_per_punkt(lat, self.zoom), 140.0);
                            let (r, _) = ui.allocate_exact_size(egui::vec2(bredd as f32, 8.0), egui::Sense::hover());
                            let vit = Stroke::new(2.0_f32, Color32::WHITE);
                            ui.painter().line_segment([r.left_bottom(), r.right_bottom()], vit);
                            ui.painter().line_segment([r.left_top(), r.left_bottom()], vit);
                            ui.painter().line_segment([r.right_top(), r.right_bottom()], vit);
                            ui.label(egui::RichText::new(skaltext(meter)).color(Color32::WHITE).small());
                            ui.label(
                                egui::RichText::new("Kartbilder: Esri, Maxar, Earthstar Geographics")
                                    .color(Color32::GRAY)
                                    .size(9.0),
                            );
                        });
                    });
            });
    }
}

fn projektmappar() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("se", "robot-control", "robotstyrning")
}

/// Här sparas hämtade kartbilder: {zoom}/{y}/{x}.jpeg (samma uppdelning som RControlStation).
fn bildmapp() -> Option<PathBuf> {
    projektmappar().map(|d| d.cache_dir().join("kartbilder"))
}

fn vyfil() -> Option<PathBuf> {
    projektmappar().map(|d| d.config_dir().join("karta.json"))
}

fn ladda_vy() -> Option<SparadVy> {
    let vy: SparadVy = serde_json::from_str(&std::fs::read_to_string(vyfil()?).ok()?).ok()?;
    ((-90.0..=90.0).contains(&vy.lat) && (-180.0..=180.0).contains(&vy.lon) && vy.zoom.is_finite()).then_some(vy)
}

/// En bakgrundstråd: tar den mest efterfrågade bilden som ingen annan hämtar,
/// läser den från disken eller nätet och skickar den till kartan.
fn hamtare(ko: DeladKo, tx: mpsc::Sender<Svar>, ctx: egui::Context, mapp: Option<PathBuf>) {
    // Allmänt klientnamn, som RControlStation ("Firefox"): inget om datorn eller roboten skickas.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) kartklient")
        .build();
    loop {
        let nyckel = {
            let mut k = ko.0.lock().unwrap();
            loop {
                if k.stang {
                    return;
                }
                if let Some(i) = k.vill.iter().position(|n| !k.pagar.contains(n)) {
                    let n = k.vill.remove(i);
                    k.pagar.insert(n);
                    break n;
                }
                k = ko.1.wait(k).unwrap();
            }
        };
        let bild = hamta_ruta(&agent, mapp.as_deref(), nyckel);
        let stangd = tx.send((nyckel, bild)).is_err();
        {
            let mut k = ko.0.lock().unwrap();
            k.pagar.remove(&nyckel);
            k.vill.retain(|n| *n != nyckel);
        }
        if stangd {
            return;
        }
        ctx.request_repaint();
    }
}

fn hamta_ruta(agent: &ureq::Agent, mapp: Option<&Path>, (z, x, y): Nyckel) -> Option<egui::ColorImage> {
    let fil = mapp.map(|m| m.join(z.to_string()).join(y.to_string()).join(format!("{x}.jpeg")));
    if let Some(bild) = fil.as_ref().and_then(|f| std::fs::read(f).ok()).and_then(|b| avkoda(&b)) {
        return Some(bild);
    }
    let svar = agent.get(&format!("{KARTSERVER}{z}/{y}/{x}")).call().ok()?;
    let mut data = Vec::new();
    svar.into_reader().take(4_000_000).read_to_end(&mut data).ok()?;
    let bild = avkoda(&data)?;
    if let Some(fil) = fil {
        // Skriv färdigt under ett annat namn först, så en halv fil aldrig blir kvar.
        let tillf = fil.with_extension(format!("del{:?}", std::thread::current().id()));
        if fil.parent().is_some_and(|d| std::fs::create_dir_all(d).is_ok()) && std::fs::write(&tillf, &data).is_ok() {
            let _ = std::fs::rename(&tillf, &fil);
        }
    }
    Some(bild)
}

fn avkoda(data: &[u8]) -> Option<egui::ColorImage> {
    let bild = image::load_from_memory(data).ok()?.to_rgb8();
    let storlek = [bild.width() as usize, bild.height() as usize];
    // En kartbild är 256×256; allt annat (felsida, trasig fil) räknas som ingen bild.
    (storlek[0] > 0 && storlek[0] <= 1024 && storlek[1] > 0 && storlek[1] <= 1024)
        .then(|| egui::ColorImage::from_rgb(storlek, bild.as_raw()))
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn varlden_fram_och_tillbaka() {
        assert_eq!(till_varld(0.0, 0.0), (0.5, 0.5));
        for (lat, lon) in [(59.838653460, 17.789013447), (60.06314934, 18.07893415), (-33.5, -70.5)] {
            let (x, y) = till_varld(lat, lon);
            let (lat2, lon2) = fran_varld(x, y);
            assert!((lat - lat2).abs() < 1e-9 && (lon - lon2).abs() < 1e-9, "{lat} {lon} -> {lat2} {lon2}");
        }
        // Lövsta ligger i kartbild 72012/38180 på nivå 17 (norr om ekvatorn = övre halvan).
        let (x, y) = till_varld(59.838653460, 17.789013447);
        assert_eq!(((x * 131072.0) as u32, (y * 131072.0) as u32), (72012, 38180));
    }

    #[test]
    fn skalan() {
        // En punkt på nivå 0 vid ekvatorn är 156 543 m.
        assert!((meter_per_punkt(0.0, 0.0) - 156_543.03).abs() < 0.1);
        // Nivå 18 i Uppland: ca 0,3 m per punkt.
        let m = meter_per_punkt(59.84, 18.0);
        assert!((0.29..0.31).contains(&m), "{m}");
        assert_eq!(skalstreck(0.3, 140.0).0, 20.0); // 42 m ryms -> 20 m
        assert_eq!(skalstreck(0.04, 140.0).0, 5.0); // 5,6 m ryms -> 5 m
        assert_eq!(skalstreck(40.0, 140.0).0, 5000.0);
        assert_eq!(skaltext(20.0), "20 m");
        assert_eq!(skaltext(5000.0), "5 km");
        assert_eq!(skaltext(0.5), "50 cm");
    }

    #[test]
    fn avstand() {
        // 0,00001° latitud är ca 1,1 m.
        let d = avstand_m((59.84, 17.79), (59.84001, 17.79));
        assert!((d - 1.113).abs() < 0.01, "{d}");
    }

    #[test]
    fn position_ur_status() {
        let mut s = StatusUpdate::default();
        assert_eq!(Position::fran_status(&s), None);
        s.lat = Some(59.84);
        s.lon = Some(17.79);
        s.gps_quality = Some(5);
        s.heading_deg = Some(90.0);
        let p = Position::fran_status(&s).unwrap();
        assert_eq!((p.lat, p.lon, p.kurs), (59.84, 17.79, Some(90.0)));
        assert!(p.kvalitetstext().0.starts_with("RTK Float"));
        // 0,0 är en mottagare utan position, inte en plats i Guineabukten.
        s.lat = Some(0.0);
        s.lon = Some(0.0);
        assert_eq!(Position::fran_status(&s), None);
    }
}
