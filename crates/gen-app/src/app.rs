use gedcom::{Person, Sex, Tree};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{ActiveTheme, IconName, Sizable, StyledExt, Theme, h_flex, v_flex};
use std::path::{Path, PathBuf};

actions!(gen_app, [Open, Save, SaveAs, NewTree, Undo, Redo, FocusSearch, Back]);

const KEY_CONTEXT: &str = "GenApp";
const UNDO_LIMIT: usize = 100;
const ROW_H: f32 = 44.;

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", Open, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-shift-s", SaveAs, None),
        KeyBinding::new("cmd-n", NewTree, None),
        KeyBinding::new("cmd-z", Undo, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-shift-z", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-f", FocusSearch, None),
        KeyBinding::new("cmd-k", FocusSearch, None),
        KeyBinding::new("cmd-[", Back, Some(KEY_CONTEXT)),
    ]);
}

/// Which relative the "add" buttons create.
#[derive(Clone, Copy)]
enum Rel {
    Father,
    Mother,
    Partner,
    Son,
    Daughter,
}

struct Form {
    given: Entity<InputState>,
    surname: Entity<InputState>,
    birth_date: Entity<InputState>,
    birth_place: Entity<InputState>,
    death_date: Entity<InputState>,
    death_place: Entity<InputState>,
    note: Entity<InputState>,
}

pub struct GenApp {
    tree: Tree,
    path: Option<PathBuf>,
    dirty: bool,
    loading: bool,
    undo: Vec<Tree>,
    redo: Vec<Tree>,
    search: Entity<InputState>,
    hits: Vec<String>,
    selected: Option<String>,
    back: Vec<String>,
    list_scroll: UniformListScrollHandle,
    form: Form,
    generations: u32,
    show_raw: bool,
    /// a destructive action waiting for its second click
    armed: Option<&'static str>,
    status: String,
    focus: FocusHandle,
    _subs: Vec<Subscription>,
}

impl GenApp {
    pub fn new(initial: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Theme::sync_system_appearance(Some(window), cx);
        let input = |ph: &str, window: &mut Window, cx: &mut Context<Self>| {
            let ph = ph.to_string();
            cx.new(|cx| InputState::new(window, cx).placeholder(ph))
        };
        let search = input("Search name, year, place…  (⌘F)", window, cx);
        let form = Form {
            given: input("Given names", window, cx),
            surname: input("Surname", window, cx),
            birth_date: input("e.g. 12 JUN 1950, ABT 1900", window, cx),
            birth_place: input("Place", window, cx),
            death_date: input("Date", window, cx),
            death_place: input("Place", window, cx),
            note: input("Note", window, cx),
        };
        let mut subs = vec![cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => this.refresh_hits(cx),
            InputEvent::PressEnter { .. } => {
                if let Some(first) = this.hits.first().cloned() {
                    this.select(first, true, window, cx);
                }
            }
            _ => {}
        })];
        for st in [&form.given, &form.surname, &form.birth_date, &form.birth_place, &form.death_date, &form.death_place, &form.note] {
            subs.push(cx.subscribe_in(st, window, |this, _, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    this.apply_form(window, cx);
                }
            }));
        }
        subs.push(window.observe_window_appearance(|window, cx| Theme::sync_system_appearance(Some(window), cx)));
        let mut this = GenApp {
            tree: Tree::new_empty(),
            path: None,
            dirty: false,
            loading: false,
            undo: vec![],
            redo: vec![],
            search,
            hits: vec![],
            selected: None,
            back: vec![],
            list_scroll: UniformListScrollHandle::new(),
            form,
            generations: 4,
            show_raw: false,
            armed: None,
            status: "Open a GEDCOM file (⌘O) or start a new tree (⌘N).".into(),
            focus: cx.focus_handle(),
            _subs: subs,
        };
        if let Some(p) = initial {
            this.load(p, window, cx);
        }
        this
    }

    // ---- files ----

    fn load(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.loading = true;
        self.status = format!("Loading {}…", path.display());
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
                        let (np, nf) = tree.stats();
                        this.tree = tree;
                        this.path = Some(path.clone());
                        this.dirty = false;
                        this.undo.clear();
                        this.redo.clear();
                        this.back.clear();
                        this.selected = None;
                        this.refresh_hits(cx);
                        let first = this.tree.order.first().cloned();
                        if let Some(first) = first {
                            this.select(first, false, window, cx);
                        }
                        this.status = format!("Loaded {np} people, {nf} families");
                        window.set_window_title(&this.title());
                    }
                    Ok(Err(e)) => this.status = format!("Not a valid GEDCOM file: {e}"),
                    Err(e) => this.status = format!("Could not read {}: {e}", path.display()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn title(&self) -> String {
        let name = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        format!("{name}{} — gen", if self.dirty { " •" } else { "" })
    }

    fn guard_unsaved(&mut self, what: &'static str, cx: &mut Context<Self>) -> bool {
        if !self.dirty || self.armed == Some(what) {
            self.armed = None;
            return true;
        }
        self.armed = Some(what);
        self.status = "Unsaved changes — repeat to discard them".into();
        cx.notify();
        false
    }

    fn open_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guard_unsaved("open", cx) {
            return;
        }
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Open GEDCOM".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.into_iter().next() {
                    this.update_in(cx, |this, window, cx| this.load(p, window, cx)).ok();
                }
            }
        })
        .detach();
    }

    fn new_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.guard_unsaved("new", cx) {
            return;
        }
        self.tree = Tree::new_empty();
        self.path = None;
        self.dirty = false;
        self.undo.clear();
        self.redo.clear();
        self.back.clear();
        self.selected = None;
        let id = self.tree.add_person("", "", Sex::Unknown);
        self.refresh_hits(cx);
        self.select(id, false, window, cx);
        self.status = "New tree — name the first person, then add relatives".into();
        window.set_window_title(&self.title());
        cx.notify();
    }

    fn save(&mut self, save_as: bool, window: &mut Window, cx: &mut Context<Self>) {
        match (&self.path, save_as) {
            (Some(p), false) => {
                let p = p.clone();
                self.write_to(p, window, cx);
            }
            _ => {
                let dir = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(PathBuf::from)).unwrap_or_else(|| PathBuf::from("."));
                let name = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "tree.ged".into());
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

    fn write_to(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_form(window, cx);
        match gedcom::save(&self.tree, &path) {
            Ok(()) => {
                self.status = format!("Saved {}", path.display());
                self.path = Some(path);
                self.dirty = false;
                window.set_window_title(&self.title());
            }
            Err(e) => self.status = format!("Save failed: {e}"),
        }
        cx.notify();
    }

    // ---- edits ----

    /// Snapshot for undo, run the edit, mark dirty. Edits are whole-tree clones: simple and
    /// fast enough for trees in the tens of thousands.
    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Tree) -> Result<(), gedcom::EditError>) {
        let before = self.tree.clone();
        match f(&mut self.tree) {
            Ok(()) => {
                self.undo.push(before);
                if self.undo.len() > UNDO_LIMIT {
                    self.undo.remove(0);
                }
                self.redo.clear();
                self.dirty = true;
                window.set_window_title(&self.title());
            }
            Err(e) => {
                self.tree = before;
                self.status = format!("Edit failed: {e}");
            }
        }
        self.refresh_hits(cx);
        cx.notify();
    }

    fn undo(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some(prev) = from.pop() else { return };
        to.push(std::mem::replace(&mut self.tree, prev));
        self.dirty = true;
        if self.selected.as_deref().is_some_and(|s| self.tree.person(s).is_none()) {
            self.selected = self.tree.order.first().cloned();
        }
        self.refresh_hits(cx);
        self.fill_form(window, cx);
        self.status = if redo { "Redone".into() } else { "Undone".into() };
        window.set_window_title(&self.title());
        cx.notify();
    }

    fn apply_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        let Some(p) = self.tree.person(&id).cloned() else { return };
        let v = |s: &Entity<InputState>| s.read(cx).value().trim().to_string();
        let (given, surname) = (v(&self.form.given), v(&self.form.surname));
        let (bd, bp, dd, dp) = (v(&self.form.birth_date), v(&self.form.birth_place), v(&self.form.death_date), v(&self.form.death_place));
        let note = v(&self.form.note);
        let cur = |e: &Option<gedcom::Event>, tag: &str| e.as_ref().filter(|e| e.tag == tag).map(|e| (e.date.clone().unwrap_or_default(), e.place.clone().unwrap_or_default())).unwrap_or_default();
        let birth_tag = p.birth.as_ref().map(|e| e.tag.clone()).unwrap_or_else(|| "BIRT".into());
        let death_tag = p.death.as_ref().map(|e| e.tag.clone()).unwrap_or_else(|| "DEAT".into());
        let name_changed = given != p.given || surname != p.surname;
        let birth_changed = (bd.clone(), bp.clone()) != cur(&p.birth, &birth_tag);
        let death_changed = (dd.clone(), dp.clone()) != cur(&p.death, &death_tag);
        let inline_note = self.tree.record(&id).and_then(|r| r.children.iter().find(|c| c.tag == "NOTE" && c.pointer().is_none())).map(|n| n.value.trim().to_string()).unwrap_or_default();
        let note_changed = note != inline_note;
        if !(name_changed || birth_changed || death_changed || note_changed) {
            return;
        }
        self.edit(window, cx, |t| {
            if name_changed {
                t.set_name(&id, &given, &surname)?;
            }
            if birth_changed {
                t.set_event(&id, &birth_tag, &bd, &bp)?;
            }
            if death_changed {
                t.set_event(&id, &death_tag, &dd, &dp)?;
            }
            if note_changed {
                t.set_note(&id, &note)?;
            }
            Ok(())
        });
        self.status = "Saved changes to person (file not yet written — ⌘S)".into();
    }

    fn set_sex(&mut self, sex: Sex, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        self.apply_form(window, cx);
        self.edit(window, cx, |t| t.set_sex(&id, sex));
    }

    fn add_relative(&mut self, rel: Rel, family: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        self.apply_form(window, cx);
        let me = self.tree.person(&id).cloned().unwrap_or_default();
        let (sex, surname) = match rel {
            Rel::Father => (Sex::Male, String::new()),
            Rel::Mother => (Sex::Female, String::new()),
            Rel::Partner => (match me.sex {
                Sex::Male => Sex::Female,
                Sex::Female => Sex::Male,
                Sex::Unknown => Sex::Unknown,
            }, String::new()),
            Rel::Son => (Sex::Male, me.surname.clone()),
            Rel::Daughter => (Sex::Female, me.surname.clone()),
        };
        let mut new_id = String::new();
        self.edit(window, cx, |t| {
            new_id = t.add_person("", &surname, sex);
            match rel {
                Rel::Father | Rel::Mother => t.add_parent_link(&id, &new_id),
                Rel::Partner => t.add_partner_link(&id, &new_id).map(|_| ()),
                Rel::Son | Rel::Daughter => t.add_child_link(&id, &new_id, family.as_deref()).map(|_| ()),
            }
        });
        if !new_id.is_empty() && self.tree.person(&new_id).is_some() {
            self.select(new_id, true, window, cx);
            self.form.given.update(cx, |s, cx| s.focus(window, cx));
            self.status = "New relative added — type the name and press Enter".into();
        }
    }

    fn delete_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.selected.clone() else { return };
        if self.armed != Some("delete") {
            self.armed = Some("delete");
            self.status = "Click Delete again to remove this person and their links".into();
            cx.notify();
            return;
        }
        self.armed = None;
        let next = self.back.iter().rev().find(|b| **b != id).cloned().or_else(|| self.tree.parents(&id).first().map(|p| p.id.clone()));
        self.edit(window, cx, |t| t.delete_person(&id));
        self.back.retain(|b| *b != id);
        self.selected = None;
        if let Some(n) = next.or_else(|| self.tree.order.first().cloned()) {
            self.select(n, false, window, cx);
        }
        self.status = "Person deleted (⌘Z to undo)".into();
    }

    fn unlink(&mut self, fam: String, child: Option<String>, partner: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(window, cx, |t| {
            if let Some(c) = &child {
                t.unlink_child(&fam, c)?;
            }
            if let Some(p) = &partner {
                t.unlink_partner(&fam, p)?;
            }
            Ok(())
        });
    }

    // ---- selection / search ----

    fn refresh_hits(&mut self, cx: &mut Context<Self>) {
        let q = self.search.read(cx).value().to_string();
        self.hits = gedcom::search(&self.tree, &q).into_iter().map(str::to_string).collect();
        cx.notify();
    }

    fn select(&mut self, id: String, push_history: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.as_deref() == Some(id.as_str()) {
            return;
        }
        self.apply_form(window, cx);
        if push_history {
            if let Some(cur) = self.selected.take() {
                self.back.push(cur);
                if self.back.len() > 200 {
                    self.back.remove(0);
                }
            }
        }
        self.armed = None;
        self.selected = Some(id.clone());
        if let Some(ix) = self.hits.iter().position(|h| *h == id) {
            self.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
        }
        self.fill_form(window, cx);
        cx.notify();
    }

    fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(prev) = self.back.pop() {
            if self.tree.person(&prev).is_some() {
                self.apply_form(window, cx);
                self.selected = None;
                self.select(prev, false, window, cx);
                return;
            }
        }
    }

    fn fill_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.selected.as_deref().and_then(|s| self.tree.person(s)).cloned().unwrap_or_default();
        let note = self
            .selected
            .as_deref()
            .and_then(|s| self.tree.record(s))
            .and_then(|r| r.children.iter().find(|c| c.tag == "NOTE" && c.pointer().is_none()))
            .map(|n| n.value.clone())
            .unwrap_or_default();
        let ev = |e: &Option<gedcom::Event>| e.as_ref().map(|e| (e.date.clone().unwrap_or_default(), e.place.clone().unwrap_or_default())).unwrap_or_default();
        let (bd, bp) = ev(&p.birth);
        let (dd, dp) = ev(&p.death);
        for (st, v) in [(&self.form.given, p.given), (&self.form.surname, p.surname), (&self.form.birth_date, bd), (&self.form.birth_place, bp), (&self.form.death_date, dd), (&self.form.death_place, dp), (&self.form.note, note)] {
            st.update(cx, |s, cx| s.set_value(v, window, cx));
        }
    }
}

// ---- rendering ----

fn sex_color(sex: Sex) -> Hsla {
    match sex {
        Sex::Male => hsla(212. / 360., 0.65, 0.55, 1.),
        Sex::Female => hsla(335. / 360., 0.6, 0.6, 1.),
        Sex::Unknown => hsla(0., 0., 0.55, 1.),
    }
}

fn section(title: impl Into<SharedString>, cx: &App) -> Div {
    div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child(title.into())
}

impl GenApp {
    /// Clickable person chip used everywhere in the family view.
    fn person_card(&self, key: impl Into<ElementId>, p: &Person, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = p.id.clone();
        let selected = self.selected.as_deref() == Some(p.id.as_str());
        h_flex()
            .id(key.into())
            .gap_2()
            .pl_1()
            .pr_2()
            .py_1()
            .min_w(px(150.))
            .rounded(cx.theme().radius)
            .border_1()
            .border_color(cx.theme().border)
            .bg(if selected { cx.theme().accent } else { cx.theme().secondary })
            .cursor_pointer()
            .hover(|s| s.bg(cx.theme().accent))
            .on_click(cx.listener(move |this, _, window, cx| this.select(id.clone(), true, window, cx)))
            .child(div().w(px(3.)).h(px(30.)).rounded(px(2.)).bg(sex_color(p.sex)))
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().font_semibold().truncate().child(p.name()))
                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child(if p.lifespan().is_empty() { "\u{00a0}".to_string() } else { p.lifespan() })),
            )
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (np, nf) = self.tree.stats();
        let file = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into());
        h_flex()
            .gap_2()
            .px_3()
            .py_2()
            .items_center()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(Button::new("open").small().outline().icon(IconName::FolderOpen).label("Open").tooltip("⌘O").on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))))
            .child(Button::new("new").small().ghost().icon(IconName::Plus).tooltip("New tree (⌘N)").on_click(cx.listener(|this, _, window, cx| this.new_tree(window, cx))))
            .child(Button::new("save").small().when(self.dirty, |b| b.primary()).when(!self.dirty, |b| b.outline()).label("Save").tooltip("⌘S").on_click(cx.listener(|this, _, window, cx| this.save(false, window, cx))))
            .child(Button::new("save-as").small().ghost().label("Save as…").tooltip("⌘⇧S").on_click(cx.listener(|this, _, window, cx| this.save(true, window, cx))))
            .child(Button::new("undo").small().ghost().icon(IconName::Undo).tooltip("Undo (⌘Z)").on_click(cx.listener(|this, _, window, cx| this.undo(false, window, cx))))
            .child(Button::new("redo").small().ghost().icon(IconName::Redo).tooltip("Redo (⌘⇧Z)").on_click(cx.listener(|this, _, window, cx| this.undo(true, window, cx))))
            .child(div().text_sm().font_semibold().child(format!("{file}{}", if self.dirty { " •" } else { "" })))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{np} people · {nf} families")))
            .child(div().flex_1())
            .child(div().w(px(420.)).child(Input::new(&self.search).small().cleanable(true)))
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let count = self.hits.len();
        v_flex()
            .w(px(300.))
            .flex_shrink_0()
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(div().px_3().py_1().text_xs().text_color(cx.theme().muted_foreground).child(format!("{count} shown")))
            .child(
                uniform_list(
                    "people",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        range
                            .filter_map(|ix| {
                                let id = this.hits.get(ix)?.clone();
                                let p = this.tree.person(&id)?;
                                let selected = this.selected.as_deref() == Some(id.as_str());
                                let name = format!("{}{}", p.surname, if p.given.is_empty() { String::new() } else { format!(", {}", p.given) });
                                let name = if name.trim().is_empty() { "(unnamed)".to_string() } else { name };
                                Some(
                                    h_flex()
                                        .id(("person", ix))
                                        .h(px(ROW_H))
                                        .px_3()
                                        .gap_2()
                                        .items_center()
                                        .cursor_pointer()
                                        .when(selected, |d| d.bg(cx.theme().accent))
                                        .hover(|s| s.bg(cx.theme().secondary))
                                        .on_click(cx.listener(move |this, _, window, cx| this.select(id.clone(), true, window, cx)))
                                        .child(div().w(px(4.)).h(px(24.)).rounded(px(2.)).bg(sex_color(p.sex)))
                                        .child(
                                            v_flex()
                                                .min_w_0()
                                                .child(div().text_sm().truncate().child(name))
                                                .child(div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(p.lifespan())),
                                        ),
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.list_scroll)
                .flex_1(),
            )
    }

    fn render_pedigree(&self, id: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let g = self.generations;
        let slots = self.tree.pedigree(id, g);
        let mut cols = h_flex().gap_3().items_stretch().h(px(((1u32 << (g - 1)) as f32 * 46.).max(240.)));
        for gen_ix in 0..g {
            let start = (1usize << gen_ix) - 1;
            let n = 1usize << gen_ix;
            let mut col = v_flex().flex_1().min_w(px(140.)).justify_around();
            for k in 0..n {
                let slot = start + k;
                let cell = match slots[slot].as_deref().and_then(|x| self.tree.person(x)) {
                    Some(p) => self.person_card(("ped", slot), p, cx).into_any_element(),
                    None => {
                        // an empty parent slot of the focused person offers to create that parent
                        let child_slot = (slot - 1) / 2;
                        let is_father = slot % 2 == 1;
                        if slot > 0 && child_slot == 0 {
                            Button::new(("ped-add", slot))
                                .xsmall()
                                .ghost()
                                .icon(IconName::Plus)
                                .label(if is_father { "Add father" } else { "Add mother" })
                                .on_click(cx.listener(move |this, _, window, cx| this.add_relative(if is_father { Rel::Father } else { Rel::Mother }, None, window, cx)))
                                .into_any_element()
                        } else {
                            div().h(px(36.)).into_any_element()
                        }
                    }
                };
                col = col.child(cell);
            }
            cols = cols.child(col);
        }
        cols
    }

    fn render_person(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(p) = self.selected.as_deref().and_then(|s| self.tree.person(s)).cloned() else {
            let msg = if self.loading { "Loading…" } else { "Nobody selected. Open a GEDCOM file (⌘O), or pick someone from the list." };
            return div().p_6().text_color(cx.theme().muted_foreground).child(msg).into_any_element();
        };
        let id = p.id.clone();
        let mut gens = h_flex().gap_1();
        for n in [3u32, 4, 5] {
            gens = gens.child(Button::new(("gens", n as usize)).xsmall().when(self.generations == n, |b| b.primary()).when(self.generations != n, |b| b.ghost()).label(format!("{n}")).on_click(cx.listener(move |this, _, _, cx| {
                this.generations = n;
                cx.notify();
            })));
        }
        let header = h_flex()
            .gap_3()
            .items_center()
            .child(Button::new("back").small().ghost().icon(IconName::ArrowLeft).tooltip("Back (⌘[)").when(self.back.is_empty(), |b| b.opacity(0.4)).on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx))))
            .child(div().w(px(6.)).h(px(32.)).rounded(px(3.)).bg(sex_color(p.sex)))
            .child(v_flex().child(div().text_xl().font_semibold().child(p.name())).child(div().text_sm().text_color(cx.theme().muted_foreground).child(format!("{}{}", p.lifespan(), if p.lifespan().is_empty() { String::new() } else { "  ·  ".into() }) + &p.id)))
            .child(div().flex_1())
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("generations"))
            .child(gens);

        let siblings = self.tree.siblings(&id);
        let unions = self.tree.unions(&id);
        let mut family = v_flex().gap_3();
        if !siblings.is_empty() {
            let mut row = h_flex().flex_wrap().gap_2();
            for (i, s) in siblings.iter().enumerate() {
                row = row.child(self.person_card(("sib", i), s, cx));
            }
            family = family.child(v_flex().gap_1().child(section(format!("Siblings ({})", siblings.len()), cx)).child(row));
        }
        for (ui, (fam, partner, kids)) in unions.iter().enumerate() {
            let fam_id = fam.id.clone();
            let marr = fam.events.iter().find(|e| e.tag == "MARR").map(|e| format!("married {}{}", e.date.clone().unwrap_or_default(), e.place.as_ref().map(|p| format!(", {p}")).unwrap_or_default()));
            let mut kids_row = h_flex().flex_wrap().gap_2();
            for (ki, k) in kids.iter().enumerate() {
                let (f, kid) = (fam_id.clone(), k.id.clone());
                kids_row = kids_row.child(
                    h_flex().gap_0p5().items_start().child(self.person_card(("kid", ui * 1000 + ki), k, cx)).child(
                        Button::new(("unkid", ui * 1000 + ki)).xsmall().ghost().icon(IconName::Close).tooltip("Unlink child from this family").on_click(cx.listener(move |this, _, window, cx| this.unlink(f.clone(), Some(kid.clone()), None, window, cx))),
                    ),
                );
            }
            let (f1, f2, f3) = (fam_id.clone(), fam_id.clone(), fam_id.clone());
            kids_row = kids_row
                .child(Button::new(("add-son", ui)).xsmall().ghost().icon(IconName::Plus).label("Son").on_click(cx.listener(move |this, _, window, cx| this.add_relative(Rel::Son, Some(f1.clone()), window, cx))))
                .child(Button::new(("add-dau", ui)).xsmall().ghost().icon(IconName::Plus).label("Daughter").on_click(cx.listener(move |this, _, window, cx| this.add_relative(Rel::Daughter, Some(f2.clone()), window, cx))));
            let me = id.clone();
            let partner_el = match partner {
                Some(pp) => h_flex().gap_1().items_center().child(self.person_card(("partner", ui), pp, cx)).into_any_element(),
                None => div().text_sm().text_color(cx.theme().muted_foreground).child("(unknown partner)").into_any_element(),
            };
            family = family.child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(h_flex().gap_2().items_center().child(section(format!("Partner · {}", fam.id), cx)).when_some(marr, |d, m| d.child(div().text_xs().text_color(cx.theme().muted_foreground).child(m))).child(div().flex_1()).child(
                        Button::new(("unpartner", ui)).xsmall().ghost().label("Leave family").tooltip("Remove this person as a partner of this family").on_click(cx.listener(move |this, _, window, cx| this.unlink(f3.clone(), None, Some(me.clone()), window, cx))),
                    ))
                    .child(partner_el)
                    .child(section(format!("Children ({})", kids.len()), cx))
                    .child(kids_row),
            );
        }
        family = family.child(
            h_flex()
                .gap_2()
                .child(Button::new("add-partner").small().outline().icon(IconName::Plus).label("Partner").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Partner, None, window, cx))))
                .when(unions.is_empty(), |d| {
                    d.child(Button::new("add-son-0").small().outline().icon(IconName::Plus).label("Son").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Son, None, window, cx))))
                        .child(Button::new("add-dau-0").small().outline().icon(IconName::Plus).label("Daughter").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Daughter, None, window, cx))))
                }),
        );

        let mut events = v_flex().gap_1();
        for e in &p.events {
            let detail = [e.date.clone(), e.place.clone(), e.value.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            events = events.child(h_flex().gap_3().text_sm().child(div().w(px(130.)).flex_shrink_0().text_color(cx.theme().muted_foreground).child(e.label())).child(div().min_w_0().whitespace_normal().child(detail)));
        }
        let mut notes = v_flex().gap_2();
        for n in &p.notes {
            notes = notes.child(div().text_sm().whitespace_normal().p_2().rounded(cx.theme().radius).bg(cx.theme().secondary).child(clean_note(n)));
        }

        v_flex()
            .id("person-scroll")
            .size_full()
            .overflow_y_scrollbar()
            .p_4()
            .gap_4()
            .child(header)
            .child(v_flex().gap_1().child(section("Ancestors", cx)).child(self.render_pedigree(&id, cx)))
            .child(family)
            .when(!p.events.is_empty(), |d| d.child(v_flex().gap_1().child(section("Events", cx)).child(events)))
            .when(!p.notes.is_empty(), |d| d.child(v_flex().gap_1().child(section("Notes", cx)).child(notes)))
            .when(self.show_raw, |d| d.child(v_flex().gap_1().child(section("GEDCOM record", cx)).child(div().p_2().rounded(cx.theme().radius).bg(cx.theme().secondary).font_family(cx.theme().mono_font_family.clone()).text_xs().whitespace_normal().child(self.tree.record_text(&id)))))
            .into_any_element()
    }

    fn field(&self, label: &str, st: &Entity<InputState>, cx: &App) -> impl IntoElement {
        v_flex().gap_0p5().child(div().text_xs().text_color(cx.theme().muted_foreground).child(label.to_string())).child(Input::new(st).small())
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.selected.as_deref().and_then(|s| self.tree.person(s)).cloned();
        let mut panel = v_flex().id("editor").w(px(340.)).flex_shrink_0().h_full().overflow_y_scrollbar().border_l_1().border_color(cx.theme().border).p_3().gap_3();
        let Some(p) = p else {
            return panel.child(div().text_sm().text_color(cx.theme().muted_foreground).child("Select a person to edit."));
        };
        let birth_label = p.birth.as_ref().map(|e| gedcom::tree::event_label(&e.tag).to_string()).unwrap_or_else(|| "Birth".into());
        let death_label = p.death.as_ref().map(|e| gedcom::tree::event_label(&e.tag).to_string()).unwrap_or_else(|| "Death".into());
        let mut sex_row = h_flex().gap_1();
        for (i, (s, l)) in [(Sex::Male, "Male"), (Sex::Female, "Female"), (Sex::Unknown, "Unknown")].into_iter().enumerate() {
            sex_row = sex_row.child(Button::new(("sex", i)).xsmall().when(p.sex == s, |b| b.primary()).when(p.sex != s, |b| b.outline()).label(l).on_click(cx.listener(move |this, _, window, cx| this.set_sex(s, window, cx))));
        }
        let deleting = self.armed == Some("delete");
        panel = panel
            .child(div().font_semibold().child("Edit person"))
            .child(self.field("Given names", &self.form.given, cx))
            .child(self.field("Surname", &self.form.surname, cx))
            .child(v_flex().gap_0p5().child(div().text_xs().text_color(cx.theme().muted_foreground).child("Sex")).child(sex_row))
            .child(section(birth_label, cx))
            .child(self.field("Date", &self.form.birth_date, cx))
            .child(self.field("Place", &self.form.birth_place, cx))
            .child(section(death_label, cx))
            .child(self.field("Date", &self.form.death_date, cx))
            .child(self.field("Place", &self.form.death_place, cx))
            .child(self.field("Note", &self.form.note, cx))
            .child(Button::new("apply").small().primary().label("Apply  ↵").on_click(cx.listener(|this, _, window, cx| this.apply_form(window, cx))))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Dates use GEDCOM form: 12 JUN 1950 · ABT 1900 · BEF 1850 · BET 1800 AND 1810. Clear date and place to remove the event."))
            .child(section("Add relative", cx))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .child(Button::new("r-father").xsmall().outline().label("Father").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Father, None, window, cx))))
                    .child(Button::new("r-mother").xsmall().outline().label("Mother").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Mother, None, window, cx))))
                    .child(Button::new("r-partner").xsmall().outline().label("Partner").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Partner, None, window, cx))))
                    .child(Button::new("r-son").xsmall().outline().label("Son").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Son, None, window, cx))))
                    .child(Button::new("r-daughter").xsmall().outline().label("Daughter").on_click(cx.listener(|this, _, window, cx| this.add_relative(Rel::Daughter, None, window, cx)))),
            )
            .child(section("More", cx))
            .child(
                h_flex()
                    .gap_1()
                    .child(Button::new("raw").xsmall().outline().label(if self.show_raw { "Hide GEDCOM" } else { "Show GEDCOM" }).on_click(cx.listener(|this, _, _, cx| {
                        this.show_raw = !this.show_raw;
                        cx.notify();
                    })))
                    .child(Button::new("delete").xsmall().when(deleting, |b| b.danger()).when(!deleting, |b| b.outline()).icon(IconName::Delete).label(if deleting { "Confirm delete" } else { "Delete" }).on_click(cx.listener(|this, _, window, cx| this.delete_selected(window, cx)))),
            );
        panel
    }
}

/// MyHeritage and others embed HTML in notes (`&lt;br&gt;`, sometimes double-escaped).
fn clean_note(s: &str) -> String {
    let mut t = s.to_string();
    for _ in 0..2 {
        t = t.replace("&amp;", "&");
    }
    t.replace("&lt;br&gt;", "\n").replace("<br>", "\n").replace("<br/>", "\n").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

impl Render for GenApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(|this, _: &Open, window, cx| this.open_dialog(window, cx)))
            .on_action(cx.listener(|this, _: &Save, window, cx| this.save(false, window, cx)))
            .on_action(cx.listener(|this, _: &SaveAs, window, cx| this.save(true, window, cx)))
            .on_action(cx.listener(|this, _: &NewTree, window, cx| this.new_tree(window, cx)))
            .on_action(cx.listener(|this, _: &Undo, window, cx| this.undo(false, window, cx)))
            .on_action(cx.listener(|this, _: &Redo, window, cx| this.undo(true, window, cx)))
            .on_action(cx.listener(|this, _: &Back, window, cx| this.go_back(window, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.search.update(cx, |s, cx| s.focus(window, cx))))
            .child(self.render_toolbar(cx))
            .child(h_flex().flex_1().min_h_0().child(self.render_list(cx)).child(div().flex_1().min_w_0().h_full().child(self.render_person(cx))).child(self.render_editor(cx)))
            .child(h_flex().px_3().py_1().border_t_1().border_color(cx.theme().border).text_xs().text_color(cx.theme().muted_foreground).child(self.status.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::clean_note;

    #[test]
    fn clean_note_unescapes_double_html() {
        assert_eq!(clean_note("a&amp;lt;br&amp;gt;b"), "a\nb");
    }
}
