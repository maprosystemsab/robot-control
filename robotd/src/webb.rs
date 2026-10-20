//! Robotstyrning i webbläsaren (telefon/iPad), steg 1 2026-10-20: bara
//! kamerabilden. HTTPS på `PORT` med ett självsignerat certifikat, eftersom
//! Safari bara avkodar video (WebCodecs) och håller skärmen tänd på
//! https-sidor.
//!
//!   GET /        testsidan (webb/index.html, inbakad i programmet)
//!   GET /video   WebSocket med kamerabilden, en binär bild per meddelande:
//!                [1 byte: 1 = nyckelbild][8 byte: tagen, ms sedan 1970, BE][H.264 Annex B]
//!
//! Ingen styrning härifrån: sidan kan bara titta. Samma kamera och ffmpeg som
//! Robotstyrnings videoport (video.rs). Åtkomst som för videoporten: den som
//! når robotens WireGuard-adress kan titta.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header::ORIGIN, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use tokio::sync::broadcast;

use crate::video::{self, Frame};

pub const PORT: u16 = 9443;
const CERT_PATH: &str = "/etc/robotd/webb-cert.pem";
const KEY_PATH: &str = "/etc/robotd/webb-key.pem";
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

const INDEX_HTML: &str = include_str!("../webb/index.html");

/// Startar webbservern i bakgrunden. Fel loggas men stoppar aldrig robotd.
pub fn spawn(control_bind_addr: &str) {
    let mut addr: SocketAddr = match control_bind_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Webb: kan inte tolka bind_addr '{control_bind_addr}': {e}");
            return;
        }
    };
    addr.set_port(PORT);
    tokio::spawn(async move {
        if let Err(e) = run(addr).await {
            tracing::error!("Webbservern avslutades med fel: {e}");
        }
    });
}

async fn run(addr: SocketAddr) -> std::io::Result<()> {
    // ring i stället för aws-lc (som är tungt att bygga på Pi:n).
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (cert, key) = load_or_create_cert(addr.ip())?;
    let tls = RustlsConfig::from_pem(cert, key).await?;
    let app = Router::new()
        .route("/", get(|| async { Html(INDEX_HTML) }))
        .route("/video", get(video_ws))
        .with_state(Arc::new(format!("https://{addr}")));

    // Som styrporten: WireGuard-adressen kan saknas en stund vid uppstart.
    let mut failures = 0u32;
    let listener = loop {
        match std::net::TcpListener::bind(addr) {
            Ok(l) => break l,
            Err(e) => {
                failures += 1;
                if failures == 1 || failures % 20 == 0 {
                    tracing::warn!("Webb: kan inte lyssna på {addr} än ({e}), försöker igen var 3:e s.");
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    };
    listener.set_nonblocking(true)?;
    tracing::info!("Webb (telefon/iPad) lyssnar på https://{addr}/");
    axum_server::from_tcp_rustls(listener, tls)
        .serve(app.into_make_service())
        .await
}

/// Certifikatet sparas, så att telefonens "Visa ändå" gäller även efter
/// omstart. Nytt skapas om filerna saknas.
fn load_or_create_cert(ip: IpAddr) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    if let (Ok(c), Ok(k)) = (std::fs::read(CERT_PATH), std::fs::read(KEY_PATH)) {
        return Ok((c, k));
    }
    let mut params = rcgen::CertificateParams::default();
    params.distinguished_name.push(rcgen::DnType::CommonName, format!("robotd {ip}"));
    params.distinguished_name.push(rcgen::DnType::OrganizationName, "Mapro System");
    params.subject_alt_names = vec![rcgen::SanType::IpAddress(ip)];
    // Apple godtar inte serverintyg längre än 825 dagar, inte ens efter "Visa ändå".
    params.not_before = rcgen::date_time_ymd(2026, 1, 1);
    params.not_after = rcgen::date_time_ymd(2028, 1, 1);
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let cert = rcgen::Certificate::from_params(params).map_err(std::io::Error::other)?;
    let c = cert.serialize_pem().map_err(std::io::Error::other)?;
    let k = cert.serialize_private_key_pem();
    if let Some(dir) = Path::new(CERT_PATH).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(CERT_PATH, &c).and_then(|_| write_private(KEY_PATH, &k)) {
        tracing::warn!("Webb: kunde inte spara certifikatet ({e}); ett nytt skapas vid nästa start.");
    } else {
        tracing::info!("Webb: nytt självsignerat certifikat för {ip} sparat i {CERT_PATH}");
    }
    Ok((c.into_bytes(), k.into_bytes()))
}

/// Nyckeln läsbar bara för root.
fn write_private(path: &str, data: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(data.as_bytes())
}

/// Bara robotens egen sida får öppna videon, inte en främmande sida som
/// någon på VPN:et råkar besöka (webbläsaren skickar alltid Origin).
fn origin_ok(headers: &HeaderMap, own: &str) -> bool {
    headers.get(ORIGIN).and_then(|v| v.to_str().ok()) == Some(own)
}

async fn video_ws(State(own): State<Arc<String>>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    if !origin_ok(&headers, &own) {
        tracing::warn!("Webb: videoanslutning nekad, Origin {:?}", headers.get(ORIGIN));
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(serve_video).into_response()
}

async fn serve_video(mut socket: WebSocket) {
    let Some(hub) = video::hub() else {
        let _ = socket.send(Message::Text("video avstängd i robotd (video.enabled=false)".into())).await;
        return;
    };
    let (mut rx, guard) = hub.subscribe();
    tracing::info!("Webb: tittare ansluten");
    let res = send_frames(&mut socket, &mut rx).await;
    drop(guard);
    tracing::info!("Webb: tittare frånkopplad ({})", res.err().unwrap_or_else(|| "stängd".into()));
}

async fn send_frames(socket: &mut WebSocket, rx: &mut broadcast::Receiver<Arc<Frame>>) -> Result<(), String> {
    // Som videoporten: börja på en nyckelbild och hoppa till nästa om länken
    // inte hänger med, så att fördröjningen aldrig växer.
    let mut need_key = true;
    loop {
        tokio::select! {
            m = socket.recv() => match m {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => return Ok(()),
                Some(Ok(_)) => {} // sidan skickar inget i steg 1
            },
            f = rx.recv() => match f {
                Ok(frame) => {
                    if need_key && !frame.keyframe {
                        continue;
                    }
                    need_key = false;
                    let mut msg = Vec::with_capacity(9 + frame.data.len());
                    msg.push(frame.keyframe as u8);
                    msg.extend_from_slice(&frame.capture_ms.to_be_bytes());
                    msg.extend_from_slice(&frame.data);
                    tokio::time::timeout(WRITE_TIMEOUT, socket.send(Message::Binary(msg)))
                        .await
                        .map_err(|_| "skrivning tog för lång tid".to_string())?
                        .map_err(|e| e.to_string())?;
                }
                Err(broadcast::error::RecvError::Lagged(_)) => need_key = true,
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn certifikatet_duger_for_https() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (c, k) = load_or_create_cert("192.168.200.14".parse().unwrap()).unwrap();
        let pem = String::from_utf8(c.clone()).unwrap();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"), "{pem}");
        RustlsConfig::from_pem(c, k).await.expect("rustls godtar certifikatet");
    }

    #[test]
    fn bara_egen_sida_far_titta() {
        let own = "https://192.168.200.14:9443";
        let mut h = HeaderMap::new();
        assert!(!origin_ok(&h, own));
        h.insert(ORIGIN, "https://evil.example".parse().unwrap());
        assert!(!origin_ok(&h, own));
        h.insert(ORIGIN, own.parse().unwrap());
        assert!(origin_ok(&h, own));
    }

    #[test]
    fn sidan_ar_inbakad() {
        assert!(INDEX_HTML.contains("/video"));
    }
}
