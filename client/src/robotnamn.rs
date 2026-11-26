//! Robotarnas namn (t.ex. "FMV 1") i stället för bara IP-adressen på anslutningsskärmen.
//!
//! Namnen kommer från serverns robotlista (samma som RControlStation visar,
//! `http://192.168.200.1:8080/all_machines`, kunddatorer får bara sina egna robotar).
//! Listan hämtas i bakgrunden vid start och sparas, så namnen syns även utan
//! kontakt med servern. Annan server: miljövariabeln `ROBOTSTYRNING_SERVER`
//! (t.ex. `192.168.200.1:8080`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const STANDARDSERVER: &str = "192.168.200.1:8080";

#[derive(Clone, Default)]
pub struct RobotNamn {
    /// IP-adress -> namn.
    namn: Arc<Mutex<BTreeMap<String, String>>>,
}

impl RobotNamn {
    /// Laddar sparade namn och börjar hämta serverns lista i bakgrunden.
    pub fn starta(rt: &tokio::runtime::Handle) -> Self {
        let r = Self { namn: Arc::new(Mutex::new(ladda())) };
        let delad = r.namn.clone();
        rt.spawn(async move {
            let server = std::env::var("ROBOTSTYRNING_SERVER").unwrap_or_else(|_| STANDARDSERVER.to_string());
            match tokio::time::timeout(Duration::from_secs(8), hamta_xml(&server)).await {
                Ok(Ok(xml)) => {
                    let ny = tolka_maskiner(&xml);
                    if !ny.is_empty() {
                        let mut m = delad.lock().unwrap();
                        // Serverns namn gäller; adresser som försvunnit därifrån behåller sitt
                        // sparade namn (kan vara en robot som bara står avstängd).
                        m.extend(ny);
                        spara(&m);
                    }
                }
                Ok(Err(e)) => tracing::info!("robotnamn: kunde inte hämta listan från {server}: {e}"),
                Err(_) => tracing::info!("robotnamn: servern {server} svarade inte"),
            }
        });
        r
    }

    /// Namnet för en adress, om det finns.
    pub fn namn_for(&self, adress: &str) -> Option<String> {
        self.namn.lock().unwrap().get(adress.trim()).cloned()
    }

    /// Adressen för ett inskrivet namn (skiftläge och mellanslag spelar ingen roll),
    /// så att man kan skriva "fmv 2" i rutan.
    pub fn adress_for(&self, text: &str) -> Option<String> {
        let nyckel = normalisera(text);
        if nyckel.is_empty() {
            return None;
        }
        self.namn
            .lock()
            .unwrap()
            .iter()
            .find(|(_, n)| normalisera(n) == nyckel)
            .map(|(ip, _)| ip.clone())
    }
}

fn normalisera(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).flat_map(|c| c.to_lowercase()).collect()
}

/// Nyckel för bokstavsordning: skiftläge spelar ingen roll, å ä ö efter z (svensk
/// ordning) och siffror i talordning ("FMV 2" före "FMV 10", ".9" före ".10").
pub fn sorteringsnyckel(s: &str) -> String {
    let mut ut = String::new();
    let mut tal = String::new();
    for c in s.trim().chars().flat_map(|c| c.to_lowercase()).chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            tal.push(c);
            continue;
        }
        if !tal.is_empty() {
            ut.push_str(&format!("{:0>12}", tal));
            tal.clear();
        }
        match c {
            'å' => ut.push('{'),
            'ä' => ut.push('|'),
            'ö' => ut.push('}'),
            '\0' => {}
            c => ut.push(c),
        }
    }
    ut
}

async fn hamta_xml(server: &str) -> Result<String, String> {
    let mut s = tokio::net::TcpStream::connect(server).await.map_err(|e| e.to_string())?;
    let host = server.split(':').next().unwrap_or(server);
    let fraga = format!("GET /all_machines HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    s.write_all(fraga.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut svar = Vec::new();
    s.read_to_end(&mut svar).await.map_err(|e| e.to_string())?;
    let svar = String::from_utf8_lossy(&svar).into_owned();
    let (huvud, kropp) = svar.split_once("\r\n\r\n").ok_or("konstigt svar")?;
    if !huvud.lines().next().unwrap_or("").contains(" 200") {
        return Err(huvud.lines().next().unwrap_or("").to_string());
    }
    Ok(kropp.to_string())
}

fn tagg<'a>(s: &'a str, namn: &str) -> Option<&'a str> {
    let start = s.find(&format!("<{namn}>"))? + namn.len() + 2;
    let slut = s[start..].find(&format!("</{namn}>"))? + start;
    Some(&s[start..slut])
}

fn avkoda(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// `<machines><machine><name>FMV 1</name><ip>192.168.200.13</ip>…</machine>…` -> IP -> namn.
pub fn tolka_maskiner(xml: &str) -> BTreeMap<String, String> {
    let mut ut = BTreeMap::new();
    for bit in xml.split("<machine>").skip(1) {
        if let (Some(n), Some(ip)) = (tagg(bit, "name"), tagg(bit, "ip")) {
            let (n, ip) = (avkoda(n).trim().to_string(), ip.trim().to_string());
            if !n.is_empty() && !ip.is_empty() {
                ut.insert(ip, n);
            }
        }
    }
    ut
}

fn sokvag() -> Option<std::path::PathBuf> {
    directories::ProjectDirs::from("se", "robot-control", "robotstyrning").map(|d| d.config_dir().join("robotnamn.json"))
}

fn ladda() -> BTreeMap<String, String> {
    sokvag()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn spara(m: &BTreeMap<String, String>) {
    if let Some(p) = sokvag() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(json) = serde_json::to_string_pretty(m) {
            let _ = std::fs::write(p, json);
        }
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    #[test]
    fn tolkar_serverns_lista() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<machines><machine><id>6</id><name>FMV 1</name><ip>192.168.200.13</ip><iVehicletype>1</iVehicletype></machine><machine><id>11</id><name>Ås &amp; Berg</name><ip>192.168.200.19</ip></machine><machine><id>12</id><name>Utan ip</name><ip></ip></machine></machines>"#;
        let m = tolka_maskiner(xml);
        assert_eq!(m.get("192.168.200.13").map(String::as_str), Some("FMV 1"));
        assert_eq!(m.get("192.168.200.19").map(String::as_str), Some("Ås & Berg"));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn bokstavsordning() {
        let mut l = vec!["RobAnt3", "fmv 10", "Ödla", "FMV 2", "192.168.200.77", "Älg", "Åsa", "192.168.200.8", "Zebra"];
        l.sort_by_cached_key(|s| sorteringsnyckel(s));
        assert_eq!(l, ["192.168.200.8", "192.168.200.77", "FMV 2", "fmv 10", "RobAnt3", "Zebra", "Åsa", "Älg", "Ödla"]);
    }

    #[test]
    fn namn_till_adress() {
        let r = RobotNamn::default();
        r.namn.lock().unwrap().insert("192.168.200.18".into(), "FMV 2".into());
        assert_eq!(r.adress_for("fmv2").as_deref(), Some("192.168.200.18"));
        assert_eq!(r.adress_for(" FMV 2 ").as_deref(), Some("192.168.200.18"));
        assert_eq!(r.adress_for("FMV 3"), None);
        assert_eq!(r.namn_for("192.168.200.18").as_deref(), Some("FMV 2"));
    }
}
