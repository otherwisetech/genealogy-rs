//! «Conexiones familiares» of one map place: how the people with events there are related.
//!
//! People at the place are joined into family branches when one is an ancestor of the other
//! (up to `LINE_DEPTH` generations) or they share an ancestor within `COMMON_DEPTH` generations
//! (siblings, cousins, uncle and nephew…), and partners are joined too. Each branch knows its
//! generation span and the longest direct line among its members. Couples and parent→child
//! pairs at the place, and where people came from / went to (by event year), complete it.
//! Pure data: the UI localizes and renders it.
use crate::places::PlaceGroup;
use crate::tree::{Sex, Tree};
use std::collections::{HashMap, HashSet, VecDeque};

/// Two people at the place sharing an ancestor at most this many generations up are kin.
pub const COMMON_DEPTH: usize = 3;
/// A direct ancestor at the place joins the branch up to this many generations up.
pub const LINE_DEPTH: usize = 8;
/// Ancestors visited per person (keeps huge pedigrees cheap).
const MAX_VISIT: usize = 600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// most common surname among the members (may be empty)
    pub surname: String,
    /// oldest generation first, then by first year at the place
    pub members: Vec<String>,
    /// generations spanned by the members (1 = siblings/cousins/couples only)
    pub generations: usize,
    /// first and last event year at the place
    pub years: Option<(i32, i32)>,
    /// longest ancestor → descendant line among the members (≥ 2 people, else empty)
    pub line: Vec<String>,
    /// the focus person is a member
    pub has_focus: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Couple {
    pub family: String,
    pub partners: [String; 2],
    /// tag of the event that places them here (`MARR`, `RESI`…)
    pub tag: String,
    pub year: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentChild {
    pub parent: String,
    pub child: String,
    /// both were born here
    pub both_born: bool,
    pub parent_year: Option<i32>,
    pub child_year: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveDir {
    /// their previous event was at `key`: they came from there
    From,
    /// their next event was at `key`: they went there
    To,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    pub dir: MoveDir,
    /// `place_key` of the other place
    pub key: String,
    pub name: String,
    pub people: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlaceLinks {
    /// everybody with an event here, by first year here
    pub people: Vec<String>,
    pub years: Option<(i32, i32)>,
    /// branches of ≥ 2 people, the focus's first, then largest
    pub branches: Vec<Branch>,
    /// people with no relative at the place
    pub alone: Vec<String>,
    pub couples: Vec<Couple>,
    pub parent_child: Vec<ParentChild>,
    /// most people first
    pub migrations: Vec<Migration>,
}

impl ParentChild {
    /// «madre e hijo», «father and daughter»: the pair by the people's sex.
    pub fn label(&self, tree: &Tree) -> String {
        let g = |id: &str| match tree.person(id).map(|p| p.sex).unwrap_or_default() {
            Sex::Male => "m",
            Sex::Female => "f",
            Sex::Unknown => "u",
        };
        let rel = |stem: &str, id: &str| i18n::tr(&format!("core.rel.{stem}.{}", g(id)), &[]);
        let (parent, child) = (rel("parent", &self.parent), rel("child", &self.child));
        i18n::t!("core.places.parent_child", parent = parent, child = child)
    }
}

/// Order of events within a year: birth first, death and burial last.
fn tag_rank(tag: &str) -> u8 {
    match tag {
        "BIRT" | "CHR" | "BAPM" => 0,
        "DEAT" => 2,
        "BURI" | "CREM" => 3,
        _ => 1,
    }
}

fn span(years: impl Iterator<Item = i32>) -> Option<(i32, i32)> {
    years.fold(None, |acc, y| Some(acc.map_or((y, y), |(a, b): (i32, i32)| (a.min(y), b.max(y)))))
}

impl Tree {
    /// Ancestors of `id` up to `max` generations → generations up (itself at 0).
    fn ancestors_within(&self, id: &str, max: usize) -> HashMap<String, usize> {
        let mut out = HashMap::from([(id.to_string(), 0)]);
        let mut queue = VecDeque::from([(id.to_string(), 0)]);
        while let Some((x, d)) = queue.pop_front() {
            if d >= max || out.len() >= MAX_VISIT {
                continue;
            }
            for p in self.parents(&x) {
                if !out.contains_key(&p.id) {
                    out.insert(p.id.clone(), d + 1);
                    queue.push_back((p.id.clone(), d + 1));
                }
            }
        }
        out
    }

    /// Family connections at the place `key` (a `place_key`) of `groups` (from `place_groups`).
    /// `only` restricts it to those people (the map filter); `focus` puts their branch first.
    /// None when the place is unknown or has nobody left after filtering.
    pub fn place_links(&self, groups: &[PlaceGroup], key: &str, focus: Option<&str>, only: Option<&HashSet<String>>) -> Option<PlaceLinks> {
        let group = groups.iter().find(|g| g.key == key)?;
        let keep = |p: &str| only.is_none_or(|s| s.contains(p)) && self.person(p).is_some();

        // people here with their events here
        let mut here: HashMap<&str, Vec<(Option<i32>, &str)>> = HashMap::new();
        for e in &group.events {
            for p in e.people.iter().filter(|p| keep(p)) {
                here.entry(p.as_str()).or_default().push((e.year, e.event.tag.as_str()));
            }
        }
        if here.is_empty() {
            return None;
        }
        let first_year = |p: &str| here[p].iter().filter_map(|e| e.0).min();
        let mut people: Vec<String> = here.keys().map(|s| s.to_string()).collect();
        people.sort_by(|a, b| (first_year(a).is_none(), first_year(a), a).cmp(&(first_year(b).is_none(), first_year(b), b)));
        let index: HashMap<&str, usize> = people.iter().enumerate().map(|(i, p)| (p.as_str(), i)).collect();
        let n = people.len();
        let years = span(group.events.iter().filter(|e| e.people.iter().any(|p| keep(p))).filter_map(|e| e.year));

        // kin edges (a, b, level(b) − level(a)); a level is a generation, larger = younger
        let anc: Vec<HashMap<String, usize>> = people.iter().map(|p| self.ancestors_within(p, LINE_DEPTH)).collect();
        let mut by_ancestor: HashMap<&str, Vec<(usize, usize)>> = HashMap::new();
        for (i, a) in anc.iter().enumerate() {
            for (x, &d) in a {
                by_ancestor.entry(x.as_str()).or_default().push((i, d));
            }
        }
        let mut edges: Vec<Vec<(usize, i64)>> = vec![vec![]; n];
        let mut link = |a: usize, b: usize, delta: i64| {
            if a != b {
                edges[a].push((b, delta));
                edges[b].push((a, -delta));
            }
        };
        let mut member_ancestors: Vec<Vec<usize>> = vec![vec![]; n];
        for (x, list) in &by_ancestor {
            if let Some(&xi) = index.get(x) {
                for &(i, d) in list.iter().filter(|(i, _)| *i != xi) {
                    link(xi, i, d as i64);
                    member_ancestors[i].push(xi);
                }
            }
            let near: Vec<&(usize, usize)> = list.iter().filter(|(_, d)| *d <= COMMON_DEPTH).collect();
            if let Some(&&(first, d0)) = near.first() {
                for &&(i, d) in &near[1..] {
                    link(first, i, d as i64 - d0 as i64);
                }
            }
        }
        for (i, p) in people.iter().enumerate() {
            for f in self.person(p).into_iter().flat_map(|p| p.fams.iter()).filter_map(|f| self.family(f)) {
                if let Some(&j) = f.other_partner(p).and_then(|o| index.get(o.as_str())) {
                    link(i, j, 0);
                }
            }
        }

        // components with levels (first assignment wins under pedigree collapse)
        let mut level: Vec<Option<i64>> = vec![None; n];
        let mut branches = Vec::new();
        let mut alone = Vec::new();
        for start in 0..n {
            if level[start].is_some() {
                continue;
            }
            level[start] = Some(0);
            let mut comp = vec![start];
            let mut q = VecDeque::from([start]);
            while let Some(i) = q.pop_front() {
                let li = level[i].unwrap_or(0);
                for &(j, d) in &edges[i] {
                    if level[j].is_none() {
                        level[j] = Some(li + d);
                        comp.push(j);
                        q.push_back(j);
                    }
                }
            }
            if comp.len() < 2 {
                alone.push(people[start].clone());
                continue;
            }
            let lv = |i: usize| level[i].unwrap_or(0);
            comp.sort_by_key(|&i| (lv(i), i));
            let (lo, hi) = (lv(comp[0]), lv(comp[comp.len() - 1]));
            // longest direct line: DP over members in level order
            let in_comp: HashSet<usize> = comp.iter().copied().collect();
            let mut best: HashMap<usize, (usize, Option<usize>)> = HashMap::new();
            for &i in &comp {
                let prev = member_ancestors[i].iter().filter(|a| in_comp.contains(*a) && lv(**a) < lv(i)).filter_map(|a| best.get(a).map(|b| (b.0, *a))).max_by_key(|(len, a)| (*len, std::cmp::Reverse(*a)));
                best.insert(i, prev.map_or((1, None), |(len, a)| (len + 1, Some(a))));
            }
            let tail = comp.iter().copied().max_by_key(|i| (best[i].0, std::cmp::Reverse(*i))).unwrap_or(comp[0]);
            let mut line = vec![];
            if best[&tail].0 >= 2 {
                let mut cur = Some(tail);
                while let Some(c) = cur {
                    line.push(people[c].clone());
                    cur = best[&c].1;
                }
                line.reverse();
            }
            let mut counts: HashMap<&str, (usize, usize)> = HashMap::new();
            for (rank, &i) in comp.iter().enumerate() {
                let s = self.person(&people[i]).map(|p| p.surname.trim()).unwrap_or("");
                if !s.is_empty() {
                    let e = counts.entry(s).or_insert((0, rank));
                    e.0 += 1;
                }
            }
            let surname = counts.iter().max_by(|a, b| a.1.0.cmp(&b.1.0).then(b.1.1.cmp(&a.1.1))).map(|(s, _)| s.to_string()).unwrap_or_default();
            let members: Vec<String> = comp.iter().map(|&i| people[i].clone()).collect();
            let years = span(members.iter().flat_map(|m| here[m.as_str()].iter().filter_map(|e| e.0)));
            let has_focus = focus.is_some_and(|f| members.iter().any(|m| m == f));
            branches.push(Branch { surname, members, generations: (hi - lo + 1) as usize, years, line, has_focus });
        }
        branches.sort_by(|a, b| b.has_focus.cmp(&a.has_focus).then(b.members.len().cmp(&a.members.len())).then(a.years.cmp(&b.years)));

        // couples: a family event here, or both partners lived (RESI) here
        let mut couples: Vec<Couple> = Vec::new();
        let mut seen_fam: HashSet<&str> = HashSet::new();
        let mut family_events: Vec<&crate::places::PlaceEvent> = group.events.iter().filter(|e| e.family.is_some() && e.people.len() == 2 && e.people.iter().all(|p| keep(p))).collect();
        family_events.sort_by_key(|e| (e.event.tag != "MARR", e.year.is_none(), e.year));
        for e in family_events {
            let Some(f) = e.family.as_deref() else { continue };
            if seen_fam.insert(f) {
                couples.push(Couple { family: f.to_string(), partners: [e.people[0].clone(), e.people[1].clone()], tag: e.event.tag.clone(), year: e.year });
            }
        }
        // Some(first dated year) when `p` has a RESI here
        let resi = |p: &str| {
            let v: Vec<Option<i32>> = here.get(p)?.iter().filter(|e| e.1 == "RESI").map(|e| e.0).collect();
            (!v.is_empty()).then(|| v.into_iter().flatten().min())
        };
        for p in &people {
            for f in self.person(p).into_iter().flat_map(|p| p.fams.iter()).filter_map(|f| self.family(f)) {
                let (Some(h), Some(w)) = (f.husband.as_deref(), f.wife.as_deref()) else { continue };
                if seen_fam.contains(f.id.as_str()) || !index.contains_key(h) || !index.contains_key(w) {
                    continue;
                }
                if let (Some(a), Some(b)) = (resi(h), resi(w)) {
                    seen_fam.insert(f.id.as_str());
                    couples.push(Couple { family: f.id.clone(), partners: [h.to_string(), w.to_string()], tag: "RESI".into(), year: match (a, b) {
                        (Some(x), Some(y)) => Some(x.min(y)),
                        _ => a.or(b),
                    } });
                }
            }
        }
        couples.sort_by_key(|c| (c.year.is_none(), c.year));

        // parent → child pairs
        let born = |p: &str| here[p].iter().any(|e| e.1 == "BIRT");
        let mut parent_child = Vec::new();
        for c in &people {
            let mut seen = HashSet::new();
            for par in self.parents(c) {
                if index.contains_key(par.id.as_str()) && seen.insert(par.id.clone()) {
                    parent_child.push(ParentChild { parent: par.id.clone(), child: c.clone(), both_born: born(&par.id) && born(c), parent_year: first_year(&par.id), child_year: first_year(c) });
                }
            }
        }
        parent_child.sort_by_key(|pc| (pc.parent_year.is_none(), pc.parent_year, pc.child_year));

        // migrations: the dated event just before the first one here / just after the last one
        let mut elsewhere: HashMap<&str, Vec<(i32, u8, usize)>> = HashMap::new();
        for (gi, g) in groups.iter().enumerate() {
            for e in &g.events {
                let Some(y) = e.year else { continue };
                for p in e.people.iter().filter(|p| index.contains_key(p.as_str())) {
                    elsewhere.entry(p.as_str()).or_default().push((y, tag_rank(&e.event.tag), gi));
                }
            }
        }
        let gi_here = groups.iter().position(|g| g.key == key);
        let mut moves: HashMap<(MoveDir, usize), Vec<String>> = HashMap::new();
        for p in &people {
            let Some(evs) = elsewhere.get_mut(p.as_str()) else { continue };
            evs.sort();
            let (Some(first), Some(last)) = (evs.iter().position(|e| Some(e.2) == gi_here), evs.iter().rposition(|e| Some(e.2) == gi_here)) else { continue };
            if let Some(prev) = evs[..first].last() {
                moves.entry((MoveDir::From, prev.2)).or_default().push(p.clone());
            }
            if let Some(next) = evs[last + 1..].first() {
                moves.entry((MoveDir::To, next.2)).or_default().push(p.clone());
            }
        }
        let mut migrations: Vec<Migration> = moves.into_iter().map(|((dir, gi), people)| Migration { dir, key: groups[gi].key.clone(), name: groups[gi].name.clone(), people }).collect();
        migrations.sort_by(|a, b| b.people.len().cmp(&a.people.len()).then((a.dir == MoveDir::To).cmp(&(b.dir == MoveDir::To))).then(a.key.cmp(&b.key)));

        Some(PlaceLinks { people, years, branches, alone, couples, parent_child, migrations })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic tree (invented names): the Prueba line Abel → Bruno → Caro → Dario at «Villa
    // Uno»; Bruno married Eva there; Caro's cousin Fito lived there; Gala (no kin) was born there.
    // Bruno came from «Pueblo Dos»; Dario left for «Ciudad Tres».
    const GED: &str = "0 HEAD\n1 CHAR UTF-8\n\
0 @A@ INDI\n1 NAME Abel /Prueba/\n1 SEX M\n1 BIRT\n2 DATE 1710\n2 PLAC Villa Uno\n1 FAMS @F1@\n\
0 @Z@ INDI\n1 NAME Zoe /Prueba/\n1 SEX M\n1 FAMS @F0@\n\
0 @B@ INDI\n1 NAME Bruno /Prueba/\n1 SEX M\n1 BIRT\n2 DATE 1740\n2 PLAC Pueblo Dos\n1 DEAT\n2 DATE 1800\n2 PLAC Villa Uno\n1 FAMC @F1@\n1 FAMS @F2@\n\
0 @H@ INDI\n1 NAME Hugo /Prueba/\n1 SEX M\n1 FAMC @F1@\n1 FAMS @F4@\n\
0 @E@ INDI\n1 NAME Eva /Ejemplo/\n1 SEX F\n1 BIRT\n2 DATE 1745\n2 PLAC Otra Villa\n1 FAMS @F2@\n\
0 @C@ INDI\n1 NAME Caro /Prueba/\n1 SEX F\n1 BIRT\n2 DATE 1770\n2 PLAC Villa Uno\n1 FAMC @F2@\n1 FAMS @F3@\n\
0 @I@ INDI\n1 NAME Ines /Prueba/\n1 SEX F\n1 FAMC @F4@\n1 FAMS @F5@\n\
0 @FI@ INDI\n1 NAME Fito /Prueba/\n1 SEX M\n1 RESI\n2 DATE 1790\n2 PLAC Villa Uno\n1 FAMC @F5@\n\
0 @D@ INDI\n1 NAME Dario /Mezcla/\n1 SEX M\n1 BIRT\n2 DATE 1798\n2 PLAC Villa Uno\n1 DEAT\n2 DATE 1850\n2 PLAC Ciudad Tres\n1 FAMC @F3@\n\
0 @G@ INDI\n1 NAME Gala /Sola/\n1 SEX F\n1 BIRT\n2 DATE 1900\n2 PLAC villa  uno\n\
0 @F0@ FAM\n1 HUSB @Z@\n1 CHIL @A@\n\
0 @F1@ FAM\n1 HUSB @A@\n1 CHIL @B@\n1 CHIL @H@\n\
0 @F2@ FAM\n1 HUSB @B@\n1 WIFE @E@\n1 MARR\n2 DATE 1765\n2 PLAC Villa Uno\n1 CHIL @C@\n\
0 @F3@ FAM\n1 WIFE @C@\n1 CHIL @D@\n\
0 @F4@ FAM\n1 HUSB @H@\n1 CHIL @I@\n\
0 @F5@ FAM\n1 WIFE @I@\n1 CHIL @FI@\n0 TRLR\n";

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn branches_lines_couples_and_moves() {
        let t = Tree::parse(GED).unwrap();
        let groups = t.place_groups();
        let l = t.place_links(&groups, "villa uno", Some("D"), None).unwrap();
        assert_eq!(l.people.len(), 7);
        assert_eq!(l.years, Some((1710, 1900)));
        assert_eq!(l.branches.len(), 1);
        let b = &l.branches[0];
        assert_eq!(b.surname, "Prueba");
        assert!(b.has_focus);
        assert_eq!(b.members.len(), 6, "Abel, Bruno, Eva, Caro, Fito, Dario");
        assert_eq!(b.members[0], "A");
        assert_eq!(b.generations, 4, "Abel → Bruno → Caro → Dario; Fito is Caro's second cousin");
        assert_eq!(b.line, ids(&["A", "B", "C", "D"]));
        assert_eq!(b.years, Some((1710, 1800)));
        assert_eq!(l.alone, ids(&["G"]));

        assert_eq!(l.couples.len(), 1);
        assert_eq!((l.couples[0].family.as_str(), l.couples[0].tag.as_str(), l.couples[0].year), ("F2", "MARR", Some(1765)));

        let pcs: Vec<(&str, &str, bool)> = l.parent_child.iter().map(|p| (p.parent.as_str(), p.child.as_str(), p.both_born)).collect();
        assert_eq!(pcs, vec![("A", "B", false), ("B", "C", false), ("E", "C", false), ("C", "D", true)]);

        let moves: Vec<(MoveDir, &str, Vec<String>)> = l.migrations.iter().map(|m| (m.dir, m.name.as_str(), m.people.clone())).collect();
        assert_eq!(moves.len(), 3);
        assert!(moves.contains(&(MoveDir::From, "Pueblo Dos", ids(&["B"]))));
        assert!(moves.contains(&(MoveDir::From, "Otra Villa", ids(&["E"]))));
        assert!(moves.contains(&(MoveDir::To, "Ciudad Tres", ids(&["D"]))));
        assert!(l.migrations.iter().all(|m| groups.iter().any(|g| g.key == m.key)));

        let _lock = crate::locale_lock();
        i18n::set_locale("es");
        assert_eq!(l.parent_child[3].label(&t), "madre e hijo");
        i18n::set_locale("en");
        assert_eq!(l.parent_child[0].label(&t), "father and son");
        i18n::set_locale("es");
    }

    #[test]
    fn filter_single_person_and_unknown_place() {
        let t = Tree::parse(GED).unwrap();
        let groups = t.place_groups();
        let only: HashSet<String> = ["G".to_string()].into();
        let l = t.place_links(&groups, "villa uno", None, Some(&only)).unwrap();
        assert_eq!(l.people, ids(&["G"]));
        assert!(l.branches.is_empty() && l.couples.is_empty() && l.parent_child.is_empty() && l.migrations.is_empty());
        assert_eq!(l.alone, ids(&["G"]));
        let l = t.place_links(&groups, "ciudad tres", None, None).unwrap();
        assert_eq!(l.migrations.len(), 1);
        assert_eq!((l.migrations[0].dir, l.migrations[0].name.as_str()), (MoveDir::From, "Villa Uno"));
        assert!(t.place_links(&groups, "nowhere", None, None).is_none());
        let none: HashSet<String> = HashSet::new();
        assert!(t.place_links(&groups, "villa uno", None, Some(&none)).is_none());
    }

    #[test]
    fn residence_couple_and_cycles() {
        // partners who both lived there count as a couple; a parent cycle must not hang
        let ged = "0 HEAD\n\
0 @P@ INDI\n1 NAME Pia /Prueba/\n1 RESI\n2 DATE 1900\n2 PLAC Aldea\n1 FAMS @F1@\n1 FAMC @F2@\n\
0 @Q@ INDI\n1 NAME Quim /Prueba/\n1 RESI\n2 DATE 1905\n2 PLAC Aldea\n1 FAMS @F1@\n1 FAMS @F2@\n1 FAMC @F1@\n\
0 @F1@ FAM\n1 HUSB @Q@\n1 WIFE @P@\n1 CHIL @Q@\n\
0 @F2@ FAM\n1 HUSB @Q@\n1 CHIL @P@\n0 TRLR\n";
        let t = Tree::parse(ged).unwrap();
        let groups = t.place_groups();
        let l = t.place_links(&groups, "aldea", None, None).unwrap();
        assert_eq!(l.couples.len(), 1);
        assert_eq!((l.couples[0].tag.as_str(), l.couples[0].year), ("RESI", Some(1900)));
        assert_eq!(l.branches.len(), 1);
        assert_eq!(l.branches[0].members.len(), 2);
    }

    #[test]
    fn many_people_stays_fast() {
        // 1 500 siblings and their children all born at one place
        let mut g = String::from("0 HEAD\n0 @R@ INDI\n1 NAME Raiz /Prueba/\n1 FAMS @FR@\n");
        let mut fam = String::from("0 @FR@ FAM\n1 HUSB @R@\n");
        for i in 0..750 {
            g += &format!("0 @S{i}@ INDI\n1 NAME S /Prueba/\n1 BIRT\n2 DATE {}\n2 PLAC Llano\n1 FAMC @FR@\n1 FAMS @FS{i}@\n", 1800 + i % 50);
            g += &format!("0 @K{i}@ INDI\n1 NAME K /Prueba/\n1 BIRT\n2 DATE {}\n2 PLAC Llano\n1 FAMC @FS{i}@\n", 1830 + i % 50);
            g += &format!("0 @FS{i}@ FAM\n1 HUSB @S{i}@\n1 CHIL @K{i}@\n");
            fam += &format!("1 CHIL @S{i}@\n");
        }
        g += &fam;
        g += "0 TRLR\n";
        let t = Tree::parse(&g).unwrap();
        let groups = t.place_groups();
        let start = std::time::Instant::now();
        let l = t.place_links(&groups, "llano", None, None).unwrap();
        assert_eq!(l.people.len(), 1500);
        assert_eq!(l.branches.len(), 1);
        assert_eq!(l.branches[0].generations, 2);
        assert_eq!(l.parent_child.len(), 750);
        assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    }
}
