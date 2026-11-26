//! robotd:s version: `<paketets version>+<bygge>`, där bygget är tiden för senaste commit
//! i UTC som ÅÅÅÅMMDDTTMM (samma form som SentinAnts program). Den används i
//! `robotd --version` och när firmwarens krav på robotd kontrolleras. Utanför git (eller
//! för prov) sätts bygget med miljövariabeln `ROBOTD_BYGGE`; annars blir det 0.

use std::path::Path;
use std::process::Command;

fn main() {
    let mapp = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=ROBOTD_BYGGE");
    let bygge = std::env::var("ROBOTD_BYGGE").ok().filter(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit())).or_else(|| git_bygge(&mapp)).unwrap_or_else(|| "0".into());
    let paket = std::env::var("CARGO_PKG_VERSION").unwrap();
    let version = if bygge == "0" { paket } else { format!("{paket}+{bygge}") };
    println!("cargo:rustc-env=ROBOTD_VERSION={version}");
}

/// Tiden för senaste commit (UTC, ÅÅÅÅMMDDTTMM). Bygget körs om när HEAD flyttas.
fn git_bygge(mapp: &Path) -> Option<String> {
    let git = |arg: &[&str]| -> Option<String> {
        let ut = Command::new("git").args(arg).current_dir(mapp).env("TZ", "UTC").output().ok()?;
        ut.status.success().then(|| String::from_utf8_lossy(&ut.stdout).trim().to_string())
    };
    if let Some(gitmapp) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={gitmapp}/logs/HEAD");
        println!("cargo:rerun-if-changed={gitmapp}/HEAD");
    }
    let tid = git(&["log", "-1", "--format=%cd", "--date=format-local:%Y%m%d%H%M"])?;
    (tid.len() == 12 && tid.bytes().all(|c| c.is_ascii_digit())).then_some(tid)
}
