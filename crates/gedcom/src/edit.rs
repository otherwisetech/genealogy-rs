//! Whole-person edit used by the edit dialog.
use crate::node::Node;
use crate::tree::{EditError, Event, Sex, Tree};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EventEdit {
    /// GEDCOM tag actually edited (BIRT or, if the person only has one, CHR/BAPM; DEAT/BURI…)
    pub tag: String,
    /// full GEDCOM date phrase (build it with `DateValue::to_gedcom`)
    pub date: String,
    pub place: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PersonEdit {
    pub id: String,
    pub given: String,
    pub surname: String,
    pub sex: Sex,
    pub birth: EventEdit,
    pub death: EventEdit,
    /// first inline NOTE
    pub note: String,
}

fn event_edit(e: Option<&Event>, default_tag: &str) -> EventEdit {
    match e {
        Some(e) => EventEdit { tag: e.tag.clone(), date: e.date.clone().unwrap_or_default(), place: e.place.clone().unwrap_or_default() },
        None => EventEdit { tag: default_tag.into(), ..Default::default() },
    }
}

fn first_inline_note(rec: &Node) -> Option<&Node> {
    rec.children.iter().find(|c| c.tag == "NOTE" && c.pointer().is_none())
}

impl Tree {
    /// Current values of a person as an editable form.
    pub fn person_edit(&self, id: &str) -> Option<PersonEdit> {
        let p = self.person(id)?;
        let note = self.record(id).and_then(first_inline_note).map(|n| n.value.clone()).unwrap_or_default();
        Some(PersonEdit {
            id: id.into(),
            given: p.given.clone(),
            surname: p.surname.clone(),
            sex: p.sex,
            birth: event_edit(p.birth.as_ref(), "BIRT"),
            death: event_edit(p.death.as_ref(), "DEAT"),
            note,
        })
    }

    /// Apply only the fields that differ. Ok(false) when nothing changed.
    pub fn apply_person_edit(&mut self, edit: &PersonEdit) -> Result<bool, EditError> {
        let id = edit.id.as_str();
        let cur = self.person_edit(id).ok_or_else(|| EditError::NotFound(id.into()))?;
        let mut changed = false;
        if edit.given.trim() != cur.given.trim() || edit.surname.trim() != cur.surname.trim() {
            self.set_name(id, &edit.given, &edit.surname)?;
            changed = true;
        }
        if edit.sex != cur.sex {
            self.set_sex(id, edit.sex)?;
            changed = true;
        }
        for (new, old, default) in [(&edit.birth, &cur.birth, "BIRT"), (&edit.death, &cur.death, "DEAT")] {
            let tag = if new.tag.trim().is_empty() { default } else { new.tag.trim() };
            if tag != old.tag || new.date.trim() != old.date.trim() || new.place.trim() != old.place.trim() {
                self.set_event(id, tag, &new.date, &new.place)?;
                changed = true;
            }
        }
        if edit.note.trim() != cur.note.trim() {
            self.set_first_note(id, &edit.note)?;
            changed = true;
        }
        Ok(changed)
    }

    /// Like `set_note` but touches only the first inline note, so further notes survive.
    fn set_first_note(&mut self, id: &str, text: &str) -> Result<(), EditError> {
        let rec = self.record_mut(id)?;
        let text = text.trim();
        match rec.children.iter().position(|c| c.tag == "NOTE" && c.pointer().is_none()) {
            Some(ix) if text.is_empty() => {
                rec.children.remove(ix);
            }
            Some(ix) => rec.children[ix].value = text.to_string(),
            None if text.is_empty() => {}
            None => return self.set_note(id, text),
        }
        self.reindex();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../tests/fixtures/sample.ged");

    #[test]
    fn person_edit_reads_fields() {
        let t = Tree::parse(SAMPLE).unwrap();
        let e = t.person_edit("I3").unwrap();
        assert_eq!((e.given.as_str(), e.surname.as_str(), e.sex), ("Ana María", "García López", Sex::Female));
        assert_eq!(e.birth, EventEdit { tag: "BIRT".into(), date: "12 JUN 1950".into(), place: "Madrid".into() });
        assert_eq!(e.death, EventEdit { tag: "DEAT".into(), ..Default::default() });
        assert_eq!(e.note, "Una nota larga que continúa aquí.");
        assert!(t.person_edit("NOPE").is_none());
    }

    #[test]
    fn person_edit_uses_substitute_events() {
        let t = Tree::parse("0 HEAD\n0 @I1@ INDI\n1 NAME A /B/\n1 CHR\n2 DATE 1800\n1 BURI\n2 PLAC Lima\n1 NOTE @N1@\n1 NOTE segunda\n0 @N1@ NOTE ptr\n0 TRLR\n").unwrap();
        let e = t.person_edit("I1").unwrap();
        assert_eq!(e.birth.tag, "CHR");
        assert_eq!(e.birth.date, "1800");
        assert_eq!(e.death, EventEdit { tag: "BURI".into(), date: String::new(), place: "Lima".into() });
        assert_eq!(e.note, "segunda");
    }

    #[test]
    fn apply_only_changes() {
        let mut t = Tree::parse(SAMPLE).unwrap();
        let before = t.to_gedcom();
        let e = t.person_edit("I3").unwrap();
        assert!(!t.apply_person_edit(&e).unwrap());
        assert_eq!(t.to_gedcom(), before, "no-op edit leaves the file untouched");

        let mut e2 = e.clone();
        e2.death = EventEdit { tag: "DEAT".into(), date: "ABT 2020".into(), place: "Toledo".into() };
        e2.note = "Nota nueva".into();
        e2.surname = "García".into();
        assert!(t.apply_person_edit(&e2).unwrap());
        let t2 = Tree::parse(&t.to_gedcom()).unwrap();
        assert_eq!(t2.person_edit("I3").unwrap(), e2);
        assert!(t2.record_text("I3").contains("_UID KEEPME"));

        let missing = PersonEdit { id: "I99".into(), ..Default::default() };
        assert!(t.apply_person_edit(&missing).is_err());
    }

    #[test]
    fn note_edit_keeps_other_notes() {
        let mut t = Tree::parse("0 HEAD\n0 @I1@ INDI\n1 NAME A /B/\n1 NOTE uno\n1 NOTE dos\n0 TRLR\n").unwrap();
        let mut e = t.person_edit("I1").unwrap();
        e.note = "uno bis".into();
        assert!(t.apply_person_edit(&e).unwrap());
        assert_eq!(t.person("I1").unwrap().notes, vec!["uno bis", "dos"]);
    }
}
