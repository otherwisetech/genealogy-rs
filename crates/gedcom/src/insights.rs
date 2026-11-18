//! Derived facts for the person sheet and palette.
use crate::node::Node;
use crate::search::fold;
use crate::tree::{Sex, Tree, year_of};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    BirthDate,
    BirthPlace,
    DeathPlace,
    Parents,
    Partner,
    Sources,
}

impl Missing {
    /// Localized fragment for «Faltan …»: "fecha de nacimiento", "lugar de defunción", "padres"…
    pub fn label(self) -> String {
        match self {
            Missing::BirthDate => i18n::t!("core.missing.birth_date"),
            Missing::BirthPlace => i18n::t!("core.missing.birth_place"),
            Missing::DeathPlace => i18n::t!("core.missing.death_place"),
            Missing::Parents => i18n::t!("core.missing.parents"),
            Missing::Partner => i18n::t!("core.missing.partner"),
            Missing::Sources => i18n::t!("core.missing.sources"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Completeness {
    pub percent: u8,
    pub missing: Vec<Missing>,
}

/// «a, b y c» / «a, b and c».
pub fn join_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => i18n::t!("core.list.last", a = a, b = b),
        [a, rest @ ..] => i18n::t!("core.list.pair", a = a, b = join_list(rest)),
    }
}

impl Completeness {
    /// «Faltan lugar de defunción, pareja y fuentes.» or "" when complete.
    pub fn sentence(&self) -> String {
        if self.missing.is_empty() {
            return String::new();
        }
        let labels: Vec<String> = self.missing.iter().map(|m| m.label()).collect();
        i18n::tn!("core.missing.sentence", labels.len(), list = join_list(&labels))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineItem {
    pub year: Option<i32>,
    pub date: Option<String>,
    pub tag: String,
    /// localized: Nacimiento, Bautizo, Matrimonio, Defunción, Entierro, Residencia…
    pub label: String,
    pub place: Option<String>,
    /// for MARR/DIV: the family and the other partner
    pub family: Option<String>,
    pub partner: Option<String>,
}

/// People older than this without a death record are presumed dead for completeness.
const PRESUMED_DEAD_AFTER: i32 = 110;

fn current_year() -> i32 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    1970 + (secs / 31_556_952) as i32
}

fn has_source(n: &Node) -> bool {
    n.children.iter().any(|c| c.tag == "SOUR" || has_source(c))
}

#[derive(Clone, Copy)]
enum Step {
    Up,
    Down,
    Spouse,
}

/// Step patterns we can name, as (U=parent, D=child, S=partner) strings → catalog stem.
const PATTERNS: &[(&str, &str)] = &[
    ("U", "parent"),
    ("UU", "grandparent"),
    ("UUU", "great_grandparent"),
    ("UUUU", "great2_grandparent"),
    ("D", "child"),
    ("DD", "grandchild"),
    ("DDD", "great_grandchild"),
    ("DDDD", "great2_grandchild"),
    ("UD", "sibling"),
    ("UUD", "uncle"),
    ("UDD", "nephew"),
    ("UUDD", "cousin"),
    ("S", "spouse"),
    ("SU", "parent_in_law"),
    ("DS", "child_in_law"),
    ("SUD", "sibling_in_law"),
    ("UDS", "sibling_in_law"),
];

impl Tree {
    /// «Familia Ordóñez»: most common first surname word; «Árbol familiar» when empty.
    pub fn title(&self) -> String {
        let mut counts: HashMap<String, (usize, usize, &str)> = HashMap::new();
        for (i, id) in self.order.iter().enumerate() {
            let Some(w) = self.people[id].surname.split_whitespace().next() else { continue };
            let e = counts.entry(fold(w)).or_insert((0, i, w));
            e.0 += 1;
        }
        match counts.values().max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1))) {
            Some((_, _, w)) => i18n::t!("core.title.family", surname = w),
            None => i18n::t!("core.title.default"),
        }
    }

    /// Weights: birth date 20, birth place 15, death place 15 (only when dead or presumed dead),
    /// parents 20, partner 15, sources 15; rescaled to 100 over the applicable items.
    pub fn completeness(&self, id: &str) -> Completeness {
        let Some(p) = self.person(id) else { return Completeness::default() };
        let birth = p.birth.as_ref();
        let dead = p.death.is_some() || p.birth_year().is_some_and(|y| current_year() - y > PRESUMED_DEAD_AFTER);
        let has_partner = p.fams.iter().filter_map(|f| self.family(f)).any(|f| f.other_partner(id).is_some());
        let checks = [
            (Missing::BirthDate, 20, true, birth.is_some_and(|e| e.date.is_some())),
            (Missing::BirthPlace, 15, true, birth.is_some_and(|e| e.place.is_some())),
            (Missing::DeathPlace, 15, dead, p.death.as_ref().is_some_and(|e| e.place.is_some())),
            (Missing::Parents, 20, true, !self.parents(id).is_empty()),
            (Missing::Partner, 15, true, has_partner),
            (Missing::Sources, 15, true, self.record(id).is_some_and(has_source)),
        ];
        let (mut total, mut got, mut missing) = (0u32, 0u32, Vec::new());
        for (m, w, applies, ok) in checks {
            if !applies {
                continue;
            }
            total += w;
            if ok {
                got += w;
            } else {
                missing.push(m);
            }
        }
        Completeness { percent: (got * 100 / total) as u8, missing }
    }

    /// Person events + family events (marriage…) sorted by year; undated last, but the birth
    /// always first and death/burial last.
    pub fn timeline(&self, id: &str) -> Vec<TimelineItem> {
        let Some(p) = self.person(id) else { return vec![] };
        let birth_tag = p.birth.as_ref().map(|e| e.tag.as_str()).unwrap_or("BIRT");
        let item = |e: &crate::tree::Event, family: Option<&str>, partner: Option<&String>| {
            let label = match &e.value {
                Some(v) if e.kind.is_none() => i18n::t!("core.event.with_value", label = e.label(), value = v),
                _ => e.label(),
            };
            TimelineItem {
                year: e.date.as_deref().and_then(year_of),
                date: e.date.clone(),
                tag: e.tag.clone(),
                label,
                place: e.place.clone(),
                family: family.map(str::to_string),
                partner: partner.cloned(),
            }
        };
        let mut out: Vec<TimelineItem> = p.events.iter().map(|e| item(e, None, None)).collect();
        for f in p.fams.iter().filter_map(|f| self.family(f)) {
            out.extend(f.events.iter().map(|e| item(e, Some(&f.id), f.other_partner(id))));
        }
        let mut seen_birth = false;
        let mut keyed: Vec<(u8, bool, i32, TimelineItem)> = out
            .into_iter()
            .map(|it| {
                let rank = if !seen_birth && it.tag == birth_tag && it.family.is_none() {
                    seen_birth = true;
                    0
                } else if matches!(it.tag.as_str(), "DEAT" | "BURI" | "CREM") && it.family.is_none() {
                    2
                } else {
                    1
                };
                (rank, it.year.is_none(), it.year.unwrap_or(0), it)
            })
            .collect();
        keyed.sort_by_key(|k| (k.0, k.1, k.2));
        keyed.into_iter().map(|k| k.3).collect()
    }

    /// (place, number of events) over every person and family event.
    pub(crate) fn place_counts(&self) -> Vec<(String, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let events = self.people.values().flat_map(|p| p.events.iter()).chain(self.families.values().flat_map(|f| f.events.iter()));
        for pl in events.filter_map(|e| e.place.as_deref()).map(str::trim).filter(|s| !s.is_empty()) {
            *counts.entry(pl).or_default() += 1;
        }
        let mut v: Vec<(String, usize)> = counts.into_iter().map(|(k, n)| (k.to_string(), n)).collect();
        v.sort_by_cached_key(|(k, n)| (std::cmp::Reverse(*n), fold(k)));
        v
    }

    /// Places used in events, prefix/word match (accent-insensitive), most used first.
    pub fn place_suggestions(&self, query: &str, limit: usize) -> Vec<(String, usize)> {
        let q = fold(query);
        let tokens = crate::search::tokens(&q);
        self.place_counts().into_iter().filter(|(p, _)| crate::search::words_match(&fold(p), &tokens)).take(limit).collect()
    }

    fn neighbors(&self, id: &str, step: Step) -> Vec<&str> {
        let Some(p) = self.person(id) else { return vec![] };
        match step {
            Step::Up => self.parents(id).into_iter().map(|x| x.id.as_str()).collect(),
            Step::Down => p.fams.iter().filter_map(|f| self.family(f)).flat_map(|f| f.children.iter().map(String::as_str)).collect(),
            Step::Spouse => p.fams.iter().filter_map(|f| self.family(f)).filter_map(|f| f.other_partner(id)).map(String::as_str).collect(),
        }
    }

    /// Localized relationship of `other` seen from `focus`, gendered by `other`'s sex: "padre",
    /// "abuela", "tío", "esposa", "pareja", "persona en foco"… None when unrelated within 4 steps.
    pub fn relationship(&self, focus: &str, other: &str) -> Option<String> {
        self.person(focus)?;
        let o = self.person(other)?;
        if focus == other {
            return Some(i18n::t!("core.rel.self"));
        }
        // BFS over step patterns; only prefixes of nameable patterns are expanded, so the first
        // hit is the closest nameable relationship.
        let mut frontier: Vec<(&str, String)> = vec![(focus, String::new())];
        let mut seen: HashSet<(&str, String)> = HashSet::new();
        for _ in 0..4 {
            let mut next = Vec::new();
            for (id, pat) in &frontier {
                for (step, c) in [(Step::Up, 'U'), (Step::Down, 'D'), (Step::Spouse, 'S')] {
                    let np = format!("{pat}{c}");
                    if !PATTERNS.iter().any(|(p, _)| p.starts_with(np.as_str())) {
                        continue;
                    }
                    for n in self.neighbors(id, step) {
                        if n == focus || !seen.insert((n, np.clone())) {
                            continue;
                        }
                        if n == other {
                            if let Some((_, stem)) = PATTERNS.iter().find(|(p, _)| *p == np) {
                                return Some(self.rel_label(stem, focus, other, o.sex));
                            }
                        }
                        next.push((n, np.clone()));
                    }
                }
            }
            frontier = next;
        }
        None
    }

    fn rel_label(&self, stem: &str, focus: &str, other: &str, sex: Sex) -> String {
        let married = self.families.values().any(|f| {
            let ps: Vec<&String> = f.partners().collect();
            ps.iter().any(|p| *p == focus) && ps.iter().any(|p| *p == other) && f.events.iter().any(|e| e.tag == "MARR")
        });
        let stem = if stem == "spouse" && !married { "partner" } else { stem };
        let g = match sex {
            Sex::Male => "m",
            Sex::Female => "f",
            Sex::Unknown => "u",
        };
        i18n::tr(&format!("core.rel.{stem}.{g}"), &[])
    }
}

impl crate::tree::Person {
    /// Whole years between birth and death when both have a year.
    pub fn age_at_death(&self) -> Option<i32> {
        Some(self.death_year()? - self.birth_year()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample.ged");
    const FAMILY: &str = include_str!("../tests/fixtures/family.ged");

    #[test]
    fn completeness_weights() {
        let t = Tree::parse(SAMPLE).unwrap();
        // I1: birth date+place (35), partner (15); dead without place, no parents, no sources
        let c = t.completeness("I1");
        assert_eq!(c.missing, vec![Missing::DeathPlace, Missing::Parents, Missing::Sources]);
        assert_eq!(c.percent, 50);
        // I3: alive (b. 1950) → death place not applicable; has birth, parents; F2 has no partner
        let c = t.completeness("I3");
        assert_eq!(c.missing, vec![Missing::Partner, Missing::Sources]);
        assert_eq!(c.percent, 64);
        let f = Tree::parse(FAMILY).unwrap();
        assert_eq!(f.completeness("I10").percent, 100);
        assert!(f.completeness("I10").sentence().is_empty());
        assert_eq!(t.completeness("NOPE"), Completeness::default());
    }

    #[test]
    fn timeline_order() {
        let t = Tree::parse(FAMILY).unwrap();
        let tl = t.timeline("I10");
        let tags: Vec<&str> = tl.iter().map(|i| i.tag.as_str()).collect();
        assert_eq!(tags, vec!["BIRT", "MARR", "OCCU", "RESI", "DEAT", "BURI"]);
        let marr = &tl[1];
        assert_eq!((marr.family.as_deref(), marr.partner.as_deref(), marr.year), (Some("F10"), Some("I11"), Some(1812)));
    }

    #[test]
    fn places_and_relationships() {
        let t = Tree::parse(FAMILY).unwrap();
        let s = t.place_suggestions("bogo", 5);
        assert_eq!(s[0], ("Santafé de Bogotá".to_string(), 3));
        assert_eq!(t.place_suggestions("SANTAFE", 5).len(), 1);
        assert!(t.place_suggestions("xyz", 5).is_empty());
        assert_eq!(t.place_suggestions("", 1).len(), 1);

        // unrelated / far
        assert_eq!(t.relationship("I10", "I99"), None);
    }

    /// All text assertions in one test: the locale is global.
    #[test]
    fn localized_text() {
        let t = Tree::parse(SAMPLE).unwrap();
        let f = Tree::parse(FAMILY).unwrap();
        let _l = crate::locale_lock();
        i18n::set_locale("es");
        assert_eq!(t.title(), "Familia García");
        assert_eq!(Tree::new_empty().title(), "Árbol familiar");
        let c = Completeness { percent: 40, missing: vec![Missing::DeathPlace, Missing::Partner, Missing::Sources] };
        assert_eq!(c.sentence(), "Faltan lugar de defunción, pareja y fuentes.");
        assert_eq!(Completeness { percent: 80, missing: vec![Missing::Sources] }.sentence(), "Falta fuentes.");
        let rel = |a: &str, b: &str| f.relationship(a, b);
        assert_eq!(rel("I20", "I10").as_deref(), Some("abuelo"));
        assert_eq!(rel("I20", "I12").as_deref(), Some("madre"));
        assert_eq!(rel("I20", "I13").as_deref(), Some("padre"));
        assert_eq!(rel("I20", "I11").as_deref(), Some("abuela"));
        assert_eq!(rel("I20", "I21").as_deref(), Some("hermano/a"));
        assert_eq!(rel("I20", "I14").as_deref(), Some("tío"));
        assert_eq!(rel("I20", "I22").as_deref(), Some("prima"));
        assert_eq!(rel("I20", "I20").as_deref(), Some("persona en foco"));
        assert_eq!(rel("I10", "I11").as_deref(), Some("esposa"));
        assert_eq!(rel("I12", "I13").as_deref(), Some("pareja"));
        assert_eq!(rel("I13", "I10").as_deref(), Some("suegro"));
        assert_eq!(rel("I10", "I13").as_deref(), Some("yerno"));
        assert_eq!(rel("I13", "I14").as_deref(), Some("cuñado"));
        assert_eq!(rel("I14", "I20").as_deref(), Some("sobrino"));
        assert_eq!(rel("I10", "I20").as_deref(), Some("nieto"));
        assert_eq!(rel("I10", "I12").as_deref(), Some("hija"));
        assert_eq!(rel("I30", "I20").as_deref(), Some("bisnieto"));
        assert_eq!(rel("I20", "I30").as_deref(), Some("bisabuelo"));
        let tl = f.timeline("I10");
        assert_eq!(tl[0].label, "Nacimiento");
        assert_eq!(tl[2].label, "Ocupación: Carpintero");

        i18n::set_locale("en");
        assert_eq!(t.title(), "The García family");
        assert_eq!(c.sentence(), "Missing death place, partner and sources.");
        assert_eq!(rel("I20", "I11").as_deref(), Some("grandmother"));
        assert_eq!(rel("I20", "I21").as_deref(), Some("sibling"));
        assert_eq!(rel("I10", "I13").as_deref(), Some("son-in-law"));
        assert_eq!(f.timeline("I10")[1].label, "Marriage");

        i18n::set_locale("pt");
        assert_eq!(t.title(), "Família García");
        assert_eq!(c.sentence(), "Faltam o local de falecimento, o cônjuge e as fontes.");
        assert_eq!(Completeness { percent: 80, missing: vec![Missing::Sources] }.sentence(), "Falta as fontes.");
        assert_eq!(rel("I20", "I11").as_deref(), Some("avó"));
        assert_eq!(rel("I20", "I30").as_deref(), Some("bisavô"));
        assert_eq!(rel("I10", "I13").as_deref(), Some("genro"));

        i18n::set_locale("fr");
        assert_eq!(t.title(), "Famille García");
        assert_eq!(c.sentence(), "Il manque le lieu de décès, le conjoint et les sources.");
        assert_eq!(rel("I20", "I11").as_deref(), Some("grand-mère"));
        assert_eq!(rel("I20", "I30").as_deref(), Some("arrière-grand-père"));
        assert_eq!(rel("I20", "I22").as_deref(), Some("cousine"));
        assert_eq!(rel("I13", "I14").as_deref(), Some("beau-frère"));
        assert_eq!(f.timeline("I10")[1].label, "Mariage");
        i18n::set_locale("es");
    }
}
