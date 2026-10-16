//! robotd:s WebSocket-server. Lyssnar direkt på en port (rekommenderat:
//! bind till robotens WireGuard-IP, t.ex. 192.168.200.8, eller 0.0.0.0 med
//! en brandväggsregel som bara släpper in på wg0 — se
//! PROJECT_SPEC.md §14 och Sakerhetsanalys-rekommendationen om `ufw`).
//!
//! Ersätter den gamla relay_client.rs (som ringde HEM till en central
//! reläserver). Nu är det tvärtom: robotd är servern, klienten ansluter
//! rakt in över VPN-tunneln. En-förare-i-taget och tillfälliga titta-koder
//! hanteras lokalt här, ingen extern registry behövs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use relay_protocol::{ClientMessage, ControlCommand, RobotMessage, StatusUpdate};
use tokio::sync::{mpsc, watch, Mutex};

struct ViewCode {
    code: String,
    expires_at: Instant,
}

#[derive(Default)]
struct SharedState {
    /// Finns en förare just nu? (enkel närvaromarkör, se `connect`)
    driver_connected: bool,
    view_codes: Vec<ViewCode>,
    /// Antal anslutna klienter (förare + åskådare).
    clients: usize,
}

#[derive(Clone)]
struct AppState {
    shared: Arc<Mutex<SharedState>>,
    /// Styrkommandon (bara från den aktiva föraren) vidare till huvudloopen.
    control_tx: mpsc::Sender<ControlCommand>,
    /// Övriga förfrågningar (VESC-skanning/profil) vidare till huvudloopen.
    request_tx: mpsc::Sender<RobotRequest>,
    /// Senaste status, spegling för nya klienter direkt vid anslutning.
    latest_status: Arc<Mutex<Option<StatusUpdate>>>,
    /// true medan minst en klient är ansluten (styr Car_Client-länken).
    clients_tx: Arc<watch::Sender<bool>>,
}

/// Förfrågningar från en klient som huvudloopen (main.rs) hanterar och
/// svarar på via `response_tx` i samma struct.
pub enum RobotRequest {
    ScanVescBus { respond_to: mpsc::Sender<RobotMessage> },
    SaveVescProfile { profile: relay_protocol::VescProfileMsg, respond_to: mpsc::Sender<RobotMessage> },
    ReadActuators { respond_to: mpsc::Sender<RobotMessage> },
    WriteActuators { config: relay_protocol::ActuatorConfig, respond_to: mpsc::Sender<RobotMessage> },
    ListDriveLogs { respond_to: mpsc::Sender<RobotMessage> },
    GetDriveLog { name: String, respond_to: mpsc::Sender<RobotMessage> },
    TestSpinVesc { can_id: u8, respond_to: mpsc::Sender<RobotMessage> },
    SetAutocenter { on: bool },
}

pub struct ServerHandles {
    pub incoming_control: mpsc::Receiver<ControlCommand>,
    pub incoming_requests: mpsc::Receiver<RobotRequest>,
    /// main.rs anropar den här för att sprida en ny StatusUpdate till alla
    /// anslutna klienter (drivrutinen håller koll på vilka som är kopplade).
    pub status_broadcast: mpsc::Sender<StatusUpdate>,
    /// true medan minst en klient är ansluten.
    pub clients_connected: watch::Receiver<bool>,
}

/// Startar servern på `bind_addr` (t.ex. "192.168.200.8:9000" eller
/// "0.0.0.0:9000"). Körs i en egen task; returnerar handtag för
/// huvudloopen i main.rs.
pub fn spawn(bind_addr: String) -> ServerHandles {
    let (control_tx, control_rx) = mpsc::channel::<ControlCommand>(32);
    let (request_tx, request_rx) = mpsc::channel::<RobotRequest>(16);
    let (status_tx, mut status_rx) = mpsc::channel::<StatusUpdate>(16);

    let latest_status = Arc::new(Mutex::new(None));
    let (clients_tx, clients_rx) = watch::channel(false);
    let broadcast_targets: Arc<Mutex<Vec<mpsc::Sender<RobotMessage>>>> = Arc::new(Mutex::new(Vec::new()));

    let state = AppState {
        shared: Arc::new(Mutex::new(SharedState::default())),
        control_tx,
        request_tx,
        latest_status: latest_status.clone(),
        clients_tx: Arc::new(clients_tx),
    };

    // Sprid varje ny status till alla anslutna klienters kanaler.
    {
        let latest_status = latest_status.clone();
        let broadcast_targets = broadcast_targets.clone();
        tokio::spawn(async move {
            while let Some(status) = status_rx.recv().await {
                *latest_status.lock().await = Some(status.clone());
                let targets = broadcast_targets.lock().await;
                for t in targets.iter() {
                    let _ = t.send(RobotMessage::Status(status.clone())).await;
                }
            }
        });
    }

    {
        let state = state.clone();
        let broadcast_targets = broadcast_targets.clone();
        tokio::spawn(async move {
            let app = Router::new()
                .route("/ws", get(move |ws: WebSocketUpgrade, State(s): State<AppState>| {
                    let broadcast_targets = broadcast_targets.clone();
                    async move { ws.on_upgrade(move |socket| handle_socket(socket, s, broadcast_targets)) }
                }))
                .with_state(state);

            // Vid uppstart (robotd.service) kan WireGuard-adressen saknas en
            // stund — försök igen i stället för att ge upp för gott.
            let mut failures = 0u32;
            let listener = loop {
                match tokio::net::TcpListener::bind(&bind_addr).await {
                    Ok(l) => break l,
                    Err(e) => {
                        failures += 1;
                        if failures == 1 || failures % 20 == 0 {
                            tracing::warn!(
                                "Kunde inte binda {bind_addr}: {e}. Väntar på att adressen finns \
                                 (WireGuard-tunneln uppe?) och att porten är ledig — försöker igen var 3:e s."
                            );
                        }
                        tokio::time::sleep(Duration::from_secs(3)).await;
                    }
                }
            };
            tracing::info!("robotd lyssnar på {bind_addr}");
            // tcp_nodelay: status och svar skickas direkt i stället för att vänta på
            // kvittens (Nagle), vilket över 4G gav fördröjningar på flera hundra ms.
            if let Err(e) = axum::serve(listener, app).tcp_nodelay(true).await {
                tracing::error!("servern stannade: {e}");
            }
        });
    }

    ServerHandles {
        incoming_control: control_rx,
        incoming_requests: request_rx,
        status_broadcast: status_tx,
        clients_connected: clients_rx,
    }
}

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    broadcast_targets: Arc<Mutex<Vec<mpsc::Sender<RobotMessage>>>>,
) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<RobotMessage>(32);

    // Vänta på första meddelandet: måste vara Connect.
    let Some(Ok(Message::Text(text))) = ws_rx.next().await else { return };
    let Ok(ClientMessage::Connect { as_viewer, view_code }) = serde_json::from_str(&text) else {
        return;
    };

    let is_driver = if as_viewer {
        let mut ok = true;
        if let Some(code) = &view_code {
            let mut shared = state.shared.lock().await;
            let now = Instant::now();
            shared.view_codes.retain(|c| c.expires_at > now);
            ok = shared.view_codes.iter().any(|c| &c.code == code);
        }
        if !ok {
            tracing::info!("Åskådaranslutning nekad: ogiltig eller utgången titta-kod.");
            let _ = out_tx.send(RobotMessage::ConnectDenied {
                reason: "ogiltig eller utgången titta-kod".to_string(),
            }).await;
            return;
        }
        false
    } else {
        let mut shared = state.shared.lock().await;
        if shared.driver_connected {
            // Syntes tidigare bara hos klienten (Dator 1:s förslag efter
            // regressionstestet 2026-09-19) — nu loggas avslaget här också,
            // så det går att felsöka från robotd-sidan utan att fråga
            // föraren vad de såg.
            tracing::info!("Förar-anslutning nekad: roboten körs redan av någon annan.");
            let _ = out_tx.send(RobotMessage::ConnectDenied {
                reason: "roboten körs redan av någon annan".to_string(),
            }).await;
            return;
        }
        shared.driver_connected = true;
        true
    };

    let _ = out_tx.send(if is_driver {
        RobotMessage::ConnectedAsDriver
    } else {
        RobotMessage::ConnectedAsViewer
    }).await;

    // Skicka senaste kända status direkt så nya klienter inte väntar tomt.
    if let Some(status) = state.latest_status.lock().await.clone() {
        let _ = out_tx.send(RobotMessage::Status(status)).await;
    }

    broadcast_targets.lock().await.push(out_tx.clone());
    {
        let mut shared = state.shared.lock().await;
        shared.clients += 1;
        state.clients_tx.send_replace(true);
    }

    // Om ingen giltig ClientMessage kommit på STALE_TIMEOUT: släpp
    // förar-platsen. Löser risken båda testomgångarna 2026-09-19 flaggade —
    // att `driver_connected` annars kan fastna om VPN-tunneln dör tyst
    // (utan en riktig TCP-close), eftersom WebSocket-läsningen då aldrig
    // returnerar `None`/`Close` av sig själv.
    const STALE_TIMEOUT: Duration = Duration::from_secs(5);
    let mut last_msg_at = Instant::now();
    let mut stale_check = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            Some(msg) = out_rx.recv() => {
                let text = serde_json::to_string(&msg).unwrap();
                if ws_tx.send(Message::Text(text)).await.is_err() {
                    break;
                }
            }
            _ = stale_check.tick() => {
                if last_msg_at.elapsed() > STALE_TIMEOUT {
                    tracing::warn!("Klient (förare={is_driver}) tyst i >{STALE_TIMEOUT:?}, kopplar ner.");
                    break;
                }
            }
            incoming = ws_rx.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last_msg_at = Instant::now();
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Control(cmd)) if is_driver => {
                                let _ = state.control_tx.send(cmd).await;
                            }
                            Ok(ClientMessage::ScanVescBus) => {
                                let _ = state.request_tx.send(RobotRequest::ScanVescBus {
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::SaveVescProfile(profile)) => {
                                let _ = state.request_tx.send(RobotRequest::SaveVescProfile {
                                    profile,
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::RequestViewCode) => {
                                let code = random_code();
                                let ttl = Duration::from_secs(1800);
                                state.shared.lock().await.view_codes.push(ViewCode {
                                    code: code.clone(),
                                    expires_at: Instant::now() + ttl,
                                });
                                let _ = out_tx.send(RobotMessage::ViewCode {
                                    code,
                                    ttl_secs: ttl.as_secs() as u32,
                                }).await;
                            }
                            Ok(ClientMessage::ReadActuators) => {
                                let _ = state.request_tx.send(RobotRequest::ReadActuators {
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::WriteActuators(config)) if is_driver => {
                                let _ = state.request_tx.send(RobotRequest::WriteActuators {
                                    config,
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::WriteActuators(_)) => {
                                let _ = out_tx.send(RobotMessage::ActuatorError(
                                    "bara föraren kan skriva till styrkortet".to_string(),
                                )).await;
                            }
                            Ok(ClientMessage::TestSpinVesc { can_id }) if is_driver => {
                                let _ = state.request_tx.send(RobotRequest::TestSpinVesc {
                                    can_id,
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::TestSpinVesc { can_id }) => {
                                let _ = out_tx.send(RobotMessage::VescTestSpin {
                                    can_id,
                                    ok: false,
                                    message: "bara föraren kan provsnurra".to_string(),
                                }).await;
                            }
                            Ok(ClientMessage::SetAutocenter { on }) if is_driver => {
                                let _ = state.request_tx.send(RobotRequest::SetAutocenter { on }).await;
                            }
                            Ok(ClientMessage::SetAutocenter { .. }) => {}
                            Ok(ClientMessage::ListDriveLogs) => {
                                let _ = state.request_tx.send(RobotRequest::ListDriveLogs {
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::GetDriveLog { name }) => {
                                let _ = state.request_tx.send(RobotRequest::GetDriveLog {
                                    name,
                                    respond_to: out_tx.clone(),
                                }).await;
                            }
                            Ok(ClientMessage::Ping { sent_ms }) => {
                                let _ = out_tx.send(RobotMessage::Pong { sent_ms }).await;
                            }
                            Ok(ClientMessage::Control(_)) => {
                                // Inte förare — skickar styrkommandon utan att få lov.
                                tracing::warn!("Icke-förare försökte skicka Control, ignoreras.");
                            }
                            Ok(other) => {
                                // Samma kompileringsfel som Dator 1 hittade i
                                // mock-robotd (E0004) fanns här också —
                                // `Connect`/`VideoSignal` saknade arm efter att
                                // den tysta `_ => {}` togs bort. `Connect`
                                // skickas bara som första meddelandet (hanteras
                                // separat ovanför loopen) — kommer den igen här
                                // är det avvikande, logga det. `VideoSignal`
                                // är inte ihopkopplat än (väntar på WebRTC,
                                // spec §12).
                                tracing::warn!("Meddelande som robotd inte hanterar här, ignoreras: {other:?}");
                            }
                            Err(e) => {
                                // Tidigare: tyst `_ => {}` här (upptäckt av
                                // testerna 2026-09-19 — trasiga/okända
                                // meddelanden gav aldrig svar eller loggrad,
                                // vilket t.ex. gjorde en profil med NaN
                                // omöjlig att felsöka). Nu loggas det.
                                tracing::warn!("Kunde inte tolka meddelande från klient: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        tracing::warn!("websocket-fel från klient: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    {
        let mut shared = state.shared.lock().await;
        if is_driver {
            shared.driver_connected = false;
        }
        shared.clients -= 1;
        state.clients_tx.send_replace(shared.clients > 0);
    }
    broadcast_targets.lock().await.retain(|t| !t.same_channel(&out_tx));
}

fn random_code() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..6).map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    async fn connect_client(port: u16) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
        let url = format!("ws://127.0.0.1:{port}/ws");
        let mut last_err = None;
        for _ in 0..50 {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((mut ws, _)) => {
                    let hello = serde_json::to_string(&ClientMessage::Connect { as_viewer: false, view_code: None }).unwrap();
                    ws.send(WsMessage::Text(hello)).await.unwrap();
                    return ws;
                }
                Err(e) => {
                    last_err = Some(e);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            }
        }
        panic!("kunde inte ansluta till servern: {last_err:?}");
    }

    /// `clients_connected` styr om robotd håller Car_Client — den måste bli
    /// true när en klient ansluter och false igen när den kopplar ner.
    #[tokio::test]
    async fn klientnarvaro_foljer_anslutningen() {
        let port = 39_000 + (std::process::id() % 1000) as u16;
        let mut handles = spawn(format!("127.0.0.1:{port}"));
        assert!(!*handles.clients_connected.borrow());

        let mut ws = connect_client(port).await;
        tokio::time::timeout(Duration::from_secs(2), handles.clients_connected.wait_for(|c| *c))
            .await
            .unwrap()
            .unwrap();

        // Status från huvudloopen ska nå klienten.
        let status = StatusUpdate {
            speed_kmh: Some(1.0),
            battery_percent: Some(80.0),
            gps_fix: false,
            vesc_temps_c: vec![],
            last_error: None,
            link_quality: relay_protocol::LinkQuality::Green,
            battery_voltage: Some(52.6),
            vescs_responding: vec![28, 36, 76],
            vescs_expected: vec![28, 36, 76],
            ..Default::default()
        };
        handles.status_broadcast.send(status).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(Ok(WsMessage::Text(t))) = ws.next().await {
                    if let Ok(RobotMessage::Status(s)) = serde_json::from_str::<RobotMessage>(&t) {
                        return s;
                    }
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(got.battery_voltage, Some(52.6));
        assert_eq!(got.vescs_responding, vec![28, 36, 76]);

        ws.close(None).await.unwrap();
        drop(ws);
        tokio::time::timeout(Duration::from_secs(2), handles.clients_connected.wait_for(|c| !*c))
            .await
            .unwrap()
            .unwrap();
    }
}
