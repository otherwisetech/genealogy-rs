//! Every event/attribute of a person (BIRT, OCCU, RESI, EVEN…) and of their unions (MARR, DIV…)
//! as an editable list, for the «Eventos» tab of the edit dialog. Items are addressed by record
//! id + [`FactRef`] (nth line with that tag), the same scheme the citations use.
use crate::node::Node;
use crate::sources::FactRef;
use crate::tree::{EVENT_TAGS, EditError, Tree};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FactEdit {
    /// the person, or one of their FAM records for union events
    pub record: String,
    /// position in the file; None for a fact added in the dialog
    pub fact: Option<FactRef>,
    pub tag: String,
    /// `2 TYPE` (custom EVEN/FACT kind)
    pub kind: String,
    /// line value: «Agricultor» for OCCU, «Católica» for RELI…
    pub value: String,
    /// GEDCOM date phrase
    pub date: String,
    pub place: String,
    pub delete: bool,
}

/// Tags offered when adding an event to a person.
pub const PERSON_EVENT_TAGS: &[&str] = &[
    "BIRT", "BAPM", "CHR", "CONF", "FCOM", "EDUC", "GRAD", "OCCU", "RESI", "EMIG", "IMMI", "NATU", "CENS", "RELI", "TITL", "ORDN", "RETI", "WILL", "PROB", "DEAT", "BURI", "CREM", "EVEN",
];
/// Tags offered when adding an event to one of the person's unions.
pub const FAMILY_EVENT_TAGS: &[&str] = &["ENGA", "MARR", "DIV", "EVEN"];

fn read(record: &str, fact: FactRef, n: &Node) -> FactEdit {
    let v = n.value.trim();
    FactEdit {
        record: record.to_string(),
        fact: Some(fact),
        tag: n.tag.clone(),
        kind: n.child_value("TYPE").unwrap_or_default().trim().to_string(),
        value: if v == "Y" { String::new() } else { v.to_string() },
        date: n.child_value("DATE").unwrap_or_default().trim().to_string(),
        place: n.child_value("PLAC").unwrap_or_default().trim().to_string(),
        delete: false,
    }
}

fn facts_of(record: &str, rec: &Node) -> Vec<FactEdit> {
    let mut seen: std::collections::HashMap<&str, usize> = Default::default();
    let mut out = Vec::new();
    for c in &rec.children {
        if EVENT_TAGS.contains(&c.tag.as_str()) {
            let nth = seen.entry(c.tag.as_str()).or_default();
            out.push(read(record, FactRef { tag: c.tag.clone(), nth: *nth }, c));
            *nth += 1;
        }
    }
    out
}

fn nth_index(rec: &Node, f: &FactRef) -> Option<usize> {
    rec.children.iter().enumerate().filter(|(_, c)| c.tag == f.tag).nth(f.nth).map(|(i, _)| i)
}

/// Write the editable parts of `e` into the event line `n`; sub-lines it doesn't manage
/// (SOUR, NOTE, AGE, ADDR…) are kept.
fn write(n: &mut Node, e: &FactEdit) {
    let value = e.value.trim();
    // a bare event (`1 DEAT`) whose date and place are both empty would vanish; GEDCOM's «Y» keeps it
    n.value = if !value.is_empty() {
        value.to_string()
    } else if n.value.trim() == "Y" || (e.date.trim().is_empty() && e.place.trim().is_empty() && n.children.iter().all(|c| matches!(c.tag.as_str(), "TYPE" | "DATE" | "PLAC"))) {
        "Y".to_string()
    } else {
        String::new()
    };
    let at_front = |n: &mut Node, tag: &str, v: &str| {
        let v = v.trim();
        match n.children.iter().position(|c| c.tag == tag) {
            Some(i) if v.is_empty() && n.children[i].children.is_empty() => {
                n.children.remove(i);
            }
            Some(i) => n.children[i].value = v.to_string(),
            None if v.is_empty() => {}
            None => {
                let at = n.children.iter().position(|c| !matches!(c.tag.as_str(), "TYPE" | "DATE" | "PLAC")).unwrap_or(n.children.len());
                n.children.insert(at, Node::new(tag, v));
            }
        }
    };
    at_front(n, "TYPE", &e.kind);
    at_front(n, "DATE", &e.date);
    at_front(n, "PLAC", &e.place);
}

impl Tree {
    /// Events of the person and of each of their unions, in file order (person first).
    pub fn fact_edits(&self, id: &str) -> Vec<FactEdit> {
        let Some(rec) = self.record(id) else { return vec![] };
        let mut out = facts_of(id, rec);
        if let Some(p) = self.person(id) {
            for f in &p.fams {
                if let Some(r) = self.record(f) {
                    out.extend(facts_of(f, r));
                }
            }
        }
        out
    }

    /// Bring the events from `old` (as read by [`Tree::fact_edits`]) to `new`: changed values in
    /// place, then deletions (last first, so earlier positions stay valid), then additions.
    pub(crate) fn apply_facts(&mut self, old: &[FactEdit], new: &[FactEdit]) -> Result<bool, EditError> {
        let mut changed = false;
        for n in new.iter().filter(|n| n.fact.is_some() && !n.delete) {
            let Some(o) = old.iter().find(|o| o.record == n.record && o.fact == n.fact) else { continue };
            let same = |a: &str, b: &str| a.trim() == b.trim();
            if same(&o.kind, &n.kind) && same(&o.value, &n.value) && same(&o.date, &n.date) && same(&o.place, &n.place) {
                continue;
            }
            let rec = self.record_mut(&n.record)?;
            if let Some(i) = nth_index(rec, n.fact.as_ref().unwrap()) {
                write(&mut rec.children[i], n);
                changed = true;
            }
        }
        let mut gone: Vec<&FactEdit> = new.iter().filter(|n| n.delete && n.fact.is_some()).collect();
        gone.sort_by_key(|n| std::cmp::Reverse(n.fact.as_ref().map(|f| f.nth)));
        for n in gone {
            let rec = self.record_mut(&n.record)?;
            if let Some(i) = nth_index(rec, n.fact.as_ref().unwrap()) {
                rec.children.remove(i);
                changed = true;
            }
        }
        for n in new.iter().filter(|n| n.fact.is_none() && !n.delete && !n.tag.trim().is_empty()) {
            let rec = self.record_mut(&n.record)?;
            let mut node = Node::new(n.tag.trim(), "");
            write(&mut node, n);
            // next to the record's other events, so the file stays readable
            let at = rec.children.iter().rposition(|c| EVENT_TAGS.contains(&c.tag.as_str())).map(|i| i + 1).unwrap_or(rec.children.len());
            rec.children.insert(at, node);
            changed = true;
        }
        if changed {
            self.reindex();
        }
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GED: &str = "0 HEAD\n0 @I1@ INDI\n1 NAME Ana /Ruiz/\n1 BIRT\n2 DATE 1850\n1 OCCU Agricultora\n2 SOUR @S1@\n3 PAGE p. 9\n1 RESI\n2 PLAC Lima\n1 RESI\n2 PLAC Quito\n1 FAMS @F1@\n0 @F1@ FAM\n1 WIFE @I1@\n1 MARR\n2 DATE 1870\n0 @S1@ SOUR\n0 TRLR\n";

    #[test]
    fn lists_person_and_union_events() {
        let t = Tree::parse(GED).unwrap();
        let f = t.fact_edits("I1");
        let tags: Vec<(&str, &str)> = f.iter().map(|e| (e.record.as_str(), e.tag.as_str())).collect();
        assert_eq!(tags, vec![("I1", "BIRT"), ("I1", "OCCU"), ("I1", "RESI"), ("I1", "RESI"), ("F1", "MARR")]);
        assert_eq!((f[1].value.as_str(), f[3].place.as_str(), f[3].fact.as_ref().unwrap().nth), ("Agricultora", "Quito", 1));
    }

    #[test]
    fn edit_delete_add() {
        let mut t = Tree::parse(GED).unwrap();
        let old = t.fact_edits("I1");
        assert!(!t.apply_facts(&old, &old).unwrap());
        let mut new = old.clone();
        new[1].value = "Maestra".into();
        new[1].date = "1880".into();
        new[2].delete = true; // RESI Lima
        new[4].place = "Cusco".into();
        new.push(FactEdit { record: "I1".into(), tag: "EVEN".into(), kind: "Viaje".into(), date: "1890".into(), place: "Panamá".into(), ..Default::default() });
        new.push(FactEdit { record: "I1".into(), tag: "DEAT".into(), ..Default::default() });
        assert!(t.apply_facts(&old, &new).unwrap());
        let text = t.record_text("I1");
        assert!(text.contains("1 OCCU Maestra\n2 DATE 1880\n2 SOUR @S1@\n3 PAGE p. 9\n"), "{text}");
        assert!(!text.contains("Lima") && text.contains("Quito"));
        assert!(text.contains("1 EVEN\n2 TYPE Viaje\n2 DATE 1890\n2 PLAC Panamá\n1 DEAT Y\n1 FAMS"), "{text}");
        assert!(t.record_text("F1").contains("1 MARR\n2 DATE 1870\n2 PLAC Cusco\n"));
        let again = t.fact_edits("I1");
        assert!(!t.apply_facts(&again, &again).unwrap());
    }

    #[test]
    fn deleting_two_of_a_kind() {
        let mut t = Tree::parse(GED).unwrap();
        let old = t.fact_edits("I1");
        let mut new = old.clone();
        new[2].delete = true;
        new[3].delete = true;
        t.apply_facts(&old, &new).unwrap();
        assert!(!t.record_text("I1").contains("RESI"));
    }
}
