//! Changes the assistant suggests. The server never edits the tree: it checks a proposal against
//! the tree it reads, drops it in the app's proposals folder, and the app shows it as a card.
//! [`Proposal::apply`] runs only when the user presses Aplicar, as one undo step.
use gedcom::{EditError, Sex, Tree, normalize_date};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How `person` relates to `relative`: `Parent` means `person` is a parent of `relative`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Parent,
    Child,
    Partner,
}

impl Relation {
    pub fn parse(s: &str) -> Option<Relation> {
        match s.trim().to_ascii_lowercase().as_str() {
            "parent" | "father" | "mother" => Some(Relation::Parent),
            "child" | "son" | "daughter" => Some(Relation::Child),
            "partner" | "spouse" | "husband" | "wife" => Some(Relation::Partner),
            _ => None,
        }
    }
}

/// Person events a proposal may date or place.
pub const EVENTS: &[&str] = &["BIRT", "CHR", "BAPM", "DEAT", "BURI", "CREM"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Proposal {
    /// A new person, optionally linked to someone already in the tree.
    AddPerson {
        given: String,
        surname: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sex: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        birth_date: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        birth_place: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        death_date: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        death_place: Option<String>,
        /// The new person is `relation` of `relative`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relative: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relation: Option<Relation>,
    },
    /// Link two people already in the tree: `person` is `relation` of `relative`.
    AddRelationship { person: String, relative: String, relation: Relation },
    /// Set the date and / or place of one event (`None` keeps what is there).
    SetEvent {
        person: String,
        event: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        date: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        place: Option<String>,
    },
    SetName { person: String, given: String, surname: String },
    /// Fold `duplicate` into `keep`.
    Merge { keep: String, duplicate: String },
}

/// A proposal as it travels to the app, with the agent's reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub proposal: Proposal,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub reason: String,
}

/// Why a proposal can't be applied to the tree as it is now.
#[derive(Debug, Clone, PartialEq)]
pub enum Invalid {
    NoPerson(String),
    SamePerson,
    NoName,
    BadEvent(String),
    NothingToSet,
    Edit(String),
}

impl Invalid {
    /// For the agent (English); the app shows its own localized text.
    pub fn for_agent(&self) -> String {
        match self {
            Invalid::NoPerson(id) => format!("no person with id {id:?} in the tree; use search_people to find the id"), // i18n-ok: for agents
            Invalid::SamePerson => "a person can't be related to or merged with themselves".into(), // i18n-ok: for agents
            Invalid::NoName => "a given name or a surname is needed".into(), // i18n-ok: for agents
            Invalid::BadEvent(e) => format!("unsupported event {e:?}; use one of {}", EVENTS.join(", ")), // i18n-ok: for agents
            Invalid::NothingToSet => "give a date, a place or both".into(), // i18n-ok: for agents
            Invalid::Edit(e) => e.clone(),
        }
    }
}

impl From<EditError> for Invalid {
    fn from(e: EditError) -> Invalid {
        Invalid::Edit(e.to_string())
    }
}

fn clean(s: &Option<String>) -> Option<String> {
    s.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

pub fn sex_of(s: Option<&str>) -> Sex {
    match s.map(|s| s.trim().to_ascii_uppercase()) {
        Some(s) if s.starts_with('M') => Sex::Male,
        Some(s) if s.starts_with('F') => Sex::Female,
        _ => Sex::Unknown,
    }
}

impl Proposal {
    /// Dates in GEDCOM form ("12/3/1852" → "12 MAR 1852"), events upper-cased, blanks dropped.
    pub fn normalized(mut self) -> Proposal {
        let date = |d: &mut Option<String>| *d = clean(d).map(|v| normalize_date(&v));
        match &mut self {
            Proposal::AddPerson { given, surname, birth_date, death_date, birth_place, death_place, sex, relative, .. } => {
                *given = given.trim().into();
                *surname = surname.trim().into();
                date(birth_date);
                date(death_date);
                *birth_place = clean(birth_place);
                *death_place = clean(death_place);
                *sex = clean(sex).map(|s| sex_of(Some(&s)).code().to_string());
                *relative = clean(relative);
            }
            Proposal::SetEvent { event, date: d, place, .. } => {
                *event = event.trim().to_ascii_uppercase();
                // here "" means "clear it", unlike a missing value
                *d = d.as_deref().map(|v| if v.trim().is_empty() { String::new() } else { normalize_date(v) });
                *place = place.as_deref().map(|p| p.trim().to_string());
            }
            Proposal::SetName { given, surname, .. } => {
                *given = given.trim().into();
                *surname = surname.trim().into();
            }
            _ => {}
        }
        self
    }

    /// Checks the proposal against `tree` without changing it.
    pub fn validate(&self, tree: &Tree) -> Result<(), Invalid> {
        let has = |id: &str| if tree.person(id).is_some() { Ok(()) } else { Err(Invalid::NoPerson(id.to_string())) };
        match self {
            Proposal::AddPerson { given, surname, relative, .. } => {
                if given.trim().is_empty() && surname.trim().is_empty() {
                    return Err(Invalid::NoName);
                }
                if let Some(r) = relative {
                    has(r)?;
                }
            }
            Proposal::AddRelationship { person, relative, .. } => {
                has(person)?;
                has(relative)?;
                if person == relative {
                    return Err(Invalid::SamePerson);
                }
            }
            Proposal::SetEvent { person, event, date, place } => {
                has(person)?;
                if !EVENTS.contains(&event.as_str()) {
                    return Err(Invalid::BadEvent(event.clone()));
                }
                if date.is_none() && place.is_none() {
                    return Err(Invalid::NothingToSet);
                }
            }
            Proposal::SetName { person, given, surname } => {
                has(person)?;
                if given.trim().is_empty() && surname.trim().is_empty() {
                    return Err(Invalid::NoName);
                }
            }
            Proposal::Merge { keep, duplicate } => {
                has(keep)?;
                has(duplicate)?;
                if keep == duplicate {
                    return Err(Invalid::SamePerson);
                }
                tree.can_merge(keep, duplicate)?;
            }
        }
        Ok(())
    }

    /// Applies it to `tree`; returns the person to focus afterwards (the new one for AddPerson).
    pub fn apply(&self, tree: &mut Tree) -> Result<String, Invalid> {
        self.validate(tree)?;
        match self {
            Proposal::AddPerson { given, surname, sex, birth_date, birth_place, death_date, death_place, relative, relation } => {
                let id = tree.add_person(given, surname, sex_of(sex.as_deref()));
                for (tag, d, p) in [("BIRT", birth_date, birth_place), ("DEAT", death_date, death_place)] {
                    if d.is_some() || p.is_some() {
                        tree.set_event(&id, tag, d.as_deref().unwrap_or(""), p.as_deref().unwrap_or(""))?;
                    }
                }
                if let Some(r) = relative {
                    link(tree, &id, r, relation.unwrap_or(Relation::Child))?;
                }
                Ok(id)
            }
            Proposal::AddRelationship { person, relative, relation } => {
                link(tree, person, relative, *relation)?;
                Ok(person.clone())
            }
            Proposal::SetEvent { person, event, date, place } => {
                let (old_date, old_place) = current_event(tree, person, event);
                let date = date.clone().unwrap_or(old_date);
                let place = place.clone().unwrap_or(old_place);
                tree.set_event(person, event, &date, &place)?;
                Ok(person.clone())
            }
            Proposal::SetName { person, given, surname } => {
                tree.set_name(person, given, surname)?;
                Ok(person.clone())
            }
            Proposal::Merge { keep, duplicate } => {
                tree.merge_persons(keep, duplicate)?;
                Ok(keep.clone())
            }
        }
    }
}

/// The date and place of `person`'s first `tag` event ("" when missing).
pub fn current_event(tree: &Tree, person: &str, tag: &str) -> (String, String) {
    let p = tree.person(person);
    let ev = p.and_then(|p| match tag {
        "BIRT" if p.birth.as_ref().is_some_and(|e| e.tag == "BIRT") => p.birth.clone(),
        "DEAT" if p.death.is_some() => p.death.clone(),
        _ => p.events.iter().find(|e| e.tag == tag).cloned(),
    });
    ev.map(|e| (e.date.unwrap_or_default(), e.place.unwrap_or_default())).unwrap_or_default()
}

/// `person` becomes `relation` of `relative`.
fn link(tree: &mut Tree, person: &str, relative: &str, relation: Relation) -> Result<(), EditError> {
    match relation {
        Relation::Parent => tree.add_parent_link(relative, person),
        Relation::Child => tree.add_child_link(relative, person, None).map(|_| ()),
        Relation::Partner => tree.add_partner_link(person, relative).map(|_| ()),
    }
}

/// Drops proposals into the app's folder: `<n>.json` written next to it first, then renamed, so
/// the app never reads half a file.
pub fn deliver(dir: &Path, env: &Envelope) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let name = format!("{stamp:032}-{}.json", std::process::id());
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, serde_json::to_vec(env).map_err(std::io::Error::other)?)?;
    let path = dir.join(name);
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Takes every delivered proposal out of `dir`, oldest first. Unreadable files are dropped.
pub fn collect(dir: &Path) -> Vec<Envelope> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "json") && !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'))).collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|p| {
            let v = std::fs::read(&p).ok();
            let _ = std::fs::remove_file(&p);
            serde_json::from_slice(&v?).ok()
        })
        .collect()
}
