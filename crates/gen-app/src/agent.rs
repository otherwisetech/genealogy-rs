//! The AI panel (otherwise-agent-panel), docked next to the tree and toggled with ⌘J: the user's
//! own Claude Code, Codex or opencode with `genealogy-rs mcp` attached for that session only (the
//! user's agent configs are never written).
//!
//! The assistant reads a snapshot of the open tree (unsaved edits included) that this window
//! keeps in `<app dir>/assistant/live/<pid>/tree.ged` while a session exists, and removes on
//! quit. "Solo lectura" offers read tools only; the other modes add `propose_*` tools whose
//! changes land in `…/proposals/` and show here as Aplicar / Rechazar cards. Aplicar is one undo
//! step (⌘Z), like any other edit.
use crate::app::GenApp;
use crate::theme::Tokens;
use genmcp::proposal::{self, Envelope, Invalid, Proposal, Relation};
use gpui::*;
use otherwise_agent_panel::ui::{AgentHost, AgentPanel, PanelStyle};
use otherwise_agent_panel::{McpServer, PendingChange, PermissionMode};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

const INSTRUCTIONS: &str = "This is genealogy-rs, a family-tree (GEDCOM) app. The MCP server \"genealogy\" is the tree open in the app, unsaved edits included. Answer with as few tool calls as possible and prefer the aggregate tools to repeated lookups: most common names → name_stats once (never search_people per guessed name); size, men / women, living, centuries, lifespans, generations → tree_stats; where the family comes from → place_stats; how two people are related → relationship; events in order → timeline; any \"who / how many …\" with conditions → find_people (typed filters, count_by); errors and missing data → data_quality. Use search_people only to get a person's id, then get_person, ancestors or descendants. Counts from these tools are exact: give them as they are. Never invent people, dates or places; say what the tree doesn't know. Changes (a new person, a relationship, a date or place, a name, merging duplicates) are only proposals when the propose_* tools are offered: each becomes a card the user accepts or rejects in the app, so tell them to review the card instead of saying it is done. Write answers in the user's language. This is personal family data: only repeat what the question needs."; // i18n-ok: agent instructions (English)

const POLL: Duration = Duration::from_millis(800);

/// The panel's side of the window: the snapshot the server reads and the cards waiting.
pub struct AgentState {
    pub panel: Option<WeakEntity<AgentPanel>>,
    pending: HashMap<String, Envelope>,
    seq: u32,
    /// `tree.rev` the snapshot was written at; `None` until a session asks for it.
    snapshot_rev: Rc<Cell<Option<u64>>>,
}

impl AgentState {
    pub fn new() -> AgentState {
        clean_stale(&live_root());
        AgentState { panel: None, pending: HashMap::new(), seq: 0, snapshot_rev: Rc::new(Cell::new(None)) }
    }
}

fn live_root() -> PathBuf {
    crate::storage::app_dir().join("assistant").join("live")
}

/// This window's folder: per process, so two copies of the app never read each other's tree.
pub fn live_dir() -> PathBuf {
    live_root().join(std::process::id().to_string())
}

/// Folders left behind by a copy of the app that didn't quit cleanly (a day old or more).
fn clean_stale(root: &Path) {
    let Ok(rd) = std::fs::read_dir(root) else { return };
    let mine = std::process::id().to_string();
    for e in rd.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|m| m.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(24 * 3600));
        if e.file_name().to_string_lossy() != mine && old {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// `genealogy-rs mcp` for a session in `mode`.
pub fn mcp_server(exe: &Path, live: &Path, mode: PermissionMode, focus: Option<&str>, lang: &str) -> McpServer {
    let tree = live.join("tree.ged").display().to_string();
    let props = live.join("proposals").display().to_string();
    let mut args = vec!["mcp", "--tree", tree.as_str(), "--lang", lang]; // i18n-ok: command line
    if let Some(f) = focus {
        args.extend(["--focus", f]); // i18n-ok: flag
    }
    if mode != PermissionMode::ReadOnly {
        args.extend(["--propose", props.as_str()]); // i18n-ok: flag
    }
    McpServer::new(genmcp::NAME, exe, &args)
}

/// Runs `genealogy-rs mcp …` (the panel starts this binary for its sessions). `None`: not an MCP
/// invocation.
pub fn run_mcp(args: &[String]) -> Option<i32> {
    if args.first().map(String::as_str) != Some("mcp") {
        return None;
    }
    Some(match genmcp::Server::from_args(&args[1..]) {
        Ok(mut s) => match s.serve() {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("{e}");
                1
            }
        },
        Err(e) => {
            eprintln!("{e}");
            2
        }
    })
}

/// The panel's privacy note, worded for family data, in the current language.
pub fn apply_labels() {
    otherwise_agent_panel::i18n::set_locale(i18n::locale());
    otherwise_agent_panel::i18n::override_label("panel.privacy.cli", i18n::t!("app.agent.privacy_cli"));
    otherwise_agent_panel::i18n::override_label("panel.privacy.none", i18n::t!("app.agent.privacy_none"));
}

fn relation_label(r: Relation) -> String {
    match r {
        Relation::Parent => i18n::t!("app.agent.rel.parent"),
        Relation::Child => i18n::t!("app.agent.rel.child"),
        Relation::Partner => i18n::t!("app.agent.rel.partner"),
    }
}

fn shown(v: &str) -> String {
    if v.trim().is_empty() { i18n::t!("app.agent.card.empty") } else { v.to_string() }
}

fn date_place(date: &str, place: &str) -> String {
    match (date.trim().is_empty(), place.trim().is_empty()) {
        (false, false) => format!("{date} · {place}"), // i18n-ok: separator
        (false, true) => date.to_string(),
        (true, false) => place.to_string(),
        (true, true) => i18n::t!("app.agent.card.empty"),
    }
}

/// The card's title and detail lines.
pub fn card_text(tree: &gedcom::Tree, env: &Envelope) -> (String, Vec<String>) {
    let name = |id: &str| tree.person(id).map(|p| p.name()).unwrap_or_else(|| id.to_string());
    let mut lines = Vec::new();
    let title = match &env.proposal {
        Proposal::AddPerson { given, surname, birth_date, birth_place, death_date, death_place, relative, relation, .. } => {
            for (tag, d, p) in [("BIRT", birth_date, birth_place), ("DEAT", death_date, death_place)] {
                if d.is_some() || p.is_some() {
                    lines.push(i18n::t!("app.agent.card.fact", label = gedcom::tree::event_label(tag), value = date_place(d.as_deref().unwrap_or(""), p.as_deref().unwrap_or(""))));
                }
            }
            if let Some(r) = relative {
                lines.push(i18n::t!("app.agent.card.relation", relation = relation_label(relation.unwrap_or(Relation::Child)), relative = name(r)));
            }
            i18n::t!("app.agent.card.add_person", name = format!("{given} {surname}").trim())
        }
        Proposal::AddRelationship { person, relative, relation } => i18n::t!("app.agent.card.add_relationship", person = name(person), relation = relation_label(*relation), relative = name(relative)),
        Proposal::SetEvent { person, event, date, place } => {
            let (old_d, old_p) = proposal::current_event(tree, person, event);
            let new_d = date.clone().unwrap_or_else(|| old_d.clone());
            let new_p = place.clone().unwrap_or_else(|| old_p.clone());
            lines.push(i18n::t!("app.agent.card.before", value = date_place(&old_d, &old_p)));
            lines.push(i18n::t!("app.agent.card.after", value = date_place(&new_d, &new_p)));
            i18n::t!("app.agent.card.set_event", event = gedcom::tree::event_label(event), name = name(person))
        }
        Proposal::SetName { person, given, surname } => {
            lines.push(i18n::t!("app.agent.card.after", value = shown(format!("{given} {surname}").trim())));
            i18n::t!("app.agent.card.set_name", name = name(person))
        }
        Proposal::Merge { keep, duplicate } => {
            let life = |id: &str| tree.person(id).map(|p| p.lifespan()).filter(|l| !l.is_empty());
            for id in [keep, duplicate] {
                if let Some(l) = life(id) {
                    lines.push(format!("{} · {l}", name(id))); // i18n-ok: name · years
                }
            }
            i18n::t!("app.agent.card.merge", duplicate = name(duplicate), keep = name(keep))
        }
    };
    if !env.reason.is_empty() {
        lines.push(i18n::t!("app.agent.card.reason", reason = env.reason));
    }
    (title, lines)
}

fn invalid_text(e: &Invalid) -> String {
    match e {
        Invalid::NoPerson(id) => i18n::t!("app.agent.person_gone", id = id),
        Invalid::Edit(e) => e.clone(),
        _ => i18n::t!("app.agent.invalid"),
    }
}

pub struct Host {
    pub app: WeakEntity<GenApp>,
}

impl AgentHost for Host {
    fn app_id(&self) -> String {
        crate::otherwise::APP_ID.into()
    }

    fn app_name(&self) -> String {
        crate::APP_NAME.into()
    }

    fn mcp_servers(&self, mode: PermissionMode, cx: &App) -> Vec<McpServer> {
        let Some(app) = self.app.upgrade() else { return Vec::new() };
        let app = app.read(cx);
        let live = live_dir();
        if let Err(e) = write_snapshot(&live, &app.tree) {
            eprintln!("assistant snapshot: {e}"); // i18n-ok: log only
            return Vec::new();
        }
        app.agent.snapshot_rev.set(Some(app.rev));
        let exe = std::env::current_exe().unwrap_or_default();
        vec![mcp_server(&exe, &live, mode, app.focus_id.as_deref(), i18n::locale())]
    }

    fn context(&self, cx: &App) -> Option<String> {
        Some(self.app.upgrade()?.read(cx).agent_context())
    }

    fn instructions(&self) -> Option<String> {
        Some(INSTRUCTIONS.into())
    }

    fn suggestions(&self) -> Vec<String> {
        // the panel asks outside our updates, but it has no `cx` here: name the person on screen
        // through the state the window last rendered
        let name = FOCUS_NAME.with(|n| n.borrow().clone()).unwrap_or_else(|| i18n::t!("app.agent.this_person"));
        vec![
            i18n::t!("app.agent.suggest_names"),
            i18n::t!("app.agent.suggest_origins"),
            i18n::t!("app.agent.suggest_related", name = name),
            i18n::t!("app.agent.suggest_problems"),
            i18n::t!("app.agent.suggest_great_grandparents", name = name),
            i18n::t!("app.agent.suggest_bio", name = name),
        ]
    }

    fn resolve_change(&self, id: &str, approve: bool, window: &mut Window, cx: &mut App) -> Result<(), String> {
        self.app.update(cx, |a, cx| a.resolve_proposal(id, approve, window, cx)).map_err(|e| e.to_string())?
    }
}

thread_local! {
    static FOCUS_NAME: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn write_snapshot(live: &Path, tree: &gedcom::Tree) -> std::io::Result<()> {
    std::fs::create_dir_all(live.join("proposals"))?;
    gedcom::save(tree, &live.join("tree.ged"))
}

/// The panel in the app's colours.
pub fn style(t: &Tokens) -> PanelStyle {
    let mut note_soft = t.warning;
    note_soft.a = 0.14;
    let mut danger_soft = t.danger;
    danger_soft.a = 0.12;
    PanelStyle {
        background: t.canvas,
        surface: t.surface,
        raised: t.surface,
        border: t.border,
        ink: t.ink,
        muted: t.ink_muted,
        accent: t.accent,
        accent_soft: t.accent_soft,
        danger: t.danger,
        danger_soft,
        ok: t.accent,
        note: t.warning,
        note_soft,
        terminal_bg: if t.dark { rgb(0x161514).into() } else { rgb(0xFFFFFF).into() },
        terminal_fg: t.ink,
        mono_font: "Menlo".into(), // i18n-ok: font name
        dark: t.dark,
    }
}

impl GenApp {
    fn agent_panel(&self) -> Option<Entity<AgentPanel>> {
        self.agent.panel.as_ref().and_then(|p| p.upgrade())
    }

    pub(crate) fn toggle_agent_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.agent_panel() {
            p.update(cx, |p, cx| p.toggle(window, cx));
        }
    }

    /// "Lo que estoy viendo" for the agent (English).
    pub fn agent_context(&self) -> String {
        let tab = match self.tab {
            crate::app::Tab::Pedigree => "Pedigree (ancestors of the person in focus)",
            crate::app::Tab::Descendants => "Descendants of the person in focus",
            crate::app::Tab::Fan => "Fan chart of ancestors",
            crate::app::Tab::List => "List of people",
            crate::app::Tab::Map => "Map of places",
        }; // i18n-ok: context for the agent (English)
        let mut v = serde_json::json!({ "tab": tab, "unsaved_changes": self.dirty });
        if let Some(p) = self.path.as_ref().and_then(|p| p.file_name()) {
            v["file"] = serde_json::json!(p.to_string_lossy());
        }
        if let Some(p) = self.focus_id.as_deref().and_then(|f| self.tree.person(f)) {
            v["person_in_focus"] = serde_json::json!({ "id": p.id, "name": p.name(), "lifespan": p.lifespan() });
        }
        v.to_string()
    }

    /// Per render: what the suggestions name, and the panel's colours.
    pub(crate) fn sync_agent(&self, t: &Tokens, cx: &mut Context<Self>) {
        let name = self.focus_id.as_deref().and_then(|f| self.tree.person(f)).map(|p| p.name());
        FOCUS_NAME.with(|n| {
            if *n.borrow() != name {
                *n.borrow_mut() = name;
            }
        });
        if cx.try_global::<PanelStyle>().is_none_or(|s| s.dark != t.dark) {
            cx.set_global(style(t));
        }
    }

    /// While a session exists: keep its snapshot current and turn delivered proposals into cards.
    pub(crate) fn start_agent_pump(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.agent_tick(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn agent_tick(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.agent.snapshot_rev.get() else { return };
        let live = live_dir();
        if at != self.rev {
            match write_snapshot(&live, &self.tree) {
                Ok(()) => self.agent.snapshot_rev.set(Some(self.rev)),
                Err(e) => eprintln!("assistant snapshot: {e}"), // i18n-ok: log only
            }
        }
        let Some(panel) = self.agent_panel() else { return };
        for env in proposal::collect(&live.join("proposals")) {
            self.agent.seq += 1;
            let id = format!("p{}", self.agent.seq); // i18n-ok: card id
            let (title, lines) = card_text(&self.tree, &env);
            self.agent.pending.insert(id.clone(), env);
            let change = PendingChange::new(id, title, lines.join("\n"));
            panel.update(cx, |p, cx| p.report_pending_change(change, cx));
        }
    }

    /// Aplicar / Rechazar on a card. Aplicar is one undo step.
    pub(crate) fn resolve_proposal(&mut self, id: &str, approve: bool, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(env) = self.agent.pending.remove(id) else {
            return if approve { Err(i18n::t!("app.agent.card_gone")) } else { Ok(()) };
        };
        if !approve {
            return Ok(());
        }
        // checked on a copy first so a refusal shows on the card, not as an edit toast
        let mut probe = (*self.tree).clone();
        let focus = env.proposal.apply(&mut probe).map_err(|e| invalid_text(&e))?;
        self.edit(window, cx, |t| {
            *t = probe;
            Ok(true)
        });
        self.focus_from_outside(focus, window, cx);
        self.show_toast("app.toast.agent_applied", String::new(), true, cx);
        Ok(())
    }

    /// The window is going away: the snapshot is a copy of family data, so it doesn't stay.
    pub(crate) fn remove_agent_files(&self) {
        if self.agent.snapshot_rev.get().is_some() {
            let _ = std::fs::remove_dir_all(live_dir());
        }
    }
}

#[cfg(test)]
mod tests {
    // not `super::*`: gpui's glob brings its own `test` attribute
    use super::{Envelope, PermissionMode, Proposal, Relation, apply_labels, card_text, clean_stale, mcp_server, run_mcp};
    use std::path::Path;

    #[test]
    fn modes_start_the_tree_server_read_only_or_proposing() {
        let exe = Path::new("/Applications/genealogy-rs.app/Contents/MacOS/genealogy-rs");
        let live = Path::new("/data/genealogy/assistant/live/7");
        let ro = mcp_server(exe, live, PermissionMode::ReadOnly, Some("I3"), "es");
        assert_eq!(ro.name, "genealogy");
        assert_eq!(ro.args, ["mcp", "--tree", "/data/genealogy/assistant/live/7/tree.ged", "--lang", "es", "--focus", "I3"]);
        for mode in [PermissionMode::Ask, PermissionMode::Auto] {
            let s = mcp_server(exe, live, mode, None, "en");
            assert_eq!(s.args[s.args.len() - 2..], ["--propose", "/data/genealogy/assistant/live/7/proposals"], "{mode:?}: still only proposals");
        }
        assert!(run_mcp(&["genealogy.ged".into()]).is_none(), "a file argument opens the app");
    }

    #[test]
    fn cards_say_what_changes() {
        let _l = crate::locale_lock();
        i18n::set_locale("es");
        let mut t = gedcom::Tree::parse(include_str!("../../gedcom/tests/fixtures/family.ged")).unwrap();
        t.set_event("I10", "BIRT", "1890", "Toledo").unwrap();
        let env = |p| Envelope { proposal: p, reason: "Según el acta".into() };
        let (title, lines) = card_text(&t, &env(Proposal::SetEvent { person: "I10".into(), event: "BIRT".into(), date: Some("3 APR 1891".into()), place: None }));
        assert!(title.contains("Tomás Ordóñez"), "{title}");
        assert_eq!(lines[0], i18n::t!("app.agent.card.before", value = "1890 · Toledo"));
        assert_eq!(lines[1], i18n::t!("app.agent.card.after", value = "3 APR 1891 · Toledo"));
        assert_eq!(lines[2], i18n::t!("app.agent.card.reason", reason = "Según el acta"));
        let (title, lines) = card_text(&t, &env(Proposal::AddPerson { given: "Pía".into(), surname: "Ordóñez".into(), sex: None, birth_date: Some("1952".into()), birth_place: None, death_date: None, death_place: None, relative: Some("I22".into()), relation: Some(Relation::Child) }));
        assert!(title.contains("Pía Ordóñez"));
        assert!(lines[1].contains("Lucía Ordóñez Pinto"), "{lines:?}");
    }

    #[test]
    fn privacy_note_names_family_data() {
        // other tests read Spanish text, so this stays in the default language
        let _l = crate::locale_lock();
        i18n::set_locale("es");
        apply_labels();
        let note = otherwise_agent_panel::t!("panel.privacy.cli", agent = "Claude Code", vendor = "Anthropic");
        assert!(note.contains("Anthropic") && note.contains("Claude Code") && note.contains("árbol"), "{note}");
        assert!(otherwise_agent_panel::t!("panel.privacy.none").contains("árbol"));
        otherwise_agent_panel::i18n::clear_overrides();
    }

    #[test]
    fn stale_live_folders_go_but_fresh_ones_stay() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("123")).unwrap();
        clean_stale(d.path());
        assert!(d.path().join("123").exists(), "another running copy's folder stays");
    }
}
