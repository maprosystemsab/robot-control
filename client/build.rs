//! Byggskript för SentinAnt Lite (klienten, paketet `robotstyrning`): programmets version
//! och, för Windows, ikonen och versionsuppgifterna i programfilen.
//!
//! Versionen blir `<paketets version>+<bygge>`, där bygget är tiden för senaste
//! commit i UTC som ÅÅÅÅMMDDTTMM (t.ex. `0.1.0+202611261256`). Den tiden växer med
//! varje ny commit oavsett gren, så två versioner går alltid att jämföra, och
//! Windows- och Linux-bygget av samma commit får samma version. Utanför git (eller
//! för prov) sätts bygget med miljövariabeln `SENTINANT_LITE_BYGGE`; annars blir det 0.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let mapp = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/ikon.ico");
    println!("cargo:rerun-if-env-changed=SENTINANT_LITE_BYGGE");

    let bygge = std::env::var("SENTINANT_LITE_BYGGE").ok().filter(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit())).or_else(|| git_bygge(&mapp)).unwrap_or_else(|| "0".into());
    let paket = std::env::var("CARGO_PKG_VERSION").unwrap();
    let version = if bygge == "0" { paket.clone() } else { format!("{paket}+{bygge}") };
    println!("cargo:rustc-env=SENTINANT_LITE_VERSION={version}");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_resurser(&mapp, &paket, &version);
    }
}

/// Tiden för senaste commit (UTC, ÅÅÅÅMMDDTTMM). Bygget körs om när HEAD flyttas.
fn git_bygge(mapp: &Path) -> Option<String> {
    let git = |arg: &[&str]| -> Option<String> {
        let ut = Command::new("git").args(arg).current_dir(mapp).env("TZ", "UTC").output().ok()?;
        ut.status.success().then(|| String::from_utf8_lossy(&ut.stdout).trim().to_string())
    };
    if let Some(gitmapp) = git(&["rev-parse", "--absolute-git-dir"]) {
        // logs/HEAD får en rad vid varje commit, byte av gren och sammanslagning.
        println!("cargo:rerun-if-changed={gitmapp}/logs/HEAD");
        println!("cargo:rerun-if-changed={gitmapp}/HEAD");
    }
    let tid = git(&["log", "-1", "--format=%cd", "--date=format-local:%Y%m%d%H%M"])?;
    (tid.len() == 12 && tid.bytes().all(|c| c.is_ascii_digit())).then_some(tid)
}

/// Windows: ikonen (det Utforskaren och aktivitetsfältet visar) och versionsuppgifterna
/// läggs i programfilen med windres (följer med mingw-w64). Saknas windres byggs
/// programmet ändå, utan ikon.
fn windows_resurser(mapp: &Path, paket: &str, version: &str) {
    let ut = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let siffror: Vec<&str> = paket.split('.').collect();
    let (a, b, c) = (siffror.first().unwrap_or(&"0"), siffror.get(1).unwrap_or(&"0"), siffror.get(2).unwrap_or(&"0"));
    let ikon = mapp.join("assets").join("ikon.ico").display().to_string().replace('\\', "/");
    // 041D = svenska, 04B0 = Unicode. Bara ASCII i texterna, så teckentabellen spelar ingen roll.
    let rc = format!(
        r#"1 ICON "{ikon}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "041D04B0"
    BEGIN
      VALUE "CompanyName", "Mapro Systems AB"
      VALUE "FileDescription", "SentinAnt Lite"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "sentinant-lite"
      VALUE "LegalCopyright", "Mapro Systems AB"
      VALUE "OriginalFilename", "sentinant-lite.exe"
      VALUE "ProductName", "SentinAnt Lite"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x41D, 1200
  END
END
"#
    );
    let rc_fil = ut.join("sentinant_lite.rc");
    let o_fil = ut.join("sentinant_lite_resurser.o");
    std::fs::write(&rc_fil, rc).expect("kunde inte skriva resursfilen");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
    match Command::new(&windres).arg("--input").arg(&rc_fil).arg("--output-format=coff").arg("--output").arg(&o_fil).status() {
        Ok(s) if s.success() => println!("cargo:rustc-link-arg-bins={}", o_fil.display()),
        Ok(s) => println!("cargo:warning=windres misslyckades ({s}): programmet byggs utan ikon"),
        Err(e) => println!("cargo:warning={windres} gick inte att köra ({e}): programmet byggs utan ikon"),
    }
}
