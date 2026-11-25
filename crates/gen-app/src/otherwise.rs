//! otherwise-link: `otherwise-genealogy://` links in, other Otherwise apps in About, people
//! shared with the entity registry (opt-in), birthdays and anniversaries in Otherwise Hub's
//! Today view, and "Track a document in Vigía".
//!
//! What a shared card carries: the name (given / family), and exact birth / death dates. Never
//! notes, places, sources or anything else of the record.
use chrono::{Datelike, NaiveDate};
use gedcom::date::exact_ymd;
use gedcom::tree::{Person, Tree};
use otherwise_link::apps::{self, AppSpec, Installed, Locations};
use otherwise_link::entity::{Entity, Kind};
use otherwise_link::registry::{self, Registry};
use otherwise_link::today::{Event, EventKind, Severity};
use otherwise_link::{Link, Root};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

pub const APP_ID: &str = "genealogy";
#[cfg(target_os = "linux")]
const BUNDLE_ID: &str = "io.github.otherwisetech.genealogy-rs";
/// How far ahead Today looks for birthdays and anniversaries.
pub const AHEAD_DAYS: i64 = 14;
/// Nobody living is older than this; older births without a death are left out of Today.
const MAX_AGE: i32 = 110;

// ---- links ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Focus,
    /// Open a tree.
    Open(PathBuf),
    /// Show a person (`file` opens that tree first if it isn't the one open).
    Person { file: Option<PathBuf>, id: String },
    /// Show the person behind a shared entity.
    Entity(Uuid),
}

fn abs(link: &Link, key: &str) -> Option<PathBuf> {
    link.get(key).map(PathBuf::from).filter(|p| p.is_absolute())
}

/// `open?file=…`, `person?file=…&id=I12`, `entity/<uuid>`; anything else of ours focuses.
pub fn action(link: &Link) -> Option<Action> {
    if link.app != APP_ID {
        return None;
    }
    Some(match (link.at(0), link.at(1)) {
        (Some("open"), _) => abs(link, "file").map(Action::Open).unwrap_or(Action::Focus),
        (Some("person"), _) => match link.get("id").map(|i| i.trim_matches('@')).filter(|i| !i.is_empty()) {
            Some(id) => Action::Person { file: abs(link, "file"), id: id.to_string() },
            None => Action::Focus,
        },
        (Some("entity"), Some(id)) => Uuid::parse_str(id).map(Action::Entity).unwrap_or(Action::Focus),
        _ => Action::Focus,
    })
}

pub fn person_link(file: Option<&Path>, id: &str) -> Link {
    let l = Link::new(APP_ID).segment("person");
    let l = match file {
        Some(f) => l.param("file", f.to_string_lossy()),
        None => l,
    };
    l.param("id", id)
}

/// The URL handed to us: one of our links or a file (argv, Apple events).
pub fn url_action(url: &str) -> Option<Action> {
    if let Ok(l) = Link::parse(url) {
        return action(&l);
    }
    crate::open_files::paths_from_urls(&[url.to_string()]).into_iter().next().map(Action::Open)
}

static INBOUND: Mutex<Vec<Action>> = Mutex::new(Vec::new());

pub fn push(a: Action) {
    INBOUND.lock().unwrap_or_else(|e| e.into_inner()).push(a);
}

pub fn take() -> Vec<Action> {
    std::mem::take(&mut *INBOUND.lock().unwrap_or_else(|e| e.into_inner()))
}

/// Windows and Linux learn about our scheme at run time (macOS from Info.plist).
pub fn register_scheme() {
    std::thread::spawn(|| {
        let scheme = otherwise_link::link::app_scheme(APP_ID);
        #[cfg(windows)]
        if let Ok(exe) = std::env::current_exe() {
            use std::os::windows::process::CommandExt;
            // reg.exe is a console program: without this a window flashes on every start
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            for args in otherwise_link::register::windows_reg_commands(&scheme, &exe, crate::APP_NAME) {
                let _ = std::process::Command::new("reg").args(&args).creation_flags(CREATE_NO_WINDOW).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
            }
        }
        #[cfg(target_os = "linux")]
        {
            let apps = otherwise_link::paths::home().join(".local/share/applications");
            for file in ["genealogy-rs.desktop".to_string(), format!("{BUNDLE_ID}.desktop")] { // i18n-ok: file names
                if apps.join(&file).is_file() {
                    let _ = otherwise_link::register::register_linux(&file, &[&scheme]);
                    break;
                }
            }
        }
        let _ = scheme;
    });
}

// ---- other apps ----

/// Other Otherwise apps installed on this computer with a window to open (not CLI-only), by name.
pub fn other_apps(loc: &Locations) -> Vec<(AppSpec, Installed)> {
    let mut found: Vec<_> = apps::discover(loc).into_iter().filter(|(s, i)| s.id != APP_ID && (i.path.is_some() || i.executable.is_some())).collect();
    found.sort_by_key(|(s, _)| s.name.to_lowercase());
    found
}

/// Starts an installed app: the bundle on macOS, its executable elsewhere.
pub fn launch(app: &Installed) -> std::io::Result<()> {
    if cfg!(target_os = "macos")
        && let Some(p) = &app.path
    {
        return std::process::Command::new("open").arg(p).status().map(|_| ());
    }
    let exe = app.executable.as_ref().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
    std::process::Command::new(exe).spawn().map(|_| ())
}

/// Vigía's "new item" for this person: a passport by default.
pub fn vigia_link(entity: Uuid) -> Link {
    Link::new("vigia").segment("new").param("owner", entity.to_string()).param("template", "passport")
}

// ---- entity cards ----

/// Short stable key for a tree file (its canonical path), so refs from two trees never collide.
pub fn tree_key(path: &Path) -> String {
    let p = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    // FNV-1a: stable across Rust versions, unlike DefaultHasher
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in p.to_string_lossy().as_bytes() {
        h = (h ^ *b as u64).wrapping_mul(0x100_0000_01b3);
    }
    format!("{:012x}", h & 0xffff_ffff_ffff)
}

/// Our id for a person in the registry: the record's `_UID` / `UID` when it has one (it
/// survives exports to other programs), else tree key + xref.
pub fn person_ref(tree: &Tree, key: &str, p: &Person) -> String {
    let uid = tree.record(&p.id).and_then(|r| r.child_value("_UID").or_else(|| r.child_value("UID"))).map(|u| u.trim().to_ascii_uppercase()).filter(|u| !u.is_empty() && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    match uid {
        Some(u) => format!("uid-{u}"),
        None => format!("{key}-{}", p.id.trim_matches('@')),
    }
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD` from an exact GEDCOM date.
pub fn card_date(gedcom: &str) -> Option<String> {
    Some(match exact_ymd(gedcom)? {
        (y, Some(m), Some(d)) => format!("{y:04}-{m:02}-{d:02}"),
        (y, Some(m), None) => format!("{y:04}-{m:02}"),
        (y, _, _) => format!("{y:04}"),
    })
}

/// The shared card for a person: names and exact dates only.
pub fn person_card(tree: &Tree, key: &str, p: &Person) -> Option<Entity> {
    let name = p.name();
    if name.trim().is_empty() {
        return None;
    }
    let mut card = Entity::new(Kind::Person, name.trim()).with_ref(APP_ID, person_ref(tree, key, p));
    let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
    card.person = Some(otherwise_link::entity::Person {
        given_name: opt(&p.given),
        family_name: opt(&p.surname),
        birth_date: p.birth.as_ref().and_then(|e| e.date.as_deref()).and_then(card_date),
        death_date: p.death.as_ref().and_then(|e| e.date.as_deref()).and_then(card_date),
        ..Default::default()
    });
    Some(card)
}

/// What we last pushed, per ref, so unchanged people aren't written again.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncState {
    /// ref → (fingerprint of the card we pushed, shared entity id)
    #[serde(default)]
    pub pushed: BTreeMap<String, (String, String)>,
}

impl SyncState {
    pub fn load(file: &Path) -> SyncState {
        std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        otherwise_link::fsutil::write_atomic(file, serde_json::to_string(self).map_err(std::io::Error::other)?.as_bytes())
    }

    pub fn entity(&self, r: &str) -> Option<Uuid> {
        self.pushed.get(r).and_then(|(_, id)| Uuid::parse_str(id).ok())
    }
}

pub fn state_file() -> Option<PathBuf> {
    crate::settings::dir().map(|d| d.join("otherwise-sync.json"))
}

fn fingerprint(card: &Entity) -> String {
    let p = card.person.clone().unwrap_or_default();
    format!("{}|{:?}|{:?}|{:?}|{:?}", card.name, p.given_name, p.family_name, p.birth_date, p.death_date)
}

/// Pushes every person whose card changed since the last push. Returns how many were written.
pub fn push_tree(reg: &mut Registry, tree: &Tree, key: &str, state: &mut SyncState) -> registry::Result<usize> {
    let mut n = 0;
    for id in &tree.order {
        let Some(p) = tree.person(id) else { continue };
        let Some(card) = person_card(tree, key, p) else { continue };
        let r = person_ref(tree, key, p);
        let fp = fingerprint(&card);
        if state.pushed.get(&r).is_some_and(|(f, _)| *f == fp) {
            continue;
        }
        let up = reg.upsert(APP_ID, card)?;
        state.pushed.insert(r, (fp, up.id.to_string()));
        n += 1;
    }
    Ok(n)
}

/// One person, on an explicit user action (Track in Vigía): its shared id.
pub fn push_person(reg: &mut Registry, tree: &Tree, key: &str, p: &Person, state: &mut SyncState) -> registry::Result<Option<Uuid>> {
    let Some(card) = person_card(tree, key, p) else { return Ok(None) };
    let r = person_ref(tree, key, p);
    let fp = fingerprint(&card);
    let up = reg.upsert(APP_ID, card)?;
    state.pushed.insert(r, (fp, up.id.to_string()));
    Ok(Some(up.id))
}

/// The person in `tree` behind a shared entity (following merges), if it is ours.
pub fn person_for_entity(reg: &Registry, id: Uuid, tree: &Tree, key: &str) -> Option<String> {
    let id = reg.resolve(id).ok()??;
    let card = reg.get(id).ok()??;
    let ours = card.refs.get(APP_ID)?;
    tree.order.iter().find(|pid| tree.person(pid).is_some_and(|p| person_ref(tree, key, p) == *ours)).cloned()
}

/// Opens the shared registry; `None` when the user hasn't turned it on.
pub fn open_shared(root: &Root) -> registry::Result<Option<Registry>> {
    match Registry::open_shared(root) {
        Ok(r) => Ok(Some(r)),
        Err(registry::Error::Disabled) => Ok(None),
        Err(e) => Err(e),
    }
}

// ---- Today ----

fn on_or_after(today: NaiveDate, month: u32, day: u32) -> Option<NaiveDate> {
    // 29 Feb celebrates on 28 Feb in common years
    let at = |y: i32| NaiveDate::from_ymd_opt(y, month, day).or_else(|| (month == 2 && day == 29).then(|| NaiveDate::from_ymd_opt(y, 2, 28)).flatten());
    let this = at(today.year())?;
    if this >= today { Some(this) } else { at(today.year() + 1) }
}

fn full_date(p: Option<&gedcom::tree::Event>) -> Option<(i32, u32, u32)> {
    match exact_ymd(p?.date.as_deref()?)? {
        (y, Some(m), Some(d)) => Some((y, m, d)),
        _ => None,
    }
}

fn alive(p: &Person) -> bool {
    p.death.is_none()
}

/// Birthdays of living people and wedding anniversaries of living couples in the next
/// [`AHEAD_DAYS`] days, in the current language.
pub fn upcoming(tree: &Tree, file: Option<&Path>, key: &str, today: NaiveDate, state: &SyncState) -> Vec<Event> {
    let mut out = Vec::new();
    let horizon = today + chrono::Days::new(AHEAD_DAYS as u64);
    for id in &tree.order {
        let Some(p) = tree.person(id) else { continue };
        let Some((y, m, d)) = full_date(p.birth.as_ref()) else { continue };
        let Some(day) = on_or_after(today, m, d) else { continue };
        let age = day.year() - y;
        if !alive(p) || day > horizon || age <= 0 || age > MAX_AGE {
            continue;
        }
        let mut e = Event::new(format!("birthday-{key}-{}-{}", id.trim_matches('@'), day.year()), EventKind::Due, i18n::t!("app.today.birthday", name = p.name(), age = age)) // i18n-ok: event id
            .when(day.to_string())
            .severity(if day == today { Severity::High } else { Severity::Normal })
            .link(&person_link(file, id))
            .expires((day + chrono::Days::new(1)).to_string());
        e.entity = state.entity(&person_ref(tree, key, p));
        out.push(e);
    }
    let mut fams: Vec<_> = tree.families.values().collect();
    fams.sort_by(|a, b| a.id.cmp(&b.id));
    for f in fams {
        let (Some(a), Some(b)) = (f.husband.as_deref().and_then(|i| tree.person(i)), f.wife.as_deref().and_then(|i| tree.person(i))) else { continue };
        if !alive(a) || !alive(b) || f.events.iter().any(|e| e.tag == "DIV" || e.tag == "ANUL") {
            continue;
        }
        let Some((y, m, d)) = full_date(f.events.iter().find(|e| e.tag == "MARR")) else { continue };
        let Some(day) = on_or_after(today, m, d) else { continue };
        let years = day.year() - y;
        if day > horizon || years <= 0 {
            continue;
        }
        let title = i18n::tn!("app.today.anniversary", years, a = a.name(), b = b.name());
        out.push(
            Event::new(format!("anniversary-{key}-{}-{}", f.id.trim_matches('@'), day.year()), EventKind::Due, title) // i18n-ok: event id
                .when(day.to_string())
                .severity(if day == today { Severity::High } else { Severity::Normal })
                .link(&person_link(file, &a.id))
                .expires((day + chrono::Days::new(1)).to_string()),
        );
    }
    out.sort_by(|x, y| x.when.cmp(&y.when).then(x.id.cmp(&y.id)));
    out
}

/// The same people for Otherwise Hub's Calendar (`<data root>/genealogy/events.json`): one
/// yearly event per living person's exact birthday and per living couple's wedding day. Names
/// only, never notes, places or sources. 29 Feb birthdays fall on the last day of February.
pub fn calendar(tree: &Tree, file: Option<&Path>, key: &str, today: NaiveDate, state: &SyncState) -> Vec<otherwise_link::events::Event> {
    use otherwise_link::events::{Event as Cal, Kind as CalKind};
    let yearly = |m: u32, d: u32| if m == 2 && d == 29 { "FREQ=YEARLY;BYMONTH=2;BYMONTHDAY=-1".to_string() } else { "FREQ=YEARLY".to_string() }; // i18n-ok: RRULE
    let mut out = Vec::new();
    for id in &tree.order {
        let Some(p) = tree.person(id) else { continue };
        let Some((y, m, d)) = full_date(p.birth.as_ref()) else { continue };
        let Some(born) = NaiveDate::from_ymd_opt(y, m, d) else { continue };
        if !alive(p) || today.year() - y > MAX_AGE || born > today {
            continue;
        }
        let mut e = Cal::day(format!("birthday-{key}-{}", id.trim_matches('@')), CalKind::Birthday, i18n::t!("app.cal.birthday", name = p.name()), born).rrule(yearly(m, d)).link(&person_link(file, id)); // i18n-ok: event id
        if let Some(u) = state.entity(&person_ref(tree, key, p)) {
            e = e.entity(u);
        }
        out.push(e);
    }
    let mut fams: Vec<_> = tree.families.values().collect();
    fams.sort_by(|a, b| a.id.cmp(&b.id));
    for f in fams {
        let (Some(a), Some(b)) = (f.husband.as_deref().and_then(|i| tree.person(i)), f.wife.as_deref().and_then(|i| tree.person(i))) else { continue };
        if !alive(a) || !alive(b) || f.events.iter().any(|e| e.tag == "DIV" || e.tag == "ANUL") {
            continue;
        }
        let Some((y, m, d)) = full_date(f.events.iter().find(|e| e.tag == "MARR")) else { continue };
        let Some(wed) = NaiveDate::from_ymd_opt(y, m, d).filter(|w| *w <= today) else { continue };
        out.push(Cal::day(format!("anniversary-{key}-{}", f.id.trim_matches('@')), CalKind::Anniversary, i18n::t!("app.cal.anniversary", a = a.name(), b = b.name()), wed).rrule(yearly(m, d)).link(&person_link(file, &a.id))); // i18n-ok: event id
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use otherwise_link::settings::HubSettings;
    use otherwise_link::today;

    const GED: &str = "0 HEAD\n1 GEDC\n2 VERS 5.5.1\n1 CHAR UTF-8\n\
0 @I1@ INDI\n1 NAME Ana /Prueba/\n1 SEX F\n1 BIRT\n2 DATE 25 NOV 1990\n2 PLAC Cali\n1 NOTE secreto de familia\n1 FAMS @F1@\n\
0 @I2@ INDI\n1 NAME Luis /Ejemplo/\n1 SEX M\n1 _UID abc123\n1 BIRT\n2 DATE ABT 1988\n1 FAMS @F1@\n\
0 @I3@ INDI\n1 NAME Rosa /Prueba/\n1 BIRT\n2 DATE 22 NOV 1920\n1 DEAT\n2 DATE 1999\n\
0 @I4@ INDI\n1 NAME Teo /Prueba/\n1 BIRT\n2 DATE 30 NOV 2010\n\
0 @F1@ FAM\n1 HUSB @I2@\n1 WIFE @I1@\n1 MARR\n2 DATE 1 DEC 2015\n\
0 TRLR\n";

    fn tree() -> Tree {
        Tree::parse(GED).unwrap()
    }

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn links() {
        let f = if cfg!(windows) { PathBuf::from(r"C:\Árbol\familia.ged") } else { PathBuf::from("/Users/ana/Árbol familia.ged") };
        let l = person_link(Some(&f), "I1");
        assert!(l.to_url().starts_with("otherwise://genealogy/person?file="));
        assert_eq!(action(&Link::parse(&l.to_url()).unwrap()), Some(Action::Person { file: Some(f.clone()), id: "I1".into() }));
        assert_eq!(action(&Link::parse(&l.app_url()).unwrap()), Some(Action::Person { file: Some(f), id: "I1".into() }));
        let a = |s: &str| action(&Link::parse(s).unwrap());
        assert_eq!(a("otherwise://genealogy/person?id=%40I7%40"), Some(Action::Person { file: None, id: "I7".into() }));
        assert_eq!(a("otherwise://genealogy/entity/6f0c2c1e-0000-4000-8000-000000000001"), Some(Action::Entity(Uuid::parse_str("6f0c2c1e-0000-4000-8000-000000000001").unwrap())));
        assert_eq!(a("otherwise://genealogy/entity/x"), Some(Action::Focus));
        assert_eq!(a("otherwise://genealogy/person"), Some(Action::Focus));
        assert_eq!(a("otherwise-genealogy://"), Some(Action::Focus));
        assert_eq!(a("otherwise://vigia/item/1"), None);
        assert_eq!(vigia_link(Uuid::nil()).to_url(), "otherwise://vigia/new?owner=00000000-0000-0000-0000-000000000000&template=passport");
        #[cfg(unix)]
        assert_eq!(url_action("file:///tmp/a%20b.ged"), Some(Action::Open(PathBuf::from("/tmp/a b.ged"))));
    }

    #[test]
    fn cards_carry_names_and_exact_dates_only() {
        let t = tree();
        let ana = person_card(&t, "k", t.person("I1").unwrap()).unwrap();
        assert_eq!(ana.name, "Ana Prueba");
        assert_eq!(ana.refs.get(APP_ID).map(String::as_str), Some("k-I1"));
        let p = ana.person.as_ref().unwrap();
        assert_eq!((p.given_name.as_deref(), p.family_name.as_deref(), p.birth_date.as_deref()), (Some("Ana"), Some("Prueba"), Some("1990-11-25")));
        assert!(ana.notes.is_none() && !ana.to_json().contains("secreto") && !ana.to_json().contains("Cali"));
        let luis = person_card(&t, "k", t.person("I2").unwrap()).unwrap();
        assert_eq!(luis.refs.get(APP_ID).map(String::as_str), Some("uid-ABC123"), "the record's _UID wins");
        assert_eq!(luis.person.unwrap().birth_date, None, "ABT dates aren't shared");
        assert_eq!(card_date("MAR 1852").as_deref(), Some("1852-03"));
        assert_eq!(card_date("1852").as_deref(), Some("1852"));
    }

    #[test]
    fn sync_pushes_changes_only_and_finds_people_back() {
        let dir = tempfile::tempdir().unwrap();
        let root = Root::at(dir.path());
        assert!(open_shared(&root).unwrap().is_none(), "off until the user opts in");
        assert!(!root.entities_db().exists());
        HubSettings::update(&root, |s| s.entities = true).unwrap();
        let mut reg = open_shared(&root).unwrap().unwrap();
        let t = tree();
        let mut state = SyncState::default();
        assert_eq!(push_tree(&mut reg, &t, "k", &mut state).unwrap(), 4);
        assert_eq!(push_tree(&mut reg, &t, "k", &mut state).unwrap(), 0, "nothing changed");
        let ana = state.entity("k-I1").unwrap();
        let card = reg.get(ana).unwrap().unwrap();
        assert_eq!(card.kind, Kind::Person);
        assert_eq!(card.person.unwrap().birth_date.as_deref(), Some("1990-11-25"));
        assert_eq!(person_for_entity(&reg, ana, &t, "k").as_deref(), Some("I1"));
        assert_eq!(person_for_entity(&reg, ana, &t, "other-tree"), None);
        // another app's edit to the shared card isn't overwritten by an unchanged person
        let mut edited = reg.get(ana).unwrap().unwrap();
        edited.aliases.push("Anita".into());
        reg.replace("otherwise-apps", edited).unwrap();
        assert_eq!(push_tree(&mut reg, &t, "k", &mut state).unwrap(), 0);
        assert_eq!(reg.get(ana).unwrap().unwrap().aliases, ["Anita"]);
        // a rename is pushed and keeps the same card
        let mut t2 = tree();
        t2.set_name("I1", "Ana María", "Prueba").unwrap();
        assert_eq!(push_tree(&mut reg, &t2, "k", &mut state).unwrap(), 1);
        assert_eq!(state.entity("k-I1"), Some(ana));
        assert_eq!(reg.get(ana).unwrap().unwrap().name, "Ana María Prueba");
        // the state round-trips through its file
        let f = dir.path().join("state.json");
        state.save(&f).unwrap();
        assert_eq!(SyncState::load(&f), state);
        // merged in the registry: the survivor still leads back to our person
        let other = reg.upsert("vigia", Entity::new(Kind::Person, "Ana P.").with_ref("vigia", "owner-1")).unwrap().id;
        reg.merge("otherwise-apps", other, ana).unwrap();
        assert_eq!(person_for_entity(&reg, ana, &t2, "k").as_deref(), Some("I1"));
    }

    #[test]
    fn today_lists_the_next_two_weeks() {
        let t = tree();
        let state = SyncState::default();
        let ev = upcoming(&t, Some(Path::new("/x/f.ged")), "k", day("2026-11-21"), &state);
        let ids: Vec<&str> = ev.iter().map(|e| e.id.as_str()).collect();
        // Rosa (dead) and Luis (ABT date) are out; Ana's birthday, Teo's, the wedding on 1 Dec
        assert_eq!(ids, ["birthday-k-I1-2026", "birthday-k-I4-2026", "anniversary-k-F1-2026"]);
        assert_eq!(ev[0].when.as_deref(), Some("2026-11-25"));
        assert_eq!(ev[0].expires.as_deref(), Some("2026-11-26"));
        assert!(ev[0].title.contains("Ana Prueba") && ev[0].title.contains("36"), "{}", ev[0].title);
        assert!(ev[2].title.contains("11"), "{}", ev[2].title);
        assert_eq!(action(&Link::parse(ev[0].link.as_deref().unwrap()).unwrap()), Some(Action::Person { file: Some(PathBuf::from("/x/f.ged")), id: "I1".into() }));
        // published to a temp shared folder and read back
        let dir = tempfile::tempdir().unwrap();
        let root = Root::at(dir.path());
        today::publish(&root, APP_ID, &ev).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-11-21T12:00:00Z").unwrap().with_timezone(&chrono::Utc);
        assert_eq!(today::read_in(&root, now, &chrono::Utc).events.len(), 3);
        // a year later the same dates are new events
        let next = upcoming(&t, None, "k", day("2027-11-20"), &state);
        assert!(next.iter().any(|e| e.id == "birthday-k-I1-2027"));
        assert!(upcoming(&t, None, "k", day("2026-06-01"), &state).is_empty());
    }

    #[test]
    fn calendar_has_yearly_birthdays_and_anniversaries() {
        let t = tree();
        let today = day("2026-11-20");
        let ev = calendar(&t, Some(Path::new("/x/familia.ged")), "k", today, &SyncState::default());
        let ids: Vec<&str> = ev.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["birthday-k-I1", "birthday-k-I4", "anniversary-k-F1"], "Rosa died, Luis has no exact date");
        assert!(ev.iter().all(|e| e.rrule.as_deref() == Some("FREQ=YEARLY") && e.body.is_none() && e.location.is_none()));
        let dir = tempfile::tempdir().unwrap();
        let root = Root::at(dir.path());
        otherwise_link::events::publish(&root, APP_ID, &ev, None).unwrap();
        let m = otherwise_link::events::read_in(&root, day("2026-11-01"), day("2026-12-31"), &chrono::Utc);
        let got: Vec<String> = m.occurrences.iter().map(|o| o.first_day.to_string()).collect();
        assert_eq!(got, ["2026-11-25", "2026-11-30", "2026-12-01"]);
        assert!(m.occurrences[0].event.link.as_deref().unwrap().starts_with("otherwise://genealogy/person?"));
        let raw = std::fs::read_to_string(dir.path().join("genealogy/events.json")).unwrap();
        assert!(!raw.contains("Cali") && !raw.contains("secreto"));
    }

    #[test]
    fn leap_day_birthdays() {
        assert_eq!(on_or_after(day("2027-02-01"), 2, 29), Some(day("2027-02-28")));
        assert_eq!(on_or_after(day("2028-02-01"), 2, 29), Some(day("2028-02-29")));
        assert_eq!(on_or_after(day("2026-12-30"), 1, 2), Some(day("2027-01-02")));
    }

    #[test]
    fn about_lists_other_installed_apps_only() {
        let dir = tempfile::tempdir().unwrap();
        let opt = dir.path().join("opt");
        for (folder, exe) in [("genealogy-rs", "genealogy-rs"), ("vigia", "vigia-app")] {
            std::fs::create_dir_all(opt.join(folder)).unwrap();
            std::fs::write(opt.join(folder).join(exe), "").unwrap();
        }
        let loc = Locations { os: apps::Os::Linux, mac_dirs: vec![], roots: vec![opt], path: vec![] };
        let ids: Vec<String> = other_apps(&loc).into_iter().map(|(s, _)| s.id).collect();
        assert_eq!(ids, ["vigia"]);
    }
}
