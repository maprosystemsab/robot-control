//! `mock-robotd` — en låtsas-robot för att testa HELA kedjan (klient +
//! nätverk + UI) utan någon hårdvara alls. Sedan arkitekturändringen
//! 2026-09-19 (se PROJECT_SPEC.md §14) är den, precis som riktiga
//! `robotd`, en SERVER som lyssnar direkt på en adress — klienten
//! ansluter rakt in, inget relä.
//!
//! Använd den här för att testköra klientens hela UX (anslutningsskärm,
//! AKTIVERA-flödet, färgad ram, Settings-vyn) medan ni väntar på riktig
//! hårdvara, eller för att testa över er faktiska WireGuard-tunnel innan
//! den riktiga roboten är redo.
//!
//! Körs så här (två terminaler räcker, samma dator eller olika VPN-noder):
//! ```bash
//! # Terminal 1: låtsas-roboten, lyssnar på alla gränssnitt port 9000
//! cargo run -p mock-robotd -- --bind 0.0.0.0:9000
//!
//! # Terminal 2: klienten
//! cargo run -p robotstyrning
//! # skriv in "127.0.0.1" (eller robotens VPN-IP om ni testar över tunneln)
//! # på anslutningsskärmen
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use relay_protocol::{ClientMessage, ControlCommand, LinkQuality, RobotMessage, StatusUpdate, VescSighting};
use tokio::sync::Mutex;

#[derive(Clone, Default)]
struct AppState {
    driver_connected: Arc<Mutex<bool>>,
    latest_throttle: Arc<Mutex<f32>>,
    latest_steering: Arc<Mutex<f32>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args: Vec<String> = std::env::args().collect();
    let bind_addr = arg_value(&args, "--bind").unwrap_or_else(|| "0.0.0.0:9000".to_string());

    println!("=== mock-robotd ===");
    println!("Lyssnar på: {bind_addr}");
    println!("Anslut med klienten mot den här maskinens IP (t.ex. 127.0.0.1 lokalt, eller VPN-IP:n om ni testar över WireGuard).");
    println!();

    let state = AppState::default();

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .unwrap_or_else(|e| panic!("kunde inte binda {bind_addr}: {e}"));
    axum::serve(listener, app).await.unwrap();
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut ws_tx, mut ws_rx) = socket.split();

    let Some(Ok(Message::Text(text))) = ws_rx.next().await else { return };
    let Ok(ClientMessage::Connect { as_viewer, .. }) = serde_json::from_str(&text) else { return };

    let is_driver = if as_viewer {
        false
    } else {
        let mut d = state.driver_connected.lock().await;
        if *d {
            println!("Förar-anslutning nekad: roboten körs redan av någon annan.");
            let _ = send(&mut ws_tx, &RobotMessage::ConnectDenied {
                reason: "roboten körs redan av någon annan".to_string(),
            }).await;
            return;
        }
        *d = true;
        true
    };

    let _ = send(&mut ws_tx, &if is_driver {
        RobotMessage::ConnectedAsDriver
    } else {
        RobotMessage::ConnectedAsViewer
    }).await;

    println!("Klient ansluten som {}.", if is_driver { "FÖRARE" } else { "ÅSKÅDARE" });

    let mut battery_percent: f32 = 87.0;
    let mut speed_kmh: f32 = 0.0;
    let mut lights_on = false;
    // Låtsas-styrkortets aktuatorer, som RobAnt: 28/36 fart, 76 styrning.
    let mock_actuators = tokio::sync::Mutex::new(relay_protocol::ActuatorConfig {
        count: 3,
        slots: vec![
            relay_protocol::ActuatorSlot { kind: 0, vesc_id: 28, activity: 10, mode: 0 },
            relay_protocol::ActuatorSlot { kind: 0, vesc_id: 36, activity: 10, mode: 0 },
            relay_protocol::ActuatorSlot { kind: 0, vesc_id: 76, activity: 11, mode: 0 },
            relay_protocol::ActuatorSlot::default(),
        ],
    });
    let mut status_tick = tokio::time::interval(Duration::from_millis(500));

    const STALE_TIMEOUT: Duration = Duration::from_secs(5);
    let mut last_msg_at = Instant::now();
    let mut stale_check = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = status_tick.tick() => {
                let throttle = *state.latest_throttle.lock().await;
                let target_speed = throttle * 15.0;
                speed_kmh += (target_speed - speed_kmh) * 0.3;
                battery_percent = (battery_percent - 0.01).max(0.0);

                let steering = *state.latest_steering.lock().await;
                let status = StatusUpdate {
                    speed_kmh: Some(speed_kmh.abs()),
                    battery_percent: Some(battery_percent),
                    gps_fix: true,
                    vesc_temps_c: vec![32.0, 33.5, 29.0],
                    last_error: None,
                    link_quality: LinkQuality::Green,
                    battery_voltage: Some(48.0 + 6.4 * battery_percent / 100.0),
                    vescs_responding: vec![28, 36, 76],
                    vescs_expected: vec![28, 36, 76],
                    // Påhittade fordonsdata, för att se instrumentpanelen utan robot.
                    battery_charging: if speed_kmh.abs() < 0.2 { Some(true) } else { None },
                    power_w: Some(40.0 + speed_kmh.abs() * 60.0),
                    wh_per_km: Some(85.0),
                    range_km: Some(battery_percent / 100.0 * 4096.0 / 85.0),
                    steering_deg: Some(steering * 25.0),
                    steering_percent: Some(steering * 100.0),
                    roll_deg: Some(2.5),
                    pitch_deg: Some(-1.0),
                    yaw_deg: Some(123.0),
                    fault_text: (battery_percent < 15.0).then(|| "Underspänning – batteriet är nästan tomt".to_string()),
                    rssi_dbm: Some(-71),
                    signal_info: Some("4G Telia · RSRP −95 dBm · SINR 12 dB".to_string()),
                    estop: None,
                };
                if send(&mut ws_tx, &RobotMessage::Status(status)).await.is_err() {
                    break;
                }
            }
            _ = stale_check.tick() => {
                if last_msg_at.elapsed() > STALE_TIMEOUT {
                    println!("Klient tyst i >{STALE_TIMEOUT:?}, kopplar ner.");
                    break;
                }
            }
            incoming = ws_rx.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        last_msg_at = Instant::now();
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(ClientMessage::Control(cmd)) if is_driver => {
                                *state.latest_throttle.lock().await = cmd.throttle;
                                *state.latest_steering.lock().await = cmd.steering;
                                if cmd.lights != lights_on {
                                    lights_on = cmd.lights;
                                    println!("Belysning: {}", if lights_on { "PÅ" } else { "AV" });
                                }
                                print_command(&cmd);
                            }
                            Ok(ClientMessage::ScanVescBus) => {
                                println!("Settings-vy begärde VESC-skanning — svarar med påhittade VESC:ar.");
                                let fake = vec![
                                    VescSighting { can_id: 32, responding: true },
                                    VescSighting { can_id: 44, responding: true },
                                    VescSighting { can_id: 97, responding: true },
                                    VescSighting { can_id: 88, responding: true },
                                ];
                                let _ = send(&mut ws_tx, &RobotMessage::VescBusResult(fake)).await;
                            }
                            Ok(ClientMessage::SaveVescProfile(profile)) => {
                                println!("Settings-vy sparade en profil ({} roller) — mock, sparar inte på riktigt.", profile.roller.len());
                                let _ = send(&mut ws_tx, &RobotMessage::VescProfileSaved).await;
                            }
                            Ok(ClientMessage::Ping { sent_ms }) => {
                                let _ = send(&mut ws_tx, &RobotMessage::Pong { sent_ms }).await;
                            }
                            Ok(ClientMessage::ListDriveLogs) => {
                                let logs = vec![
                                    relay_protocol::DriveLogInfo { name: "2026-10-05_14-03-22.csv".into(), bytes: 182_400 },
                                    relay_protocol::DriveLogInfo { name: "2026-10-04_09-31-07.csv".into(), bytes: 1_204_331 },
                                ];
                                let _ = send(&mut ws_tx, &RobotMessage::DriveLogs(logs)).await;
                            }
                            Ok(ClientMessage::GetDriveLog { name }) => {
                                let csv = "tid;fart_kmh\n2026-10-05 14:03:22;3.2\n".to_string();
                                let _ = send(&mut ws_tx, &RobotMessage::DriveLog { name, csv }).await;
                            }
                            Ok(ClientMessage::ReadActuators) => {
                                println!("Läser aktuatorer (påhittade, som RobAnt).");
                                let cfg = mock_actuators.lock().await.clone();
                                let _ = send(&mut ws_tx, &RobotMessage::Actuators(cfg)).await;
                            }
                            Ok(ClientMessage::WriteActuators(cfg)) => {
                                println!("Skriver aktuatorer (bara i minnet): {cfg:?}");
                                *mock_actuators.lock().await = cfg.clone();
                                let _ = send(&mut ws_tx, &RobotMessage::ActuatorsWritten(cfg)).await;
                            }
                            Ok(ClientMessage::TestSpinVesc { can_id }) => {
                                println!("Provsnurr VESC {can_id} (påhittat).");
                                let message = format!("VESC {can_id} snurrar 1 s.");
                                let _ = send(&mut ws_tx, &RobotMessage::VescTestSpin { can_id, ok: true, message }).await;
                            }
                            Ok(ClientMessage::Control(_)) => {
                                println!("Icke-förare försökte skicka Control, ignoreras.");
                            }
                            Ok(other) => {
                                // Kompileringsfel förra omgången (E0004, icke-
                                // uttömmande match) — upptäckt av Dator 1 i
                                // regressionstestet 2026-09-19. `Connect`,
                                // `VideoSignal` och `RequestViewCode` saknade
                                // arm här efter att den tysta `_ => {}` togs
                                // bort. Mock-robotd hanterar dem inte (ingen
                                // titta-kod/video i mocken), men loggar dem nu
                                // istället för att inte kompilera alls.
                                println!("Meddelande som mock-robotd inte hanterar, ignoreras: {other:?}");
                            }
                            Err(e) => {
                                // Tidigare tyst `_ => {}` — upptäckt av testerna
                                // 2026-09-19 (en profil med t.ex. NaN gav aldrig
                                // svar eller loggrad). Nu loggas det.
                                println!("Kunde inte tolka meddelande från klient: {e}");
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(e)) => {
                        println!("websocket-fel: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    if is_driver {
        *state.driver_connected.lock().await = false;
        // Bugg hittad av Dator 1 (2026-09-19): utan den här raden stod det
        // senaste gasvärdet kvar delat i AppState, så en ny åskådare som
        // anslöt direkt efteråt såg hastigheten fortsätta klättra mot ~15
        // km/h utan att någon körde.
        *state.latest_throttle.lock().await = 0.0;
    }
    println!("Klient frånkopplad.");
}

fn print_command(cmd: &ControlCommand) {
    if cmd.throttle.abs() > 0.05 || cmd.steering.abs() > 0.05 {
        println!("-> gas={:+.2} styr={:+.2} aktiverad={}", cmd.throttle, cmd.steering, cmd.activated);
    }
}

async fn send<T: serde::Serialize>(
    tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    msg: &T,
) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).unwrap();
    tx.send(Message::Text(text)).await
}
