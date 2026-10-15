//! Körlogg: en CSV-fil per körning med en rad per sekund (2026-10-05).
//!
//! En körning börjar när en klient ansluter och slutar när den kopplar ner. Filen
//! heter efter starttiden, t.ex. `2026-10-05_14-03-22.csv`, och ligger i
//! `VehicleConfig::drive_log_dir`. Loggar äldre än `drive_log_keep_days` tas bort.
//! Klienten listar och hämtar loggarna via robotd (`ListDriveLogs`/`GetDriveLog`).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use relay_protocol::{DriveLogInfo, StatusUpdate};

pub const HEADER: &str = "tid;aktiverad;gas;styr;fart_kmh;spanning_v;batteri_procent;laddar;effekt_w;\
wh_per_km;rackvidd_km;styrvinkel_grad;styrning_procent;roll_grad;pitch_grad;kurs_grad;vesc_temp_c;\
vesc_svarar;fel;rssi_dbm;signal;vesc_duty_rpm";

/// Största fil som skickas till klienten (en hel dag ≈ 86 400 rader ≈ 10 MB).
const MAX_SEND_BYTES: u64 = 16 * 1024 * 1024;

pub struct DriveLog {
    dir: PathBuf,
    file: Option<File>,
}

impl DriveLog {
    /// `dir` tom = ingen loggning. Städar bort gamla loggar.
    pub fn new(dir: &str, keep_days: u32) -> Self {
        let dir = PathBuf::from(dir);
        if !dir.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(&dir) {
                tracing::warn!("Körloggens mapp {} går inte att skapa: {e}", dir.display());
            }
            remove_old(&dir, Duration::from_secs(keep_days as u64 * 86_400));
        }
        Self { dir, file: None }
    }

    pub fn enabled(&self) -> bool {
        !self.dir.as_os_str().is_empty()
    }

    pub fn is_open(&self) -> bool {
        self.file.is_some()
    }

    /// Ny fil för en ny körning.
    pub fn start(&mut self) {
        if !self.enabled() {
            return;
        }
        let name = format!("{}.csv", chrono::Local::now().format("%Y-%m-%d_%H-%M-%S"));
        let path = self.dir.join(&name);
        match File::create(&path).and_then(|mut f| writeln!(f, "{HEADER}").map(|_| f)) {
            Ok(f) => {
                tracing::info!("Körlogg: {}", path.display());
                self.file = Some(f);
            }
            Err(e) => tracing::warn!("Körloggen {} går inte att skapa: {e}", path.display()),
        }
    }

    pub fn stop(&mut self) {
        self.file = None;
    }

    pub fn write(&mut self, row: &str) {
        if let Some(f) = &mut self.file {
            if let Err(e) = writeln!(f, "{row}") {
                tracing::warn!("Körloggen kunde inte skrivas ({e}), stänger den.");
                self.file = None;
            }
        }
    }
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn f1(v: Option<f32>) -> String {
    v.map(|x| format!("{x:.1}")).unwrap_or_default()
}

/// Gör om semikolon och radbrytningar så att CSV:n håller ihop.
fn clean(s: &str) -> String {
    s.replace([';', '\n', '\r'], " ")
}

/// En rad i loggen från statusen och förarens senaste kommando.
/// VESC:ernas duty och varvtal, t.ex. "28:0.30/1520 36:0.18/910" (för att se
/// differentialen: vänster och höger sida i samma rad som styrvinkeln).
pub fn vesc_text(vescs: &[crate::vesc_can::VescStatusEntry]) -> String {
    let mut v: Vec<_> = vescs.iter().filter(|e| e.is_fresh()).collect();
    v.sort_by_key(|e| e.can_id);
    v.iter().map(|e| format!("{}:{:.2}/{}", e.can_id, e.duty, e.rpm)).collect::<Vec<_>>().join(" ")
}

pub fn row(time: &str, activated: bool, throttle: f32, steering: f32, s: &StatusUpdate, vesc_values: &str) -> String {
    let temp = s.vesc_temps_c.iter().copied().reduce(f32::max);
    let vescs = s.vescs_responding.iter().map(u8::to_string).collect::<Vec<_>>().join(" ");
    let fault = s.fault_text.clone().or_else(|| s.last_error.clone()).unwrap_or_default();
    [
        time.to_string(),
        (activated as u8).to_string(),
        format!("{throttle:.2}"),
        format!("{steering:.2}"),
        f1(s.speed_kmh),
        s.battery_voltage.map(|v| format!("{v:.2}")).unwrap_or_default(),
        f1(s.battery_percent),
        s.battery_charging.map(|c| if c { "ja" } else { "nej" }.to_string()).unwrap_or_default(),
        f1(s.power_w),
        f1(s.wh_per_km),
        s.range_km.map(|v| format!("{v:.2}")).unwrap_or_default(),
        f1(s.steering_deg),
        f1(s.steering_percent),
        f1(s.roll_deg),
        f1(s.pitch_deg),
        f1(s.yaw_deg),
        f1(temp),
        vescs,
        clean(&fault),
        opt(s.rssi_dbm),
        clean(s.signal_info.as_deref().unwrap_or("")),
        clean(vesc_values),
    ]
    .join(";")
}

/// Ett giltigt loggnamn: bara datum/tid-tecken och `.csv`, inga sökvägar.
fn valid_name(name: &str) -> bool {
    name.ends_with(".csv")
        && name.len() < 64
        && name.chars().all(|c| c.is_ascii_digit() || c == '-' || c == '_' || c == '.' || c.is_ascii_alphabetic())
        && !name.contains("..")
}

pub fn list(dir: &Path) -> Result<Vec<DriveLogInfo>, String> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).map_err(|e| format!("körloggarna går inte att läsa: {e}"))? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let bytes = e.metadata().map(|m| m.len()).unwrap_or(0);
        out.push(DriveLogInfo { name, bytes });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name)); // namnet är tiden: nyaste först
    Ok(out)
}

pub fn read(dir: &Path, name: &str) -> Result<String, String> {
    if !valid_name(name) {
        return Err("ogiltigt loggnamn".to_string());
    }
    let path = dir.join(name);
    let len = fs::metadata(&path).map_err(|e| format!("{name}: {e}"))?.len();
    if len > MAX_SEND_BYTES {
        return Err(format!("{name} är för stor ({len} byte) för att skickas"));
    }
    fs::read_to_string(&path).map_err(|e| format!("{name}: {e}"))
}

fn remove_old(dir: &Path, keep: Duration) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_name(&name) {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > keep);
        if old {
            let _ = fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("robotd_korlogg_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn rad_har_lika_manga_falt_som_rubriken() {
        let s = StatusUpdate {
            speed_kmh: Some(3.24),
            battery_voltage: Some(48.25),
            battery_percent: Some(50.0),
            battery_charging: Some(true),
            fault_text: Some("Underspänning; ladda".into()),
            vescs_responding: vec![90, 87],
            rssi_dbm: Some(-71),
            ..Default::default()
        };
        let r = row("2026-10-05 14:03:22", true, 0.5, -0.25, &s, "28:0.30/1520 36:0.18/910");
        assert_eq!(r.split(';').count(), HEADER.split(';').count(), "{r}");
        assert!(r.contains(";3.2;48.25;50.0;ja;"));
        assert!(r.contains(";90 87;Underspänning  ladda;-71;"));
    }

    #[test]
    fn skriva_lista_och_lasa() {
        let d = tmpdir("skriv");
        let mut log = DriveLog::new(d.to_str().unwrap(), 90);
        log.start();
        assert!(log.is_open());
        log.write("rad1");
        log.stop();
        let l = list(&d).unwrap();
        assert_eq!(l.len(), 1);
        let csv = read(&d, &l[0].name).unwrap();
        assert!(csv.starts_with("tid;") && csv.contains("rad1"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn farliga_namn_nekas() {
        let d = tmpdir("namn");
        fs::create_dir_all(&d).unwrap();
        for bad in ["../etc/passwd", "/etc/shadow.csv", "a/../../b.csv", "x.txt", "..csv"] {
            assert!(read(&d, bad).is_err(), "{bad}");
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn avstangd_utan_mapp() {
        let mut log = DriveLog::new("", 90);
        assert!(!log.enabled());
        log.start();
        assert!(!log.is_open());
    }
}
