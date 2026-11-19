//! Places for the map: every person/family event with a PLAC, grouped by a normalized place
//! string, with coordinates from `PLAC.MAP.LATI/LONG` when the file has them.
use crate::node::Node;
use crate::search::fold;
use crate::tree::{EVENT_TAGS, Event, Tree, year_of};
use std::collections::{HashMap, HashSet};

/// One event that happened at a place. `people` is the person, or both partners of a family
/// event (marriage…).
#[derive(Debug, Clone)]
pub struct PlaceEvent {
    pub people: Vec<String>,
    pub family: Option<String>,
    pub event: Event,
    pub year: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PlaceGroup {
    /// `place_key` of the place text: the grouping and geocoding-cache key
    pub key: String,
    /// most used spelling, for display and geocoding
    pub name: String,
    /// (lat, lon) from the file, if any event of the group carries `MAP`
    pub coords: Option<(f64, f64)>,
    pub events: Vec<PlaceEvent>,
}

/// Grouping key: accent/case-folded, whitespace collapsed, comma parts trimmed, empty parts
/// dropped («Villa Rica ,  Reino» ≡ «villa rica, reino»).
pub fn place_key(place: &str) -> String {
    fold(place).split(',').map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", ")
}

/// GEDCOM coordinate (`N4.6097`, `W74.08`, `S33.9`, `E151.2`, or a plain signed number) in
/// degrees; S/W are negative. Out-of-range values are rejected.
pub fn parse_coord(s: &str, latitude: bool) -> Option<f64> {
    let s = s.trim();
    let (sign, rest) = match s.chars().next()?.to_ascii_uppercase() {
        'N' if latitude => (1., &s[1..]),
        'S' if latitude => (-1., &s[1..]),
        'E' if !latitude => (1., &s[1..]),
        'W' if !latitude => (-1., &s[1..]),
        'N' | 'S' | 'E' | 'W' => return None,
        _ => (1., s),
    };
    let v: f64 = rest.trim().replace(',', ".").parse().ok()?;
    let v = sign * v;
    let max = if latitude { 90. } else { 180. };
    (v.is_finite() && v.abs() <= max).then_some(v)
}

/// (lat, lon) of a `PLAC` node's `MAP` child.
pub fn plac_coords(plac: &Node) -> Option<(f64, f64)> {
    let map = plac.child("MAP")?;
    Some((parse_coord(map.child_value("LATI")?, true)?, parse_coord(map.child_value("LONG")?, false)?))
}

impl Tree {
    /// Every event with a place, grouped by `place_key`; most events first.
    pub fn place_groups(&self) -> Vec<PlaceGroup> {
        struct Acc {
            spellings: HashMap<String, usize>,
            coords: Option<(f64, f64)>,
            events: Vec<PlaceEvent>,
        }
        let mut groups: HashMap<String, Acc> = HashMap::new();
        for r in self.records() {
            let (Some(id), "INDI" | "FAM") = (r.xref.as_deref(), r.tag.as_str()) else { continue };
            let people: Vec<String> = if r.tag == "INDI" {
                vec![id.to_string()]
            } else {
                ["HUSB", "WIFE"].iter().filter_map(|t| r.child(t)?.pointer()).filter(|p| self.person(p).is_some()).map(str::to_string).collect()
            };
            if people.is_empty() {
                continue;
            }
            for ev in r.children.iter().filter(|c| EVENT_TAGS.contains(&c.tag.as_str())) {
                let Some(plac) = ev.child("PLAC") else { continue };
                let name = plac.value.trim();
                let key = place_key(name);
                if key.is_empty() {
                    continue;
                }
                let acc = groups.entry(key).or_insert_with(|| Acc { spellings: HashMap::new(), coords: None, events: vec![] });
                *acc.spellings.entry(name.to_string()).or_default() += 1;
                if acc.coords.is_none() {
                    acc.coords = plac_coords(plac);
                }
                let event = Event {
                    tag: ev.tag.clone(),
                    kind: ev.child_value("TYPE").map(str::to_string),
                    date: ev.child_value("DATE").map(str::to_string),
                    place: Some(name.to_string()),
                    value: None,
                };
                let year = event.date.as_deref().and_then(year_of);
                acc.events.push(PlaceEvent { people: people.clone(), family: (r.tag == "FAM").then(|| id.to_string()), event, year });
            }
        }
        let mut out: Vec<PlaceGroup> = groups
            .into_iter()
            .map(|(key, mut a)| {
                let name = a.spellings.iter().max_by(|x, y| x.1.cmp(y.1).then_with(|| y.0.cmp(x.0))).map(|(s, _)| s.clone()).unwrap_or_default();
                a.events.sort_by_key(|e| (e.year.is_none(), e.year));
                PlaceGroup { key, name, coords: a.coords, events: a.events }
            })
            .collect();
        out.sort_by(|a, b| b.events.len().cmp(&a.events.len()).then_with(|| a.key.cmp(&b.key)));
        out
    }

    /// `id` and all of its ancestors over every FAMC link (cycle-safe).
    pub fn ancestors_and_self(&self, id: &str) -> HashSet<String> {
        let mut seen = HashSet::new();
        let mut stack = vec![id.to_string()];
        while let Some(x) = stack.pop() {
            if self.person(&x).is_none() || !seen.insert(x.clone()) {
                continue;
            }
            stack.extend(self.parents(&x).into_iter().map(|p| p.id.clone()));
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinates() {
        assert_eq!(parse_coord("N4.6097", true), Some(4.6097));
        assert_eq!(parse_coord("S33.5", true), Some(-33.5));
        assert_eq!(parse_coord("W74.08", false), Some(-74.08));
        assert_eq!(parse_coord("e 12,5", false), Some(12.5));
        assert_eq!(parse_coord("-3.25", false), Some(-3.25));
        assert_eq!(parse_coord("E10", true), None, "E is not a latitude");
        assert_eq!(parse_coord("N95", true), None);
        assert_eq!(parse_coord("W181", false), None);
        assert_eq!(parse_coord("", true), None);
        assert_eq!(parse_coord("Nx", true), None);
    }

    #[test]
    fn keys() {
        assert_eq!(place_key("  Villa Rica ,  Reino Alto "), "villa rica, reino alto");
        assert_eq!(place_key("Villarrubia, , Ñandú"), "villarrubia, nandu");
        assert_eq!(place_key(" , "), "");
    }

    const GED: &str = "0 HEAD\n1 CHAR UTF-8\n\
0 @I1@ INDI\n1 NAME Ana /Prueba/\n1 BIRT\n2 DATE 1901\n2 PLAC Villa Uno, Comarca\n3 MAP\n4 LATI N10.5\n4 LONG W20.25\n1 FAMS @F1@\n1 FAMC @F2@\n\
0 @I2@ INDI\n1 NAME Beto /Ejemplo/\n1 BIRT\n2 DATE ABT 1899\n2 PLAC villa uno ,comarca\n1 DEAT\n2 PLAC Pueblo Dos\n1 RESI\n2 PLAC   \n1 FAMS @F1@\n\
0 @I3@ INDI\n1 NAME Cata /Prueba/\n1 FAMS @F2@\n1 FAMC @F3@\n\
0 @I4@ INDI\n1 NAME Dino /Prueba/\n1 FAMS @F3@\n1 FAMC @F2@\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I1@\n1 MARR\n2 DATE 1925\n2 PLAC Villa Uno, Comarca\n\
0 @F2@ FAM\n1 WIFE @I3@\n1 CHIL @I1@\n1 CHIL @I4@\n\
0 @F3@ FAM\n1 HUSB @I4@\n1 CHIL @I3@\n0 TRLR\n";

    #[test]
    fn groups_events_by_place() {
        let t = Tree::parse(GED).unwrap();
        let g = t.place_groups();
        assert_eq!(g.len(), 2, "blank PLAC ignored");
        assert_eq!(g[0].key, "villa uno, comarca");
        assert_eq!(g[0].name, "Villa Uno, Comarca", "most used spelling");
        assert_eq!(g[0].coords, Some((10.5, -20.25)));
        assert_eq!(g[0].events.len(), 3);
        assert_eq!(g[0].events.iter().map(|e| e.year).collect::<Vec<_>>(), vec![Some(1899), Some(1901), Some(1925)]);
        let marr = &g[0].events[2];
        assert_eq!((marr.event.tag.as_str(), marr.family.as_deref()), ("MARR", Some("F1")));
        assert_eq!(marr.people, vec!["I2".to_string(), "I1".to_string()]);
        assert_eq!((g[1].name.as_str(), g[1].coords, g[1].events[0].year), ("Pueblo Dos", None, None));
    }

    #[test]
    fn ancestors_survive_cycles() {
        let t = Tree::parse(GED).unwrap();
        let a = t.ancestors_and_self("I1");
        // I1 ← I3 ← I4 ← I3 … (I4 is both I1's brother and I3's father: a cycle)
        assert_eq!(a, ["I1", "I3", "I4"].iter().map(|s| s.to_string()).collect());
        assert_eq!(t.ancestors_and_self("I2").len(), 1);
        assert!(t.ancestors_and_self("nope").is_empty());
        let before = t.to_gedcom();
        let _ = t.place_groups();
        assert_eq!(t.to_gedcom(), before, "read-only");
    }
}
