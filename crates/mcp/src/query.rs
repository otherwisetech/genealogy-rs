//! Typed person filters and name matching shared by the aggregate tools (`name_stats`,
//! `tree_stats`, `place_stats`), `find_people` and `search_people`. The model only passes typed
//! parameters; nothing here evaluates code or query strings.
use gedcom::{Missing, Person, Sex, Tree, current_year, fold, place_key};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};

/// People with no death record born more than this long ago are counted as deceased.
pub const PRESUMED_DEAD_AFTER: i32 = 110;

/// Little words inside names: «María de los Ángeles» has two given names, «de la Cruz» is one
/// surname.
const PARTICLES: &[&str] = &["de", "del", "la", "las", "los", "le", "les", "lo", "y", "e", "da", "das", "do", "dos", "di", "du", "des", "van", "von", "der", "den", "ter", "ten", "el", "al", "st", "san", "santa"];

fn is_particle(folded: &str) -> bool {
    PARTICLES.contains(&folded)
}

fn clean_word(w: &str) -> &str {
    w.trim_matches(|c: char| !c.is_alphanumeric())
}

/// The given names of «Ana María», «María de los Ángeles», «Juan J.»: Ana + María, María +
/// Ángeles, Juan (particles, initials and nicknames in quotes or brackets are left out).
pub fn given_parts(given: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut skip_until: Option<char> = None;
    for raw in given.split_whitespace() {
        if let Some(end) = skip_until {
            if raw.ends_with(end) {
                skip_until = None;
            }
            continue;
        }
        if let Some(open) = raw.chars().next().filter(|c| matches!(c, '"' | '(' | '\'' | '“' | '«')) {
            let close = match open {
                '(' => ')',
                '“' => '”',
                '«' => '»',
                c => c,
            };
            if !(raw.len() > 1 && raw.ends_with(close)) {
                skip_until = Some(close);
            }
            continue;
        }
        let w = clean_word(raw);
        if w.chars().count() < 2 || is_particle(&fold(w)) {
            continue;
        }
        out.push(w);
    }
    out
}

/// The surnames of «García López» (García, López) and «de la Cruz Pérez» (de la Cruz, Pérez):
/// particles stay with the word after them.
pub fn surname_parts(surname: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    for raw in surname.split_whitespace() {
        let w = clean_word(raw);
        if w.is_empty() {
            continue;
        }
        if is_particle(&fold(w)) {
            pending.push(w);
            continue;
        }
        pending.push(w);
        out.push(pending.join(" "));
        pending.clear();
    }
    out
}

/// Folded unless accents count; whitespace collapsed.
pub fn norm(s: &str, accents: bool) -> String {
    let s = if accents { s.to_lowercase() } else { fold(s) };
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// the whole given name, surname or full name is the query
    Exact,
    /// every query word is a whole word of the name: «Juan» never finds «Juana»
    Word,
    /// every query word starts a word of the name
    Prefix,
    /// every query word appears anywhere in the name
    Substring,
}

impl MatchMode {
    pub fn parse(s: &str) -> Result<MatchMode, String> {
        Ok(match s {
            "exact" => MatchMode::Exact,
            "word" => MatchMode::Word,
            "prefix" => MatchMode::Prefix,
            "substring" => MatchMode::Substring,
            _ => return Err(format!("match must be exact, word, prefix or substring, not {s:?}")), // i18n-ok: for agents
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamePart {
    Full,
    Given,
    Surname,
    /// name, years, places and id (the old search behaviour)
    All,
}

#[derive(Debug, Clone)]
pub struct NameQuery {
    pub text: String,
    tokens: Vec<String>,
    pub mode: MatchMode,
    pub accents: bool,
    pub part: NamePart,
}

impl NameQuery {
    pub fn new(text: &str, mode: MatchMode, accents: bool, part: NamePart) -> NameQuery {
        let text = norm(text, accents);
        let tokens = words(&text).map(str::to_string).collect();
        NameQuery { text, tokens, mode, accents, part }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// One word only (the guard watches for many of these in a row).
    pub fn single_word(&self) -> Option<&str> {
        (self.tokens.len() == 1).then(|| self.tokens[0].as_str())
    }

    pub fn matches(&self, p: &Person) -> bool {
        if self.tokens.is_empty() {
            return true;
        }
        let given = norm(&p.given, self.accents);
        let surname = norm(&p.surname, self.accents);
        let full = format!("{given} {surname}").trim().to_string();
        let hay = match self.part {
            NamePart::Given => given.clone(),
            NamePart::Surname => surname.clone(),
            NamePart::Full => full.clone(),
            // the search key is always folded
            NamePart::All => format!("{full} {}", p.search_key),
        };
        match self.mode {
            MatchMode::Exact => match self.part {
                NamePart::Given => given == self.text,
                NamePart::Surname => surname == self.text,
                _ => full == self.text || given == self.text || surname == self.text,
            },
            MatchMode::Word => self.tokens.iter().all(|t| words(&hay).any(|w| w == t)),
            MatchMode::Prefix => self.tokens.iter().all(|t| words(&hay).any(|w| w.starts_with(t.as_str()))),
            MatchMode::Substring => self.tokens.iter().all(|t| hay.contains(t.as_str())),
        }
    }
}

/// Which events a place filter (or `place_stats`) looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Birth,
    Death,
    Marriage,
    Residence,
    Burial,
    Any,
}

impl EventKind {
    pub fn parse(s: &str) -> Result<EventKind, String> {
        Ok(match s {
            "birth" => EventKind::Birth,
            "death" => EventKind::Death,
            "marriage" => EventKind::Marriage,
            "residence" => EventKind::Residence,
            "burial" => EventKind::Burial,
            "any" => EventKind::Any,
            _ => return Err(format!("event must be birth, death, marriage, residence, burial or any, not {s:?}")), // i18n-ok: for agents
        })
    }

    pub fn id(self) -> &'static str {
        match self {
            EventKind::Birth => "birth",
            EventKind::Death => "death",
            EventKind::Marriage => "marriage",
            EventKind::Residence => "residence",
            EventKind::Burial => "burial",
            EventKind::Any => "any",
        }
    }

    /// The kind an event tag belongs to (for `by_event` breakdowns).
    pub fn of_tag(tag: &str) -> &'static str {
        match tag {
            "BIRT" | "CHR" | "BAPM" => "birth",
            "DEAT" => "death",
            "BURI" | "CREM" => "burial",
            "MARR" | "ENGA" | "MARB" | "MARC" | "MARL" | "MARS" => "marriage",
            "RESI" | "CENS" => "residence",
            _ => "other",
        }
    }
}

/// (event tag, date, place) of every event of `kind` a person took part in, family events
/// (marriages) included. Birth / death use the person's main birth / death record.
pub fn events_of<'a>(tree: &'a Tree, p: &'a Person, kind: EventKind) -> Vec<(&'a str, Option<&'a str>, Option<&'a str>)> {
    let e3 = |e: &'a gedcom::Event| (e.tag.as_str(), e.date.as_deref(), e.place.as_deref());
    let fam_events = || p.fams.iter().filter_map(|f| tree.family(f)).flat_map(|f| f.events.iter());
    match kind {
        EventKind::Birth => p.birth.iter().map(e3).collect(),
        EventKind::Death => p.death.iter().filter(|e| e.tag == "DEAT").map(e3).collect(),
        EventKind::Burial => p.events.iter().filter(|e| matches!(e.tag.as_str(), "BURI" | "CREM")).map(e3).collect(),
        EventKind::Residence => p.events.iter().filter(|e| matches!(e.tag.as_str(), "RESI" | "CENS")).map(e3).collect(),
        EventKind::Marriage => fam_events().filter(|e| EventKind::of_tag(&e.tag) == "marriage").map(e3).collect(),
        EventKind::Any => p.events.iter().chain(fam_events()).map(e3).collect(),
    }
}

/// Whole-word containment over place keys: «Bogotá» matches «Santafé de Bogotá, Colombia».
pub fn place_matches(place: &str, query_key: &str) -> bool {
    let key = place_key(place);
    let q: Vec<&str> = words(query_key).collect();
    !q.is_empty() && q.iter().all(|t| words(&key).any(|w| w == *t))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    BirthDate,
    BirthPlace,
    Death,
    DeathDate,
    DeathPlace,
    Parents,
    Partner,
    Children,
    Sources,
    Notes,
    GivenName,
    Surname,
    Sex,
}

pub const FIELDS: &[&str] = &["birth_date", "birth_place", "death", "death_date", "death_place", "parents", "partner", "children", "sources", "notes", "given_name", "surname", "sex"];

impl Field {
    fn parse(s: &str) -> Result<Field, String> {
        Ok(match s {
            "birth_date" => Field::BirthDate,
            "birth_place" => Field::BirthPlace,
            "death" => Field::Death,
            "death_date" => Field::DeathDate,
            "death_place" => Field::DeathPlace,
            "parents" => Field::Parents,
            "partner" => Field::Partner,
            "children" => Field::Children,
            "sources" => Field::Sources,
            "notes" => Field::Notes,
            "given_name" => Field::GivenName,
            "surname" => Field::Surname,
            "sex" => Field::Sex,
            _ => return Err(format!("unknown field {s:?}; fields: {}", FIELDS.join(", "))), // i18n-ok: for agents
        })
    }

    fn present(self, tree: &Tree, p: &Person) -> bool {
        let filled = |s: Option<&String>| s.is_some_and(|v| !v.trim().is_empty());
        match self {
            Field::BirthDate => filled(p.birth.as_ref().and_then(|e| e.date.as_ref())),
            Field::BirthPlace => filled(p.birth.as_ref().and_then(|e| e.place.as_ref())),
            Field::Death => p.death.is_some(),
            Field::DeathDate => filled(p.death.as_ref().and_then(|e| e.date.as_ref())),
            Field::DeathPlace => filled(p.death.as_ref().and_then(|e| e.place.as_ref())),
            Field::Parents => p.famc.iter().filter_map(|f| tree.family(f)).any(|f| f.partners().any(|x| tree.person(x).is_some())),
            Field::Partner => p.fams.iter().filter_map(|f| tree.family(f)).any(|f| f.other_partner(&p.id).is_some()),
            Field::Children => p.fams.iter().filter_map(|f| tree.family(f)).any(|f| !f.children.is_empty()),
            Field::Sources => !tree.completeness(&p.id).missing.contains(&Missing::Sources),
            Field::Notes => !p.notes.is_empty(),
            Field::GivenName => !p.given.trim().is_empty(),
            Field::Surname => !p.surname.trim().is_empty(),
            Field::Sex => p.sex != Sex::Unknown,
        }
    }
}

/// Deceased (recorded, or presumed for the very old), presumed living, or unknown (no death and
/// no birth year).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    Deceased,
    Living,
    Unknown,
}

pub fn life(p: &Person) -> Life {
    if p.death.is_some() {
        return Life::Deceased;
    }
    match p.birth_year() {
        Some(y) if current_year() - y > PRESUMED_DEAD_AFTER => Life::Deceased,
        Some(_) => Life::Living,
        None => Life::Unknown,
    }
}

pub fn person_or_err<'a>(tree: &'a Tree, id: &str) -> Result<&'a Person, String> {
    tree.person(id).ok_or_else(|| format!("no person with id {id:?}; use search_people to find the id")) // i18n-ok: for agents
}

/// Children of a person over every union.
pub fn children_of<'a>(tree: &'a Tree, id: &str) -> impl Iterator<Item = &'a str> {
    tree.person(id).into_iter().flat_map(|p| p.fams.iter()).filter_map(|f| tree.family(f)).flat_map(|f| f.children.iter()).map(String::as_str)
}

fn parents_of<'a>(tree: &'a Tree, id: &str) -> impl Iterator<Item = &'a str> {
    tree.person(id).into_iter().flat_map(|p| p.famc.iter()).filter_map(|f| tree.family(f)).flat_map(|f| f.partners()).map(String::as_str)
}

/// `start` and everyone reached by `step` within `max` generations (cycle-safe), with depth.
pub fn walk<'a, I: Iterator<Item = &'a str>>(tree: &'a Tree, start: &'a str, max: usize, step: impl Fn(&'a str) -> I) -> HashMap<&'a str, usize> {
    let mut seen = HashMap::from([(start, 0usize)]);
    let mut q = VecDeque::from([start]);
    while let Some(x) = q.pop_front() {
        let d = seen[x];
        if d >= max {
            continue;
        }
        for y in step(x) {
            if tree.person(y).is_some() && !seen.contains_key(y) {
                seen.insert(y, d + 1);
                q.push_back(y);
            }
        }
    }
    seen
}

pub fn descendants<'a>(tree: &'a Tree, id: &'a str, max: usize) -> HashMap<&'a str, usize> {
    walk(tree, id, max, |x| children_of(tree, x))
}

pub fn ancestors<'a>(tree: &'a Tree, id: &'a str, max: usize) -> HashMap<&'a str, usize> {
    walk(tree, id, max, |x| parents_of(tree, x))
}

/// Blood relatives: descendants of every ancestor (the person included).
pub fn blood_relatives<'a>(tree: &'a Tree, id: &'a str) -> HashSet<&'a str> {
    let anc = ancestors(tree, id, usize::MAX);
    let mut out: HashSet<&str> = HashSet::new();
    let mut q: VecDeque<&str> = anc.keys().copied().collect();
    while let Some(x) = q.pop_front() {
        if out.insert(x) {
            q.extend(children_of(tree, x).filter(|c| tree.person(c).is_some() && !out.contains(c)));
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct Filter<'a> {
    pub sex: Option<Sex>,
    pub born: (Option<i32>, Option<i32>),
    pub died: (Option<i32>, Option<i32>),
    pub age: (Option<i32>, Option<i32>),
    pub living: Option<bool>,
    pub place: Option<(String, EventKind)>,
    pub branch: Option<HashSet<&'a str>>,
    pub names: Vec<NameQuery>,
    pub has: Vec<Field>,
    pub missing: Vec<Field>,
}

/// The parameters every filtering tool accepts (merged into each tool's JSON schema).
pub fn filter_schema() -> Value {
    json!({
        "sex": {"type": "string", "enum": ["M", "F", "U"], "description": "M male, F female, U unknown"}, // i18n-ok: for agents
        "born_from": {"type": "integer", "description": "birth year ≥ this (people without a birth year are left out). The 1800s: born_from 1800, born_to 1899"}, // i18n-ok: for agents
        "born_to": {"type": "integer", "description": "birth year ≤ this"}, // i18n-ok: for agents
        "died_from": {"type": "integer", "description": "death year ≥ this"}, // i18n-ok: for agents
        "died_to": {"type": "integer", "description": "death year ≤ this"}, // i18n-ok: for agents
        "age_at_death_min": {"type": "integer", "description": "lived at least this many years (needs birth and death years)"}, // i18n-ok: for agents
        "age_at_death_max": {"type": "integer"},
        "living": {"type": "boolean", "description": "true: presumed living (no death, born < 110 years ago); false: deceased (death recorded or born > 110 years ago)"}, // i18n-ok: for agents
        "place": {"type": "string", "description": "place name, whole words, accent-insensitive: \"Bogotá\" matches \"Santafé de Bogotá, Colombia\""}, // i18n-ok: for agents
        "place_event": {"type": "string", "enum": ["birth", "death", "marriage", "residence", "burial", "any"], "description": "which events `place` looks at (default any)"}, // i18n-ok: for agents
        "descendants_of": {"type": "string", "description": "person id: only that person and their descendants"}, // i18n-ok: for agents
        "ancestors_of": {"type": "string", "description": "person id: only that person and their ancestors"}, // i18n-ok: for agents
        "relatives_of": {"type": "string", "description": "person id: only their blood relatives (descendants of any of their ancestors)"}, // i18n-ok: for agents
        "max_generations": {"type": "integer", "minimum": 1, "description": "with descendants_of / ancestors_of: how many generations"}, // i18n-ok: for agents
        "include_partners": {"type": "boolean", "description": "with a branch filter: also the partners of the people in it"}, // i18n-ok: for agents
        "given": {"type": "string", "description": "given name filter (see `match`)"}, // i18n-ok: for agents
        "surname": {"type": "string", "description": "surname filter (see `match`)"}, // i18n-ok: for agents
        "match": {"type": "string", "enum": ["exact", "word", "prefix", "substring"], "description": "how name filters match (default word: \"Juan\" finds \"Juan José\" but never \"Juana\")"}, // i18n-ok: for agents
        "accent_sensitive": {"type": "boolean", "description": "default false: \"Ines\" finds \"Inés\""}, // i18n-ok: for agents
        "has": {"type": "array", "items": {"type": "string", "enum": FIELDS}, "description": "only people who have all these"}, // i18n-ok: for agents
        "missing": {"type": "array", "items": {"type": "string", "enum": FIELDS}, "description": "only people missing all these"} // i18n-ok: for agents
    })
}

pub fn arg_str<'v>(a: &'v Value, k: &str) -> Option<&'v str> {
    a.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

pub fn arg_i32(a: &Value, k: &str) -> Result<Option<i32>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_i64().map(|n| Some(n as i32)).ok_or_else(|| format!("{k} must be an integer")), // i18n-ok: for agents
    }
}

pub fn arg_bool(a: &Value, k: &str) -> Result<Option<bool>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("{k} must be true or false")), // i18n-ok: for agents
    }
}

fn arg_fields(a: &Value, k: &str) -> Result<Vec<Field>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::String(s)) => Ok(vec![Field::parse(s)?]),
        Some(Value::Array(v)) => v.iter().map(|x| x.as_str().ok_or_else(|| format!("{k} must be a list of field names")).and_then(Field::parse)).collect(), // i18n-ok: for agents
        _ => Err(format!("{k} must be a list of field names")), // i18n-ok: for agents
    }
}

pub fn match_args(a: &Value, default: MatchMode) -> Result<(MatchMode, bool), String> {
    let mode = arg_str(a, "match").map(MatchMode::parse).transpose()?.unwrap_or(default);
    Ok((mode, arg_bool(a, "accent_sensitive")?.unwrap_or(false)))
}

impl<'a> Filter<'a> {
    pub fn parse(tree: &'a Tree, a: &Value) -> Result<Filter<'a>, String> {
        let mut f = Filter {
            sex: match arg_str(a, "sex") {
                None => None,
                Some(s) => Some(match s.to_ascii_uppercase().as_str() {
                    "M" => Sex::Male,
                    "F" => Sex::Female,
                    "U" => Sex::Unknown,
                    _ => return Err("sex must be M, F or U".into()), // i18n-ok: for agents
                }),
            },
            born: (arg_i32(a, "born_from")?, arg_i32(a, "born_to")?),
            died: (arg_i32(a, "died_from")?, arg_i32(a, "died_to")?),
            age: (arg_i32(a, "age_at_death_min")?, arg_i32(a, "age_at_death_max")?),
            living: arg_bool(a, "living")?,
            has: arg_fields(a, "has")?,
            missing: arg_fields(a, "missing")?,
            ..Default::default()
        };
        if let Some(p) = arg_str(a, "place") {
            let kind = arg_str(a, "place_event").map(EventKind::parse).transpose()?.unwrap_or(EventKind::Any);
            f.place = Some((place_key(p), kind));
        }
        let (mode, accents) = match_args(a, MatchMode::Word)?;
        for (k, part) in [("given", NamePart::Given), ("surname", NamePart::Surname)] {
            if let Some(v) = arg_str(a, k) {
                f.names.push(NameQuery::new(v, mode, accents, part));
            }
        }
        let max = a.get("max_generations").and_then(Value::as_u64).map(|n| n as usize).unwrap_or(usize::MAX);
        let mut branch: Option<HashSet<&'a str>> = None;
        let mut narrow = |set: HashSet<&'a str>| {
            branch = Some(match branch.take() {
                Some(b) => b.intersection(&set).copied().collect(),
                None => set,
            });
        };
        for k in ["descendants_of", "ancestors_of", "relatives_of"] {
            let Some(id) = arg_str(a, k) else { continue };
            let id = person_or_err(tree, id)?.id.as_str();
            narrow(match k {
                "descendants_of" => descendants(tree, id, max).into_keys().collect(),
                "ancestors_of" => ancestors(tree, id, max).into_keys().collect(),
                _ => blood_relatives(tree, id),
            });
        }
        if let Some(mut b) = branch {
            if arg_bool(a, "include_partners")?.unwrap_or(false) {
                let partners: Vec<&str> = b.iter().filter_map(|id| tree.person(id)).flat_map(|p| p.fams.iter().filter_map(|f| tree.family(f)).filter_map(|f| f.other_partner(&p.id))).map(String::as_str).collect();
                b.extend(partners);
            }
            f.branch = Some(b);
        }
        Ok(f)
    }

    pub fn is_empty(&self) -> bool {
        self.sex.is_none()
            && self.born == (None, None)
            && self.died == (None, None)
            && self.age == (None, None)
            && self.living.is_none()
            && self.place.is_none()
            && self.branch.is_none()
            && self.names.is_empty()
            && self.has.is_empty()
            && self.missing.is_empty()
    }

    pub fn matches(&self, tree: &Tree, p: &Person) -> bool {
        let in_range = |y: Option<i32>, (lo, hi): (Option<i32>, Option<i32>)| match (lo, hi) {
            (None, None) => true,
            _ => y.is_some_and(|y| lo.is_none_or(|l| y >= l) && hi.is_none_or(|h| y <= h)),
        };
        if self.sex.is_some_and(|s| s != p.sex) {
            return false;
        }
        if self.branch.as_ref().is_some_and(|b| !b.contains(p.id.as_str())) {
            return false;
        }
        if !in_range(p.birth_year(), self.born) || !in_range(p.death_year(), self.died) {
            return false;
        }
        if self.age != (None, None) {
            let age = p.birth_year().zip(p.death_year()).map(|(b, d)| d - b);
            if !in_range(age, self.age) {
                return false;
            }
        }
        if let Some(l) = self.living {
            let state = life(p);
            if (l && state != Life::Living) || (!l && state != Life::Deceased) {
                return false;
            }
        }
        if !self.names.iter().all(|q| q.matches(p)) {
            return false;
        }
        if let Some((q, kind)) = &self.place
            && !events_of(tree, p, *kind).iter().any(|(_, _, pl)| pl.is_some_and(|pl| place_matches(pl, q)))
        {
            return false;
        }
        self.has.iter().all(|f| f.present(tree, p)) && self.missing.iter().all(|f| !f.present(tree, p))
    }

    /// The matching people, in the tree's name order.
    pub fn people(&self, tree: &'a Tree) -> Vec<&'a Person> {
        tree.order.iter().filter_map(|id| tree.person(id)).filter(|p| self.matches(tree, p)).collect()
    }
}

/// Short and flat, for lists: id, name, sex, «born» / «died» as "date, place".
pub fn row(p: &Person) -> Value {
    let mut v = json!({"id": p.id, "name": p.name(), "sex": p.sex.code()});
    let dp = |e: &gedcom::Event| {
        let parts: Vec<&str> = [e.date.as_deref(), e.place.as_deref()].into_iter().flatten().map(str::trim).filter(|s| !s.is_empty()).collect();
        parts.join(", ")
    };
    if let Some(b) = p.birth.as_ref().map(dp).filter(|s| !s.is_empty()) {
        v["born"] = json!(b);
    }
    match p.death.as_ref().map(dp) {
        Some(d) if !d.is_empty() => v["died"] = json!(d),
        Some(_) => v["died"] = json!(true),
        None => {}
    }
    v
}

/// Grouping spellings: most common wins; ties prefer the accented form («María» over «Maria»);
/// ALL-CAPS («GARCÍA») is shown as «García».
#[derive(Default)]
pub struct Spellings(HashMap<String, usize>);

impl Spellings {
    pub fn add(&mut self, s: &str) {
        *self.0.entry(s.to_string()).or_default() += 1;
    }

    pub fn best(&self) -> String {
        let mut by_lower: HashMap<String, (usize, HashMap<&str, usize>)> = HashMap::new();
        for (s, n) in &self.0 {
            let e = by_lower.entry(s.to_lowercase()).or_default();
            e.0 += n;
            *e.1.entry(s.as_str()).or_default() += n;
        }
        let accented = |s: &str| s.chars().filter(|c| !c.is_ascii()).count();
        let Some((_, (_, forms))) = by_lower.iter().max_by(|a, b| a.1.0.cmp(&b.1.0).then(accented(a.0).cmp(&accented(b.0))).then(b.0.cmp(a.0))) else { return String::new() };
        let cased = |s: &str| s.chars().any(|c| c.is_lowercase());
        let best = forms.iter().max_by(|a, b| cased(a.0).cmp(&cased(b.0)).then(a.1.cmp(b.1)).then(b.0.cmp(a.0))).map(|(s, _)| *s).unwrap_or_default();
        if cased(best) { best.to_string() } else { title_case(best) }
    }

    /// Other spellings, when there are any (folded the same, written differently).
    pub fn variants(&self) -> Vec<String> {
        let mut v: Vec<(&String, &usize)> = self.0.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        let best = self.best();
        let mut seen = HashSet::from([best.to_lowercase()]);
        v.into_iter().filter(|(s, _)| seen.insert(s.to_lowercase())).map(|(s, _)| s.clone()).collect()
    }
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let lower = w.to_lowercase();
            if is_particle(&fold(&lower)) {
                return lower;
            }
            let mut c = lower.chars();
            match c.next() {
                Some(f) => f.to_uppercase().chain(c).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_names_split_into_names() {
        assert_eq!(given_parts("Ana María"), ["Ana", "María"]);
        assert_eq!(given_parts("María de los Ángeles"), ["María", "Ángeles"]);
        assert_eq!(given_parts("Juan J."), ["Juan"]);
        assert_eq!(given_parts("José \"Pepe\" Luis"), ["José", "Luis"]);
        assert_eq!(given_parts("Rosa (la menor) Elena"), ["Rosa", "Elena"]);
        assert_eq!(given_parts("Jean-Pierre"), ["Jean-Pierre"]);
    }

    #[test]
    fn surnames_keep_their_particles() {
        assert_eq!(surname_parts("García López"), ["García", "López"]);
        assert_eq!(surname_parts("de la Cruz Pérez"), ["de la Cruz", "Pérez"]);
        assert_eq!(surname_parts("Van Dyke"), ["Van Dyke"]);
    }

    #[test]
    fn spelling_shown_is_the_common_one() {
        let mut s = Spellings::default();
        for w in ["María", "Maria", "María", "MARÍA"] {
            s.add(w);
        }
        assert_eq!(s.best(), "María");
        assert_eq!(s.variants(), ["Maria"]);
        let mut caps = Spellings::default();
        caps.add("GARCÍA");
        assert_eq!(caps.best(), "García");
        let mut tie = Spellings::default();
        tie.add("Ines");
        tie.add("Inés");
        assert_eq!(tie.best(), "Inés");
    }
}
