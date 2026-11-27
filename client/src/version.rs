//! Programmets version och hur två versioner jämförs.
//!
//! En version skrivs `1.2.3+202611261256`: paketets version (ur Cargo.toml) och
//! bygget, tiden för senaste commit i UTC (ÅÅÅÅMMDDTTMM, sätts av `build.rs`).
//! Jämförelsen går siffra för siffra från vänster, bygget sist. Byggtiden växer med
//! varje ny commit oavsett gren, så "nyare" betyder alltid "gjord senare". De äldre
//! paketens `0.1.0+412` (antal commits) tolkas också, och är äldre än alla tidsbyggen.

use std::fmt;

/// Den här programfilens version.
pub const DENNA: &str = env!("SENTINANT_LITE_VERSION");

/// Fälten jämförs i den ordning de står.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    huvud: u32,
    under: u32,
    rattning: u32,
    bygge: u64,
}

impl Version {
    /// Tolkar `X.Y.Z` eller `X.Y.Z+bygge`. Bara siffror; allt annat är ingen version.
    pub fn tolka(text: &str) -> Option<Version> {
        fn tal<T: std::str::FromStr>(s: &str) -> Option<T> {
            (!s.is_empty() && s.len() <= 18 && s.bytes().all(|c| c.is_ascii_digit())).then(|| s.parse().ok()).flatten()
        }
        let (paket, bygge) = match text.split_once('+') {
            Some((p, b)) => (p, tal::<u64>(b)?),
            None => (text, 0),
        };
        let mut delar = paket.split('.');
        let v = Version { huvud: tal(delar.next()?)?, under: tal(delar.next()?)?, rattning: tal(delar.next()?)?, bygge };
        delar.next().is_none().then_some(v)
    }

    /// Den här programfilens version.
    pub fn denna() -> Version {
        Version::tolka(DENNA).unwrap_or(Version { huvud: 0, under: 0, rattning: 0, bygge: 0 })
    }

    /// Så som versionen visas för användaren: `0.1.0 (2026-11-26 12:56)` när bygget är
    /// en tid, annars som den skrivs.
    pub fn visning(&self) -> String {
        let b = self.bygge.to_string();
        if b.len() == 12 && (2020..2200).contains(&(self.bygge / 100_000_000)) {
            format!("{}.{}.{} ({}-{}-{} {}:{})", self.huvud, self.under, self.rattning, &b[0..4], &b[4..6], &b[6..8], &b[8..10], &b[10..12])
        } else {
            self.to_string()
        }
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.huvud, self.under, self.rattning)?;
        if self.bygge != 0 {
            write!(f, "+{}", self.bygge)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tester {
    use super::*;

    fn v(s: &str) -> Version {
        Version::tolka(s).unwrap_or_else(|| panic!("{s} borde vara en version"))
    }

    #[test]
    fn tolkar_och_skriver() {
        assert_eq!(v("0.1.0+202611261256").to_string(), "0.1.0+202611261256");
        assert_eq!(v("1.2.3").to_string(), "1.2.3");
        assert_eq!(v("1.2.3+0"), v("1.2.3"));
        assert_eq!(v("0.1.0+202611261256").visning(), "0.1.0 (2026-11-26 12:56)");
        assert_eq!(v("0.1.0+412").visning(), "0.1.0+412");
        assert_eq!(v("2.0.0").visning(), "2.0.0");
    }

    #[test]
    fn jamfor() {
        // Bygget avgör inom samma paketversion.
        assert!(v("0.1.0+202611261256") < v("0.1.0+202611261257"));
        assert!(v("0.1.0+202611261256") < v("0.1.0+202612010000"));
        // Paketversionen går före bygget, och jämförs som tal (10 > 9).
        assert!(v("0.2.0+202601010000") > v("0.1.0+202612312359"));
        assert!(v("0.10.0") > v("0.9.9"));
        assert!(v("1.0.0") > v("0.99.99+209912312359"));
        // De gamla paketens antal commits är äldre än alla tidsbyggen, och utan bygge äldst.
        assert!(v("0.1.0+412") < v("0.1.0+202611261256"));
        assert!(v("0.1.0") < v("0.1.0+1"));
        assert_eq!(v("0.1.0+202611261256"), v("0.1.0+202611261256"));
    }

    #[test]
    fn skrap_ar_ingen_version() {
        for s in [
            "", "1", "1.2", "1.2.3.4", "1.2.x", "v1.2.3", "1.2.3+", "1.2.3+abc", "1.2.3-rc1", " 1.2.3", "1.2.3 ", "1..3", "+5", "1.2.3+4+5", "-1.2.3", "1.2.3+-4",
            "99999999999999999999.0.0", "1.2.3+99999999999999999999", "4294967296.0.0",
        ] {
            assert_eq!(Version::tolka(s), None, "{s:?}");
        }
    }

    #[test]
    fn programmets_egen_version_gar_att_tolka() {
        assert!(Version::tolka(DENNA).is_some(), "{DENNA}");
        assert_eq!(Version::denna().to_string(), DENNA);
    }
}
