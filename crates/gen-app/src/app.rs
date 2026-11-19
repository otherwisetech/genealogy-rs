use crate::edit_dialog::{EditDialog, EditDialogEvent};
use crate::kinship_dialog::{KinshipDialog, KinshipDialogEvent};
use crate::merge_dialog::{MergeDialog, MergeDialogEvent};
use crate::palette::{Palette, PaletteEvent};
use crate::pedigree::layout::{self, Layout, V2, View, v2};
use crate::settings::Settings;
use crate::theme::{self, SANS, tokens};
use gedcom::{EditError, ImportReport, Sex, Tree};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::Theme;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

actions!(
    gen_app,
    [Open, Save, SaveAs, NewTree, Undo, Redo, OpenPalette, Back, NavLeft, NavRight, NavUp, NavDown, CenterFocus, EditFocus, ZoomIn, ZoomOut, ZoomReset, FitView, CloseRaw, MoreGenerations, FewerGenerations]
);

pub(crate) const KEY_CONTEXT: &str = "GenApp";
pub(crate) const CANVAS_CONTEXT: &str = "GenCanvas";
const RAW_CONTEXT: &str = "GenRaw";
const UNDO_LIMIT: usize = 100;
const ANIM: Duration = Duration::from_millis(200);
pub(crate) const HOVER_DELAY: Duration = Duration::from_millis(300);
const TOAST_FOR: Duration = Duration::from_secs(4);
/// the import summary is longer and suggests a next step
const IMPORT_TOAST_FOR: Duration = Duration::from_secs(10);

pub fn bind_keys(cx: &mut App) {
    let c = Some(CANVAS_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
        KeyBinding::new("cmd-n", NewTree, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-z", Undo, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-k", OpenPalette, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-f", OpenPalette, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-[", Back, Some(KEY_CONTEXT)),
        KeyBinding::new("backspace", Back, c),
        KeyBinding::new("left", NavLeft, c),
        KeyBinding::new("right", NavRight, c),
        KeyBinding::new("up", NavUp, c),
        KeyBinding::new("down", NavDown, c),
        KeyBinding::new("f", CenterFocus, c),
        KeyBinding::new("cmd-enter", EditFocus, c),
        KeyBinding::new("enter", EditFocus, c),
        KeyBinding::new("cmd-=", ZoomIn, c),
        KeyBinding::new("cmd-+", ZoomIn, c),
        KeyBinding::new("cmd--", ZoomOut, c),
        KeyBinding::new("cmd-0", ZoomReset, c),
        KeyBinding::new("cmd-9", FitView, c),
        // ⌘⇧= is ⌘+ (zoom in) on many layouts, so generations get the bare keys
        KeyBinding::new("=", MoreGenerations, c),
        KeyBinding::new("+", MoreGenerations, c),
        KeyBinding::new("-", FewerGenerations, c),
        KeyBinding::new("escape", CloseRaw, Some(RAW_CONTEXT)),
    ]);
}

/// (tree revision, focus, generations, tab)
type LayoutKey = (u64, Option<String>, usize, Tab);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tab {
    Pedigree,
    Descendants,
    Fan,
    List,
    Map,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Rel {
    Father,
    Mother,
    Partner,
    Son,
    Daughter,
    /// child of unknown sex
    Child,
}

pub(crate) struct Toast {
    key: &'static str,
    arg: String,
    at: Instant,
    /// show a «Deshacer» action
    undo: bool,
    /// import summary, rendered (localized) instead of `key`
    report: Option<ImportReport>,
    lasts: Duration,
}

impl Toast {
    /// Built at render time so a language switch while it shows retranslates it.
    fn lines(&self) -> Vec<String> {
        match &self.report {
            Some(r) => {
                let others = r.records() - r.people - r.families - r.sources;
                let counts = if others == 0 {
                    i18n::t!("app.toast.imported", people = i18n::tn!("app.import.people", r.people), families = i18n::tn!("app.import.families", r.families), sources = i18n::tn!("app.import.sources", r.sources))
                } else {
                    i18n::t!(
                        "app.toast.imported_other",
                        people = i18n::tn!("app.import.people", r.people),
                        families = i18n::tn!("app.import.families", r.families),
                        sources = i18n::tn!("app.import.sources", r.sources),
                        other = i18n::tn!("app.import.other", others)
                    )
                };
                vec![counts, i18n::t!("app.toast.import_merge_hint")]
            }
            None => vec![i18n::tr(self.key, &[("arg", &self.arg)])],
        }
    }
}

struct Anim {
    from: View,
    to: View,
    start: Instant,
}

pub(crate) struct Drag {
    pub start: Point<Pixels>,
    pub view: View,
}

pub struct GenApp {
    pub(crate) tree: Rc<Tree>,
    /// bumped on every tree change; part of the layout cache key
    pub(crate) rev: u64,
    pub(crate) path: Option<PathBuf>,
    pub(crate) dirty: bool,
    pub(crate) loading: bool,
    undo: Vec<Rc<Tree>>,
    redo: Vec<Rc<Tree>>,
    pub(crate) focus_id: Option<String>,
    /// breadcrumb trail of previous foci (oldest first)
    pub(crate) trail: Vec<String>,
    pub(crate) tab: Tab,
    pub(crate) generations: usize,
    pub(crate) view: View,
    anim: Option<Anim>,
    pub(crate) needs_center: bool,
    pub(crate) canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    layout_cache: Option<(LayoutKey, Rc<Layout>)>,
    pub(crate) drag: Option<Drag>,
    /// hovered layout node and since when
    pub(crate) hovered: Option<(usize, Instant)>,
    pub(crate) edit_dialog: Option<Entity<EditDialog>>,
    pub(crate) palette: Option<Entity<Palette>>,
    pub(crate) merge_dialog: Option<Entity<MergeDialog>>,
    /// «Revisión» side panel (non-modal, survives `close_overlays`)
    pub(crate) review: Option<Entity<crate::review::ReviewPanel>>,
    pub(crate) review_state: crate::review::ReviewState,
    pub(crate) kinship_dialog: Option<Entity<KinshipDialog>>,
    /// id whose raw GEDCOM record is shown
    pub(crate) raw: Option<String>,
    raw_focus: FocusHandle,
    pub(crate) armed_delete: Option<String>,
    pub(crate) siblings_expanded: bool,
    pub(crate) notes_expanded: bool,
    guard: Option<&'static str>,
    toast: Option<Toast>,
    pub(crate) list_scroll: UniformListScrollHandle,
    pub(crate) sheet_scroll: ScrollHandle,
    pub(crate) map: crate::map::MapState,
    focus: FocusHandle,
    pub(crate) canvas_focus: FocusHandle,
    overlay_subs: Vec<Subscription>,
    _subs: Vec<Subscription>,
}

pub(crate) fn sync_theme(window: Option<&mut Window>, cx: &mut App) {
    Theme::sync_system_appearance(window, cx);
    Theme::global_mut(cx).font_family = SANS.into();
}

impl GenApp {
    pub fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        sync_theme(Some(window), cx);
        let subs = vec![window.observe_window_appearance(|window, cx| sync_theme(Some(window), cx))];
        let canvas_focus = cx.focus_handle();
        canvas_focus.focus(window, cx);
        let settings = Settings::load();
        let mut this = GenApp {
            tree: Rc::new(Tree::new_empty()),
            rev: 0,
            path: None,
            dirty: false,
            loading: false,
            undo: vec![],
            redo: vec![],
            focus_id: None,
            trail: vec![],
            tab: Tab::Pedigree,
            generations: settings.generations.map_or(layout::GEN_DEFAULT, |g| g.clamp(layout::GEN_MIN, layout::GEN_MAX)),
            view: View::default(),
            anim: None,
            needs_center: true,
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            layout_cache: None,
            drag: None,
            hovered: None,
            edit_dialog: None,
            palette: None,
            merge_dialog: None,
            review: None,
            review_state: Default::default(),
            kinship_dialog: None,
            raw: None,
            raw_focus: cx.focus_handle(),
            armed_delete: None,
            siblings_expanded: false,
            notes_expanded: false,
            guard: None,
            toast: None,
            list_scroll: UniformListScrollHandle::new(),
            sheet_scroll: ScrollHandle::new(),
            map: crate::map::MapState::new(&settings),
            focus: cx.focus_handle(),
            canvas_focus,
            overlay_subs: vec![],
            _subs: subs,
        };
        if let Some(p) = initial {
            this.load(p, window, cx);
        }
        this
    }

    // ---- notices ----

    pub(crate) fn toast(&mut self, key: &'static str, arg: impl Into<String>, cx: &mut Context<Self>) {
        self.show_toast(key, arg.into(), false, cx);
    }

    fn show_toast(&mut self, key: &'static str, arg: String, undo: bool, cx: &mut Context<Self>) {
        self.put_toast(Toast { key, arg, at: Instant::now(), undo, report: None, lasts: TOAST_FOR }, cx);
    }

    fn put_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        let lasts = toast.lasts;
        self.toast = Some(toast);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(lasts + Duration::from_millis(50)).await;
            this.update(cx, |this, cx| {
                if this.toast.as_ref().is_some_and(|t| t.at.elapsed() >= t.lasts) {
                    this.toast = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ---- files ----

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        cx.notify();
        let task = cx.background_executor().spawn({
            let path = path.clone();
            async move { gedcom::load(&path) }
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |this, window, cx| {
                this.loading = false;
                match res {
                    Ok(Ok(tree)) => {
                        let first = tree.order.first().cloned();
                        this.reset_tree(tree, Some(path.clone()));
                        if let Some(first) = first {
                            this.set_focus(first, false, window, cx);
                        }
                        this.update_title(window);
                    }
                    Ok(Err(e)) => this.toast("app.toast.invalid_file", e.to_string(), cx),
                    Err(e) => this.toast("app.toast.read_failed", format!("{}: {}", path.display(), io_error(&e)), cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn reset_tree(&mut self, tree: Tree, path: Option<PathBuf>) {
        self.tree = Rc::new(tree);
        self.rev += 1;
        self.path = path;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.trail.clear();
        self.focus_id = None;
        self.armed_delete = None;
        self.needs_center = true;
        self.review_state.forget_ignored();
        self.map.reset();
    }

    fn window_title(&self) -> String {
        let name = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.tree.title());
        format!("{name}{} — {}", if self.dirty { " •" } else { "" }, crate::APP_NAME)
    }

    fn update_title(&self, window: &mut Window) {
        window.set_window_title(&self.window_title());
    }

    /// Two-step confirmation before discarding unsaved changes.
    fn guard_unsaved(&mut self, what: &'static str, cx: &mut Context<Self>) -> bool {
        if !self.dirty || self.guard == Some(what) {
            self.guard = None;
            return true;
        }
        self.guard = Some(what);
        self.toast("app.toast.unsaved_repeat", "", cx);
        false
    }

    pub(crate) fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guard_unsaved("open", cx) {
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(i18n::t!("app.file.open_prompt").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.load(p, window, cx)).ok();
            }
        })
        .detach();
    }

    pub(crate) fn new_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guard_unsaved("new", cx) {
            return;
        }
        let mut tree = Tree::new_empty();
        let id = tree.add_person("", "", Sex::Unknown);
        self.reset_tree(tree, None);
        self.set_focus(id.clone(), false, window, cx);
        self.update_title(window);
        self.open_edit(id, window, cx);
    }

    pub(crate) fn save(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        match (&self.path, save_as) {
            (Some(p), false) => {
                let p = p.clone();
                self.write_to(p, window, cx);
            }
            _ => {
                let dir = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
                let name = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| format!("{}.ged", self.tree.title()));
                let rx = cx.prompt_for_new_path(&dir, Some(&name));
                cx.spawn_in(window, async move |this, cx| {
                    if let Ok(Ok(Some(p))) = rx.await {
                        this.update_in(cx, |this, window, cx| this.write_to(p, window, cx)).ok();
                    }
                })
                .detach();
            }
        }
    }

    /// Save a GEDCOM 7.0 copy; the open file, its path and the dirty flag stay as they are.
    pub(crate) fn export_gedcom7(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
        let stem = self.path.as_deref().and_then(Path::file_stem).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| self.tree.title());
        let rx = cx.prompt_for_new_path(&dir, Some(&format!("{stem}-gedcom7.ged")));
        let tree = self.tree.clone();
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(p))) = rx.await {
                let res = gedcom::save_gedcom7(&tree, &p);
                this.update(cx, |this, cx| match res {
                    Ok(()) => this.toast("app.toast.exported7", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), cx),
                    Err(e) => this.toast("app.toast.save_failed", io_error(&e), cx),
                })
                .ok();
            }
        })
        .detach();
    }

    /// Pick another GEDCOM and append all its records to the open tree as one undo step.
    pub(crate) fn import_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(i18n::t!("app.file.import_prompt").into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let res = cx.background_executor().spawn({
                let path = path.clone();
                async move { gedcom::load(&path) }
            });
            let res = res.await;
            this.update_in(cx, |this, window, cx| match res {
                Ok(Ok(other)) => this.import_tree(other, window, cx),
                Ok(Err(e)) => this.toast("app.toast.invalid_file", e.to_string(), cx),
                Err(e) => this.toast("app.toast.read_failed", format!("{}: {}", path.display(), io_error(&e)), cx),
            })
            .ok();
        })
        .detach();
    }

    fn import_tree(&mut self, other: Tree, window: &mut Window, cx: &mut Context<Self>) {
        let mut report = ImportReport::default();
        self.edit(window, cx, |t| {
            report = t.import(&other);
            Ok(report.records() > 0)
        });
        if report.records() == 0 {
            self.toast("app.toast.import_empty", "", cx);
            return;
        }
        if self.focus_id.is_none()
            && let Some(first) = report.first_person.clone()
        {
            self.set_focus(first, false, window, cx);
        }
        self.put_toast(Toast { key: "app.toast.imported", arg: String::new(), at: Instant::now(), undo: false, report: Some(report), lasts: IMPORT_TOAST_FOR }, cx);
    }

    fn write_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match gedcom::save(&self.tree, &path) {
            Ok(()) => {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.path = Some(path);
                self.dirty = false;
                self.update_title(window);
                self.toast("app.toast.saved", name, cx);
            }
            Err(e) => self.toast("app.toast.save_failed", io_error(&e), cx),
        }
        cx.notify();
    }

    // ---- edits ----

    /// Snapshot for undo, run the edit on a copy-on-write tree, mark dirty. `Ok(false)` from the
    /// closure means "nothing changed" and leaves no undo step.
    pub(crate) fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Tree) -> Result<bool, EditError>) -> bool {
        let before = self.tree.clone();
        let res = f(Rc::make_mut(&mut self.tree));
        let changed = match res {
            Ok(true) => {
                self.undo.push(before);
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                self.rev += 1;
                self.guard = None;
                true
            }
            Ok(false) => {
                self.tree = before;
                false
            }
            Err(e) => {
                self.tree = before;
                self.toast("app.toast.edit_failed", e.to_string(), cx);
                false
            }
        };
        self.update_title(window);
        cx.notify();
        changed
    }

    pub(crate) fn can_undo(&self, redo: bool) -> bool {
        if redo { !self.redo.is_empty() } else { !self.undo.is_empty() }
    }

    pub(crate) fn undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_dialog.is_some() || self.palette.is_some() || self.merge_dialog.is_some() || self.kinship_dialog.is_some() {
            return;
        }
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(prev) = from.pop() else { return };
        to.push(std::mem::replace(&mut self.tree, prev));
        self.rev += 1;
        self.dirty = true;
        self.armed_delete = None;
        if self.focus_id.as_deref().is_none_or(|s| self.tree.person(s).is_none()) {
            self.focus_id = None;
            if let Some(f) = self.fallback_focus(None) {
                self.set_focus(f, false, window, cx);
            }
        }
        self.trail.retain(|t| self.tree.person(t).is_some());
        self.toast(if redo { "app.toast.redone" } else { "app.toast.undone" }, "", cx);
        self.update_title(window);
        cx.notify();
    }

    fn fallback_focus(&self, gone: Option<&str>) -> Option<String> {
        self.trail
            .iter()
            .rev()
            .find(|b| Some(b.as_str()) != gone && self.tree.person(b).is_some())
            .cloned()
            .or_else(|| gone.and_then(|g| self.tree.parents(g).first().map(|p| p.id.clone())))
            .or_else(|| self.tree.order.iter().find(|o| Some(o.as_str()) != gone).cloned())
    }

    /// Create a relative of `of` and link it; returns the new id. Focus stays where it is.
    pub(crate) fn add_relative(&mut self, of: &str, rel: Rel, family: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Option<String> {
        let me = self.tree.person(of)?.clone();
        let (sex, surname) = match rel {
            Rel::Father => (Sex::Male, String::new()),
            Rel::Mother => (Sex::Female, String::new()),
            Rel::Partner => (
                match me.sex {
                    Sex::Male => Sex::Female,
                    Sex::Female => Sex::Male,
                    Sex::Unknown => Sex::Unknown,
                },
                String::new(),
            ),
            Rel::Son => (Sex::Male, me.surname.clone()),
            Rel::Daughter => (Sex::Female, me.surname.clone()),
            Rel::Child => (Sex::Unknown, me.surname.clone()),
        };
        let mut new_id = String::new();
        let id = of.to_string();
        let ok = self.edit(window, cx, |t| {
            new_id = t.add_person("", &surname, sex);
            match rel {
                Rel::Father | Rel::Mother => t.add_parent_link(&id, &new_id)?,
                Rel::Partner => {
                    t.add_partner_link(&id, &new_id)?;
                }
                Rel::Son | Rel::Daughter | Rel::Child => {
                    t.add_child_link(&id, &new_id, family.as_deref())?;
                }
            }
            Ok(true)
        });
        ok.then_some(new_id)
    }

    /// Add a relative and open the dialog on it.
    pub(crate) fn add_and_edit(&mut self, of: &str, rel: Rel, family: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.add_relative(of, rel, family, window, cx) {
            self.open_edit(id, window, cx);
        }
    }

    /// «+ Persona»: unnamed, unlinked person that becomes the focus.
    pub(crate) fn create_person(&mut self, given: &str, surname: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut id = String::new();
        if self.edit(window, cx, |t| {
            id = t.add_person(given, surname, Sex::Unknown);
            Ok(true)
        }) {
            self.set_focus(id.clone(), true, window, cx);
            self.open_edit(id, window, cx);
        }
    }

    pub(crate) fn delete_person(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let next = self.fallback_focus(Some(&id));
        if !self.edit(window, cx, |t| t.delete_person(&id).map(|_| true)) {
            return;
        }
        self.armed_delete = None;
        self.trail.retain(|b| *b != id);
        if self.focus_id.as_deref() == Some(id.as_str()) {
            self.focus_id = None;
            if let Some(n) = next.filter(|n| self.tree.person(n).is_some()) {
                self.set_focus(n, false, window, cx);
            }
        }
        self.toast("app.toast.deleted", "", cx);
    }

    /// Fold `dup` into `keep` as one undo step, then focus `keep`.
    pub(crate) fn merge_persons(&mut self, keep: String, dup: String, window: &mut Window, cx: &mut Context<Self>) {
        if !self.edit(window, cx, |t| t.merge_persons(&keep, &dup).map(|_| true)) {
            return;
        }
        self.armed_delete = None;
        self.trail.retain(|b| *b != dup);
        if self.focus_id.as_deref() == Some(dup.as_str()) {
            self.focus_id = None;
        }
        self.focus_from_outside(keep, window, cx);
        self.show_toast("app.toast.merged", String::new(), true, cx);
    }

    // ---- focus & navigation ----

    pub(crate) fn set_focus(&mut self, id: String, push_history: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.person(&id).is_none() {
            return;
        }
        if self.focus_id.as_deref() == Some(id.as_str()) {
            self.recenter(true, window);
            return;
        }
        // keep the clicked card where it was, then glide it to the centre
        let old_screen = self.layout().index_of(&id).map(|i| {
            let n = &self.layout().nodes[i];
            self.view.to_screen(v2(n.pos.x + n.w / 2., n.pos.y + n.h / 2.))
        });
        if push_history && let Some(cur) = self.focus_id.take() {
            self.trail.retain(|t| *t != cur);
            self.trail.push(cur);
            if self.trail.len() > 50 {
                self.trail.remove(0);
            }
        }
        self.trail.retain(|t| *t != id);
        self.focus_id = Some(id.clone());
        self.armed_delete = None;
        self.siblings_expanded = false;
        self.notes_expanded = false;
        self.hovered = None;
        if let Some(ix) = self.tree.order.iter().position(|o| *o == id) {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
        if let Some(s) = old_screen {
            let z = self.view.zoom;
            let w = focus_center();
            self.view.pan = v2(s.x - w.x * z, s.y - w.y * z);
            self.recenter(true, window);
        } else {
            self.recenter(false, window);
        }
        cx.notify();
    }

    pub(crate) fn canvas_size(&self) -> V2 {
        let b = self.canvas_bounds.get();
        v2(b.size.width.into(), b.size.height.into())
    }

    pub(crate) fn recenter(&mut self, animate: bool, window: &mut Window) {
        let size = self.canvas_size();
        if size.x <= 0. {
            self.needs_center = true;
            return;
        }
        let target = self.view.centered_on(focus_center(), size);
        self.animate_to(target, animate, window);
    }

    pub(crate) fn animate_to(&mut self, target: View, animate: bool, window: &mut Window) {
        if animate {
            self.anim = Some(Anim { from: self.view, to: target, start: Instant::now() });
            window.refresh();
        } else {
            self.anim = None;
            self.view = target;
        }
    }

    pub(crate) fn fit_view(&mut self, window: &mut Window) {
        let size = self.canvas_size();
        let b = self.layout().bounds();
        self.animate_to(View::fit(b, size, 48.), true, window);
    }

    pub(crate) fn zoom_by(&mut self, factor: f32, anchor: Option<V2>, window: &mut Window) {
        let size = self.canvas_size();
        self.anim = None;
        let a = anchor.unwrap_or(v2(size.x / 2., size.y / 2.));
        let z = self.view.zoom * factor;
        self.view.zoom_around(a, z);
        window.refresh();
    }

    pub(crate) fn stop_anim(&mut self) {
        self.anim = None;
    }

    /// Card click: focus now, but hold the recenter for a double-click's worth of time so a
    /// second click still lands on the same card.
    pub(crate) fn focus_by_click(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_focus(id, true, window, cx);
        if let Some(a) = &mut self.anim {
            a.start += Duration::from_millis(250);
        }
    }

    /// Advance the recenter animation; called from render.
    pub(crate) fn tick(&mut self, window: &mut Window) {
        if self.needs_center && self.canvas_size().x > 0. {
            self.needs_center = false;
            self.recenter(false, window);
        } else if self.needs_center {
            window.request_animation_frame();
        }
        if let Some(a) = &self.anim {
            let t = Instant::now().saturating_duration_since(a.start).as_secs_f32() / ANIM.as_secs_f32();
            self.view = View::lerp(a.from, a.to, layout::ease_out_cubic(t));
            if t >= 1. {
                self.anim = None;
            } else {
                window.request_animation_frame();
            }
        }
    }

    /// Generations stepper (canvas pill, `+` / `−` keys); the choice is remembered.
    pub(crate) fn step_generations(&mut self, up: bool, window: &mut Window, cx: &mut Context<Self>) {
        let g = layout::step_generations(self.generations, up);
        if g == self.generations {
            return;
        }
        self.generations = g;
        self.hovered = None;
        let mut s = Settings::load();
        s.generations = Some(g);
        s.save();
        self.recenter(true, window);
        cx.notify();
    }

    pub(crate) fn layout(&mut self) -> Rc<Layout> {
        let key = (self.rev, self.focus_id.clone(), self.generations, self.tab);
        if let Some((k, l)) = &self.layout_cache
            && *k == key
        {
            return l.clone();
        }
        let l = Rc::new(match (&self.focus_id, self.tab) {
            (Some(f), Tab::Descendants) => layout::descendants(&self.tree, f, self.generations),
            (Some(f), _) => layout::pedigree(&self.tree, f, self.generations),
            (None, _) => Layout::default(),
        });
        self.layout_cache = Some((key, l.clone()));
        l
    }

    pub(crate) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(prev) = self.trail.pop() {
            if self.tree.person(&prev).is_some() {
                self.set_focus(prev, false, window, cx);
                return;
            }
        }
    }

    /// Breadcrumb click: focus `ix` of the trail and drop what came after it.
    pub(crate) fn jump_trail(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.trail.len() {
            return;
        }
        let id = self.trail[ix].clone();
        self.trail.truncate(ix);
        self.focus_id = None;
        self.set_focus(id, false, window, cx);
    }

    /// Arrow keys. Pedigrí (ancestors to the right): → father (else mother), ← back to a child,
    /// ↑/↓ switch between the two parents of that child, otherwise between siblings.
    /// Descendientes: → first child, ← a parent, ↑/↓ siblings.
    fn navigate(&mut self, dir: (i32, i32), window: &mut Window, cx: &mut Context<Self>) {
        let Some(cur) = self.focus_id.clone() else { return };
        let t = self.tree.clone();
        let came_from = self.trail.last().cloned();
        let children: Vec<String> = t.unions(&cur).into_iter().flat_map(|(_, _, k)| k.into_iter().map(|p| p.id.clone())).collect();
        let (father, mother) = t.father_mother(&cur);
        let (father, mother) = (father.map(|p| p.id.clone()), mother.map(|p| p.id.clone()));
        let child = came_from.clone().filter(|c| children.contains(c)).or_else(|| children.first().cloned());
        let parent = came_from.clone().filter(|c| Some(c) == father.as_ref() || Some(c) == mother.as_ref()).or_else(|| father.clone()).or_else(|| mother.clone());
        let siblings = || {
            let fam = t.person(&cur).and_then(|p| p.famc.first()).and_then(|f| t.family(f));
            fam.map(|f| f.children.clone()).unwrap_or_default()
        };
        let step = |list: &[String], d: i32| -> Option<String> {
            let i = list.iter().position(|x| *x == cur)? as i32 + d;
            list.get(usize::try_from(i).ok()?).cloned()
        };
        let pedigree = self.tab != Tab::Descendants;
        let target = match dir {
            (1, 0) if pedigree => father.or(mother),
            (1, 0) => children.first().cloned(),
            (-1, 0) if pedigree => child,
            (-1, 0) => parent,
            (0, d) => {
                let couple = child.as_deref().map(|c| t.father_mother(c)).map(|(f, m)| [f, m].into_iter().flatten().map(|p| p.id.clone()).collect::<Vec<_>>());
                match couple.filter(|c| pedigree && c.len() == 2 && c.contains(&cur)) {
                    Some(c) => step(&c, d),
                    None => step(&siblings(), d),
                }
            }
            _ => None,
        };
        if let Some(t) = target {
            // returning to the child we came from pops it rather than growing the trail
            if came_from.as_deref() == Some(t.as_str()) {
                self.go_back(window, cx);
            } else {
                self.set_focus(t, true, window, cx);
            }
        }
    }

    // ---- overlays ----

    pub(crate) fn open_edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.person(&id).is_none() {
            return;
        }
        self.palette = None;
        let tree = self.tree.clone();
        let dialog = cx.new(|cx| EditDialog::new(tree.clone(), &id, window, cx));
        self.overlay_subs = vec![cx.subscribe_in(&dialog, window, |this, _, ev: &EditDialogEvent, window, cx| match ev {
            EditDialogEvent::Save(edit) => {
                let edit = edit.clone();
                this.edit(window, cx, |t| t.apply_person_edit(&edit));
                this.close_overlays(window, cx);
            }
            EditDialogEvent::Cancel => this.close_overlays(window, cx),
            EditDialogEvent::Delete(id) => {
                let id = id.clone();
                this.close_overlays(window, cx);
                this.delete_person(id, window, cx);
            }
            EditDialogEvent::ViewGedcom(id) => this.show_raw(id.clone(), window, cx),
        })];
        dialog.update(cx, |d, cx| d.focus(window, cx));
        self.edit_dialog = Some(dialog);
        cx.notify();
    }

    /// «Unir con otra persona…»: the palette in picker mode, then the confirmation dialog.
    pub(crate) fn open_merge_picker(&mut self, source: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.person(&source).is_none() {
            return;
        }
        self.close_overlays(window, cx);
        let palette = cx.new(|cx| Palette::for_merge(self.tree.clone(), source.clone(), window, cx));
        self.overlay_subs = vec![cx.subscribe_in(&palette, window, move |this, _, ev: &PaletteEvent, window, cx| match ev {
            PaletteEvent::MergeWith(other) => {
                let other = other.clone();
                this.open_merge_confirm(source.clone(), other, window, cx);
            }
            _ => this.close_overlays(window, cx),
        })];
        palette.update(cx, |p, cx| p.focus(window, cx));
        self.palette = Some(palette);
        cx.notify();
    }

    pub(crate) fn open_merge_confirm(&mut self, keep: String, dup: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlays(window, cx);
        let dialog = cx.new(|cx| MergeDialog::new(self.tree.clone(), keep, dup, cx));
        self.overlay_subs = vec![cx.subscribe_in(&dialog, window, |this, _, ev: &MergeDialogEvent, window, cx| match ev {
            MergeDialogEvent::Confirm { keep, dup } => {
                let (keep, dup) = (keep.clone(), dup.clone());
                this.close_overlays(window, cx);
                this.merge_persons(keep, dup, window, cx);
            }
            MergeDialogEvent::Cancel => this.close_overlays(window, cx),
        })];
        dialog.update(cx, |d, cx| d.focus(window, cx));
        self.merge_dialog = Some(dialog);
        cx.notify();
    }

    /// «Calcular parentesco con…»: the palette picks the other person, then the result dialog.
    pub(crate) fn open_kinship_picker(&mut self, source: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.tree.person(&source).is_none() {
            return;
        }
        self.close_overlays(window, cx);
        let palette = cx.new(|cx| Palette::for_kinship(self.tree.clone(), source.clone(), window, cx));
        self.overlay_subs = vec![cx.subscribe_in(&palette, window, move |this, _, ev: &PaletteEvent, window, cx| match ev {
            PaletteEvent::KinshipWith(other) => {
                let other = other.clone();
                this.open_kinship(source.clone(), other, window, cx);
            }
            _ => this.close_overlays(window, cx),
        })];
        palette.update(cx, |p, cx| p.focus(window, cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Result dialog; it reads «`a` es … de `b`», so the ficha person is the subject.
    pub(crate) fn open_kinship(&mut self, a: String, b: String, window: &mut Window, cx: &mut Context<Self>) {
        self.close_overlays(window, cx);
        let dialog = cx.new(|cx| KinshipDialog::new(self.tree.clone(), b, a, cx));
        self.overlay_subs = vec![cx.subscribe_in(&dialog, window, |this, _, ev: &KinshipDialogEvent, window, cx| match ev {
            KinshipDialogEvent::Focus(id) => {
                let id = id.clone();
                this.close_overlays(window, cx);
                this.focus_from_outside(id, window, cx);
            }
            KinshipDialogEvent::Close => this.close_overlays(window, cx),
        })];
        dialog.update(cx, |d, cx| d.focus(window, cx));
        self.kinship_dialog = Some(dialog);
        cx.notify();
    }

    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_dialog.is_some() || self.merge_dialog.is_some() || self.kinship_dialog.is_some() {
            return;
        }
        if let Some(p) = &self.palette {
            p.update(cx, |p, cx| p.focus(window, cx));
            return;
        }
        let tree = self.tree.clone();
        let focus = self.focus_id.clone();
        let palette = cx.new(|cx| Palette::new(tree, focus, window, cx));
        self.overlay_subs = vec![cx.subscribe_in(&palette, window, |this, _, ev: &PaletteEvent, window, cx| match ev {
            PaletteEvent::Focus(id) => {
                let id = id.clone();
                this.close_overlays(window, cx);
                this.focus_from_outside(id, window, cx);
            }
            PaletteEvent::Edit(id) => {
                let id = id.clone();
                this.close_overlays(window, cx);
                this.focus_from_outside(id.clone(), window, cx);
                this.open_edit(id, window, cx);
            }
            PaletteEvent::Place(place) => {
                let place = place.clone();
                this.close_overlays(window, cx);
                if this.tab == Tab::Map {
                    this.map_show_place(&place, cx);
                } else if let Some(id) = first_at_place(&this.tree, &place) {
                    this.focus_from_outside(id, window, cx);
                }
            }
            PaletteEvent::CreatePerson(q) => {
                let (given, surname) = split_name(q);
                this.close_overlays(window, cx);
                this.create_person(&given, &surname, window, cx);
            }
            PaletteEvent::MergeWith(_) | PaletteEvent::KinshipWith(_) => {}
            PaletteEvent::Kinship => {
                if let Some(id) = this.focus_id.clone() {
                    this.open_kinship_picker(id, window, cx);
                }
            }
            PaletteEvent::Dismiss => this.close_overlays(window, cx),
        })];
        palette.update(cx, |p, cx| p.focus(window, cx));
        self.palette = Some(palette);
        cx.notify();
    }

    /// Focus coming from the palette or the list: tree tabs only.
    pub(crate) fn focus_from_outside(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.tab, Tab::List | Tab::Fan) {
            self.tab = Tab::Pedigree;
            self.restore_focus(window, cx);
        }
        self.set_focus(id, true, window, cx);
    }

    pub(crate) fn close_overlays(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit_dialog = None;
        self.palette = None;
        self.merge_dialog = None;
        self.kinship_dialog = None;
        self.raw = None;
        self.overlay_subs.clear();
        self.restore_focus(window, cx);
        cx.notify();
    }

    /// Canvas keys only exist on the tree tabs; elsewhere the app root takes focus.
    pub(crate) fn restore_focus(&self, window: &mut Window, cx: &mut App) {
        match self.tab {
            Tab::Pedigree | Tab::Descendants => self.canvas_focus.focus(window, cx),
            Tab::List | Tab::Fan | Tab::Map => self.focus.focus(window, cx),
        }
    }

    pub(crate) fn show_raw(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.raw = Some(id);
        self.raw_focus.focus(window, cx);
        cx.notify();
    }

    fn close_raw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.raw = None;
        match &self.edit_dialog {
            Some(d) => d.update(cx, |d, cx| d.focus(window, cx)),
            None => self.restore_focus(window, cx),
        }
        cx.notify();
    }

    pub(crate) fn set_language(&mut self, code: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if i18n::set_locale(code) {
            let mut s = Settings::load();
            s.language = Some(code.to_string());
            s.save();
            gpui_component::set_locale(code);
            crate::set_app_menus(cx);
            self.rev += 1;
            self.update_title(window);
            cx.refresh_windows();
        }
    }
}

/// OS error text is in the system language, so the common cases get our own wording.
fn io_error(e: &std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => i18n::t!("app.error.not_found"),
        std::io::ErrorKind::PermissionDenied => i18n::t!("app.error.permission_denied"),
        _ => e.to_string(),
    }
}

/// World centre of the focus card.
pub(crate) fn focus_center() -> V2 {
    v2(layout::CARD_W / 2., 0.)
}

fn first_at_place(tree: &Tree, place: &str) -> Option<String> {
    let f = gedcom::fold(place);
    let hit = |e: &gedcom::Event| e.place.as_deref().is_some_and(|p| gedcom::fold(p) == f);
    tree.order.iter().find(|id| tree.people[*id].events.iter().any(hit) || tree.people[*id].birth.as_ref().is_some_and(hit) || tree.people[*id].death.as_ref().is_some_and(hit)).cloned()
}

/// Palette query → (given, surname): one word is a given name; two are given + surname; with
/// more, the last two are the (Hispanic double) surname.
pub(crate) fn split_name(q: &str) -> (String, String) {
    let w: Vec<&str> = q.split_whitespace().collect();
    match w.len() {
        0 => (String::new(), String::new()),
        1 => (w[0].to_string(), String::new()),
        2 => (w[0].to_string(), w[1].to_string()),
        n => (w[..n - 2].join(" "), w[n - 2..].join(" ")),
    }
}

impl Render for GenApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tick(window);
        self.ensure_quality(cx);
        let t = tokens(cx);
        let main = match self.tab {
            Tab::Pedigree | Tab::Descendants => self.render_canvas(window, cx).into_any_element(),
            Tab::List => self.render_list(cx).into_any_element(),
            Tab::Fan => self.render_fan(cx).into_any_element(),
            Tab::Map => self.render_map(window, cx),
        };
        let toast = self.toast.as_ref().map(|toast| {
            div()
                .absolute()
                .bottom(px(72.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .px(px(16.))
                        .py(px(10.))
                        .rounded(px(12.))
                        .bg(t.ink)
                        .text_color(t.on_fill())
                        .text_size(px(13.))
                        .child(div().flex().flex_col().gap(px(2.)).children(toast.lines()))
                        .when(toast.undo, |d| {
                            d.child(
                                div()
                                    .id("toast-undo")
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.underline())
                                    .child(i18n::t!("app.toast.undo"))
                                    .on_click(cx.listener(|this, _, window, cx| this.undo(false, window, cx))),
                            )
                        }),
                )
        });
        div()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.canvas)
            .text_color(t.ink)
            .font_family(SANS)
            .text_size(px(13.))
            .on_action(cx.listener(|this, _: &Open, window, cx| this.open_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, window, cx)))
            .on_action(cx.listener(|this, _: &NewTree, window, cx| this.new_tree(window, cx)))
            .on_action(cx.listener(|this, _: &Undo, window, cx| this.undo(false, window, cx)))
            .on_action(cx.listener(|this, _: &Redo, window, cx| this.undo(true, window, cx)))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.go_back(window, cx)))
            .on_action(cx.listener(|this, _: &OpenPalette, window, cx| this.open_palette(window, cx)))
            .on_action(cx.listener(|this, _: &crate::review::ToggleReview, window, cx| this.toggle_review(window, cx)))
            .on_action(cx.listener(|this, _: &NavLeft, window, cx| this.navigate((-1, 0), window, cx)))
            .on_action(cx.listener(|this, _: &NavRight, window, cx| this.navigate((1, 0), window, cx)))
            .on_action(cx.listener(|this, _: &NavUp, window, cx| this.navigate((0, -1), window, cx)))
            .on_action(cx.listener(|this, _: &NavDown, window, cx| this.navigate((0, 1), window, cx)))
            .on_action(cx.listener(|this, _: &CenterFocus, window, cx| {
                this.recenter(true, window);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &EditFocus, window, cx| {
                if let Some(id) = this.focus_id.clone() {
                    this.open_edit(id, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, _| this.zoom_by(1.25, None, window)))
            .on_action(cx.listener(|this, _: &ZoomOut, window, _| this.zoom_by(0.8, None, window)))
            .on_action(cx.listener(|this, _: &ZoomReset, window, _| {
                let z = 1. / this.view.zoom;
                this.zoom_by(z, None, window)
            }))
            .on_action(cx.listener(|this, _: &FitView, window, _| this.fit_view(window)))
            .on_action(cx.listener(|this, _: &MoreGenerations, window, cx| this.step_generations(true, window, cx)))
            .on_action(cx.listener(|this, _: &FewerGenerations, window, cx| this.step_generations(false, window, cx)))
            .child(self.render_topbar(window, cx))
            .child(div().flex().flex_row().flex_1().min_h_0().child(div().flex_1().min_w_0().h_full().child(main)).when_some(self.review.clone(), |d, r| d.child(r)).child(self.render_sheet(window, cx)))
            .children(toast)
            .when_some(self.palette.clone(), |d, p| d.child(p))
            .when_some(self.edit_dialog.clone(), |d, e| d.child(e))
            .when_some(self.merge_dialog.clone(), |d, m| d.child(m))
            .when_some(self.kinship_dialog.clone(), |d, k| d.child(k))
            .when_some(self.raw.clone(), |d, id| d.child(self.render_raw(&id, cx)))
    }
}

impl GenApp {
    fn render_raw(&self, id: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens(cx);
        let text = self.tree.record_text(id);
        div()
            .id("raw-backdrop")
            .key_context(RAW_CONTEXT)
            .track_focus(&self.raw_focus)
            .on_action(cx.listener(|this, _: &CloseRaw, window, cx| this.close_raw(window, cx)))
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(t.backdrop())
            .occlude()
            .on_click(cx.listener(|this, _, window, cx| this.close_raw(window, cx)))
            .child(
                div()
                    .id("raw-modal")
                    .w(px(640.))
                    .max_h(relative(0.8))
                    .flex()
                    .flex_col()
                    .rounded(theme::size::DIALOG_RADIUS)
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .px(px(20.))
                            .py(px(14.))
                            .border_b_1()
                            .border_color(t.border)
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(i18n::t!("app.raw.title", id = id)))
                            .child(crate::ui::icon_button(&t, "raw-close", gpui_component::IconName::Close, true).on_click(cx.listener(|this, _, window, cx| this.close_raw(window, cx)))),
                    )
                    .child(
                        div()
                            .id("raw-scroll")
                            .overflow_y_scroll()
                            .p(px(20.))
                            .font_family({
                                use gpui_component::ActiveTheme as _;
                                cx.theme().mono_font_family.clone()
                            })
                            .text_size(px(12.))
                            .whitespace_normal()
                            .children(text.lines().map(|l| div().child(l.to_string())).collect::<Vec<_>>()),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::split_name;

    #[test]
    fn palette_query_to_names() {
        assert_eq!(split_name("ana"), ("ana".into(), "".into()));
        assert_eq!(split_name("ana  ruiz"), ("ana".into(), "ruiz".into()));
        assert_eq!(split_name("ana maria ruiz diaz"), ("ana maria".into(), "ruiz diaz".into()));
    }
}
