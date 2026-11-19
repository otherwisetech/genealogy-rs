//! Historical context for a person's timeline: world events, plus events of the countries and
//! regions that appear in their places (an event about Colombia is only shown to someone with a
//! place in Colombia). Labels live in the `core.history.*` catalog keys.
use crate::search::fold;
use crate::tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    World,
    /// matched against the person's places (accent/case-insensitive)
    Place(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistEvent {
    pub year: i32,
    /// last year of a period (wars…)
    pub end: Option<i32>,
    /// catalog key suffix: `core.history.<key>`
    pub key: &'static str,
    pub scope: Scope,
}

const fn ev(year: i32, end: Option<i32>, key: &'static str, scope: Scope) -> HistEvent {
    HistEvent { year, end, key, scope }
}

use Scope::{Place, World};
pub const EVENTS: &[HistEvent] = &[
    ev(1492, None, "columbus", World),
    ev(1517, None, "reformation", World),
    ev(1538, None, "bogota_founded", Place("Colombia")),
    ev(1549, None, "pamplona_founded", Place("Norte de Santander")),
    ev(1583, None, "salazar_founded", Place("Norte de Santander")),
    ev(1717, None, "new_granada", Place("Colombia")),
    ev(1733, None, "cucuta_founded", Place("Norte de Santander")),
    ev(1776, None, "us_independence", World),
    ev(1781, None, "comuneros", Place("Colombia")),
    ev(1789, None, "french_revolution", World),
    ev(1810, None, "colombia_independence", Place("Colombia")),
    ev(1813, None, "battle_cucuta", Place("Norte de Santander")),
    ev(1819, None, "battle_boyaca", Place("Colombia")),
    ev(1821, None, "congress_cucuta", Place("Colombia")),
    ev(1830, None, "gran_colombia_end", Place("Colombia")),
    ev(1851, None, "slavery_abolished", Place("Colombia")),
    ev(1863, None, "rionegro", Place("Colombia")),
    ev(1875, None, "cucuta_earthquake", Place("Norte de Santander")),
    ev(1876, Some(1877), "war_1876", Place("Colombia")),
    ev(1886, None, "constitution_1886", Place("Colombia")),
    ev(1899, Some(1902), "thousand_days", Place("Colombia")),
    ev(1903, None, "panama", Place("Colombia")),
    ev(1914, Some(1918), "ww1", World),
    ev(1918, Some(1920), "flu_1918", World),
    ev(1928, None, "banana_massacre", Place("Colombia")),
    ev(1932, Some(1933), "peru_war", Place("Colombia")),
    ev(1939, Some(1945), "ww2", World),
    ev(1948, None, "bogotazo", Place("Colombia")),
    ev(1948, Some(1958), "la_violencia", Place("Colombia")),
    ev(1958, Some(1974), "national_front", Place("Colombia")),
    ev(1969, None, "moon", World),
    ev(1989, None, "berlin_wall", World),
    ev(1991, None, "constitution_1991", Place("Colombia")),
    ev(2010, None, "gramalote", Place("Norte de Santander")),
    ev(2016, None, "peace_agreement", Place("Colombia")),
    ev(2020, Some(2021), "covid", World),
];

impl HistEvent {
    pub fn label(&self) -> String {
        i18n::tr(&format!("core.history.{}", self.key), &[])
    }

    /// «1899–1902» or «1948».
    pub fn years(&self) -> String {
        match self.end {
            Some(e) => format!("{}–{e}", self.year),
            None => self.year.to_string(),
        }
    }
}

impl Tree {
    /// Events from [`EVENTS`] during the person's life (birth year to death year, or 90 years when
    /// the death is unknown), filtered to the places they lived in. Empty without a birth year.
    pub fn history_for(&self, id: &str, now: i32) -> Vec<HistEvent> {
        let Some(p) = self.person(id) else { return vec![] };
        let Some(born) = p.birth_year() else { return vec![] };
        let died = p.death_year().unwrap_or((born + 90).min(now));
        let mut places: Vec<&str> = p.events.iter().filter_map(|e| e.place.as_deref()).collect();
        for f in p.fams.iter().filter_map(|f| self.family(f)) {
            places.extend(f.events.iter().filter_map(|e| e.place.as_deref()));
        }
        let places = fold(&places.join(" | "));
        EVENTS
            .iter()
            .filter(|e| e.end.unwrap_or(e.year) >= born && e.year <= died)
            .filter(|e| match e.scope {
                Scope::World => true,
                Scope::Place(name) => places.contains(&fold(name)),
            })
            .copied()
            .collect()
    }
}

/// Current calendar year (UTC), for open-ended lifespans.
pub fn current_year() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // days → civil year (Howard Hinnant's algorithm, year part only)
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_by_lifespan_and_place() {
        let t = Tree::parse("0 HEAD\n0 @I1@ INDI\n1 BIRT\n2 DATE 1880\n2 PLAC Gramalote, Norte de Santander, Colombia\n1 DEAT\n2 DATE 1950\n0 @I2@ INDI\n1 BIRT\n2 DATE 1880\n2 PLAC Madrid, España\n1 DEAT\n2 DATE 1950\n0 @I3@ INDI\n0 TRLR\n").unwrap();
        let keys = |id: &str| t.history_for(id, 2026).iter().map(|e| e.key).collect::<Vec<_>>();
        let co = keys("I1");
        assert!(co.contains(&"thousand_days") && co.contains(&"ww2") && co.contains(&"bogotazo") && co.contains(&"la_violencia"));
        assert!(!co.contains(&"cucuta_earthquake") && !co.contains(&"moon"), "outside the lifespan");
        let es = keys("I2");
        assert!(es.contains(&"ww1") && !es.contains(&"thousand_days"), "Colombian events only for Colombian places");
        assert!(keys("I3").is_empty());
    }

    #[test]
    fn open_lifespan_and_year() {
        let t = Tree::parse("0 HEAD\n0 @I1@ INDI\n1 BIRT\n2 DATE 2000\n0 TRLR\n").unwrap();
        assert_eq!(t.history_for("I1", 2026).iter().map(|e| e.key).collect::<Vec<_>>(), vec!["covid"]);
        let y = current_year();
        assert!((2024..2200).contains(&y));
        assert_eq!(EVENTS.iter().find(|e| e.key == "ww2").unwrap().years(), "1939–1945");
    }

    #[test]
    fn every_event_has_a_label() {
        let _l = crate::locale_lock();
        for lang in ["es", "en", "pt", "fr"] {
            i18n::set_locale(lang);
            for e in EVENTS {
                assert!(!e.label().starts_with("core.history"), "{lang}: {}", e.key);
            }
        }
        i18n::set_locale("es");
    }
}
