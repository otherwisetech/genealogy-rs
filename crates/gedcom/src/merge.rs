//! Merging two INDI records that describe the same person.
//!
//! `merge_persons(keep, dup)` rewrites every `@dup@` pointer in the file to `@keep@`, folds dup's
//! record into keep's without losing data, then removes dup. Folding rules:
//! - NAME: an identical NAME gains the sub-lines it lacks; a different one becomes an alternate NAME.
//! - Single events (BIRT, CHR, BAPM, DEAT, BURI, CREM): moved over when keep has none; otherwise
//!   keep's event gains the DATE/PLAC it lacks and any sub-line it doesn't already have. When both
//!   carry a *different* DATE or PLAC the dup event is kept as a second, alternate event instead of
//!   producing an event with two dates.
//! - SEX: only when keep's is missing or `U`.
//! - Bookkeeping of dup (CHAN, _UPD, RIN) is dropped: it describes a record that no longer exists.
//!   `_UID`/REFN are kept as extra lines so external tools can still match the old record.
//! - Everything else (NOTE, SOUR, OBJE, OCCU, vendor `_` tags, FAMC/FAMS…) is appended unless an
//!   identical subtree is already there.
//!
//! Afterwards duplicate links are collapsed (a family listing the same CHIL twice, keep with two
//! FAMC to one family), and a link of keep to itself (ASSO) is removed. Two families that end up
//! with the same couple are left as they are.
use crate::node::Node;
use crate::search::{fold, tokens};
use crate::tree::{EditError, Person, Sex, Tree};

const SINGLE_EVENTS: &[&str] = &["BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM"];
const DROPPED: &[&str] = &["CHAN", "_UPD", "RIN"];
const LINK_TAGS: &[&str] = &["FAMC", "FAMS", "CHIL", "HUSB", "WIFE"];

impl Tree {
    /// Whether `a` and `b` may be merged: both exist, differ, and are neither partners in one
    /// family nor parent and child.
    pub fn can_merge(&self, a: &str, b: &str) -> Result<(), EditError> {
        let pa = self.person(a).ok_or_else(|| EditError::NotFound(a.into()))?;
        let pb = self.person(b).ok_or_else(|| EditError::NotFound(b.into()))?;
        if a == b {
            return Err(EditError::MergeSame);
        }
        let related = self.families.values().any(|f| {
            let partner = |x: &str| f.partners().any(|p| p == x);
            let child = |x: &str| f.children.iter().any(|c| c == x);
            (partner(a) && partner(b)) || (partner(a) && child(b)) || (partner(b) && child(a))
        });
        if related {
            return Err(EditError::MergeRelated(pa.name(), pb.name()));
        }
        Ok(())
    }

    /// Merge `dup` into `keep` (see the module docs); `dup` no longer exists afterwards.
    pub fn merge_persons(&mut self, keep: &str, dup: &str) -> Result<(), EditError> {
        self.can_merge(keep, dup)?;
        let at = self.records.iter().position(|r| r.xref.as_deref() == Some(dup)).ok_or_else(|| EditError::NotFound(dup.into()))?;
        let mut dup_rec = self.records.remove(at);
        let (from, to) = (format!("@{dup}@"), format!("@{keep}@"));
        for r in &mut self.records {
            rewrite_pointers(r, &from, &to);
        }
        rewrite_pointers(&mut dup_rec, &from, &to);
        self.reix();

        let rec = self.record_mut(keep)?;
        for c in dup_rec.children {
            fold_child(rec, c);
        }
        rec.children.retain(|c| !(matches!(c.tag.as_str(), "ASSO" | "ALIA") && c.value.trim() == to));
        dedupe_links(rec);
        let fams: Vec<String> = rec.children.iter().filter(|c| c.tag == "FAMC" || c.tag == "FAMS").filter_map(|c| c.pointer()).map(str::to_string).collect();
        for f in fams {
            if let Ok(fr) = self.record_mut(&f) {
                dedupe_links(fr);
            }
        }
        self.reindex();
        Ok(())
    }

    /// People that look like duplicates of `id`, best first, with a 0–1 score: similar
    /// accent-folded names, compatible sex, birth/death years within 2 (or unknown), and not a
    /// partner/parent/child of `id`.
    pub fn duplicate_candidates(&self, id: &str) -> Vec<(String, f32)> {
        let Some(me) = self.person(id) else { return vec![] };
        let mine = NameParts::of(me);
        if mine.is_empty() {
            return vec![];
        }
        let mut out: Vec<(usize, String, f32)> = self
            .order
            .iter()
            .enumerate()
            .filter(|(_, o)| o.as_str() != id)
            .filter_map(|(i, o)| {
                let p = &self.people[o];
                let s = duplicate_score(me, &mine, p)?;
                self.can_merge(id, o).ok()?;
                Some((i, o.clone(), s))
            })
            .collect();
        out.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)));
        out.into_iter().map(|(_, id, s)| (id, s)).collect()
    }
}

fn rewrite_pointers(n: &mut Node, from: &str, to: &str) {
    if n.value.trim() == from {
        n.value = to.to_string();
    }
    for c in &mut n.children {
        rewrite_pointers(c, from, to);
    }
}

fn norm(v: &str) -> String {
    v.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Insert after the last sibling with the same tag, else before the first link/bookkeeping line.
fn insert_near(rec: &mut Node, n: Node) {
    let at = match rec.children.iter().rposition(|c| c.tag == n.tag) {
        Some(i) => i + 1,
        None => rec.children.iter().position(|c| matches!(c.tag.as_str(), "FAMC" | "FAMS" | "CHAN" | "_UPD")).unwrap_or(rec.children.len()),
    };
    rec.children.insert(at, n);
}

/// Add each sub-line of `from` that `into` doesn't already have identically.
fn add_missing_children(into: &mut Node, from: Node) {
    for c in from.children {
        if !into.children.contains(&c) {
            into.children.push(c);
        }
    }
}

fn fold_child(rec: &mut Node, c: Node) {
    let tag = c.tag.as_str();
    if DROPPED.contains(&tag) || rec.children.contains(&c) {
        return;
    }
    match tag {
        "NAME" => {
            let has_name = rec.children.iter().any(|n| n.tag == "NAME" && !n.value.trim().is_empty());
            match rec.children.iter_mut().find(|n| n.tag == "NAME" && norm(&n.value) == norm(&c.value)) {
                Some(n) => add_missing_children(n, c),
                None if !has_name => {
                    rec.children.retain(|n| !(n.tag == "NAME" && n.value.trim().is_empty() && n.children.is_empty()));
                    rec.children.insert(0, c);
                }
                None => insert_near(rec, c),
            }
        }
        "SEX" => {
            let mine = rec.child_value("SEX").map(Sex::parse).unwrap_or_default();
            let theirs = Sex::parse(&c.value);
            if mine == Sex::Unknown && theirs != Sex::Unknown {
                rec.set_child_value("SEX", theirs.code());
            }
        }
        t if SINGLE_EVENTS.contains(&t) => match rec.children.iter_mut().find(|e| e.tag == t) {
            None => insert_near(rec, c),
            Some(ev) => {
                let conflict = ["DATE", "PLAC"].iter().any(|s| matches!((ev.child_value(s), c.child_value(s)), (Some(a), Some(b)) if norm(a) != norm(b)));
                if conflict {
                    insert_near(rec, c);
                } else {
                    if ev.value.trim().is_empty() && !c.value.trim().is_empty() {
                        ev.value = c.value.clone();
                    }
                    add_missing_children(ev, c);
                }
            }
        },
        _ => insert_near(rec, c),
    }
}

/// Collapse repeated link lines (same tag and pointer); sub-lines of the repeats are kept.
fn dedupe_links(rec: &mut Node) {
    let mut out: Vec<Node> = Vec::with_capacity(rec.children.len());
    for c in std::mem::take(&mut rec.children) {
        let first = LINK_TAGS.contains(&c.tag.as_str()) && c.pointer().is_some();
        match out.iter_mut().find(|o| first && o.tag == c.tag && o.value.trim() == c.value.trim()) {
            Some(o) => add_missing_children(o, c),
            None => out.push(c),
        }
    }
    rec.children = out;
}

struct NameParts {
    given: Vec<String>,
    surname: Vec<String>,
}

impl NameParts {
    fn of(p: &Person) -> NameParts {
        let toks = |s: &str| tokens(&fold(s)).into_iter().map(str::to_string).collect::<Vec<_>>();
        NameParts { given: toks(&p.given), surname: toks(&p.surname) }
    }
    fn is_empty(&self) -> bool {
        self.given.is_empty() && self.surname.is_empty()
    }
}

fn token_like(a: &str, b: &str) -> bool {
    let n = a.chars().count().min(b.chars().count());
    a == b || (n >= 3 && (a.starts_with(b) || b.starts_with(a))) || (n >= 4 && within_one_edit(a, b))
}

fn within_one_edit(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (s, l) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if l.len() - s.len() > 1 {
        return false;
    }
    let p = s.iter().zip(&l).take_while(|(x, y)| x == y).count();
    if p == s.len() {
        return true;
    }
    if s.len() == l.len() { s[p + 1..] == l[p + 1..] } else { s[p..] == l[p + 1..] }
}

/// Share of tokens that pair up (each token of `b` used once), 0–1.
fn overlap(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.;
    }
    let mut used = vec![false; b.len()];
    let mut hits = 0;
    for x in a {
        if let Some(i) = (0..b.len()).find(|&i| !used[i] && token_like(x, &b[i])) {
            used[i] = true;
            hits += 1;
        }
    }
    2. * hits as f32 / (a.len() + b.len()) as f32
}

fn duplicate_score(me: &Person, mine: &NameParts, other: &Person) -> Option<f32> {
    if me.sex != Sex::Unknown && other.sex != Sex::Unknown && me.sex != other.sex {
        return None;
    }
    let theirs = NameParts::of(other);
    // both parts must agree when both people have them
    let part = |a: &[String], b: &[String]| if a.is_empty() || b.is_empty() { None } else { Some(overlap(a, b)) };
    let g = part(&mine.given, &theirs.given);
    let s = part(&mine.surname, &theirs.surname);
    if g == Some(0.) || s == Some(0.) {
        return None;
    }
    let name = match (g, s) {
        (Some(g), Some(s)) => 0.5 * g + 0.5 * s,
        (Some(x), None) | (None, Some(x)) => 0.6 * x,
        (None, None) => return None,
    };
    if name < 0.5 {
        return None;
    }
    let year = |a: Option<i32>, b: Option<i32>| match (a, b) {
        (Some(a), Some(b)) if (a - b).abs() > 2 => None,
        (Some(a), Some(b)) => Some(1. - (a - b).abs() as f32 / 3.),
        _ => Some(0.3),
    };
    let b = year(me.birth_year(), other.birth_year())?;
    let d = year(me.death_year(), other.death_year())?;
    Some((0.7 * name + 0.15 * b + 0.15 * d).clamp(0., 1.))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUPS: &str = "0 HEAD
1 CHAR UTF-8
0 @I1@ INDI
1 NAME Tomás /Quiroga/
1 SEX M
1 BIRT
2 DATE 1801
1 FAMS @F1@
1 _UID AAA
0 @I2@ INDI
1 NAME Tomas /Quiroga Lema/
1 SEX U
1 BIRT
2 PLAC Villaverde
2 SOUR @S1@
1 DEAT
2 DATE 1870
1 OCCU herrero
1 _CUSTOM vendor data
2 _SUB nested
1 CHAN
2 DATE 1 JAN 2000
1 RIN 77
1 _UID BBB
1 FAMC @F2@
1 FAMS @F3@
1 ASSO @I1@
2 RELA godfather
0 @I3@ INDI
1 NAME Lucía /Pardo/
1 SEX F
1 FAMS @F1@
1 FAMS @F3@
0 @I4@ INDI
1 NAME Hijo /Quiroga/
1 FAMC @F1@
1 FAMC @F3@
0 @I5@ INDI
1 NAME Padre /Quiroga/
1 SEX M
1 FAMS @F2@
1 _LINK @I2@
0 @I6@ INDI
1 NAME Otro /Ruiz/
1 SEX M
1 BIRT
2 DATE 1801
0 @F1@ FAM
1 HUSB @I1@
1 WIFE @I3@
1 CHIL @I4@
0 @F2@ FAM
1 HUSB @I5@
1 CHIL @I2@
0 @F3@ FAM
1 HUSB @I2@
1 WIFE @I3@
1 CHIL @I4@
0 @S1@ SOUR
1 TITL Parish book
0 TRLR
";

    #[test]
    fn merge_rewrites_pointers_and_folds_data() {
        let mut t = Tree::parse(DUPS).unwrap();
        t.merge_persons("I1", "I2").unwrap();
        let out = t.to_gedcom();
        assert!(t.person("I2").is_none());
        assert!(!out.contains("@I2@"), "no pointer to the removed record");
        assert!(out.contains("1 _LINK @I1@"), "vendor pointer rewritten");
        let p = t.person("I1").unwrap();
        assert_eq!(p.given, "Tomás", "keep's NAME stays primary");
        assert_eq!(p.sex, Sex::Male);
        assert_eq!(p.birth_year(), Some(1801));
        assert_eq!(p.death_year(), Some(1870), "missing event moved over");
        assert_eq!(p.birth.as_ref().unwrap().place.as_deref(), Some("Villaverde"), "DATE/PLAC filled in");
        assert_eq!(p.famc, vec!["F2"]);
        assert_eq!(p.fams, vec!["F1", "F3"]);
        assert_eq!(t.family("F3").unwrap().husband.as_deref(), Some("I1"));
        assert_eq!(t.family("F2").unwrap().children, vec!["I1"]);
        let rec = t.record_text("I1");
        assert!(rec.contains("1 NAME Tomas /Quiroga Lema/"), "alternate name");
        assert!(rec.contains("2 SOUR @S1@") && rec.contains("1 OCCU herrero"));
        assert!(rec.contains("1 _CUSTOM vendor data\n2 _SUB nested"), "unknown tags preserved");
        assert!(rec.contains("_UID AAA") && rec.contains("_UID BBB"));
        assert!(!rec.contains("CHAN") && !rec.contains("RIN"), "dup bookkeeping dropped");
        assert!(!rec.contains("ASSO"), "self-association removed");
        let t2 = Tree::parse(&out).unwrap();
        assert_eq!(t2.to_gedcom(), out, "round-trips");
        assert_eq!(t2.stats(), (5, 3));
    }

    #[test]
    fn merge_dedupes_links() {
        // siblings entered twice: both CHIL of F1
        let src = "0 HEAD\n0 @I1@ INDI\n1 NAME A /B/\n1 FAMC @F1@\n2 PEDI birth\n0 @I2@ INDI\n1 NAME A /B/\n1 FAMC @F1@\n2 _NOTE x\n0 @I3@ INDI\n1 NAME P /B/\n1 FAMS @F1@\n0 @F1@ FAM\n1 HUSB @I3@\n1 CHIL @I1@\n1 CHIL @I2@\n0 TRLR\n";
        let mut t = Tree::parse(src).unwrap();
        t.merge_persons("I1", "I2").unwrap();
        assert_eq!(t.family("F1").unwrap().children, vec!["I1"]);
        assert_eq!(t.person("I1").unwrap().famc, vec!["F1"]);
        let rec = t.record_text("I1");
        assert_eq!(rec.matches("1 NAME").count(), 1, "identical name not repeated");
        assert!(rec.contains("2 PEDI birth") && rec.contains("2 _NOTE x"), "sub-lines of repeated links kept");
    }

    #[test]
    fn conflicting_event_becomes_alternate() {
        let src = "0 HEAD\n0 @I1@ INDI\n1 NAME A /B/\n1 SEX U\n1 BIRT\n2 DATE 1800\n0 @I2@ INDI\n1 NAME A /B/\n1 SEX F\n1 BIRT\n2 DATE 1801\n0 TRLR\n";
        let mut t = Tree::parse(src).unwrap();
        t.merge_persons("I1", "I2").unwrap();
        let p = t.person("I1").unwrap();
        assert_eq!(p.birth_year(), Some(1800));
        assert_eq!(p.sex, Sex::Female, "unknown sex filled");
        assert_eq!(p.events.iter().filter(|e| e.tag == "BIRT").count(), 2);
    }

    #[test]
    fn merge_refusals() {
        let mut t = Tree::parse(DUPS).unwrap();
        assert!(matches!(t.merge_persons("I1", "I1"), Err(EditError::MergeSame)));
        assert!(matches!(t.merge_persons("I1", "I99"), Err(EditError::NotFound(_))));
        assert!(matches!(t.merge_persons("I1", "F1"), Err(EditError::NotFound(_))));
        assert!(matches!(t.merge_persons("I1", "I3"), Err(EditError::MergeRelated(..))), "partners");
        assert!(matches!(t.merge_persons("I4", "I1"), Err(EditError::MergeRelated(..))), "child and parent");
        assert_eq!(t.to_gedcom(), Tree::parse(DUPS).unwrap().to_gedcom(), "refusal leaves the tree untouched");
    }

    #[test]
    fn candidates() {
        let t = Tree::parse(DUPS).unwrap();
        let c = t.duplicate_candidates("I1");
        assert_eq!(c.first().map(|c| c.0.as_str()), Some("I2"));
        assert!(c.iter().all(|(id, _)| id != "I3" && id != "I4" && id != "I6"), "{c:?}");
        assert!(c.iter().all(|(id, _)| id != "I5"), "different given name");
        assert!(t.duplicate_candidates("I99").is_empty());
        assert!(within_one_edit("garcia", "garsia") && within_one_edit("lema", "lemas") && !within_one_edit("lema", "lima x"));
    }
}
