//! Appending another GEDCOM file to the open tree.
//!
//! `Tree::import(other)` copies every record of `other` except HEAD and TRLR (INDI, FAM, SOUR,
//! NOTE, OBJE, REPO, SUBM, vendor `_` records…) in file order, before our TRLR. Each record with
//! an xref gets a fresh id with the usual prefix for its tag (I, F, S, N, O, R, U; other tags keep
//! the letters of their old id) numbered above everything already in the tree, and every pointer
//! inside the imported records is rewritten in one pass (so `@I1@` → `@I7@` never chains into
//! another rewrite). A pointer to a record that `other` does not define also gets a fresh id, so
//! it stays dangling instead of silently pointing at an unrelated record of the open tree.
//! Nothing else changes: unknown tags and values are copied as they are. People are not merged;
//! the duplicate finder does that afterwards.
use crate::node::Node;
use crate::tree::Tree;
use std::collections::HashMap;

/// What [`Tree::import`] appended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub people: usize,
    pub families: usize,
    pub sources: usize,
    pub notes: usize,
    pub media: usize,
    pub repositories: usize,
    pub submitters: usize,
    /// records of any other type (vendor `_` records, SUBN…)
    pub other: usize,
    /// new id of the first imported person
    pub first_person: Option<String>,
}

impl ImportReport {
    pub fn records(&self) -> usize {
        self.people + self.families + self.sources + self.notes + self.media + self.repositories + self.submitters + self.other
    }
}

fn prefix_for(tag: &str, old: &str) -> String {
    match tag {
        "INDI" => "I".into(),
        "FAM" => "F".into(),
        "SOUR" => "S".into(),
        "NOTE" | "SNOTE" => "N".into(),
        "OBJE" => "O".into(),
        "REPO" => "R".into(),
        "SUBM" => "U".into(),
        _ => {
            let p: String = old.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
            if p.is_empty() { "X".into() } else { p }
        }
    }
}

struct Fresh {
    next: HashMap<String, u64>,
}

impl Fresh {
    fn id(&mut self, tree: &Tree, prefix: &str) -> String {
        let n = self.next.entry(prefix.to_string()).or_insert_with(|| tree.next_id(prefix)[prefix.len()..].parse().unwrap_or(1));
        let id = format!("{prefix}{n}");
        *n += 1;
        id
    }
}

fn collect_pointers<'a>(n: &'a Node, out: &mut Vec<&'a str>) {
    if let Some(p) = n.pointer() {
        out.push(p);
    }
    for c in &n.children {
        collect_pointers(c, out);
    }
}

fn remap(n: &mut Node, map: &HashMap<String, String>) {
    if let Some(new) = n.pointer().and_then(|p| map.get(p)) {
        n.value = format!("@{new}@");
    }
    for c in &mut n.children {
        remap(c, map);
    }
}

impl Tree {
    /// Append every record of `other` (see the module docs) and report what came in.
    pub fn import(&mut self, other: &Tree) -> ImportReport {
        let incoming: Vec<&Node> = other.records.iter().filter(|r| r.tag != "HEAD" && r.tag != "TRLR").collect();
        let mut fresh = Fresh { next: HashMap::new() };
        let mut map: HashMap<String, String> = HashMap::new();
        for r in &incoming {
            if let Some(x) = &r.xref
                && !map.contains_key(x)
            {
                let id = fresh.id(self, &prefix_for(&r.tag, x));
                map.insert(x.clone(), id);
            }
        }
        let mut dangling = Vec::new();
        for r in &incoming {
            collect_pointers(r, &mut dangling);
        }
        for p in dangling {
            if !map.contains_key(p) {
                let id = fresh.id(self, &prefix_for("", p));
                map.insert(p.to_string(), id);
            }
        }

        let mut report = ImportReport::default();
        for r in incoming {
            let mut rec = r.clone();
            if let Some(x) = &rec.xref {
                rec.xref = map.get(x).cloned();
            }
            remap(&mut rec, &map);
            match rec.tag.as_str() {
                "INDI" => {
                    report.people += 1;
                    if report.first_person.is_none() {
                        report.first_person = rec.xref.clone();
                    }
                }
                "FAM" => report.families += 1,
                "SOUR" => report.sources += 1,
                "NOTE" | "SNOTE" => report.notes += 1,
                "OBJE" => report.media += 1,
                "REPO" => report.repositories += 1,
                "SUBM" => report.submitters += 1,
                _ => report.other += 1,
            }
            self.insert_record(rec);
        }
        self.reindex();
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample.ged");
    const FAMILY: &str = include_str!("../tests/fixtures/family.ged");

    #[test]
    fn appends_with_fresh_ids_and_rewritten_pointers() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        let other = Tree::parse(FAMILY).unwrap();
        let before = t.stats();
        let r = t.import(&other);
        assert_eq!((r.people, r.families, r.sources), (other.people.len(), other.families.len(), 1));
        assert_eq!(r.records(), other.records().len() - 2, "all but HEAD and TRLR");
        assert_eq!(t.stats(), (before.0 + other.people.len(), before.1 + other.families.len()));
        assert_eq!(t.records().iter().filter(|r| r.tag == "HEAD").count(), 1);
        assert_eq!(t.records().last().unwrap().tag, "TRLR");

        // the open tree is untouched
        assert_eq!(t.person("I3").unwrap().name(), "Ana María García López");
        assert_eq!(t.father_mother("I3").0.unwrap().id, "I1");

        // I10 (Tomás) is the first person of the other file; sample's max id is I5, family F2
        let tomas = r.first_person.clone().unwrap();
        assert_eq!(tomas, "I6");
        let p = t.person(&tomas).unwrap();
        assert_eq!(p.given, "Tomás");
        let (f, _) = t.father_mother(&tomas);
        assert_eq!(f.unwrap().given, "Gaspar");
        let kids: Vec<String> = t.unions(&tomas)[0].2.iter().map(|k| k.given.clone()).collect();
        assert_eq!(kids, ["Rosa", "Andrés"]);
        // the citation follows the source to its new id
        let rec = t.record(&tomas).unwrap();
        let sour = rec.child("BIRT").unwrap().child("SOUR").unwrap().pointer().unwrap();
        assert_eq!(t.record(sour).unwrap().child_value("TITL"), Some("Registro parroquial"));
        assert!(t.record("F3").is_some() && t.record("S1").is_some());
    }

    #[test]
    fn import_is_lossless_up_to_ids() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        let src = "0 HEAD\n0 @A@ INDI\n1 NAME Uno /Prueba/\n1 _UID XYZ\n2 _SUB @A@\n1 ASSO @B@\n1 FAMS @F1@\n0 @F1@ FAM\n1 HUSB @A@\n1 _X raro\n0 @N1@ NOTE nota\n1 CONT dos\n0 @R1@ REPO\n1 NAME Archivo\n0 @U1@ SUBM\n0 @Q1@ _PLAC lugar\n0 TRLR\n";
        let other = Tree::parse(src).unwrap();
        let r = t.import(&other);
        assert_eq!((r.people, r.families, r.notes, r.repositories, r.submitters, r.other), (1, 1, 1, 1, 1, 1));
        let p = t.record("I6").unwrap();
        assert_eq!(t.record_text("I6"), "0 @I6@ INDI\n1 NAME Uno /Prueba/\n1 _UID XYZ\n2 _SUB @I6@\n1 ASSO @B1@\n1 FAMS @F3@\n");
        assert_eq!(p.child("ASSO").unwrap().pointer(), Some("B1"), "dangling pointer gets its own fresh id");
        assert!(t.record("B1").is_none());
        assert_eq!(t.record_text("F3"), "0 @F3@ FAM\n1 HUSB @I6@\n1 _X raro\n");
        assert_eq!(t.record("N1").unwrap().value, "nota\ndos");
        assert!(t.record("R1").is_some() && t.record("U1").is_some());
        assert_eq!(t.record("Q1").unwrap().value, "lugar");
        // re-importing the same file never collides
        let r2 = t.import(&other);
        assert_eq!(r2.first_person.as_deref(), Some("I7"));
        assert_eq!(t.people.len(), 5 + 2);
    }
}
