//! Ansluter till det redan körande `Car_Client` (rise_sdvp) på Pi:n via
//! lokal TCP, istället för att ta över `/dev/car` direkt. Beslut (se
//! PROJECT_SPEC.md §11): Car_Client är den del av nuvarande system som
//! bevisligen redan fungerar (bekräftat på riktig hårdvara: /dev/car ->
//! ttyACM0, STM32 VCP, processen kör med --usetcp) — det ni upplevt som
//! opålitligt är RControlStation/nätverkslagret, inte den här länken. Så
//! `robotd` bygger ovanpå den fungerande delen istället för att
//! reverse-engineera USB-serieprotokollet mot kortet på nytt.
//!
//! Paket-inramningen (start/längd/payload/CRC16/slut) är samma som i
//! `serial_bridge.rs`, verifierad byte för byte mot en fångst 2026-09-16
//! (PROJECT_SPEC.md §13). Styrningen använder samma kommando som
//! RControlStation (`CMD_RC_CONTROL_ADV`), se länkuppgiften längre ner.

use std::net::SocketAddr;

use tokio::net::TcpStream;

use crate::serial_bridge::{encode_packet, try_decode_packet, DecodeError};

/// Port Car_Client lyssnar på, bekräftat mot riktig hårdvara 2026-09-16
/// (loggutskrift: "Trying to start TCP server. Port: 8300").
pub const DEFAULT_CAR_CLIENT_TCP_PORT: u16 = 8_300;

pub struct CarClientLink {
    stream: TcpStream,
    rx_buf: Vec<u8>,
}

impl CarClientLink {
    pub async fn connect(addr: SocketAddr) -> Result<Self, String> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("kunde inte ansluta till Car_Client på {addr}: {e}"))?;
        // Styrkommandon ska iväg direkt, inte samlas ihop av Nagle.
        let _ = stream.set_nodelay(true);
        tracing::info!("Ansluten till Car_Client på {addr}");
        Ok(Self {
            stream,
            rx_buf: Vec::new(),
        })
    }

    pub async fn send_raw_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let framed = encode_packet(payload);
        self.stream
            .write_all(&framed)
            .await
            .map_err(|e| format!("skrivfel mot Car_Client: {e}"))
    }

    /// Läs in mer data från socketen och försök tolka ut färdiga paket ur
    /// bufferten. Returnerar de payloads som gick att avkoda; ofullständiga
    /// paket ligger kvar i bufferten till nästa anrop.
    pub async fn poll_packets(&mut self) -> Result<Vec<Vec<u8>>, String> {
        use tokio::io::AsyncReadExt;
        let mut tmp = [0u8; 4096];
        let n = self
            .stream
            .read(&mut tmp)
            .await
            .map_err(|e| format!("läsfel mot Car_Client: {e}"))?;
        if n == 0 {
            return Err("Car_Client stängde anslutningen".to_string());
        }
        self.rx_buf.extend_from_slice(&tmp[..n]);

        let mut packets = Vec::new();
        loop {
            match try_decode_packet(&self.rx_buf) {
                Ok((payload, consumed)) => {
                    self.rx_buf.drain(..consumed);
                    packets.push(payload);
                }
                Err(DecodeError::Incomplete) => break,
                Err(DecodeError::BadCrc) | Err(DecodeError::BadFraming) => {
                    // Ramningsantagandet stämde inte — släng första byten och
                    // försök synka om, hellre än att fastna. Loggas tydligt
                    // så det syns direkt vid test mot riktig hårdvara.
                    tracing::warn!(
                        "Kunde inte tolka paket från Car_Client (ramnings-antagandet \
                         kanske fel) — kasserar en byte och försöker synka om."
                    );
                    if self.rx_buf.is_empty() {
                        break;
                    }
                    self.rx_buf.remove(0);
                }
            }
        }
        Ok(packets)
    }
}

// ---------------------------------------------------------------------------
// Länkuppgiften: den enda anslutningen robotd har till Car_Client.
//
// Car_Client tar bara EN TCP-klient åt gången (`TcpServerSimple::newTcpConnection`
// stänger varje ny anslutning medan en annan är öppen). Därför äger en enda
// uppgift anslutningen och sköter styrning, statuspollning och skanning — och
// den håller anslutningen BARA medan en klient är ansluten till robotd. Utan
// förare släpps Car_Client, så att RControlStation kan ansluta som vanligt.

use std::time::Duration;

use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use relay_protocol::ActuatorConfig;

use crate::board_config;
use crate::vesc_can::{self, BoardState, VescStatusEntry};

/// Värden som skickas till styrkortet, redan skalade (t.ex. ±0.45).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DriveSetpoint {
    pub speed: f32,
    pub steering: f32,
    /// Bandvagnsvändning −1…+1 (+ = medurs). Skickas i stället för styrningen;
    /// firmwaren begränsar farten (30 %) och rätar ut bakbenen först.
    pub pivot: f32,
}

impl DriveSetpoint {
    pub fn is_zero(&self) -> bool {
        self.speed == 0.0 && self.steering == 0.0 && self.pivot == 0.0
    }
}

/// Senast kända tillstånd från styrkortet. Töms när länken släpps eller bryts.
#[derive(Debug, Clone, Default)]
pub struct Telemetry {
    pub board: Option<BoardState>,
    pub vescs: Vec<VescStatusEntry>,
    /// Styrvinkel (grader) och om vinkelgivaren är OK, från terminalkommandot `vinkel`.
    pub steering: Option<(f32, bool)>,
    /// Autocentreringens läge ur samma svar (firmware 30.18), se `StatusUpdate::autocenter`.
    pub autocenter: Option<u8>,
}

/// Länkens läge, för loggar, OLED och klientens statusrad.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkStatus {
    pub connected: bool,
    /// Varför länken inte är uppe, om den borde vara det.
    pub error: Option<String>,
}

/// Omsändning under körning, samma takt som RControlStation (`rcResendTick`).
/// Oftare än så svämmar firmwarens "Activity …"-utskrift över.
const RESEND_ACTIVE: Duration = Duration::from_millis(100);
/// I vila skickas 0 bara då och då, som ett "fortfarande stilla".
const RESEND_IDLE: Duration = Duration::from_secs(1);
/// Hur ofta tillstånd och VESC-status hämtas från styrkortet.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const SCAN_TIMEOUT: Duration = Duration::from_secs(3);
/// Svarar inte kortet på pingen inom så här lång tid har det äldre firmware
/// (före 30.4) — då används CAN-statuslistan i stället.
const PING_TIMEOUT: Duration = Duration::from_millis(1500);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// Svar på en skanning: VESC-ID:n som finns på bussen.
pub type ScanReply = oneshot::Sender<Result<Vec<u8>, String>>;

/// Skanning pågår: först ping (firmware 30.4+), annars CAN-statuslistan.
enum ScanPhase {
    Ping,
    Status,
}
pub type ConfigReply = oneshot::Sender<Result<ActuatorConfig, String>>;

/// Läs eller skriv styrkortets aktuatorer (se `board_config`).
pub enum ConfigRequest {
    Read(ConfigReply),
    /// Skriv och kontrolläs. Svaret är det som faktiskt står på kortet efteråt.
    Write(ActuatorConfig, ConfigReply),
    /// Provsnurra en VESC (firmware 30.4). Svaret kommer direkt; styrkortet
    /// sköter tiden och stoppet självt.
    TestSpin(u8, oneshot::Sender<Result<(), String>>),
    /// Terminalkommando utan svar (t.ex. `autocentrering pa`).
    Terminal(String),
}

impl ConfigRequest {
    fn fail(self, why: &str) {
        match self {
            ConfigRequest::Read(r) | ConfigRequest::Write(_, r) => {
                let _ = r.send(Err(why.to_string()));
            }
            ConfigRequest::TestSpin(_, r) => {
                let _ = r.send(Err(why.to_string()));
            }
            ConfigRequest::Terminal(cmd) => {
                tracing::warn!("Terminalkommandot `{cmd}` skickades inte: {why}");
            }
        }
    }
}

const CONFIG_TIMEOUT: Duration = Duration::from_secs(3);
/// Styrkortet sparar i sitt EEPROM innan det kvitterar — ge det tid.
const CONFIG_WRITE_TIMEOUT: Duration = Duration::from_secs(8);

pub struct LinkHandle {
    pub setpoint: watch::Sender<DriveSetpoint>,
    pub scan: mpsc::Sender<ScanReply>,
    pub config: mpsc::Sender<ConfigRequest>,
    pub status: watch::Receiver<LinkStatus>,
    pub telemetry: watch::Receiver<Telemetry>,
}

#[derive(Clone, Copy)]
pub struct LinkParams {
    pub addr: SocketAddr,
    pub car_id: u8,
    pub speed_activity: u8,
    pub steering_activity: u8,
    pub pivot_activity: u8,
    /// Fråga styrkortet om styrvinkeln (`vinkel`) en gång per sekund.
    pub steering_sensor: bool,
}

/// Svarar firmwaren inte på `vinkel` efter så många frågor (saknar vinkelgivare)
/// slutar vi fråga under den här anslutningen.
const ANGLE_GIVE_UP: u32 = 10;

/// Ska `sp` skickas nu, givet vad som skickades senast? Övergång till stillastående
/// skickas direkt, annars styr omsändningstakten.
fn should_send(sp: DriveSetpoint, last: Option<(DriveSetpoint, Instant)>, now: Instant) -> bool {
    let Some((prev, at)) = last else { return true };
    let elapsed = now.saturating_duration_since(at);
    if sp.is_zero() {
        !prev.is_zero() || elapsed >= RESEND_IDLE
    } else {
        elapsed >= RESEND_ACTIVE
    }
}

/// `wanted` = någon klient är ansluten till robotd. Bara då hålls Car_Client.
pub fn spawn(params: LinkParams, wanted: watch::Receiver<bool>) -> LinkHandle {
    let (setpoint_tx, setpoint_rx) = watch::channel(DriveSetpoint::default());
    let (scan_tx, scan_rx) = mpsc::channel(4);
    let (config_tx, config_rx) = mpsc::channel(2);
    let (status_tx, status_rx) = watch::channel(LinkStatus::default());
    let (telemetry_tx, telemetry_rx) = watch::channel(Telemetry::default());
    tokio::spawn(run(
        params,
        wanted,
        Channels {
            setpoint: setpoint_rx,
            scans: scan_rx,
            config: config_rx,
            status: status_tx,
            telemetry: telemetry_tx,
        },
    ));
    LinkHandle { setpoint: setpoint_tx, scan: scan_tx, config: config_tx, status: status_rx, telemetry: telemetry_rx }
}

struct Channels {
    setpoint: watch::Receiver<DriveSetpoint>,
    scans: mpsc::Receiver<ScanReply>,
    config: mpsc::Receiver<ConfigRequest>,
    status: watch::Sender<LinkStatus>,
    telemetry: watch::Sender<Telemetry>,
}

enum SessionEnd {
    /// Huvudloopen har släppt sina handtag — robotd avslutas.
    MainGone,
    /// Ingen klient ansluten längre — Car_Client släpps.
    Released,
}

/// Väntar `delay`, men avbryter så fort `wanted` ändras (klient kom eller gick)
/// och svarar skanningar och inställningsfrågor med fel direkt.
/// `false` = huvudloopen är borta.
async fn wait_or_release(
    delay: Duration,
    wanted: &mut watch::Receiver<bool>,
    ch: &mut Channels,
    why: &str,
) -> bool {
    let wait = tokio::time::sleep(delay);
    tokio::pin!(wait);
    loop {
        tokio::select! {
            _ = &mut wait => return true,
            changed = wanted.changed() => {
                return changed.is_ok();
            }
            req = ch.scans.recv() => match req {
                Some(reply) => { let _ = reply.send(Err(why.to_string())); }
                None => return false,
            },
            req = ch.config.recv() => match req {
                Some(req) => req.fail(why),
                None => return false,
            },
        }
    }
}

async fn run(params: LinkParams, mut wanted: watch::Receiver<bool>, mut ch: Channels) {
    let mut failures: u32 = 0;
    loop {
        if !*wanted.borrow() {
            ch.status.send_replace(LinkStatus::default());
            // Ingen klient: vänta tills någon ansluter (lång väntan = ingen tidsgräns).
            if !wait_or_release(Duration::MAX / 4, &mut wanted, &mut ch, "ingen klient ansluten").await {
                return;
            }
            continue;
        }

        let error = match CarClientLink::connect(params.addr).await {
            Ok(link) => {
                failures = 0;
                ch.status.send_replace(LinkStatus { connected: true, error: None });
                tracing::info!("Klient ansluten — robotd tar Car_Client-länken (RControlStation kan inte ansluta nu).");
                let started = Instant::now();
                let res = session(link, &params, &mut wanted, &mut ch).await;
                ch.telemetry.send_replace(Telemetry::default());
                match res {
                    Ok(SessionEnd::MainGone) => return,
                    Ok(SessionEnd::Released) => {
                        ch.status.send_replace(LinkStatus::default());
                        tracing::info!("Ingen klient ansluten — släppte Car_Client (RControlStation kan ansluta igen).");
                        continue;
                    }
                    Err(e) if started.elapsed() < Duration::from_secs(2) => {
                        tracing::warn!("Car_Client-länken bröts direkt: {e}");
                        "Car_Client upptagen — är RControlStation ansluten? (Car_Client tar bara en klient)".to_string()
                    }
                    Err(e) => {
                        tracing::warn!("Car_Client-länken bröts: {e}");
                        format!("Car_Client-länken bröts: {e}")
                    }
                }
            }
            Err(e) => {
                failures += 1;
                if failures == 1 || failures % 20 == 0 {
                    tracing::warn!("{e} (försök {failures}). Kör car_client.service?");
                }
                format!("når inte Car_Client ({e})")
            }
        };
        ch.status.send_replace(LinkStatus { connected: false, error: Some(error.clone()) });
        if !wait_or_release(RECONNECT_DELAY, &mut wanted, &mut ch, &error).await {
            return;
        }
    }
}

/// En anslutning, tills den bryts (`Err`) eller inte behövs längre.
async fn session(
    mut link: CarClientLink,
    p: &LinkParams,
    wanted: &mut watch::Receiver<bool>,
    ch: &mut Channels,
) -> Result<SessionEnd, String> {
    let mut last_sent: Option<(DriveSetpoint, Instant)> = None;
    let mut pending_scan: Option<(ScanReply, ScanPhase, Instant)> = None;
    let mut tick = tokio::time::interval(RESEND_ACTIVE);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut poll = tokio::time::interval(POLL_INTERVAL);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Styrvinkeln: varannan poll (1 s). `angle_asked` räknas ner när svar kommer.
    let mut poll_count: u32 = 0;
    let mut angle_unanswered: u32 = 0;
    let mut angle_supported = p.steering_sensor;

    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = poll.tick() => {
                link.send_raw_payload(&vesc_can::make_state_request(p.car_id)).await?;
                link.send_raw_payload(&vesc_can::make_vesc_status_request(p.car_id)).await?;
                poll_count += 1;
                if angle_supported && poll_count % 2 == 0 {
                    if angle_unanswered >= ANGLE_GIVE_UP {
                        angle_supported = false;
                        tracing::info!("Styrkortet svarar inte på `vinkel` (ingen vinkelgivare?) — slutar fråga.");
                    } else {
                        link.send_raw_payload(&vesc_can::make_terminal_cmd(p.car_id, "vinkel")).await?;
                        angle_unanswered += 1;
                    }
                }
            }
            changed = ch.setpoint.changed() => {
                if changed.is_err() {
                    return Ok(SessionEnd::MainGone);
                }
            }
            changed = wanted.changed() => {
                if changed.is_err() {
                    return Ok(SessionEnd::MainGone);
                }
                if !*wanted.borrow() {
                    // Lämna motorerna i vila innan anslutningen släpps.
                    link.send_raw_payload(&vesc_can::make_rc_control_adv(p.car_id, p.speed_activity, 0.0)).await?;
                    link.send_raw_payload(&vesc_can::make_rc_control_adv(p.car_id, p.steering_activity, 0.0)).await?;
                    return Ok(SessionEnd::Released);
                }
            }
            req = ch.scans.recv(), if pending_scan.is_none() => {
                let Some(reply) = req else { return Ok(SessionEnd::MainGone) };
                link.send_raw_payload(&vesc_can::make_vesc_ping_request(p.car_id)).await?;
                pending_scan = Some((reply, ScanPhase::Ping, Instant::now() + PING_TIMEOUT));
            }
            req = ch.config.recv() => {
                let Some(req) = req else { return Ok(SessionEnd::MainGone) };
                // Tar upp till några sekunder; styrvärden skickas inte under tiden,
                // därför tillåter huvudloopen bara detta när roboten står stilla.
                handle_config(&mut link, p.car_id, req).await?;
                last_sent = None; // skicka aktuellt (noll-)värde direkt efteråt
            }
            packets = link.poll_packets() => {
                for pkt in packets? {
                    if let Some(ids) = vesc_can::parse_vesc_ping_reply(&pkt) {
                        if let Some((reply, ScanPhase::Ping, _)) = pending_scan.take() {
                            let _ = reply.send(Ok(ids));
                        }
                    } else if let Some(entries) = vesc_can::parse_vesc_status_reply(&pkt) {
                        if matches!(pending_scan, Some((_, ScanPhase::Status, _))) {
                            let (reply, _, _) = pending_scan.take().unwrap();
                            let fresh = entries.iter().filter(|e| e.is_fresh()).map(|e| e.can_id).collect();
                            let _ = reply.send(Ok(fresh));
                        }
                        ch.telemetry.send_modify(|t| t.vescs = entries);
                    } else if let Some(state) = vesc_can::parse_state_reply(&pkt) {
                        ch.telemetry.send_modify(|t| t.board = Some(state));
                    } else if let Some(angle) = vesc_can::parse_angle_printf(&pkt) {
                        angle_unanswered = 0;
                        let auto = vesc_can::parse_autocenter_printf(&pkt);
                        ch.telemetry.send_modify(|t| {
                            t.steering = Some(angle);
                            t.autocenter = auto;
                        });
                    }
                }
            }
        }

        let now = Instant::now();
        if pending_scan.as_ref().is_some_and(|(_, _, deadline)| now >= *deadline) {
            let (reply, phase, _) = pending_scan.take().unwrap();
            match phase {
                ScanPhase::Ping => {
                    tracing::info!(
                        "Styrkortet svarade inte på VESC-ping (firmware före 30.4?) — \
                         visar de VESC som skickar CAN-status i stället."
                    );
                    link.send_raw_payload(&vesc_can::make_vesc_status_request(p.car_id)).await?;
                    pending_scan = Some((reply, ScanPhase::Status, now + SCAN_TIMEOUT));
                }
                ScanPhase::Status => {
                    let _ = reply.send(Err(format!(
                        "inget svar på VESC-statusfrågan inom {SCAN_TIMEOUT:?} \
                         (saknar kortets firmware CMD_GET_VESC_STATUS?)"
                    )));
                }
            }
        }

        let sp = *ch.setpoint.borrow();
        if should_send(sp, last_sent, now) {
            link.send_raw_payload(&vesc_can::make_rc_control_adv(p.car_id, p.speed_activity, sp.speed))
                .await?;
            // Under vändning (och en gång efteråt, med 0) går spaken till vändningen.
            if sp.pivot != 0.0 || last_sent.is_some_and(|(prev, _)| prev.pivot != 0.0) {
                link.send_raw_payload(&vesc_can::make_rc_control_adv(p.car_id, p.pivot_activity, sp.pivot))
                    .await?;
            } else {
                link.send_raw_payload(&vesc_can::make_rc_control_adv(p.car_id, p.steering_activity, sp.steering))
                    .await?;
            }
            last_sent = Some((sp, now));
        }
    }
}

/// Skickar `request` och väntar på det första paket som `want` känner igen.
/// Andra paket (status, utskrifter) under tiden hoppas över.
async fn request<T>(
    link: &mut CarClientLink,
    request: &[u8],
    timeout: Duration,
    what: &str,
    mut want: impl FnMut(&[u8]) -> Option<T>,
) -> Result<Result<T, String>, String> {
    link.send_raw_payload(request).await?;
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(packets) = tokio::time::timeout(left, link.poll_packets()).await else {
            return Ok(Err(format!("inget svar från styrkortet på {what} inom {timeout:?}")));
        };
        for pkt in packets? {
            if let Some(t) = want(&pkt) {
                return Ok(Ok(t));
            }
        }
    }
}

async fn read_config(link: &mut CarClientLink, car_id: u8) -> Result<Result<(Vec<u8>, ActuatorConfig), String>, String> {
    let r = request(link, &board_config::make_read_request(car_id), CONFIG_TIMEOUT, "läsning av inställningarna", |p| {
        board_config::parse_read_reply(p)
    })
    .await?;
    Ok(r.and_then(|inner| inner))
}

/// Läser, eller läser–ändrar–skriver–kontrolläser, styrkortets aktuatorer.
/// Yttre `Err` = länken bröts; fel i själva operationen går till svaret.
async fn handle_config(link: &mut CarClientLink, car_id: u8, req: ConfigRequest) -> Result<(), String> {
    match req {
        ConfigRequest::Read(reply) => {
            let r = read_config(link, car_id).await?.map(|(_, cfg)| cfg);
            let _ = reply.send(r);
        }
        ConfigRequest::Write(wanted, reply) => {
            let result = async {
                let (body, before) = read_config(link, car_id).await??;
                let write = board_config::make_write_request(car_id, &body, &wanted)?;
                tracing::info!("Skriver aktuatorer till styrkortet: {before:?} -> {wanted:?}");
                request(link, &write, CONFIG_WRITE_TIMEOUT, "skrivningen", |p| {
                    board_config::is_write_ack(p).then_some(())
                })
                .await??;
                let (_, after) = read_config(link, car_id).await??;
                if after == wanted {
                    tracing::info!("Aktuatorerna skrivna och kontrollästa.");
                    Ok(after)
                } else {
                    Err(format!(
                        "styrkortet kvitterade men visar något annat efteråt: {after:?}"
                    ))
                }
            }
            .await;
            if let Err(e) = &result {
                tracing::warn!("Skrivning av aktuatorer misslyckades: {e}");
            }
            let _ = reply.send(result);
        }
        ConfigRequest::Terminal(cmd) => {
            tracing::info!("Terminalkommando till styrkortet: {cmd}");
            link.send_raw_payload(&vesc_can::make_terminal_cmd(car_id, &cmd)).await?;
        }
        ConfigRequest::TestSpin(vesc, reply) => {
            tracing::info!("Provsnurr VESC {vesc}.");
            let req = vesc_can::make_test_spin_request(car_id, vesc);
            let r = request(link, &req, CONFIG_TIMEOUT, "provsnurret", |p| {
                vesc_can::parse_test_spin_reply(p).filter(|(id, _)| *id == vesc).map(|(_, started)| started)
            })
            .await?;
            let _ = reply.send(match r {
                Ok(true) => Ok(()),
                Ok(false) => Err("ett provsnurr pågår redan — vänta en sekund".to_string()),
                Err(e) => Err(format!("{e} (styrkortet behöver firmware 30.4)")),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    fn sp(speed: f32, steering: f32) -> DriveSetpoint {
        DriveSetpoint { speed, steering, pivot: 0.0 }
    }

    fn params(addr: SocketAddr) -> LinkParams {
        LinkParams { addr, car_id: 4, speed_activity: 10, steering_activity: 11, pivot_activity: 22, steering_sensor: false }
    }

    #[test]
    fn forsta_vardet_skickas_alltid() {
        assert!(should_send(sp(0.0, 0.0), None, Instant::now()));
    }

    #[test]
    fn stopp_skickas_direkt_men_vila_bara_varje_sekund() {
        let t = Instant::now();
        assert!(should_send(sp(0.0, 0.0), Some((sp(0.1, 0.0), t)), t));
        assert!(!should_send(sp(0.0, 0.0), Some((sp(0.0, 0.0), t)), t + Duration::from_millis(500)));
        assert!(should_send(sp(0.0, 0.0), Some((sp(0.0, 0.0), t)), t + Duration::from_millis(1000)));
    }

    #[test]
    fn korning_skickas_var_100_ms() {
        let t = Instant::now();
        assert!(!should_send(sp(0.1, 0.0), Some((sp(0.05, 0.0), t)), t + Duration::from_millis(40)));
        assert!(should_send(sp(0.1, 0.0), Some((sp(0.1, 0.0), t)), t + Duration::from_millis(100)));
    }

    /// Läser ramade paket från en falsk Car_Client tills `n` paket med
    /// kommandot `cmd` kommit (statuspollningen blandas in, den hoppas över).
    /// Tom lista = robotd stängde anslutningen.
    async fn read_cmd(sock: &mut TcpStream, cmd: u8, n: usize) -> Vec<Vec<u8>> {
        let mut buf = Vec::new();
        let mut out = Vec::new();
        while out.len() < n {
            let mut tmp = [0u8; 256];
            let k = sock.read(&mut tmp).await.unwrap();
            if k == 0 {
                return Vec::new();
            }
            buf.extend_from_slice(&tmp[..k]);
            while let Ok((payload, used)) = try_decode_packet(&buf) {
                buf.drain(..used);
                if payload[1] == cmd {
                    out.push(payload);
                }
            }
        }
        out
    }

    const RC: u8 = vesc_can::CMD_RC_CONTROL_ADV;

    /// Hela vägen mot en falsk Car_Client: nolla vid anslutning, spakvärden,
    /// direkt nolla vid stopp, skanning och telemetri på samma anslutning.
    #[tokio::test]
    async fn styrning_skanning_och_status_mot_falsk_car_client() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_wanted_tx, wanted) = watch::channel(true);
        let mut h = spawn(params(listener.local_addr().unwrap()), wanted);
        let (mut sock, _) = listener.accept().await.unwrap();

        // Först en nolla till båda aktiviteterna.
        let p = read_cmd(&mut sock, RC, 2).await;
        assert_eq!(p[0], vesc_can::make_rc_control_adv(4, 10, 0.0));
        assert_eq!(p[1], vesc_can::make_rc_control_adv(4, 11, 0.0));

        h.setpoint.send(sp(0.15, -0.05)).unwrap();
        let p = read_cmd(&mut sock, RC, 2).await;
        assert_eq!(p[0], vesc_can::make_rc_control_adv(4, 10, 0.15));
        assert_eq!(p[1], vesc_can::make_rc_control_adv(4, 11, -0.05));

        // Stopp ska komma direkt, inte vänta på omsändningen.
        let t = std::time::Instant::now();
        h.setpoint.send(sp(0.0, 0.0)).unwrap();
        let p = loop {
            let p = read_cmd(&mut sock, RC, 2).await;
            if p[0] == vesc_can::make_rc_control_adv(4, 10, 0.0) {
                break p;
            }
        };
        assert_eq!(p[1], vesc_can::make_rc_control_adv(4, 11, 0.0));
        assert!(t.elapsed() < Duration::from_millis(80), "stopp dröjde {:?}", t.elapsed());

        // Statuspollningen frågar efter tillstånd; svara och se att det når telemetrin.
        let req = read_cmd(&mut sock, vesc_can::CMD_GET_STATE, 1).await;
        assert_eq!(req[0], vesc_can::make_state_request(4));
        let mut state = vec![4, vesc_can::CMD_GET_STATE, 10, 5];
        for _ in 0..15 {
            state.extend(0i32.to_be_bytes());
        }
        state.extend(52_600_000i32.to_be_bytes());
        state.extend([0u8; 45]);
        sock.write_all(&encode_packet(&state)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), h.telemetry.wait_for(|t| t.board.is_some()))
            .await
            .unwrap()
            .unwrap();
        assert!((h.telemetry.borrow().board.unwrap().v_in - 52.6).abs() < 1e-3);

        // Skanning: styrkortet pingar bussen, svaret tillbaka på samma anslutning.
        let (tx, rx) = oneshot::channel();
        h.scan.send(tx).await.unwrap();
        read_cmd(&mut sock, vesc_can::CMD_PING_VESC_CAN, 1).await;
        sock.write_all(&encode_packet(&[4, vesc_can::CMD_PING_VESC_CAN, 16, 28, 94])).await.unwrap();
        assert_eq!(rx.await.unwrap().unwrap(), vec![16, 28, 94]);
        assert!(h.status.borrow().connected);

        // CAN-statuslistan går fortfarande till telemetrin.
        read_cmd(&mut sock, vesc_can::CMD_GET_VESC_STATUS, 1).await;
        let mut reply = vec![4, vesc_can::CMD_GET_VESC_STATUS, 28, 0, 50];
        reply.extend([0u8; 8]);
        sock.write_all(&encode_packet(&reply)).await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), h.telemetry.wait_for(|t| t.vescs.len() == 1))
            .await
            .unwrap()
            .unwrap();
    }

    /// Äldre firmware (före 30.4) svarar inte på pingen: då ska skanningen
    /// visa de VESC som skickar CAN-status, och statussvar som kommer medan
    /// pingen väntar ska inte avsluta skanningen i förtid.
    #[tokio::test]
    async fn skanning_faller_tillbaka_pa_can_status_utan_ping() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_wanted_tx, wanted) = watch::channel(true);
        let h = spawn(params(listener.local_addr().unwrap()), wanted);
        let (mut sock, _) = listener.accept().await.unwrap();

        let (tx, mut rx) = oneshot::channel();
        h.scan.send(tx).await.unwrap();
        read_cmd(&mut sock, vesc_can::CMD_PING_VESC_CAN, 1).await;
        let t = std::time::Instant::now();
        // Svara på varje statusfråga (pollning och reservfrågan) tills skanningen är klar.
        let ids = loop {
            if let Ok(r) = rx.try_recv() {
                break r.unwrap();
            }
            let got = tokio::time::timeout(
                Duration::from_millis(200),
                read_cmd(&mut sock, vesc_can::CMD_GET_VESC_STATUS, 1),
            )
            .await;
            if got.is_ok() {
                let mut reply = vec![4, vesc_can::CMD_GET_VESC_STATUS, 94, 0, 50];
                reply.extend([0u8; 8]);
                sock.write_all(&encode_packet(&reply)).await.unwrap();
            }
            assert!(t.elapsed() < Duration::from_secs(5), "skanningen blev aldrig klar");
        };
        assert_eq!(ids, vec![94]);
        assert!(t.elapsed() >= PING_TIMEOUT, "skanningen avslutades före pingens tidsgräns");
    }

    /// Utan klient ska Car_Client lämnas ifred; när klienten går släpps den
    /// efter en sista nolla.
    #[tokio::test]
    async fn car_client_halls_bara_medan_klient_ar_ansluten() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (wanted_tx, wanted) = watch::channel(false);
        let h = spawn(params(listener.local_addr().unwrap()), wanted);

        let early = tokio::time::timeout(Duration::from_millis(300), listener.accept()).await;
        assert!(early.is_err(), "robotd anslöt till Car_Client utan klient");

        wanted_tx.send(true).unwrap();
        let (mut sock, _) = listener.accept().await.unwrap();
        h.setpoint.send(sp(0.3, 0.0)).unwrap();
        read_cmd(&mut sock, RC, 2).await;

        wanted_tx.send(false).unwrap();
        // Sista paketen ska vara nollor, sedan stängs anslutningen.
        let mut last = Vec::new();
        loop {
            let p = read_cmd(&mut sock, RC, 1).await;
            if p.is_empty() {
                break;
            }
            last = p.last().unwrap().clone();
        }
        assert_eq!(last, vesc_can::make_rc_control_adv(4, 11, 0.0));
        let mut rx = h.status.clone();
        tokio::time::timeout(Duration::from_secs(1), rx.wait_for(|s| !s.connected)).await.unwrap().unwrap();
    }

    /// Läs–ändra–skriv–kontrolläs mot en falsk Car_Client som håller
    /// styrkortets inställning i minnet och svarar som firmware.
    #[tokio::test]
    async fn aktuatorer_skrivs_och_kontrollases() {
        use relay_protocol::ActuatorSlot;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let (_wanted_tx, wanted) = watch::channel(true);
        let h = spawn(params(listener.local_addr().unwrap()), wanted);
        let (mut sock, _) = listener.accept().await.unwrap();

        let original = board_config::fake_body("log", &[(0, 28, 10, 0), (0, 36, 10, 0), (0, 76, 11, 0), (0, 0, 0, 0)], 3);
        let fake_board = tokio::spawn(async move {
            let mut stored = original.clone();
            let mut buf = Vec::new();
            loop {
                let mut tmp = [0u8; 1024];
                let k = sock.read(&mut tmp).await.unwrap();
                if k == 0 {
                    return (original, stored);
                }
                buf.extend_from_slice(&tmp[..k]);
                while let Ok((p, used)) = try_decode_packet(&buf) {
                    buf.drain(..used);
                    match p[1] {
                        board_config::CMD_GET_MAIN_CONFIG => {
                            let mut r = vec![4, board_config::CMD_GET_MAIN_CONFIG];
                            r.extend(&stored);
                            sock.write_all(&encode_packet(&[4, 0, b'x'])).await.unwrap(); // en utskrift emellan
                            sock.write_all(&encode_packet(&r)).await.unwrap();
                        }
                        board_config::CMD_SET_MAIN_CONFIG => {
                            stored = p[2..].to_vec();
                            sock.write_all(&encode_packet(&[4, board_config::CMD_SET_MAIN_CONFIG])).await.unwrap();
                        }
                        _ => {}
                    }
                }
            }
        });

        // Läs.
        let (tx, rx) = oneshot::channel();
        h.config.send(ConfigRequest::Read(tx)).await.unwrap();
        let mut cfg = rx.await.unwrap().unwrap();
        assert_eq!(cfg.count, 3);
        assert_eq!(cfg.slots[2].vesc_id, 76);

        // Skriv en ändring.
        cfg.slots[2] = ActuatorSlot { kind: 0, vesc_id: 77, activity: 11, mode: 3 };
        let (tx, rx) = oneshot::channel();
        h.config.send(ConfigRequest::Write(cfg.clone(), tx)).await.unwrap();
        let written = rx.await.unwrap().unwrap();
        assert_eq!(written, cfg);

        // Orimligt värde vägras innan något skickas.
        let mut bad = cfg.clone();
        bad.slots[0].mode = 2;
        let (tx, rx) = oneshot::channel();
        h.config.send(ConfigRequest::Write(bad, tx)).await.unwrap();
        assert!(rx.await.unwrap().is_err());

        drop(h);
        let (original, stored) = fake_board.await.unwrap();
        // Bara aktuatorbytena skiljer sig på "kortet".
        let diff: Vec<usize> = (0..original.len()).filter(|&i| original[i] != stored[i]).collect();
        assert!(!diff.is_empty());
        let first = *diff.first().unwrap();
        let last = *diff.last().unwrap();
        assert!(last - first < 34, "ändringar utanför aktuatorblocket: {first}..{last}");
        assert_eq!(original.len(), stored.len());
    }

    #[tokio::test]
    async fn skanning_utan_car_client_svarar_med_fel() {
        // Ledig port där ingen lyssnar.
        let addr = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
        let (_wanted_tx, wanted) = watch::channel(true);
        let h = spawn(params(addr), wanted);
        let (tx, rx) = oneshot::channel();
        h.scan.send(tx).await.unwrap();
        let r = tokio::time::timeout(Duration::from_secs(1), rx).await.unwrap().unwrap();
        assert!(r.is_err());
        assert!(!h.status.borrow().connected);
        assert!(h.status.borrow().error.is_some());
    }
}
