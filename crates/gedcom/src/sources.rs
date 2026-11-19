//! Source citations on a person: `1 SOUR @S1@` (the person as a whole) or `2 SOUR @S1@` under one
//! of their events, each with an optional `PAGE`. Listed, added, re-paged and removed by the edit
//! dialog through [`crate::PersonEdit::citations`].
use crate::node::Node;
use crate::search::fold;
use crate::tree::{EVENT_TAGS, EditError, Tree, event_label, year_of};

/// The `nth` (0-based) line with `tag` among the person record's lines, e.g. the second OCCU.
#[derive(Debug, Clone, PartialEq, Eq, Default, Hash)]
pub struct FactRef {
    pub tag: String,
    pub nth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CitationEdit {
    /// None cites the person as a whole
    pub fact: Option<FactRef>,
    /// SOUR record id; empty with a `title` means «reuse the source with that title, or create it»
    pub source: String,
    /// source title (TITL), or the text of an inline `SOUR text` citation
    pub title: String,
    pub page: String,
    /// index among the SOUR lines of its fact for a citation already in the file; None for a new one
    pub slot: Option<usize>,
}

/// An event of the person that a citation can back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactOption {
    pub fact: FactRef,
    /// «Profesión: Agricultor», «Nacimiento»…
    pub label: String,
    pub year: Option<i32>,
}

fn fact_node_mut<'a>(rec: &'a mut Node, fact: &Option<FactRef>) -> Option<&'a mut Node> {
    match fact {
        None => Some(rec),
        Some(f) => rec.children.iter_mut().filter(|c| c.tag == f.tag).nth(f.nth),
    }
}

fn source_title(rec: &Node) -> Option<String> {
    ["TITL", "ABBR"].iter().find_map(|t| rec.child_value(t)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Tree {
    /// All SOUR records as (id, title), sorted by title.
    pub fn sources(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .records
            .iter()
            .filter(|r| r.tag == "SOUR")
            .filter_map(|r| {
                let id = r.xref.clone()?;
                let title = source_title(r).unwrap_or_else(|| id.clone());
                Some((id, title))
            })
            .collect();
        out.sort_by_cached_key(|(id, t)| (fold(t), id.clone()));
        out
    }

    /// Citations on the person and on each of their events, in file order.
    pub fn citations(&self, id: &str) -> Vec<CitationEdit> {
        let Some(rec) = self.record(id) else { return vec![] };
        let mut out = Vec::new();
        let mut push = |fact: Option<FactRef>, node: &Node| {
            for (slot, s) in node.children_with("SOUR").enumerate() {
                let (source, title) = match s.pointer() {
                    Some(sid) => (sid.to_string(), self.record(sid).and_then(source_title).unwrap_or_else(|| sid.to_string())),
                    None => (String::new(), s.value.trim().to_string()),
                };
                out.push(CitationEdit { fact: fact.clone(), source, title, page: s.child_value("PAGE").unwrap_or_default().trim().to_string(), slot: Some(slot) });
            }
        };
        push(None, rec);
        let mut seen: std::collections::HashMap<&str, usize> = Default::default();
        for c in &rec.children {
            if EVENT_TAGS.contains(&c.tag.as_str()) {
                let nth = seen.entry(c.tag.as_str()).or_default();
                push(Some(FactRef { tag: c.tag.clone(), nth: *nth }), c);
                *nth += 1;
            }
        }
        out
    }

    /// The person's events, in file order, as citation targets.
    pub fn citable_facts(&self, id: &str) -> Vec<FactOption> {
        let Some(rec) = self.record(id) else { return vec![] };
        let mut seen: std::collections::HashMap<&str, usize> = Default::default();
        let mut out = Vec::new();
        for c in &rec.children {
            if !EVENT_TAGS.contains(&c.tag.as_str()) {
                continue;
            }
            let nth = seen.entry(c.tag.as_str()).or_default();
            let base = event_label(&c.tag);
            let value = c.value.trim();
            let label = match c.child_value("TYPE") {
                Some(kind) => i18n::t!("core.event.with_kind", label = base, kind = kind),
                None if !value.is_empty() && value != "Y" => i18n::t!("core.event.with_value", label = base, value = value),
                None => base,
            };
            out.push(FactOption { fact: FactRef { tag: c.tag.clone(), nth: *nth }, label, year: c.child_value("DATE").and_then(year_of) });
            *nth += 1;
        }
        out
    }

    /// Id of the source titled `title` (accent/case-insensitive), creating `0 @Sn@ SOUR / 1 TITL` if none.
    pub fn find_or_add_source(&mut self, title: &str) -> String {
        let title = title.trim();
        let key = fold(title);
        if let Some((id, _)) = self.sources().into_iter().find(|(_, t)| fold(t) == key) {
            return id;
        }
        let id = self.next_id("S");
        let mut rec = Node::new("SOUR", "").with_xref(id.clone());
        rec.children.push(Node::new("TITL", title));
        self.insert_record(rec);
        self.reix();
        id
    }

    /// Bring the person's citations from `old` (as read by [`Tree::citations`]) to `new`:
    /// page changes first, then removals (last slot first, so earlier slots stay valid), then additions.
    pub(crate) fn apply_citations(&mut self, id: &str, old: &[CitationEdit], new: &[CitationEdit]) -> Result<bool, EditError> {
        let key = |c: &CitationEdit| (c.fact.clone(), c.slot);
        let mut changed = false;
        for n in new.iter().filter(|c| c.slot.is_some()) {
            if let Some(o) = old.iter().find(|o| key(o) == key(n)) {
                if o.page.trim() != n.page.trim() {
                    let rec = self.record_mut(id)?;
                    if let Some(s) = fact_node_mut(rec, &n.fact).and_then(|f| f.children.iter_mut().filter(|c| c.tag == "SOUR").nth(n.slot.unwrap_or(0))) {
                        s.set_child_value("PAGE", &n.page);
                        changed = true;
                    }
                }
            }
        }
        let mut gone: Vec<&CitationEdit> = old.iter().filter(|o| o.slot.is_some() && !new.iter().any(|n| key(n) == key(o))).collect();
        gone.sort_by_key(|c| std::cmp::Reverse(c.slot));
        for o in gone {
            let rec = self.record_mut(id)?;
            if let Some(f) = fact_node_mut(rec, &o.fact) {
                if let Some(ix) = f.children.iter().enumerate().filter(|(_, c)| c.tag == "SOUR").nth(o.slot.unwrap_or(0)).map(|(i, _)| i) {
                    f.children.remove(ix);
                    changed = true;
                }
            }
        }
        for n in new.iter().filter(|c| c.slot.is_none()) {
            let source = if !n.source.is_empty() && self.record(&n.source).is_some() {
                n.source.clone()
            } else if !n.title.trim().is_empty() {
                self.find_or_add_source(&n.title)
            } else {
                continue;
            };
            let mut cite = Node::new("SOUR", format!("@{source}@"));
            if !n.page.trim().is_empty() {
                cite.children.push(Node::new("PAGE", n.page.trim()));
            }
            let rec = self.record_mut(id)?;
            if let Some(f) = fact_node_mut(rec, &n.fact) {
                f.children.push(cite);
                changed = true;
            }
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

    const GED: &str = "0 HEAD\n0 @I1@ INDI\n1 NAME Ana /Ruiz/\n1 SOUR @S1@\n2 PAGE p. 4\n1 BIRT\n2 DATE 1850\n2 SOUR nota de bautismo\n1 OCCU Agricultora\n2 SOUR @S1@\n3 PAGE p. 9\n1 OCCU Maestra\n0 @S1@ SOUR\n1 TITL Libro de familia\n0 @S2@ SOUR\n1 ABBR Acta\n0 TRLR\n";

    fn occu(nth: usize) -> Option<FactRef> {
        Some(FactRef { tag: "OCCU".into(), nth })
    }

    #[test]
    fn lists_sources_citations_and_facts() {
        let t = Tree::parse(GED).unwrap();
        assert_eq!(t.sources(), vec![("S2".into(), "Acta".into()), ("S1".into(), "Libro de familia".into())]);
        let c = t.citations("I1");
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].fact.clone(), c[0].source.as_str(), c[0].title.as_str(), c[0].page.as_str()), (None, "S1", "Libro de familia", "p. 4"));
        assert_eq!((c[1].fact.clone(), c[1].source.as_str(), c[1].title.as_str()), (Some(FactRef { tag: "BIRT".into(), nth: 0 }), "", "nota de bautismo"));
        assert_eq!((c[2].fact.clone(), c[2].page.as_str()), (occu(0), "p. 9"));
        let f = t.citable_facts("I1");
        assert_eq!(f.iter().map(|o| o.fact.clone()).collect::<Vec<_>>(), vec![FactRef { tag: "BIRT".into(), nth: 0 }, occu(0).unwrap(), occu(1).unwrap()]);
        assert_eq!(f[0].year, Some(1850));
    }

    #[test]
    fn add_remove_and_repage() {
        let mut t = Tree::parse(GED).unwrap();
        let old = t.citations("I1");
        assert!(!t.apply_citations("I1", &old, &old).unwrap(), "no-op");
        let mut new = old.clone();
        new.remove(1); // inline BIRT citation
        new[0].page = "p. 5".into();
        new.push(CitationEdit { fact: occu(1), source: String::new(), title: "acta  ".into(), page: "folio 3".into(), slot: None });
        new.push(CitationEdit { fact: None, source: String::new(), title: "Censo 1912".into(), page: String::new(), slot: None });
        assert!(t.apply_citations("I1", &old, &new).unwrap());
        let text = t.record_text("I1");
        assert!(text.contains("1 SOUR @S1@\n2 PAGE p. 5\n"));
        assert!(!text.contains("bautismo"));
        assert!(text.contains("1 OCCU Maestra\n2 SOUR @S2@\n3 PAGE folio 3\n"), "reuses source by title: {text}");
        assert!(text.contains("1 SOUR @S3@\n"));
        assert_eq!(t.sources().iter().find(|(id, _)| id == "S3").map(|(_, t)| t.as_str()), Some("Censo 1912"));
        let again = t.citations("I1");
        assert_eq!(again.len(), 4);
        assert!(!t.apply_citations("I1", &again, &again).unwrap());
    }

    #[test]
    fn removing_two_in_same_fact_keeps_the_right_one() {
        let mut t = Tree::parse("0 HEAD\n0 @I1@ INDI\n1 SOUR @S1@\n1 SOUR @S2@\n1 SOUR @S3@\n0 @S1@ SOUR\n0 @S2@ SOUR\n0 @S3@ SOUR\n0 TRLR\n").unwrap();
        let old = t.citations("I1");
        let new = vec![old[1].clone()];
        t.apply_citations("I1", &old, &new).unwrap();
        assert_eq!(t.citations("I1").iter().map(|c| c.source.as_str()).collect::<Vec<_>>(), vec!["S2"]);
    }
}
