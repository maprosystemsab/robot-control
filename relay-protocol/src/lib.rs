//! Delade meddelandetyper mellan robotd och klienten.
//!
//! **Arkitekturändring 2026-09-19:** Tidigare byggde det här på en egen
//! reläserver med Ed25519-parkoppling för att lösa NAT-genomgång och
//! autentisering. Det behövs inte — ni har redan en fungerande, beprövad
//! WireGuard-VPN (fast IP-schema `192.168.200.x`, se PROJECT_SPEC.md §14)
//! som redan löser exakt det: kryptering, autentisering via nycklar,
//! NAT-genomgång. `robotd` lyssnar nu direkt på sin VPN-IP, och klienten
//! ansluter direkt till den — ingen mellanhand.
//!
//! Kvar i det här protokollet: bara applikationslogiken (styrkommandon,
//! status, VESC-mappning, en-förare-i-taget). Ingen kryptografi här längre
//! — WireGuard-tunneln är redan krypterad och autentiserad på nätverksnivå.

pub mod video;

use serde::{Deserialize, Serialize};

/// Skickas av klienten när den ansluter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Första meddelandet. `view_code` krävs bara om `as_viewer` är true
    /// OCH robotd är konfigurerad att kräva kod för åskådare (se spec §4:
    /// tillfälliga titta-koder). En förar-anslutning (as_viewer=false)
    /// litar på att WireGuard redan avgjort att du får vara på nätverket;
    /// robotd nekar bara om någon annan redan kör (en förare i taget).
    Connect { as_viewer: bool, view_code: Option<String> },
    Control(ControlCommand),
    ScanVescBus,
    SaveVescProfile(VescProfileMsg),
    VideoSignal(String),
    /// Klienten begär en ny tillfällig titta-kod att dela vidare.
    RequestViewCode,
    /// Skickas periodiskt av klienten. `sent_ms` ekas tillbaka oförändrad i
    /// `RobotMessage::Pong` så klienten kan räkna ut en riktig
    /// tur-och-retur-tid, istället för den tidigare hårdkodade `Pong(0)`.
    /// Fungerar också som ett levnadstecken för framtida
    /// timeout/keepalive-logik på robotd-sidan.
    Ping { sent_ms: u64 },
    /// Läs styrkortets aktuatorinställningar (som Read i RControlStation:s Confcommon).
    ReadActuators,
    /// Skriv aktuatorinställningarna till styrkortet (som Write). Bara förare.
    WriteActuators(ActuatorConfig),
    /// Lista körloggarna på roboten.
    ListDriveLogs,
    /// Hämta en körlogg (namn från `DriveLogs`).
    GetDriveLog { name: String },
    /// Provsnurra en VESC (30 % i 1 s) för att se vilken motor det är.
    /// Maskinen ska vara upphissad. Bara förare.
    TestSpinVesc { can_id: u8 },
}

/// Skickas av robotd till en ansluten klient.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RobotMessage {
    ConnectedAsDriver,
    ConnectedAsViewer,
    /// "Roboten körs redan av någon annan" eller ogiltig/utgången titta-kod.
    ConnectDenied { reason: String },
    Status(StatusUpdate),
    VescBusResult(Vec<VescSighting>),
    VescProfileSaved,
    VideoSignal(String),
    ViewCode { code: String, ttl_secs: u32 },
    Pong { sent_ms: u64 },
    /// Svar på ReadActuators.
    Actuators(ActuatorConfig),
    /// Svar på WriteActuators: skrivet och kontrolläst från styrkortet.
    ActuatorsWritten(ActuatorConfig),
    /// Läsning/skrivning av aktuatorer misslyckades; inget ändrat om inget annat sägs.
    ActuatorError(String),
    /// Svar på ListDriveLogs, nyaste först.
    DriveLogs(Vec<DriveLogInfo>),
    /// Svar på GetDriveLog: hela CSV-filen.
    DriveLog { name: String, csv: String },
    /// Körloggarna gick inte att läsa.
    DriveLogError(String),
    /// Svar på TestSpinVesc: `ok` om styrkortet startade provsnurret.
    VescTestSpin { can_id: u8, ok: bool, message: String },
}

/// En aktuator på styrkortet: vilken motor som styrs av vilken aktivitet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActuatorSlot {
    /// 0 = VESC-motor, 1 = hydraulik.
    pub kind: u16,
    /// VESC:ns CAN-ID.
    pub vesc_id: u16,
    /// Aktivitet: 10 = fart ("Speed Control"), 11 = styrning ("Steering Control") m.fl.
    pub activity: u16,
    /// 0 = duty, 1 = ström, 3 = rpm.
    pub mode: u16,
}

/// Styrkortets aktuatorer: `count` första av de fyra platserna används.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActuatorConfig {
    pub count: u16,
    pub slots: Vec<ActuatorSlot>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ControlCommand {
    /// -1.0..=1.0
    pub throttle: f32,
    /// -1.0..=1.0
    pub steering: f32,
    pub accessory_a: f32, // t.ex. lastarm (L1/L2)
    pub accessory_b: f32, // t.ex. tilt (R1/R2)
    pub lights: bool,
    pub activated: bool,
    /// Millisekunder sedan epoch när klienten skapade kommandot, för
    /// latensmätning och för watchdogens tidsbedömning.
    pub timestamp_ms: u64,
    /// Förarens max (0..1), som "Max"-rutan i RControlStation: värdet som
    /// skickas till styrkortet vid fullt spakutslag. robotd begränsar det
    /// till sitt eget tak (`drive.max_cap`). `None` = robotd:s config gäller.
    #[serde(default)]
    pub max_output: Option<f32>,
    /// Bandvagnsvändning (RobAnt, firmware 30.6): O hålls in, styrspaken
    /// vänder roboten på stället i stället för att styra. Gasen räknas inte.
    #[serde(default)]
    pub pivot: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StatusUpdate {
    pub speed_kmh: Option<f32>,
    pub battery_percent: Option<f32>,
    pub gps_fix: bool,
    pub vesc_temps_c: Vec<f32>,
    pub last_error: Option<String>,
    pub link_quality: LinkQuality,
    /// Batterispänning i volt (styrkortets v_in).
    #[serde(default)]
    pub battery_voltage: Option<f32>,
    /// VESC-ID som skickat CAN-status de senaste sekunderna.
    #[serde(default)]
    pub vescs_responding: Vec<u8>,
    /// VESC-ID som ska finnas på roboten (robotd:s `known_vesc_ids`).
    #[serde(default)]
    pub vescs_expected: Vec<u8>,

    // --- Fordonsdata (2026-10-05). Alla är `Option`/`default`, så att en äldre
    // robotd eller klient fungerar som förut: saknade värden visas som "–".
    /// Laddas batteriet? `Some(true)` = spänningen stiger när roboten står still,
    /// `Some(false)` = den sjunker eller står still, `None` = går inte att avgöra (kör).
    #[serde(default)]
    pub battery_charging: Option<bool>,
    /// Effekt ur batteriet just nu (W), uppskattad från VESC:ernas ström och duty.
    #[serde(default)]
    pub power_w: Option<f32>,
    /// Förbrukning under körningen (Wh/km).
    #[serde(default)]
    pub wh_per_km: Option<f32>,
    /// Beräknad räckvidd med kvarvarande batteri (km). Kräver batterikapacitet i robotd:s config.
    #[serde(default)]
    pub range_km: Option<f32>,
    /// Styrvinkel i grader från vinkelgivaren (+ = höger).
    #[serde(default)]
    pub steering_deg: Option<f32>,
    /// Styrvinkel i procent av största utslag (−100 = fullt vänster, +100 = fullt höger).
    #[serde(default)]
    pub steering_percent: Option<f32>,
    /// Lutning åt sidan, framåt/bakåt och kurs (grader) från styrkortets IMU.
    #[serde(default)]
    pub roll_deg: Option<f32>,
    #[serde(default)]
    pub pitch_deg: Option<f32>,
    #[serde(default)]
    pub yaw_deg: Option<f32>,
    /// Aktuell felkod från styrkortet/VESC i klartext (t.ex. "Underspänning").
    #[serde(default)]
    pub fault_text: Option<String>,
    /// Mobilnätets signalstyrka från robotens router (dBm).
    #[serde(default)]
    pub rssi_dbm: Option<i32>,
    /// Mer om mobilnätet, t.ex. "4G Telia · RSRP −95 dBm · SINR 12 dB".
    #[serde(default)]
    pub signal_info: Option<String>,
    /// Nödstoppet: "ok", "intryckt", "sparrad" (utdraget, väntar på stopp från föraren)
    /// eller "fel". `None` = inget nödstopp installerat på roboten.
    #[serde(default)]
    pub estop: Option<String>,
}

/// En körlogg på roboten (CSV, en per körning).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriveLogInfo {
    /// Filnamn, t.ex. "2026-10-05_14-03-22.csv".
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LinkQuality {
    Green,
    Yellow,
    #[default]
    Red,
}

/// En VESC som svarat på CAN-bussen, för Settings-vyn i klienten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VescSighting {
    pub can_id: u8,
    pub responding: bool,
}

/// Roll tilldelad en VESC i Settings-vyn (spec §5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VescRoleMsg {
    DriftVanster,
    DriftHoger,
    Styrning,
    Annat {
        namn: String,
        input: String, // "l1_l2" eller "r1_r2"
        min: f32,
        max: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VescProfileMsg {
    pub roller: std::collections::HashMap<u8, VescRoleMsg>,
}
