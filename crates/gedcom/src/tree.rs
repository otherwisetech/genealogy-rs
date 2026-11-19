//! Family-tree view over the raw records, plus the edits the app offers. Edits mutate the
//! records directly and keep links symmetric (FAMC↔CHIL, FAMS↔HUSB/WIFE).
use crate::node::{self, Format, Node, ParseError};
use crate::search::fold;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sex {
    Male,
    Female,
    #[default]
    Unknown,
}

impl Sex {
    pub fn code(self) -> &'static str {
        match self {
            Sex::Male => "M",
            Sex::Female => "F",
            Sex::Unknown => "U",
        }
    }
    pub(crate) fn parse(v: &str) -> Sex {
        match v.trim().chars().next() {
            Some('M' | 'm') => Sex::Male,
            Some('F' | 'f') => Sex::Female,
            _ => Sex::Unknown,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Event {
    pub tag: String,
    pub kind: Option<String>,
    pub date: Option<String>,
    pub place: Option<String>,
    pub value: Option<String>,
}

impl Event {
    pub fn label(&self) -> String {
        let base = event_label(&self.tag);
        match &self.kind {
            Some(t) => i18n::t!("core.event.with_kind", label = base, kind = t),
            None => base,
        }
    }
}

/// Localized name of an event/attribute tag; unknown tags are returned as-is.
pub fn event_label(tag: &str) -> String {
    const KNOWN: &[&str] = &[
        "BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM", "MARR", "DIV", "ENGA", "RESI", "OCCU", "EDUC", "EMIG", "IMMI", "NATU", "CENS", "RELI", "TITL", "GRAD", "RETI", "EVEN", "FACT", "ADOP",
        "BARM", "BASM", "BLES", "CHRA", "CONF", "FCOM", "ORDN", "PROB", "WILL", "NATI", "CAST", "DSCR", "IDNO", "NCHI", "NMR", "PROP", "SSN", "ANUL", "MARB", "MARC", "MARL", "MARS", "DIVF",
    ];
    if KNOWN.contains(&tag) { i18n::tr(&format!("core.event.{}", tag.to_lowercase()), &[]) } else { tag.to_string() }
}

pub(crate) const EVENT_TAGS: &[&str] = &[
    "BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM", "ADOP", "BARM", "BASM", "BLES", "CHRA", "CONF", "FCOM", "ORDN", "NATU", "EMIG", "IMMI", "CENS", "PROB", "WILL", "GRAD", "RETI", "EVEN",
    "RESI", "OCCU", "EDUC", "RELI", "TITL", "NATI", "CAST", "DSCR", "IDNO", "NCHI", "NMR", "PROP", "SSN", "FACT", "MARR", "DIV", "ENGA", "ANUL", "MARB", "MARC", "MARL", "MARS", "DIVF",
];

fn event_of(n: &Node) -> Event {
    Event {
        tag: n.tag.clone(),
        kind: n.child_value("TYPE").map(str::to_string),
        date: n.child_value("DATE").map(str::to_string),
        place: n.child_value("PLAC").map(str::to_string),
        value: (!n.value.trim().is_empty() && n.value.trim() != "Y").then(|| n.value.trim().to_string()),
    }
}

/// Summary of an INDI record, rebuilt on every change.
#[derive(Debug, Clone, Default)]
pub struct Person {
    pub id: String,
    pub given: String,
    pub surname: String,
    pub sex: Sex,
    pub birth: Option<Event>,
    pub death: Option<Event>,
    pub events: Vec<Event>,
    /// families where this person is a child
    pub famc: Vec<String>,
    /// families where this person is a spouse/partner
    pub fams: Vec<String>,
    pub notes: Vec<String>,
    /// accent/case-folded "given surname years" for search
    pub search_key: String,
    /// accent/case-folded text of the notes, searched after `search_key`
    pub note_key: String,
}

impl Person {
    pub fn name(&self) -> String {
        let n = format!("{} {}", self.given, self.surname);
        let n = n.trim();
        if n.is_empty() { i18n::t!("core.person.unnamed") } else { n.to_string() }
    }

    pub fn birth_year(&self) -> Option<i32> {
        self.birth.as_ref().and_then(|e| e.date.as_deref()).and_then(year_of)
    }

    pub fn death_year(&self) -> Option<i32> {
        self.death.as_ref().and_then(|e| e.date.as_deref()).and_then(year_of)
    }

    /// Localized "1901 – 1980", "n. 1901", "† 1980", or "".
    pub fn lifespan(&self) -> String {
        let dead = self.death.is_some();
        match (self.birth_year(), self.death_year()) {
            (Some(b), Some(d)) => i18n::t!("core.lifespan.both", birth = b, death = d),
            (Some(b), None) if dead => i18n::t!("core.lifespan.born_dead", birth = b),
            (Some(b), None) => i18n::t!("core.lifespan.born", year = b),
            (None, Some(d)) => i18n::t!("core.lifespan.died", year = d),
            (None, None) if dead => "†".into(),
            _ => String::new(),
        }
    }
}

/// Last 3–4 digit number in a GEDCOM date phrase (`ABT 1850`, `BET 1800 AND 1810` → 1810 is fine
/// for display/sorting purposes; we take the first).
pub fn year_of(date: &str) -> Option<i32> {
    date.split(|c: char| !c.is_ascii_digit()).filter(|t| (3..=4).contains(&t.len())).find_map(|t| t.parse().ok())
}

#[derive(Debug, Clone, Default)]
pub struct Family {
    pub id: String,
    pub husband: Option<String>,
    pub wife: Option<String>,
    pub children: Vec<String>,
    pub events: Vec<Event>,
}

impl Family {
    pub fn partners(&self) -> impl Iterator<Item = &String> {
        self.husband.iter().chain(self.wife.iter())
    }
    pub fn other_partner(&self, id: &str) -> Option<&String> {
        self.partners().find(|p| p.as_str() != id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub(crate) records: Vec<Node>,
    format: Format,
    rec_ix: HashMap<String, usize>,
    pub people: HashMap<String, Person>,
    pub families: HashMap<String, Family>,
    /// person ids sorted by surname, given name
    pub order: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum EditError {
    #[error("{}", i18n::t!("core.error.no_record", id = .0))]
    NotFound(String),
    #[error("{}", i18n::t!("core.error.merge_same"))]
    MergeSame,
    /// display names of the two people
    #[error("{}", i18n::t!("core.error.merge_related", a = .0, b = .1))]
    MergeRelated(String, String),
}

impl Tree {
    pub fn parse(text: &str) -> Result<Tree, ParseError> {
        let (records, format) = node::parse(text)?;
        let mut t = Tree { records, format, ..Default::default() };
        t.reindex();
        Ok(t)
    }

    /// An empty, valid GEDCOM 5.5.1 file.
    pub fn new_empty() -> Tree {
        let head = Node {
            xref: None,
            tag: "HEAD".into(),
            value: String::new(),
            children: vec![
                Node { tag: "GEDC".into(), children: vec![Node::new("VERS", "5.5.1"), Node::new("FORM", "LINEAGE-LINKED")], ..Default::default() },
                Node::new("CHAR", "UTF-8"),
                Node { tag: "SOUR".into(), value: "GEN-RS".into(), children: vec![Node::new("NAME", "gen-rs")], ..Default::default() },
            ],
        };
        let mut t = Tree { records: vec![head, Node::new("TRLR", "")], ..Default::default() };
        t.reindex();
        t
    }

    pub fn to_gedcom(&self) -> String {
        node::write(&self.records, self.format)
    }

    pub fn records(&self) -> &[Node] {
        &self.records
    }

    pub fn record(&self, id: &str) -> Option<&Node> {
        self.rec_ix.get(id).map(|&i| &self.records[i])
    }

    pub(crate) fn record_mut(&mut self, id: &str) -> Result<&mut Node, EditError> {
        let i = *self.rec_ix.get(id).ok_or_else(|| EditError::NotFound(id.into()))?;
        Ok(&mut self.records[i])
    }

    /// Record re-serialized as GEDCOM text (for the raw view).
    pub fn record_text(&self, id: &str) -> String {
        self.record(id).map(|r| node::write(std::slice::from_ref(r), Format::default())).unwrap_or_default()
    }

    pub fn person(&self, id: &str) -> Option<&Person> {
        self.people.get(id)
    }

    pub fn family(&self, id: &str) -> Option<&Family> {
        self.families.get(id)
    }

    pub fn reindex(&mut self) {
        self.rec_ix.clear();
        self.people.clear();
        self.families.clear();
        let notes: HashMap<String, String> = self.records.iter().filter(|r| r.tag == "NOTE").filter_map(|r| Some((r.xref.clone()?, r.value.clone()))).collect();
        for (i, r) in self.records.iter().enumerate() {
            if let Some(x) = &r.xref {
                self.rec_ix.insert(x.clone(), i);
            }
            match (r.tag.as_str(), &r.xref) {
                ("INDI", Some(id)) => {
                    let p = build_person(id, r, &notes);
                    self.people.insert(id.clone(), p);
                }
                ("FAM", Some(id)) => {
                    let ptr = |tag: &str| r.child(tag).and_then(|c| c.pointer()).map(str::to_string);
                    let f = Family {
                        id: id.clone(),
                        husband: ptr("HUSB"),
                        wife: ptr("WIFE"),
                        children: r.children_with("CHIL").filter_map(|c| c.pointer()).map(str::to_string).collect(),
                        events: r.children.iter().filter(|c| EVENT_TAGS.contains(&c.tag.as_str())).map(event_of).collect(),
                    };
                    self.families.insert(id.clone(), f);
                }
                _ => {}
            }
        }
        let mut order: Vec<String> = self.people.keys().cloned().collect();
        order.sort_by_cached_key(|id| {
            let p = &self.people[id];
            (fold(&p.surname), fold(&p.given), p.birth_year().unwrap_or(i32::MAX))
        });
        self.order = order;
    }

    // ---- navigation ----

    pub fn parents(&self, id: &str) -> Vec<&Person> {
        let Some(p) = self.person(id) else { return vec![] };
        p.famc.iter().filter_map(|f| self.family(f)).flat_map(|f| f.partners()).filter_map(|x| self.person(x)).collect()
    }

    /// (father, mother) from the first child-family.
    pub fn father_mother(&self, id: &str) -> (Option<&Person>, Option<&Person>) {
        let fam = self.person(id).and_then(|p| p.famc.first()).and_then(|f| self.family(f));
        match fam {
            Some(f) => (f.husband.as_deref().and_then(|x| self.person(x)), f.wife.as_deref().and_then(|x| self.person(x))),
            None => (None, None),
        }
    }

    pub fn siblings(&self, id: &str) -> Vec<&Person> {
        let Some(p) = self.person(id) else { return vec![] };
        let mut out: Vec<&Person> = Vec::new();
        for f in p.famc.iter().filter_map(|f| self.family(f)) {
            for c in &f.children {
                if c != id && !out.iter().any(|o| &o.id == c) {
                    if let Some(s) = self.person(c) {
                        out.push(s);
                    }
                }
            }
        }
        out
    }

    /// Each family this person is a partner in: (family, other partner, children).
    pub fn unions(&self, id: &str) -> Vec<(&Family, Option<&Person>, Vec<&Person>)> {
        let Some(p) = self.person(id) else { return vec![] };
        p.fams
            .iter()
            .filter_map(|f| self.family(f))
            .map(|f| (f, f.other_partner(id).and_then(|o| self.person(o)), f.children.iter().filter_map(|c| self.person(c)).collect()))
            .collect()
    }

    /// Ahnentafel slots 1..2^generations: index 0 = the person, 1/2 = father/mother, ...
    pub fn pedigree(&self, id: &str, generations: u32) -> Vec<Option<String>> {
        let n = (1usize << generations) - 1;
        let mut out = vec![None; n];
        out[0] = self.person(id).map(|p| p.id.clone());
        for i in 0..n {
            let Some(pid) = out[i].clone() else { continue };
            let (f, m) = self.father_mother(&pid);
            if 2 * i + 1 < n {
                out[2 * i + 1] = f.map(|p| p.id.clone());
            }
            if 2 * i + 2 < n {
                out[2 * i + 2] = m.map(|p| p.id.clone());
            }
        }
        out
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.people.len(), self.families.len())
    }

    // ---- edits ----

    pub(crate) fn next_id(&self, prefix: &str) -> String {
        let max = self
            .rec_ix
            .keys()
            .filter(|k| k.starts_with(prefix))
            .filter_map(|k| k[prefix.len()..].parse::<u64>().ok())
            .max()
            .unwrap_or(0);
        format!("{prefix}{}", max + 1)
    }

    pub(crate) fn insert_record(&mut self, rec: Node) {
        // keep TRLR last
        let at = self.records.iter().rposition(|r| r.tag == "TRLR").unwrap_or(self.records.len());
        self.records.insert(at, rec);
    }

    pub fn add_person(&mut self, given: &str, surname: &str, sex: Sex) -> String {
        let id = self.next_id("I");
        let mut rec = Node::new("INDI", "").with_xref(id.clone());
        rec.children.push(name_node(given, surname));
        rec.children.push(Node::new("SEX", sex.code()));
        self.insert_record(rec);
        self.reindex();
        id
    }

    pub fn set_name(&mut self, id: &str, given: &str, surname: &str) -> Result<(), EditError> {
        let rec = self.record_mut(id)?;
        let new = name_node(given, surname);
        match rec.children.iter_mut().find(|c| c.tag == "NAME") {
            Some(n) => {
                n.value = new.value;
                n.set_child_value("GIVN", given);
                n.set_child_value("SURN", surname);
            }
            None => rec.children.insert(0, new),
        }
        self.reindex();
        Ok(())
    }

    pub fn set_sex(&mut self, id: &str, sex: Sex) -> Result<(), EditError> {
        self.record_mut(id)?.set_child_value("SEX", sex.code());
        self.reindex();
        Ok(())
    }

    /// Set date/place of the first `tag` event (BIRT, DEAT, MARR…) on a person or family record.
    /// Both empty removes the event if nothing else hangs off it.
    pub fn set_event(&mut self, id: &str, tag: &str, date: &str, place: &str) -> Result<(), EditError> {
        let rec = self.record_mut(id)?;
        let (date, place) = (date.trim(), place.trim());
        if date.is_empty() && place.is_empty() {
            if let Some(ix) = rec.children.iter().position(|c| c.tag == tag) {
                let ev = &mut rec.children[ix];
                ev.remove_children_where(|c| c.tag == "DATE" || c.tag == "PLAC");
                if ev.children.is_empty() && ev.value.trim().is_empty() {
                    rec.children.remove(ix);
                }
            }
        } else {
            let ev = rec.ensure_child(tag);
            ev.set_child_value("DATE", date);
            ev.set_child_value("PLAC", place);
        }
        self.reindex();
        Ok(())
    }

    /// Replace the person's inline notes (`1 NOTE text`); pointer notes are left alone.
    pub fn set_note(&mut self, id: &str, text: &str) -> Result<(), EditError> {
        let rec = self.record_mut(id)?;
        let first = rec.children.iter().position(|c| c.tag == "NOTE" && c.pointer().is_none());
        rec.remove_children_where(|c| c.tag == "NOTE" && c.pointer().is_none());
        let text = text.trim();
        if !text.is_empty() {
            let at = first.unwrap_or(rec.children.len()).min(rec.children.len());
            rec.children.insert(at, Node::new("NOTE", text));
        }
        self.reindex();
        Ok(())
    }

    fn create_family(&mut self) -> String {
        let id = self.next_id("F");
        self.insert_record(Node::new("FAM", "").with_xref(id.clone()));
        self.rec_ix.insert(id.clone(), self.records.iter().position(|r| r.xref.as_deref() == Some(&id)).unwrap());
        id
    }

    /// Re-derives the record index after inserts (positions shift).
    pub(crate) fn reix(&mut self) {
        self.rec_ix = self.records.iter().enumerate().filter_map(|(i, r)| Some((r.xref.clone()?, i))).collect();
    }

    fn link_partner(&mut self, fam: &str, person: &str) -> Result<(), EditError> {
        self.reix();
        let sex = self.record(person).and_then(|r| r.child_value("SEX")).map(Sex::parse).unwrap_or_default();
        let f = self.record_mut(fam)?;
        let has_h = f.child("HUSB").is_some();
        let has_w = f.child("WIFE").is_some();
        let tag = match sex {
            Sex::Female if !has_w => "WIFE",
            Sex::Male if !has_h => "HUSB",
            _ if !has_h => "HUSB",
            _ => "WIFE",
        };
        let at = f.children.iter().position(|c| c.tag != "HUSB" && c.tag != "WIFE").unwrap_or(f.children.len());
        f.children.insert(at, Node::new(tag, format!("@{person}@")));
        let p = self.record_mut(person)?;
        p.children.push(Node::new("FAMS", format!("@{fam}@")));
        Ok(())
    }

    fn link_child(&mut self, fam: &str, child: &str) -> Result<(), EditError> {
        self.reix();
        self.record_mut(fam)?.children.push(Node::new("CHIL", format!("@{child}@")));
        self.record_mut(child)?.children.push(Node::new("FAMC", format!("@{fam}@")));
        Ok(())
    }

    /// Link `parent` as a parent of `child`: joins the child's first family if it has a free
    /// slot, otherwise makes a new family.
    pub fn add_parent_link(&mut self, child: &str, parent: &str) -> Result<(), EditError> {
        let sex = self.person(parent).map(|p| p.sex).unwrap_or_default();
        let slot_free = |f: &Family| match sex {
            Sex::Male => f.husband.is_none(),
            Sex::Female => f.wife.is_none(),
            Sex::Unknown => f.husband.is_none() || f.wife.is_none(),
        };
        let existing = self.person(child).ok_or_else(|| EditError::NotFound(child.into()))?.famc.iter().find(|f| self.family(f).is_some_and(slot_free)).cloned();
        match existing {
            Some(f) => self.link_partner(&f, parent)?,
            None => {
                let f = self.create_family();
                self.link_partner(&f, parent)?;
                self.link_child(&f, child)?;
            }
        }
        self.reindex();
        Ok(())
    }

    /// New family with both as partners.
    pub fn add_partner_link(&mut self, a: &str, b: &str) -> Result<String, EditError> {
        let f = self.create_family();
        self.link_partner(&f, a)?;
        self.link_partner(&f, b)?;
        self.reindex();
        Ok(f)
    }

    /// Link `child` under `parent`. Uses `family` if given, else the parent's only family, else a
    /// new single-parent family.
    pub fn add_child_link(&mut self, parent: &str, child: &str, family: Option<&str>) -> Result<String, EditError> {
        let p = self.person(parent).ok_or_else(|| EditError::NotFound(parent.into()))?;
        let fam = match family {
            Some(f) => f.to_string(),
            None if p.fams.len() == 1 => p.fams[0].clone(),
            None => {
                let f = self.create_family();
                self.link_partner(&f, parent)?;
                f
            }
        };
        self.link_child(&fam, child)?;
        self.reindex();
        Ok(fam)
    }

    /// Delete a person and every pointer to it; families left with ≤1 member are removed too.
    pub fn delete_person(&mut self, id: &str) -> Result<(), EditError> {
        if !self.people.contains_key(id) {
            return Err(EditError::NotFound(id.into()));
        }
        let ptr = format!("@{id}@");
        self.records.retain(|r| r.xref.as_deref() != Some(id));
        for r in &mut self.records {
            strip_pointer(r, &ptr);
        }
        self.reix();
        self.reindex();
        let empty: Vec<String> = self.families.values().filter(|f| f.partners().count() + f.children.len() <= 1 && self.record(&f.id).is_some_and(|r| !has_events(r))).map(|f| f.id.clone()).collect();
        for f in empty {
            let p = format!("@{f}@");
            self.records.retain(|r| r.xref.as_deref() != Some(f.as_str()));
            for r in &mut self.records {
                strip_pointer(r, &p);
            }
        }
        self.reix();
        self.reindex();
        Ok(())
    }

    /// Remove `child` from family `fam` (both sides of the link).
    pub fn unlink_child(&mut self, fam: &str, child: &str) -> Result<(), EditError> {
        let (cp, fp) = (format!("@{child}@"), format!("@{fam}@"));
        self.record_mut(fam)?.remove_children_where(|c| c.tag == "CHIL" && c.value.trim() == cp);
        self.record_mut(child)?.remove_children_where(|c| c.tag == "FAMC" && c.value.trim() == fp);
        self.reindex();
        Ok(())
    }

    /// Remove `person` as a partner of `fam`.
    pub fn unlink_partner(&mut self, fam: &str, person: &str) -> Result<(), EditError> {
        let (pp, fp) = (format!("@{person}@"), format!("@{fam}@"));
        self.record_mut(fam)?.remove_children_where(|c| (c.tag == "HUSB" || c.tag == "WIFE") && c.value.trim() == pp);
        self.record_mut(person)?.remove_children_where(|c| c.tag == "FAMS" && c.value.trim() == fp);
        self.reindex();
        Ok(())
    }
}

fn has_events(r: &Node) -> bool {
    r.children.iter().any(|c| EVENT_TAGS.contains(&c.tag.as_str()))
}

fn strip_pointer(n: &mut Node, ptr: &str) {
    n.children.retain(|c| !(matches!(c.tag.as_str(), "HUSB" | "WIFE" | "CHIL" | "FAMC" | "FAMS" | "ASSO" | "ALIA") && c.value.trim() == ptr));
    for c in &mut n.children {
        strip_pointer(c, ptr);
    }
}

fn name_node(given: &str, surname: &str) -> Node {
    let (g, s) = (given.trim(), surname.trim());
    let mut n = Node::new("NAME", format!("{g} /{s}/").trim().to_string());
    if !g.is_empty() {
        n.children.push(Node::new("GIVN", g));
    }
    if !s.is_empty() {
        n.children.push(Node::new("SURN", s));
    }
    n
}

/// "Pedro /Pérez/ Jr" → ("Juan", "Pérez").
fn split_name(v: &str) -> (String, String) {
    match (v.find('/'), v.rfind('/')) {
        (Some(a), Some(b)) if b > a => {
            let given = format!("{} {}", v[..a].trim(), v[b + 1..].trim());
            (given.trim().to_string(), v[a + 1..b].trim().to_string())
        }
        _ => (v.trim().to_string(), String::new()),
    }
}

fn build_person(id: &str, r: &Node, notes: &HashMap<String, String>) -> Person {
    let (given, surname) = match r.child("NAME") {
        Some(n) => {
            let (g, s) = split_name(&n.value);
            let g = if g.is_empty() { n.child_value("GIVN").unwrap_or_default().to_string() } else { g };
            let s = if s.is_empty() { n.child_value("SURN").unwrap_or_default().to_string() } else { s };
            (g, s)
        }
        None => Default::default(),
    };
    let events: Vec<Event> = r.children.iter().filter(|c| EVENT_TAGS.contains(&c.tag.as_str())).map(event_of).collect();
    let first = |tags: &[&str]| tags.iter().find_map(|t| events.iter().find(|e| e.tag == *t)).cloned();
    let ptrs = |tag: &str| r.children_with(tag).filter_map(|c| c.pointer()).map(str::to_string).collect::<Vec<_>>();
    let note_list: Vec<String> = r
        .children_with("NOTE")
        .filter_map(|n| match n.pointer() {
            Some(p) => notes.get(p).map(|s| crate::notes::note_text(s)),
            None => Some(crate::notes::note_text(&n.value)),
        })
        .filter(|s| !s.trim().is_empty())
        .collect();
    let mut p = Person {
        id: id.to_string(),
        given,
        surname,
        sex: r.child_value("SEX").map(Sex::parse).unwrap_or_default(),
        birth: first(&["BIRT", "CHR", "BAPM"]),
        death: first(&["DEAT", "BURI", "CREM"]),
        events,
        famc: ptrs("FAMC"),
        fams: ptrs("FAMS"),
        notes: note_list,
        search_key: String::new(),
        note_key: String::new(),
    };
    let married: Vec<&str> = r.children_with("NAME").filter_map(|n| n.child_value("_MARNM")).collect();
    let places: Vec<&str> = p.events.iter().filter_map(|e| e.place.as_deref()).collect();
    // years rather than the localized lifespan so the key doesn't depend on the UI language
    let years = [p.birth_year(), p.death_year()].iter().flatten().map(i32::to_string).collect::<Vec<_>>().join(" ");
    p.search_key = fold(&format!("{} {} {} {} {} {}", p.given, p.surname, married.join(" "), years, places.join(" "), id));
    p.note_key = fold(&p.notes.join(" "));
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample.ged");

    #[test]
    fn builds_people_and_families() {
        let t = Tree::parse(SAMPLE).unwrap();
        assert_eq!(t.stats(), (5, 2));
        let c = t.person("I3").unwrap();
        assert_eq!(c.name(), "Ana María García López");
        assert_eq!(c.birth_year(), Some(1950));
        let (f, m) = t.father_mother("I3");
        assert_eq!(f.unwrap().given, "José");
        assert_eq!(m.unwrap().surname, "López");
        assert_eq!(t.siblings("I3").len(), 1);
        assert_eq!(t.unions("I1")[0].2.len(), 2);
        assert_eq!(t.order.first().map(String::as_str), Some("I1"), "García before López");
    }

    #[test]
    fn pedigree_slots() {
        let t = Tree::parse(SAMPLE).unwrap();
        let p = t.pedigree("I5", 3);
        assert_eq!(p[0].as_deref(), Some("I5"));
        assert_eq!(p[2].as_deref(), Some("I3"), "mother");
        assert_eq!(p[5].as_deref(), Some("I1"), "maternal grandfather");
    }

    #[test]
    fn edit_name_sex_event_round_trip() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        t.set_name("I3", "Ana", "García").unwrap();
        t.set_sex("I3", Sex::Female).unwrap();
        t.set_event("I3", "DEAT", "2 FEB 2020", "Madrid").unwrap();
        t.set_event("I3", "BIRT", "", "").unwrap();
        let t2 = Tree::parse(&t.to_gedcom()).unwrap();
        let p = t2.person("I3").unwrap();
        assert_eq!((p.given.as_str(), p.surname.as_str()), ("Ana", "García"));
        assert_eq!(p.death_year(), Some(2020));
        assert!(p.birth.is_none());
        assert!(t2.record_text("I3").contains("_UID KEEPME"), "vendor tags survive edits");
    }

    #[test]
    fn add_relatives_keeps_links_symmetric() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        let kid = t.add_person("Nuevo", "García", Sex::Male);
        t.add_child_link("I4", &kid, None).unwrap();
        assert!(t.person(&kid).unwrap().famc.len() == 1);
        let spouse = t.add_person("Eva", "Ruiz", Sex::Female);
        let fam = t.add_partner_link("I4", &spouse).unwrap();
        assert_eq!(t.family(&fam).unwrap().wife.as_deref(), Some(spouse.as_str()));
        let dad = t.add_person("Padre", "X", Sex::Male);
        t.add_parent_link("I2", &dad).unwrap();
        assert_eq!(t.father_mother("I2").0.map(|p| p.id.as_str()), Some(dad.as_str()));
        let t2 = Tree::parse(&t.to_gedcom()).unwrap();
        assert_eq!(t2.stats(), t.stats());
        assert_eq!(t2.records().last().unwrap().tag, "TRLR");
    }

    #[test]
    fn delete_person_cleans_pointers_and_empty_families() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        t.delete_person("I5").unwrap();
        assert!(t.person("I5").is_none());
        assert!(!t.to_gedcom().contains("@I5@"));
        // F2 had I3 + I5 only → now 1 member, removed
        assert!(t.family("F2").is_none());
        assert!(t.person("I3").unwrap().fams.is_empty());
    }

    #[test]
    fn year_parsing() {
        assert_eq!(year_of("ABT 1850"), Some(1850));
        assert_eq!(year_of("1 MAY 1966"), Some(1966));
        assert_eq!(year_of("MAY"), None);
    }
}
