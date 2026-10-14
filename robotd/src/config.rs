//! Robotens sparade konfiguration: namn/ID, VESC-rollmappning, adressen
//! robotd lyssnar på (över WireGuard-VPN:et, se PROJECT_SPEC.md §14).
//! Sparas som JSON på disk (t.ex. /etc/robotd/config.json), läses in vid
//! start, kan skrivas om av Settings-vyn i klienten (via robotd).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VescRole {
    DriftVanster,
    DriftHoger,
    Styrning,
    /// Fri roll, t.ex. lastarm/tilt. `input` beskriver vilken kontrollindata
    /// (t.ex. "l1_l2" eller "r1_r2") som styr den, plus gränser.
    Annat {
        namn: String,
        input: String,
        min: f32,
        max: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VescProfile {
    /// VESC CAN-ID -> roll
    pub roller: HashMap<u8, VescRole>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)] // Se load_or_default: saknade fält fylls från Default
                   // istället för att hela filen kastas — viktigt för
                   // framtida fält som läggs till efter att ett SD-kort
                   // redan flashats med en äldre config.json.
pub struct RobotConfig {
    pub robot_id: String,
    /// Adress robotd lyssnar på, t.ex. robotens WireGuard-IP + port
    /// ("192.168.200.8:9000"), eller "0.0.0.0:9000" med en brandväggsregel
    /// som bara släpper in trafik på wg0 (se PROJECT_SPEC.md §14).
    pub bind_addr: String,
    pub vesc_profile: VescProfile,
    /// ms utan giltigt kommando innan mjuk nedbromsning påbörjas.
    pub watchdog_soft_ms: u64,
    /// ms utan giltigt kommando innan hård nollställning.
    pub watchdog_hard_ms: u64,
    pub lighting_gpio_pin: u8,
    /// Adress till det redan körande Car_Client-processens TCP-port på
    /// samma maskin (localhost). Se car_client_link.rs för bakgrund till
    /// varför vi bygger ovanpå den istället för att ta över /dev/car direkt.
    pub car_client_addr: String,
    /// Bil-ID som Car_Client kör med (`--setid`), första byten i varje paket
    /// mot Car_Client. Bekräftat 4 på riktiga roboten 2026-09-21.
    pub car_client_id: u8,
    /// VESC-ID:n som är kända på den här roboten (VESC Tool). Visas alltid i
    /// Settings-vyn efter en skanning — "svarar" om skanningen hittade dem,
    /// annars "svarar inte" — så att roller kan sättas även när skanningen
    /// via Car_Client inte når dem (se PROJECT_SPEC.md §13).
    pub known_vesc_ids: Vec<u8>,
    /// Sökvägen till CarController-kortets USB-enhet (t.ex. "/dev/vehicle"
    /// eller "/dev/car", se PROJECT_SPEC.md §11/§13 för vilken som gäller
    /// hos er). Används bara för OLED-displayens "USB: OK/SAKNAS"-kontroll
    /// — en enkel `finns filen`-koll, inget djupare.
    pub usb_device_path: String,
    /// Kameraströmmen till klienten, se `video.rs`.
    pub video: VideoConfig,
    /// Hur spakvärdena skickas till styrkortet, se `DriveConfig`.
    pub drive: DriveConfig,
    /// Fordonsdata till klienten: batteri, styrvinkel, router, körlogg.
    pub vehicle: VehicleConfig,
}

/// Fordonsdata som visas i Robotstyrning (2026-10-05, kundens önskemål).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VehicleConfig {
    /// "lifepo4": procent ur LiFePO4-cellernas vilospänning (`battery_series_cells`).
    /// "linear": linjärt mellan `battery_empty_v` och `battery_full_v`.
    /// Standard: 4 st Pro Supply 12,8 V 80 Ah LiFePO4 i serie = 16 celler, 51,2 V, 4096 Wh.
    pub battery_type: String,
    pub battery_series_cells: u32,
    pub battery_empty_v: f32,
    pub battery_full_v: f32,
    /// Batteriets energi (Wh). Krävs för räckvidden; `None` = ingen räckvidd.
    pub battery_capacity_wh: Option<f32>,
    /// Största styrvinkel åt varje håll (grader), för styrningen i procent.
    pub steering_max_deg: f32,
    /// Läs styrvinkeln från vinkelgivaren (terminalkommandot `vinkel`, RobAnt-firmware).
    pub steering_sensor: bool,
    /// Robotens Teltonika-router (RutOS 7, REST över HTTPS) för mobilnätets signal.
    /// `router_url` tom = ingen signalstyrka (standard: av, klienten visar ping).
    pub router_url: String,
    pub router_user: String,
    pub router_password: String,
    /// Mapp för körloggar (en CSV per körning). Tom = ingen körlogg.
    pub drive_log_dir: String,
    /// Körloggar äldre än så tas bort (dagar).
    pub drive_log_keep_days: u32,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            battery_type: "lifepo4".to_string(),
            battery_series_cells: 16,
            battery_empty_v: 48.0,
            battery_full_v: 54.4,
            battery_capacity_wh: Some(4096.0),
            steering_max_deg: 25.0,
            steering_sensor: true,
            router_url: String::new(),
            router_user: "admin".to_string(),
            router_password: String::new(),
            drive_log_dir: "/var/lib/robotd/korloggar".to_string(),
            drive_log_keep_days: 90,
        }
    }
}

/// Körning via styrkortet (`CMD_RC_CONTROL_ADV`). Kortet väljer självt vilka
/// VESC som hör till en aktivitet (aktuatorerna i Confcommon-fliken), så här
/// anges bara aktivitets-ID:n och hur stora värden som skickas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DriveConfig {
    /// Aktivitet för gas/broms ("Speed Control" i RControlStation = 10).
    pub speed_activity: u8,
    /// Aktivitet för styrning ("Steering Control" = 11).
    pub steering_activity: u8,
    /// Farten vid fullt spakutslag när klienten inte skickar eget Max.
    /// Motsvarar "Max" (throttleMaxBox) i RControlStation. Hur det tolkas
    /// (duty, ström, rpm) bestäms av aktuatorns läge på kortet.
    pub speed_max: f32,
    /// Styrningen vid fullt spakutslag. Påverkas inte av förarens Max
    /// (som i RControlStation, där styrningen alltid går på 0,8).
    pub steering_max: f32,
    /// Tak för förarens Max-reglage i klienten (0..1). 1.0 = samma frihet
    /// som i RControlStation; sänk för en robot som inte ska gå fortare.
    pub max_cap: f32,
    /// Byt tecken om roboten kör/svänger åt fel håll.
    pub invert_speed: bool,
    pub invert_steering: bool,
}

impl Default for DriveConfig {
    fn default() -> Self {
        Self {
            speed_activity: 10,
            steering_activity: 11,
            speed_max: 0.30,
            steering_max: 0.8,
            max_cap: 1.0,
            invert_speed: false,
            invert_steering: false,
        }
    }
}

/// Inställningar för kameraströmmen. Standardvärdena är valda för en länk på
/// ~1–2 Mbit/s uppåt (5G via WireGuard, uppmätt 2026-09-21 till 1–5 Mbit/s
/// med stora svängningar), inte för ett labbnät. Höj `bitrate_kbps` om länken
/// tål det.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoConfig {
    pub enabled: bool,
    /// TCP-port för videon. Lyssnar på samma adress som `bind_addr`.
    pub port: u16,
    /// V4L2-enhet för kameran (Logitech C922 = /dev/video0).
    pub device: String,
    /// Upplösning kameran levererar som MJPEG. Måste vara ett läge kameran
    /// stödjer (`v4l2-ctl -d /dev/video0 --list-formats-ext`); 1024x576 är
    /// 16:9 och delbart med 16, vilket hårdvarukodaren gillar.
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Mål-bitrate för H.264. Nyckelbild varje sekund (GOP = fps).
    pub bitrate_kbps: u32,
    /// `h264_v4l2m2m` (Pi 4:s hårdvarukodare, nästan ingen CPU) eller
    /// `libx264` (mjukvara, som reserv om hårdvarukodaren krånglar).
    pub encoder: String,
}

impl Default for VideoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 9001,
            device: "/dev/video0".to_string(),
            width: 1024,
            height: 576,
            fps: 15,
            bitrate_kbps: 900,
            encoder: "h264_v4l2m2m".to_string(),
        }
    }
}

impl Default for RobotConfig {
    fn default() -> Self {
        Self {
            robot_id: "robot-1".to_string(),
            bind_addr: "0.0.0.0:9000".to_string(),
            vesc_profile: VescProfile {
                roller: HashMap::new(),
            },
            watchdog_soft_ms: 150,
            watchdog_hard_ms: 400,
            lighting_gpio_pin: 17,
            car_client_addr: "127.0.0.1:8300".to_string(),
            car_client_id: 4,
            // RobAnt: bekräftat av användaren 2026-09-21 (VESC Tool).
            known_vesc_ids: vec![28, 36, 76],
            usb_device_path: "/dev/vehicle".to_string(),
            video: VideoConfig::default(),
            drive: DriveConfig::default(),
            vehicle: VehicleConfig::default(),
        }
    }
}

/// Konvertera från det delade nätverksprotokollets `VescRoleMsg`
/// (relay-protocol) till robotd:s interna `VescRole`.
pub fn from_msg_role(msg: relay_protocol::VescRoleMsg) -> VescRole {
    use relay_protocol::VescRoleMsg;
    match msg {
        VescRoleMsg::DriftVanster => VescRole::DriftVanster,
        VescRoleMsg::DriftHoger => VescRole::DriftHoger,
        VescRoleMsg::Styrning => VescRole::Styrning,
        VescRoleMsg::Annat { namn, input, min, max } => {
            VescRole::Annat { namn, input, min, max }
        }
    }
}

impl RobotConfig {
    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(cfg) => cfg,
                Err(e) => {
                    // VIKTIGT: utan den här loggraden skulle en config.json
                    // som saknar ett fält (t.ex. efter en uppgradering av
                    // robotd som lade till ett nytt fält i RobotConfig)
                    // tyst kastas bort i sin helhet — inte bara det trasiga
                    // fältet, HELA filen, inklusive robot_id/bind_addr/
                    // VESC-profil man satt manuellt. Hittat vid genomgång
                    // 2026-09-20 inför SD-kortsflashning. Nu varnas det
                    // högt istället för att tyst falla tillbaka.
                    tracing::error!(
                        "KUNDE INTE TOLKA {path:?} ({e}) — faller tillbaka på \
                         STANDARDVÄRDEN och kastar alltså bort alla era \
                         anpassade inställningar i den filen (robotnamn, \
                         VESC-profil, bind-adress, allt). Kontrollera filen \
                         och starta om robotd.",
                        path = path
                    );
                    RobotConfig::default()
                }
            },
            Err(_) => {
                // Filen finns helt enkelt inte än (normalt vid första
                // körning) — inget att larma om.
                RobotConfig::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let s = serde_json::to_string_pretty(self).unwrap();
        std::fs::write(path, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Samma form som scripts/install_pi.sh skriver (via ny_robot.sh).
    #[test]
    fn config_fran_installationsskriptet_las_ratt() {
        let json = r#"{
          "robot_id": "test",
          "bind_addr": "192.168.200.20:9000",
          "vesc_profile": { "roller": {} },
          "watchdog_soft_ms": 150,
          "watchdog_hard_ms": 400,
          "lighting_gpio_pin": 17,
          "car_client_addr": "127.0.0.1:8300",
          "car_client_id": 7,
          "known_vesc_ids": [28, 36, 76],
          "usb_device_path": "/dev/vehicle",
          "drive": { "speed_max": 0.5, "steering_max": 0.5 }
        }"#;
        let cfg: RobotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.car_client_id, 7);
        assert_eq!(cfg.known_vesc_ids, vec![28, 36, 76]);
        assert_eq!(cfg.drive.speed_max, 0.5);
        // Fält som inte står i filen får standardvärden.
        assert_eq!(cfg.drive.speed_activity, 10);
        assert_eq!(cfg.drive.max_cap, 1.0);
        assert!(cfg.video.enabled);
    }
}
