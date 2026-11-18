//! Whole-person edit used by the edit dialog. CONTRACT STUB — the core agent implements and tests.
use crate::tree::{EditError, Sex, Tree};

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

impl Tree {
    /// Current values of a person as an editable form.
    pub fn person_edit(&self, id: &str) -> Option<PersonEdit> {
        let p = self.person(id)?;
        Some(PersonEdit { id: id.into(), given: p.given.clone(), surname: p.surname.clone(), sex: p.sex, birth: EventEdit { tag: "BIRT".into(), ..Default::default() }, death: EventEdit { tag: "DEAT".into(), ..Default::default() }, note: String::new() })
    }

    /// Apply only the fields that differ. Ok(false) when nothing changed.
    pub fn apply_person_edit(&mut self, _edit: &PersonEdit) -> Result<bool, EditError> {
        Ok(false)
    }
}
