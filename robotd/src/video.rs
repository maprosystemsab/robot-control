//! Kameraström: USB-kameran (V4L2/MJPEG) -> `ffmpeg` (hårdvaru-H.264) ->
//! robotd -> klient över en egen TCP-port. Trådformatet och H.264-
//! uppdelningen ligger i `relay_protocol::video` (delas med klienten).
//!
//! Designval, i den ordning de spelar roll för stabilitet:
//!
//! * **Egen anslutning, inte styr-WebSocketen.** En stor bildruta kan inte
//!   fördröja ett styr-/stoppkommando som köar bakom den.
//! * **Kameran körs bara medan någon tittar.** Ingen tittare -> `ffmpeg`
//!   stoppas efter en kort karens och kameran släpps. Ingen onödig CPU/ström.
//! * **Låg och begränsad fördröjning.** Varje klient får en bounded kö; hamnar
//!   den efter (dålig länk) släpps bilder och sändningen återupptas på nästa
//!   nyckelbild (max 1 s bort) — kön och därmed fördröjningen växer aldrig.
//!   Även kärnans TCP-sändbuffert är begränsad (`SEND_BUFFER`), annars kan
//!   den tyst lagra tiotals sekunder gammal video på en långsam länk.
//! * **Självläkande.** Dör `ffmpeg` (kamera urdragen/hängd) startas den om med
//!   backoff så länge någon tittar; en hängd kamera (inga bilder på 5 s)
//!   dödas och startas om.
//! * **Ingen styrning härifrån.** Modulen läser bara kameran och skickar bild.
//!
//! Åtkomst: porten lyssnar på samma adress som styr-porten (wg0-IP), så
//! WireGuard är det som avgör vem som kommer åt den — samma förtroende som
//! för en förar-anslutning. Notera att den INTE är kopplad till titta-koder
//! (spec §4) än: alla på VPN:et kan titta.

use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use relay_protocol::video::{contains_idr, AuSplitter, FrameHeader, MAGIC};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tokio::process::Command;
use tokio::sync::{broadcast, watch};

use crate::config::VideoConfig;

/// Bilder som får köa per klient innan den anses ha hamnat efter (~1 s).
const QUEUE_FRAMES: usize = 16;
/// Kärnans sändbuffert per klientanslutning (kärnan dubblar värdet). Måste
/// rymma bandbredd x tur-och-retur-tid (≈3 Mbit/s x 0,3 s ≈ 110 kB) för att
/// inte begränsa genomströmningen, men inte mycket mer, se modulkommentaren.
const SEND_BUFFER: u32 = 128 * 1024;
/// Hur länge kameran får fortsätta gå efter att sista tittaren försvunnit
/// (undviker att starta om den om klienten bara kopplar om).
const IDLE_GRACE: Duration = Duration::from_secs(3);
/// Inga bilder på så här länge från en körande ffmpeg -> hängd, starta om.
const STALL_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

struct Frame {
    keyframe: bool,
    capture_ms: u64,
    data: Vec<u8>,
}

/// Räknar antalet anslutna tittare via en `watch`-kanal som kamerauppgiften
/// väntar på. Släpps guarden (anslutningen dör, på vilket sätt som helst)
/// räknas den ner — inga läckta tittare.
struct ViewerGuard(Arc<watch::Sender<usize>>);

impl ViewerGuard {
    fn new(tx: Arc<watch::Sender<usize>>) -> Self {
        tx.send_modify(|n| *n += 1);
        Self(tx)
    }
}

impl Drop for ViewerGuard {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n = n.saturating_sub(1));
    }
}

/// Startar videotjänsten i bakgrunden. Ett fel här (upptagen port, ogiltig
/// adress) loggas men stoppar aldrig robotd — styrningen ska fungera utan bild.
pub fn spawn(cfg: VideoConfig, control_bind_addr: &str) {
    if !cfg.enabled {
        tracing::info!("Video avstängd i config.json (video.enabled=false).");
        return;
    }
    let mut addr: SocketAddr = match control_bind_addr.parse() {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("Video: kan inte tolka bind_addr '{control_bind_addr}': {e}");
            return;
        }
    };
    addr.set_port(cfg.port);
    tokio::spawn(async move {
        if let Err(e) = run(cfg, addr).await {
            tracing::error!("Videotjänsten avslutades med fel: {e}");
        }
    });
}

fn bind_listener(addr: SocketAddr) -> std::io::Result<TcpListener> {
    let socket = if addr.is_ipv4() { TcpSocket::new_v4()? } else { TcpSocket::new_v6()? };
    socket.set_reuseaddr(true)?;
    // Sätts på lyssnarsocketen; accepterade anslutningar ärver den.
    socket.set_send_buffer_size(SEND_BUFFER)?;
    socket.bind(addr)?;
    socket.listen(8)
}

async fn run(cfg: VideoConfig, addr: SocketAddr) -> std::io::Result<()> {
    // Vid uppstart kan WireGuard-adressen saknas en stund — försök igen.
    let mut failures = 0u32;
    let listener = loop {
        match bind_listener(addr) {
            Ok(l) => break l,
            Err(e) => {
                failures += 1;
                if failures == 1 || failures % 20 == 0 {
                    tracing::warn!("Video: kan inte lyssna på {addr} än ({e}), försöker igen var 3:e s.");
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    };
    tracing::info!(
        "Video lyssnar på {addr} ({}x{} @ {} fps, {} kbit/s, {})",
        cfg.width, cfg.height, cfg.fps, cfg.bitrate_kbps, cfg.encoder
    );

    let (frames_tx, _) = broadcast::channel::<Arc<Frame>>(QUEUE_FRAMES);
    let (viewers_tx, viewers_rx) = watch::channel(0usize);
    let viewers_tx = Arc::new(viewers_tx);

    tokio::spawn(camera_task(cfg, frames_tx.clone(), viewers_rx));

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                tracing::warn!("Video: accept misslyckades: {e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        // Prenumerera OCH räkna upp innan uppgiften startar, så kameran
        // hinner startas och inga bilder missas i glappet.
        let rx = frames_tx.subscribe();
        let guard = ViewerGuard::new(viewers_tx.clone());
        tokio::spawn(async move {
            tracing::info!("Video: tittare ansluten från {peer}");
            match serve_client(stream, rx).await {
                Ok(()) => tracing::info!("Video: tittare {peer} frånkopplad"),
                Err(e) => tracing::info!("Video: tittare {peer} bortkopplad ({e})"),
            }
            drop(guard);
        });
    }
}

async fn serve_client(
    stream: TcpStream,
    mut rx: broadcast::Receiver<Arc<Frame>>,
) -> std::io::Result<()> {
    let (mut rd, mut wr) = stream.into_split();
    tokio::time::timeout(WRITE_TIMEOUT, wr.write_all(&MAGIC))
        .await
        .map_err(|_| timeout_err())??;

    // Börja alltid på en nyckelbild: en avkodare kan inte göra något av en
    // mellanbild utan sin referens.
    let mut need_key = true;
    let mut probe = [0u8; 1];
    loop {
        tokio::select! {
            // Klienten stänger sin sida -> vi märker det även om kameran
            // inte producerar något (annars skulle tittaren aldrig räknas ner).
            r = rd.read(&mut probe) => {
                match r {
                    Ok(0) | Err(_) => return Ok(()),
                    Ok(_) => {} // klienten skickar inget i v1; ignorera
                }
            }
            f = rx.recv() => match f {
                Ok(frame) => {
                    if need_key && !frame.keyframe {
                        continue;
                    }
                    need_key = false;
                    let header = FrameHeader {
                        payload_len: frame.data.len() as u32,
                        keyframe: frame.keyframe,
                        capture_ms: frame.capture_ms,
                    };
                    // Ett enda skrivanrop per bild (en syscall, ett TCP-segmentflöde).
                    let mut msg = Vec::with_capacity(relay_protocol::video::HEADER_LEN + frame.data.len());
                    msg.extend_from_slice(&header.encode());
                    msg.extend_from_slice(&frame.data);
                    tokio::time::timeout(WRITE_TIMEOUT, wr.write_all(&msg))
                        .await
                        .map_err(|_| timeout_err())??;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    // Länken hänger inte med: släpp allt gammalt och hoppa
                    // till nästa nyckelbild.
                    tracing::debug!("Video: klient efter med {n} bilder, väntar på nästa nyckelbild");
                    need_key = true;
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            }
        }
    }
}

fn timeout_err() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "skrivning till klient tog för lång tid")
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Bygger `ffmpeg`-kommandoraden. Utbruten som ren funktion så den går att testa.
///
/// * `-bsf:v dump_extra=freq=keyframe` lägger SPS/PPS före varje nyckelbild
///   (en tittare som ansluter mitt i strömmen kan då börja avkoda direkt).
/// * `h264_metadata=aud=insert` lägger en Access Unit Delimiter före varje
///   bild — det är vad `AuSplitter` delar på.
/// * `-fps_mode passthrough`: låt kameran bestämma takten. I dåligt ljus
///   sänker webbkameran sin bildfrekvens; utan detta skulle ffmpeg duplicera
///   bilder för att fylla ut.
fn ffmpeg_args(cfg: &VideoConfig) -> Vec<String> {
    let s = |x: &str| x.to_string();
    let kbps = format!("{}k", cfg.bitrate_kbps);
    let mut a: Vec<String> = vec![
        s("-hide_banner"), s("-loglevel"), s("error"), s("-nostdin"),
        s("-fflags"), s("nobuffer"),
        s("-f"), s("v4l2"), s("-input_format"), cfg.input_format.clone(),
        s("-video_size"), format!("{}x{}", cfg.width, cfg.height),
        s("-framerate"), cfg.fps.to_string(),
        s("-i"), cfg.device.clone(),
    ];
    if cfg.stereo_left_only {
        a.extend([s("-vf"), s("crop=iw/2:ih:0:0")]);
    }
    a.extend([s("-pix_fmt"), s("yuv420p"), s("-fps_mode"), s("passthrough")]);
    if cfg.encoder == "libx264" {
        a.extend([
            s("-c:v"), s("libx264"), s("-preset"), s("ultrafast"), s("-tune"), s("zerolatency"),
            s("-b:v"), kbps.clone(), s("-maxrate"), kbps.clone(),
            s("-bufsize"), format!("{}k", cfg.bitrate_kbps / 2),
        ]);
    } else {
        a.extend([s("-c:v"), cfg.encoder.clone(), s("-b:v"), kbps]);
    }
    a.extend([
        s("-g"), cfg.fps.to_string(), s("-bf"), s("0"),
        s("-bsf:v"), s("dump_extra=freq=keyframe,h264_metadata=aud=insert"),
        s("-flush_packets"), s("1"),
        s("-f"), s("h264"), s("pipe:1"),
    ]);
    a
}

/// Väntar på tittare och kör kamerapipelinen så länge det finns någon.
async fn camera_task(
    cfg: VideoConfig,
    tx: broadcast::Sender<Arc<Frame>>,
    mut viewers: watch::Receiver<usize>,
) {
    loop {
        while *viewers.borrow() == 0 {
            if viewers.changed().await.is_err() {
                return; // avsändaren borta = robotd avslutas
            }
        }
        run_while_viewed(&cfg, &tx, &mut viewers).await;
    }
}

/// Kör ffmpeg (med omstart vid fel) tills tittarna försvunnit i `IDLE_GRACE`.
async fn run_while_viewed(
    cfg: &VideoConfig,
    tx: &broadcast::Sender<Arc<Frame>>,
    viewers: &mut watch::Receiver<usize>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if *viewers.borrow() == 0 {
            return;
        }
        tracing::info!("Video: startar kameran ({} via {})", cfg.device, cfg.encoder);
        let started = Instant::now();
        match pipeline_once(cfg, tx, viewers).await {
            PipelineEnd::NoViewers => {
                tracing::info!("Video: inga tittare kvar — kameran stoppad.");
                return;
            }
            PipelineEnd::Failed(why) => {
                // Lyckades vi leverera bilder en stund är felet nytt; börja om på 1 s.
                if started.elapsed() > Duration::from_secs(20) {
                    backoff = Duration::from_secs(1);
                }
                tracing::warn!("Video: kamerapipelinen föll ({why}) — nytt försök om {backoff:?}");
                // Vänta, men avbryt direkt om alla tittare försvinner under tiden.
                let deadline = tokio::time::Instant::now() + backoff;
                loop {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => break,
                        r = viewers.changed() => {
                            if r.is_err() { return; }
                            if *viewers.borrow() == 0 { return; }
                        }
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(10));
            }
        }
    }
}

enum PipelineEnd {
    NoViewers,
    Failed(String),
}

async fn pipeline_once(
    cfg: &VideoConfig,
    tx: &broadcast::Sender<Arc<Frame>>,
    viewers: &mut watch::Receiver<usize>,
) -> PipelineEnd {
    let mut child = match Command::new("ffmpeg")
        .args(ffmpeg_args(cfg))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true) // dör robotd dör ffmpeg, kameran släpps
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return PipelineEnd::Failed(format!("kunde inte starta ffmpeg: {e} (är ffmpeg installerat?)")),
    };
    let mut stdout = child.stdout.take().expect("stdout piped");
    if let Some(stderr) = child.stderr.take() {
        // Logga ffmpegs felutskrifter (max 20 rader per körning så en
        // spammande kamera inte fyller journalen).
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            let mut n = 0;
            while let Ok(Some(line)) = lines.next_line().await {
                if n < 20 {
                    tracing::warn!("ffmpeg: {line}");
                }
                n += 1;
            }
        });
    }

    let mut splitter = AuSplitter::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut idle_since: Option<tokio::time::Instant> = None;
    let mut last_frame = Instant::now();
    let mut stats_since = Instant::now();
    let (mut n_frames, mut n_bytes) = (0u32, 0usize);
    let mut watchdog = tokio::time::interval(Duration::from_secs(1));

    let end = loop {
        let idle_sleep = async {
            match idle_since {
                Some(t) => tokio::time::sleep_until(t + IDLE_GRACE).await,
                None => std::future::pending::<()>().await,
            }
        };
        tokio::select! {
            r = stdout.read(&mut buf) => match r {
                Ok(0) => break PipelineEnd::Failed("ffmpeg avslutades (EOF)".into()),
                Err(e) => break PipelineEnd::Failed(format!("läsfel från ffmpeg: {e}")),
                Ok(n) => {
                    for au in splitter.push(&buf[..n]) {
                        last_frame = Instant::now();
                        n_frames += 1;
                        n_bytes += au.len();
                        let frame = Arc::new(Frame {
                            keyframe: contains_idr(&au),
                            capture_ms: now_ms(),
                            data: au,
                        });
                        let _ = tx.send(frame); // Err = inga mottagare just nu, inget fel
                    }
                }
            },
            ch = viewers.changed() => {
                if ch.is_err() { break PipelineEnd::NoViewers; }
                idle_since = if *viewers.borrow() == 0 { Some(tokio::time::Instant::now()) } else { None };
            }
            _ = idle_sleep => break PipelineEnd::NoViewers,
            _ = watchdog.tick() => {
                if last_frame.elapsed() > STALL_TIMEOUT {
                    break PipelineEnd::Failed(format!("inga bilder på {STALL_TIMEOUT:?}"));
                }
                let dt = stats_since.elapsed();
                if dt >= Duration::from_secs(10) {
                    tracing::info!(
                        "Video: {:.1} bilder/s, {:.0} kbit/s, {} tittare",
                        n_frames as f32 / dt.as_secs_f32(),
                        n_bytes as f32 * 8.0 / dt.as_secs_f32() / 1000.0,
                        *viewers.borrow()
                    );
                    stats_since = Instant::now();
                    n_frames = 0;
                    n_bytes = 0;
                }
            }
        }
    };

    // Vänta in att ffmpeg verkligen släppt kameran innan ev. omstart.
    let _ = child.kill().await;
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(cfg: &VideoConfig) -> String {
        ffmpeg_args(cfg).join(" ")
    }

    #[test]
    fn hardware_encoder_args() {
        let cfg = VideoConfig::default();
        let a = joined(&cfg);
        assert!(a.contains("-f v4l2 -input_format mjpeg -video_size 1024x576 -framerate 15 -i /dev/video0"), "{a}");
        assert!(a.contains("-c:v h264_v4l2m2m -b:v 900k"), "{a}");
        assert!(a.contains("-g 15 -bf 0"), "{a}");
        assert!(a.contains("h264_metadata=aud=insert"), "{a}");
        assert!(a.ends_with("-f h264 pipe:1"), "{a}");
    }

    #[test]
    fn software_fallback_args() {
        let cfg = VideoConfig { encoder: "libx264".into(), bitrate_kbps: 600, ..VideoConfig::default() };
        let a = joined(&cfg);
        assert!(a.contains("-c:v libx264 -preset ultrafast -tune zerolatency"), "{a}");
        assert!(a.contains("-b:v 600k -maxrate 600k -bufsize 300k"), "{a}");
    }

    #[test]
    fn stereo_camera_left_eye() {
        let cfg = VideoConfig {
            input_format: "yuyv422".into(),
            stereo_left_only: true,
            width: 2560,
            height: 720,
            ..VideoConfig::default()
        };
        let a = joined(&cfg);
        assert!(a.contains("-input_format yuyv422 -video_size 2560x720"), "{a}");
        assert!(a.contains("-vf crop=iw/2:ih:0:0 -pix_fmt yuv420p"), "{a}");
        assert!(!joined(&VideoConfig::default()).contains("crop"));
    }

    #[tokio::test]
    async fn viewer_guard_counts_up_and_down() {
        let (tx, rx) = watch::channel(0usize);
        let tx = Arc::new(tx);
        let g1 = ViewerGuard::new(tx.clone());
        let g2 = ViewerGuard::new(tx.clone());
        assert_eq!(*rx.borrow(), 2);
        drop(g1);
        assert_eq!(*rx.borrow(), 1);
        drop(g2);
        assert_eq!(*rx.borrow(), 0);
    }
}
