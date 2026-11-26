//! Låtsasrobotens firmwareuppdatering (`--firmware KÄLLA`): den riktiga uppdateraren ur
//! `robotuppdatering` mot ett låtsaskort, så att hela varvet i Robotstyrning går att prova
//! utan hårdvara.
//!
//! "Kortet" är filen `fw_version` i mappen (`--fw-mapp`, standard /tmp/mock-robotd-firmware),
//! och i stället för openocd körs skriptet `flasha.sh` där. En ELF-fil från en provlista
//! har versionen som text efter de 20 första byten. Filer i mappen styr vad som går fel:
//! `fel_dump`, `tom_dump`, `fel_skriv`, `fel_version`, `dott_kort`, `fel_aterstall`,
//! `tyst_installningar` (inställningarna går inte att spara), `raderar_installningar`
//! (skrivningen ändrar dem), `kortet_har_nodstopp` (kortet rapporterar nödstoppet fast
//! varianten säger utan) och `langsam` (sekunder per steg).
//!
//! Listan ska vara signerad. En provlista (provnyckeln) godkänns bara om mock-robotd är
//! byggd med `--features provnyckel`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use robotuppdatering::firmware::{FwVersion, Variant};
use robotuppdatering::robot::{Installning, Installningar, Kortet, Lagesbild, Uppdaterare};

const FLASHA: &str = r#"#!/bin/sh
D='MAPPEN'
[ -e "$D/langsam" ] && sleep "$(cat "$D/langsam")"
case "$1" in
  dumpa)
    [ -e "$D/fel_dump" ] && { echo "Error: kortet svarar inte"; exit 1; }
    if [ -e "$D/tom_dump" ]; then head -c 1048576 /dev/zero > "$2"
    else { printf '\000\000\002\040\245\301\000\010'; head -c 1048568 /dev/zero | tr '\000' '\377'; } > "$2"; fi ;;
  skriv)
    cp "$D/fw_version" "$D/fw_forra"
    [ -e "$D/fel_skriv" ] && { echo "Error: verify failed"; exit 1; }
    if [ -e "$D/fel_version" ]; then echo 30.1 > "$D/fw_version"
    elif [ -e "$D/dott_kort" ]; then rm "$D/fw_version"
    else tail -c +21 "$2" > "$D/fw_version"; fi
    if [ -e "$D/raderar_installningar" ]; then touch "$D/andra_installningar"; fi ;;
  aterstall)
    [ -e "$D/fel_aterstall" ] && { echo "Error: kortet svarar inte"; exit 1; }
    cp "$D/fw_forra" "$D/fw_version" ;;
esac
exit 0
"#;

/// Låtsaskortets inställningar: en fil i mappen.
struct Lats(PathBuf);

impl Kortet for Lats {
    fn spara_installningar(&mut self, mapp: &Path) -> Result<(), String> {
        if self.0.join("tyst_installningar").exists() {
            return Err("inget svar från styrkortet på läsning av inställningarna".into());
        }
        std::fs::write(mapp.join("installningar.bin"), b"aktuatorer 28 36 76").map_err(|e| e.to_string())
    }

    fn kontrollera_installningar(&mut self, mapp: &Path) -> Result<Installningar, String> {
        std::fs::read(mapp.join("installningar.bin")).map_err(|e| e.to_string())?;
        if std::fs::remove_file(self.0.join("andra_installningar")).is_ok() {
            return Ok(Installningar::Aterstallda);
        }
        Ok(Installningar::Lika)
    }
}

/// Det låtsasroboten gör just nu (sätts av anslutningen som kör).
#[derive(Default)]
pub struct Korlage {
    pub aktiverad: AtomicBool,
    /// Gasen i tusendelar (för att slippa ett lås).
    pub gas_promille: AtomicU32,
    pub anslutna: AtomicU32,
}

pub struct Firmware {
    pub uppdaterare: Arc<Uppdaterare>,
    pub korlage: Arc<Korlage>,
}

/// Skapar mappen, låtsasflasharen och uppdateraren, och startar tråden som spelar robotd:s
/// lägesbilder (kortets version ur filen, spärrarna ur körläget).
pub fn starta(kalla: String, variant: Option<String>, mapp: PathBuf, fw: &str) -> Firmware {
    std::fs::create_dir_all(&mapp).expect("kan inte skapa firmware-mappen");
    std::fs::write(mapp.join("fw_version"), fw).expect("kan inte skriva fw_version");
    let skript = mapp.join("flasha.sh");
    std::fs::write(&skript, FLASHA.replace("MAPPEN", &mapp.display().to_string())).expect("kan inte skriva flasha.sh");
    std::fs::set_permissions(&skript, std::os::unix::fs::PermissionsExt::from_mode(0o755)).expect("flasha.sh");
    let nodstopp = variant.as_deref().and_then(Variant::tolka).is_some_and(|v| v.nodstopp);
    let inst = Installning { kalla, variant, mapp: mapp.join("robotuppdatering"), flashkommando: Some(skript), rot: mapp.join("rot") };
    robotuppdatering::robot::registrera_komponent(&inst.mapp, "robotd", "0.1.0+202611270000");
    let uppdaterare = Arc::new(Uppdaterare::starta(inst, Box::new(Lats(mapp.clone()))));
    let korlage = Arc::new(Korlage::default());
    let (u, k) = (uppdaterare.clone(), korlage.clone());
    std::thread::spawn(move || {
        let mut stilla_sedan: Option<Instant> = None;
        loop {
            let ror_sig = k.gas_promille.load(Ordering::Relaxed) > 10;
            stilla_sedan = if ror_sig { None } else { stilla_sedan.or(Some(Instant::now())) };
            let mut sparrar = Vec::new();
            if k.aktiverad.load(Ordering::Relaxed) {
                sparrar.push("AKTIVERA är på. Stäng av den.".to_string());
            }
            if ror_sig {
                sparrar.push("Roboten rör sig. Den ska stå helt still.".into());
            } else if stilla_sedan.is_none_or(|s| s.elapsed() < Duration::from_secs(3)) {
                sparrar.push("Roboten har inte stått still tillräckligt länge än.".into());
            }
            let andra = k.anslutna.load(Ordering::Relaxed).saturating_sub(1);
            if andra > 0 {
                sparrar.push(format!("{andra} till är anslutna till roboten. Bara den som kör får vara ansluten."));
            }
            let kortversion = std::fs::read_to_string(mapp.join("fw_version")).ok().and_then(|t| FwVersion::tolka(t.trim()));
            let hors = nodstopp || mapp.join("kortet_har_nodstopp").exists();
            let nu = Some(Instant::now());
            u.lagesbild(Lagesbild { kortversion, kort_svarade: kortversion.and(nu), sparrar, nodstopp_hors: Some(hors), nodstopp_hordes: nu.filter(|_| hors), lagesknapp_hors: None });
            std::thread::sleep(Duration::from_millis(200));
        }
    });
    Firmware { uppdaterare, korlage }
}
