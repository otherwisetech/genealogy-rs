//! Edit-person dialog (docs/DESIGN.md, artboard 2).
//! The host app renders it as an overlay and reacts to its events; it never mutates the tree.
use crate::theme::{self, Tokens, size};
use crate::ui;
use std::rc::Rc;
use gedcom::{CitationEdit, DateValue, EventEdit, FAMILY_EVENT_TAGS, FactEdit, FactOption, FactRef, PERSON_EVENT_TAGS, PersonEdit, Precision, Sex, Tree, fold, normalize_date};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::{ActiveTheme, Icon, IconName, Sizable, h_flex, v_flex};

pub enum EditDialogEvent {
    /// user pressed «Guardar cambios» / ⌘↵
    Save(Box<PersonEdit>),
    /// «Cancelar», ×, esc, or backdrop click
    Cancel,
    /// «Eliminar persona» confirmed (the dialog asks for the second click itself)
    Delete(String),
    /// «Ver GEDCOM»
    ViewGedcom(String),
}

const CONTEXT: &str = "EditDialog";

actions!(edit_dialog, [SaveEdit, EscapeEdit, SuggestUp, SuggestDown, SuggestPick]);

/// Key bindings of the dialog; call once at startup.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-enter", SaveEdit, Some(CONTEXT)),
        KeyBinding::new("escape", EscapeEdit, Some(CONTEXT)),
        KeyBinding::new("up", SuggestUp, Some(CONTEXT)),
        KeyBinding::new("down", SuggestDown, Some(CONTEXT)),
        KeyBinding::new("enter", SuggestPick, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Data,
    Events,
    Family,
    Notes,
    Photos,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    Birth,
    Death,
}

struct DateInputs {
    precision: Precision,
    date: Entity<InputState>,
    date2: Entity<InputState>,
    place: Entity<InputState>,
}

struct Suggest {
    slot: Slot,
    items: Vec<(String, usize)>,
    selected: Option<usize>,
}

/// Inputs of the event being edited in the «Eventos» tab (one row open at a time).
struct FactInputs {
    value: Entity<InputState>,
    kind: Entity<InputState>,
    date: Entity<InputState>,
    place: Entity<InputState>,
}

pub struct EditDialog {
    // owned copy: `new` only borrows the tree, but place suggestions are queried while typing
    tree: Rc<Tree>,
    edit: PersonEdit,
    focus: FocusHandle,
    tab: Tab,
    given: Entity<InputState>,
    surname: Entity<InputState>,
    sex: Sex,
    birth: DateInputs,
    death: DateInputs,
    note: Entity<TextareaState>,
    /// working copy of every event of the person and their unions; written on save
    event_edits: Vec<FactEdit>,
    fact_open: Option<usize>,
    fact_inputs: FactInputs,
    /// working copy of the person's citations; written on save
    citations: Vec<CitationEdit>,
    facts: Vec<FactOption>,
    /// (id, title) of every SOUR record, for the source picker
    sources: Vec<(String, String)>,
    cite_source: Entity<InputState>,
    cite_page: Entity<InputState>,
    /// event the new citation backs; None = the person as a whole
    cite_fact: Option<FactRef>,
    source_suggest: bool,
    precision_menu: Option<Slot>,
    suggest: Option<Suggest>,
    confirm_delete: bool,
    synced_locale: &'static str,
    _subs: Vec<Subscription>,
}

impl EventEmitter<EditDialogEvent> for EditDialog {}

impl EditDialog {
    /// `tree` is read for the current values, place suggestions and the header subtitle.
    pub fn new(tree: Rc<Tree>, person_id: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut edit = tree.person_edit(person_id).unwrap_or_default();
        if edit.id.is_empty() {
            edit.id = person_id.to_string();
        }
        let text = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_string();
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let given = text(&edit.given, window, cx);
        let surname = text(&edit.surname, window, cx);
        let dates = |ev: &EventEdit, window: &mut Window, cx: &mut Context<Self>| {
            let dv = DateValue::parse(&ev.date);
            DateInputs { precision: dv.precision, date: text(&dv.date, window, cx), date2: text(&dv.date2, window, cx), place: text(&ev.place, window, cx) }
        };
        let birth = dates(&edit.birth, window, cx);
        let death = dates(&edit.death, window, cx);
        let note_text = edit.note.clone();
        let note = cx.new(|cx| TextareaState::new(window, cx).default_value(note_text));
        let fact_inputs = FactInputs { value: text("", window, cx), kind: text("", window, cx), date: text("", window, cx), place: text("", window, cx) };
        let cite_source = text("", window, cx);
        let cite_page = text("", window, cx);

        let mut subs = Vec::new();
        for st in [&given, &surname, &birth.date, &birth.date2, &death.date, &death.date2] {
            subs.push(cx.subscribe_in(st, window, |this, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::Focus => this.close_popups(cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }));
        }
        for (st, slot) in [(&birth.place, Slot::Birth), (&death.place, Slot::Death)] {
            subs.push(cx.subscribe_in(st, window, move |this, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::Focus => {
                    this.precision_menu = None;
                    this.refresh_suggest(slot, cx);
                }
                InputEvent::Change => this.refresh_suggest(slot, cx),
                _ => {}
            }));
        }
        subs.push(cx.subscribe_in(&cite_source, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Focus | InputEvent::Change => {
                this.suggest = None;
                this.precision_menu = None;
                this.source_suggest = true;
                cx.notify();
            }
            InputEvent::PressEnter { secondary: false, .. } => this.add_citation(window, cx),
            _ => {}
        }));
        subs.push(cx.subscribe_in(&cite_page, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Focus => this.close_popups(cx),
            InputEvent::PressEnter { secondary: false, .. } => this.add_citation(window, cx),
            _ => {}
        }));
        // the note is multi-line, so ⌘↵ does not bubble up as an action there
        subs.push(cx.subscribe_in(&note, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { secondary: true, .. } => this.save(cx),
            InputEvent::Focus => this.close_popups(cx),
            _ => {}
        }));

        EditDialog {
            sex: edit.sex,
            event_edits: edit.facts.clone(),
            fact_open: None,
            fact_inputs,
            citations: edit.citations.clone(),
            facts: tree.citable_facts(person_id),
            sources: tree.sources(),
            cite_source,
            cite_page,
            cite_fact: None,
            source_suggest: false,
            tree,
            edit,
            focus: cx.focus_handle(),
            tab: Tab::Data,
            given,
            surname,
            birth,
            death,
            note,
            precision_menu: None,
            suggest: None,
            confirm_delete: false,
            synced_locale: "",
            _subs: subs,
        }
    }

    /// Focus the first field; the host calls this right after opening.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.given.update(cx, |s, cx| s.focus(window, cx));
    }

    fn dates(&self, slot: Slot) -> &DateInputs {
        match slot {
            Slot::Birth => &self.birth,
            Slot::Death => &self.death,
        }
    }

    fn dates_mut(&mut self, slot: Slot) -> &mut DateInputs {
        match slot {
            Slot::Birth => &mut self.birth,
            Slot::Death => &mut self.death,
        }
    }

    fn close_popups(&mut self, cx: &mut Context<Self>) {
        if self.suggest.take().is_some() | self.precision_menu.take().is_some() | std::mem::take(&mut self.source_suggest) {
            cx.notify();
        }
    }

    fn refresh_suggest(&mut self, slot: Slot, cx: &mut Context<Self>) {
        let query = self.dates(slot).place.read(cx).value().to_string();
        let mut items = self.tree.place_suggestions(query.trim(), 6);
        if items.len() == 1 && items[0].0 == query.trim() {
            items.clear();
        }
        self.suggest = (!items.is_empty()).then_some(Suggest { slot, items, selected: None });
        cx.notify();
    }

    fn pick_suggestion(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.suggest.take() else { return };
        if let Some((place, _)) = s.items.get(index) {
            let place = place.clone();
            self.dates(s.slot).place.update(cx, |st, cx| st.set_value(place, window, cx));
        }
        cx.notify();
    }

    fn fields(&self, cx: &App) -> PersonEdit {
        let v = |st: &Entity<InputState>| st.read(cx).value().to_string();
        let ev = |d: &DateInputs| DateFields { precision: d.precision, date: v(&d.date), date2: v(&d.date2), place: v(&d.place) };
        let mut edit = build_edit(&self.edit, &v(&self.given), &v(&self.surname), self.sex, &ev(&self.birth), &ev(&self.death), &self.note.read(cx).value());
        edit.facts = self.current_facts(cx);
        // a citation typed but not yet added still counts on save
        edit.citations = self.citations.iter().cloned().chain(self.pending_citation(cx)).collect();
        edit
    }

    /// The event list with the open row's inputs applied.
    fn current_facts(&self, cx: &App) -> Vec<FactEdit> {
        let mut facts = self.event_edits.clone();
        if let Some(f) = self.fact_open.and_then(|i| facts.get_mut(i)) {
            let v = |st: &Entity<InputState>| st.read(cx).value().trim().to_string();
            f.value = v(&self.fact_inputs.value);
            f.kind = v(&self.fact_inputs.kind);
            f.date = event_date(&v(&self.fact_inputs.date));
            f.place = v(&self.fact_inputs.place);
        }
        facts
    }

    fn open_fact(&mut self, ix: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        self.event_edits = self.current_facts(cx);
        self.fact_open = ix;
        if let Some(f) = ix.and_then(|i| self.event_edits.get(i)).cloned() {
            let i = &self.fact_inputs;
            for (st, val) in [(i.value.clone(), f.value), (i.kind.clone(), f.kind), (i.date.clone(), f.date), (i.place.clone(), f.place)] {
                st.update(cx, |s, cx| s.set_value(val, window, cx));
            }
        }
        cx.notify();
    }

    fn add_fact(&mut self, record: String, tag: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.event_edits = self.current_facts(cx);
        self.event_edits.push(FactEdit { record, tag: tag.to_string(), ..Default::default() });
        let ix = self.event_edits.len() - 1;
        self.fact_open = None;
        self.open_fact(Some(ix), window, cx);
    }

    /// The first birth/death of the person: edited in «Datos», shown read-only here.
    fn in_data_tab(&self, f: &FactEdit) -> bool {
        f.record == self.edit.id && f.fact.as_ref().is_some_and(|r| r.nth == 0 && (r.tag == self.edit.birth.tag || r.tag == self.edit.death.tag))
    }

    fn event_title(&self, f: &FactEdit) -> String {
        let base = gedcom::tree::event_label(&f.tag);
        let base = if !f.kind.trim().is_empty() {
            i18n::t!("core.event.with_kind", label = base, kind = f.kind.trim())
        } else if !f.value.trim().is_empty() {
            i18n::t!("core.event.with_value", label = base, value = f.value.trim())
        } else {
            base
        };
        if f.record == self.edit.id {
            return base;
        }
        match self.tree.family(&f.record).and_then(|fam| fam.other_partner(&self.edit.id)).and_then(|p| self.tree.person(p)) {
            Some(p) => i18n::t!("dialogs.edit.events.with", label = base, name = display_name(p)),
            None => base,
        }
    }

    /// The citation in the «add» form, if a source title was typed.
    fn pending_citation(&self, cx: &App) -> Option<CitationEdit> {
        let title = self.cite_source.read(cx).value().trim().to_string();
        if title.is_empty() {
            return None;
        }
        let key = fold(&title);
        let source = self.sources.iter().find(|(_, t)| fold(t) == key).map(|(id, _)| id.clone()).unwrap_or_default();
        let page = self.cite_page.read(cx).value().trim().to_string();
        Some(CitationEdit { fact: self.cite_fact.clone(), source, title, page, slot: None })
    }

    fn add_citation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.pending_citation(cx) {
            self.citations.push(c);
            for st in [self.cite_source.clone(), self.cite_page.clone()] {
                st.update(cx, |s, cx| s.set_value("", window, cx));
            }
        }
        self.source_suggest = false;
        cx.notify();
    }

    fn fact_label(&self, fact: &Option<FactRef>) -> String {
        match fact {
            None => i18n::t!("dialogs.edit.sources.general"),
            Some(f) => self.facts.iter().find(|o| &o.fact == f).map(|o| o.label.clone()).unwrap_or_else(|| gedcom::tree::event_label(&f.tag)),
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let edit = self.fields(cx);
        cx.emit(EditDialogEvent::Save(Box::new(edit)));
    }

    fn on_save(&mut self, _: &SaveEdit, _: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
    }

    fn on_escape(&mut self, _: &EscapeEdit, _: &mut Window, cx: &mut Context<Self>) {
        if self.suggest.is_some() || self.precision_menu.is_some() {
            self.close_popups(cx);
        } else {
            cx.emit(EditDialogEvent::Cancel);
        }
    }

    fn on_up_down(&mut self, delta: isize, cx: &mut Context<Self>) {
        if let Some(s) = &mut self.suggest {
            s.selected = step(s.selected, s.items.len(), delta);
            cx.notify();
        }
    }

    fn on_pick(&mut self, _: &SuggestPick, window: &mut Window, cx: &mut Context<Self>) {
        match self.suggest.as_ref().and_then(|s| s.selected) {
            Some(i) => self.pick_suggestion(i, window, cx),
            None => cx.propagate(),
        }
    }

    /// Placeholders and event labels are stored text, so they are rebuilt when the language changes.
    fn sync_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = i18n::locale();
        if self.synced_locale == locale {
            return;
        }
        self.synced_locale = locale;
        self.facts = self.tree.citable_facts(&self.edit.id);
        let set = |st: &Entity<InputState>, key: &str, window: &mut Window, cx: &mut Context<Self>| {
            let text = i18n::tr(key, &[]);
            st.update(cx, |s, cx| s.set_placeholder(text, window, cx));
        };
        set(&self.given.clone(), "dialogs.edit.given_ph", window, cx);
        set(&self.surname.clone(), "dialogs.edit.surname_ph", window, cx);
        for slot in [Slot::Birth, Slot::Death] {
            let (d, d2, p) = {
                let d = self.dates(slot);
                (d.date.clone(), d.date2.clone(), d.place.clone())
            };
            set(&d, "dialogs.edit.date_ph", window, cx);
            set(&d2, "dialogs.edit.date2_ph", window, cx);
            set(&p, "dialogs.edit.place_ph", window, cx);
        }
        set(&self.cite_source.clone(), "dialogs.edit.sources.source_ph", window, cx);
        set(&self.fact_inputs.value.clone(), "dialogs.edit.events.value_ph", window, cx);
        set(&self.fact_inputs.kind.clone(), "dialogs.edit.events.kind_ph", window, cx);
        set(&self.fact_inputs.date.clone(), "dialogs.edit.events.date_ph", window, cx);
        set(&self.fact_inputs.place.clone(), "dialogs.edit.place_ph", window, cx);
        set(&self.cite_page.clone(), "dialogs.edit.sources.page_ph", window, cx);
        let text = i18n::t!("dialogs.edit.note_ph");
        self.note.update(cx, |s, cx| s.set_placeholder(text, window, cx));
    }

    // ---- rendering ----

    fn render_header(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let given = self.given.read(cx).value().to_string();
        let surname = self.surname.read(cx).value().to_string();
        let full = format!("{} {}", given.trim(), surname.trim()).trim().to_string();
        let title = if full.is_empty() { i18n::t!("dialogs.edit.new_person") } else { i18n::t!("dialogs.edit.title", name = full) };
        let (bg, fg) = t.sex(self.sex);
        h_flex()
            .gap_3()
            .px_5()
            .pt_5()
            .pb_3()
            .child(avatar(&theme::initials(&given, &surname), px(44.), bg, fg))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(div().font_family(theme::SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(19.)).text_color(t.ink).truncate().child(title))
                    .child(div().text_size(px(12.)).text_color(t.ink_muted).truncate().child(subtitle(&self.tree, &self.edit.id, self.sex))),
            )
            .child(
                div()
                    .id("edit-close")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .text_color(t.ink_muted)
                    .hover(|s| s.bg(t.canvas).text_color(t.ink))
                    .child(Icon::new(IconName::Close).small())
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(EditDialogEvent::Cancel))),
            )
    }

    fn render_tabs(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = [
            (Tab::Data, i18n::t!("dialogs.edit.tab.data")),
            (Tab::Events, i18n::t!("dialogs.edit.tab.events", n = self.event_edits.iter().filter(|f| !f.delete).count())),
            (Tab::Family, i18n::t!("dialogs.edit.tab.family")),
            (Tab::Notes, i18n::t!("dialogs.edit.tab.notes")),
            (Tab::Photos, i18n::t!("dialogs.edit.tab.photos")),
        ];
        let mut row = h_flex().gap_5().px_5().border_b_1().border_color(t.border);
        for (i, (tab, label)) in tabs.into_iter().enumerate() {
            let active = self.tab == tab;
            row = row.child(
                div()
                    .id(("edit-tab", i))
                    .py_2()
                    .border_b_2()
                    .border_color(if active { t.ink } else { transparent_black() })
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if active { t.ink } else { t.ink_muted })
                    .cursor_pointer()
                    .hover(|s| s.text_color(t.ink))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        this.close_popups(cx);
                        cx.notify();
                    })),
            );
        }
        row
    }

    fn render_data(&self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let mut sex_row = h_flex().p(px(3.)).gap(px(2.)).rounded(px(10.)).bg(t.canvas).w_full();
        for (i, (sex, key)) in [(Sex::Male, "dialogs.sex.male"), (Sex::Female, "dialogs.sex.female"), (Sex::Unknown, "dialogs.sex.unknown")].into_iter().enumerate() {
            let active = self.sex == sex;
            let (bg, fg) = t.sex(sex);
            sex_row = sex_row.child(
                div()
                    .id(("edit-sex", i))
                    .flex_1()
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .when(active, |d| d.bg(bg).text_color(fg).shadow_sm())
                    .when(!active, |d| d.text_color(t.ink_muted).hover(|s| s.text_color(t.ink)))
                    .child(i18n::tr(key, &[]))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.sex = sex;
                        cx.notify();
                    })),
            );
        }

        v_flex()
            .gap_4()
            .child(h_flex().gap_3().child(field(t, i18n::t!("dialogs.edit.given"), Input::new(&self.given).small())).child(field(t, i18n::t!("dialogs.edit.surname"), Input::new(&self.surname).small())))
            .child(field(t, i18n::t!("dialogs.edit.sex"), sex_row))
            .child(self.render_birth(t, cx))
            .child(self.render_death(t, cx))
            .into_any_element()
    }

    fn render_birth(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let d = &self.birth;
        card(t, i18n::t!("dialogs.edit.birth"))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().w(px(170.)).flex_none().child(field(t, i18n::t!("dialogs.edit.precision"), self.precision_select(Slot::Birth, t, cx))))
                    .child(field(t, i18n::t!("dialogs.edit.date"), self.date_inputs(Slot::Birth, t))),
            )
            .child(self.saved_as(Slot::Birth, t, cx))
            .child(field(t, i18n::t!("dialogs.edit.place"), self.place_input(Slot::Birth, &d.place, t, cx)))
    }

    fn render_death(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let d = &self.death;
        card(t, i18n::t!("dialogs.edit.death"))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(div().w(px(150.)).flex_none().child(field(t, i18n::t!("dialogs.edit.precision"), self.precision_select(Slot::Death, t, cx))))
                    .child(div().w(px(d.precision_width())).flex_none().child(field(t, i18n::t!("dialogs.edit.date"), self.date_inputs(Slot::Death, t))))
                    .child(field(t, i18n::t!("dialogs.edit.place"), self.place_input(Slot::Death, &d.place, t, cx))),
            )
            .child(self.saved_as(Slot::Death, t, cx))
    }

    fn date_inputs(&self, slot: Slot, t: &Tokens) -> impl IntoElement {
        let d = self.dates(slot);
        h_flex()
            .gap_2()
            .w_full()
            .child(div().flex_1().min_w_0().child(Input::new(&d.date).small()))
            .when(d.precision == Precision::Between, |row| {
                row.child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.and_date"))).child(div().flex_1().min_w_0().child(Input::new(&d.date2).small()))
            })
    }

    fn saved_as(&self, slot: Slot, t: &Tokens, cx: &App) -> impl IntoElement {
        let d = self.dates(slot);
        let phrase = date_phrase(d.precision, &d.date.read(cx).value(), &d.date2.read(cx).value());
        let mono = cx.theme().mono_font_family.clone();
        h_flex()
            .gap_1p5()
            .text_size(px(12.))
            .text_color(t.ink_muted)
            .child(i18n::t!("dialogs.edit.saved_as"))
            .child(if phrase.is_empty() {
                div().italic().child(i18n::t!("dialogs.edit.no_date"))
            } else {
                div().px_1p5().py_0p5().rounded(px(5.)).bg(t.canvas).border_1().border_color(t.border).text_color(t.ink).font_family(mono).child(phrase)
            })
    }

    fn precision_select(&self, slot: Slot, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let current = self.dates(slot).precision;
        let open = self.precision_menu == Some(slot);
        let mut options: Vec<Precision> = Precision::ALL.to_vec();
        if current == Precision::Phrase {
            options.push(Precision::Phrase);
        }
        let id = slot as usize;
        div()
            .relative()
            .w_full()
            .child(
                h_flex()
                    .id(("edit-precision", id))
                    .h(size::FIELD_H)
                    .px_2p5()
                    .gap_1()
                    .justify_between()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if open { t.border_strong } else { t.border })
                    .bg(t.surface)
                    .text_size(px(13.))
                    .text_color(t.ink)
                    .cursor_pointer()
                    .hover(|s| s.border_color(t.border_strong))
                    .child(div().truncate().child(current.label()))
                    .child(Icon::new(IconName::ChevronDown).xsmall().text_color(t.ink_muted))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.suggest = None;
                        this.precision_menu = if open { None } else { Some(slot) };
                        cx.notify();
                    })),
            )
            .when(open, |d| {
                let mut list = v_flex()
                    .id(("edit-precision-menu", id))
                    .absolute()
                    .top(size::FIELD_H + px(4.))
                    .left_0()
                    .min_w_full()
                    .p_1()
                    .rounded(px(10.))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface)
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.precision_menu = None;
                        cx.notify();
                    }));
                for (i, p) in options.into_iter().enumerate() {
                    list = list.child(
                        h_flex()
                            .id(("edit-precision-opt", i))
                            .px_2()
                            .h(px(28.))
                            .rounded(px(6.))
                            .text_size(px(13.))
                            .text_color(t.ink)
                            .cursor_pointer()
                            .when(p == current, |d| d.bg(t.accent_soft).text_color(t.accent).font_weight(FontWeight::MEDIUM))
                            .hover(|s| s.bg(t.canvas))
                            .child(p.label())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.dates_mut(slot).precision = p;
                                this.precision_menu = None;
                                cx.notify();
                            })),
                    );
                }
                d.child(deferred(list).with_priority(1))
            })
    }

    fn place_input(&self, slot: Slot, state: &Entity<InputState>, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let suggest = self.suggest.as_ref().filter(|s| s.slot == slot);
        div().relative().w_full().child(Input::new(state).small()).when_some(suggest, |d, s| {
            let mut list = v_flex()
                .id(("edit-suggest", slot as usize))
                .absolute()
                .top(size::FIELD_H + px(4.))
                .left_0()
                .w_full()
                .min_w(px(260.))
                .p_1()
                .rounded(px(10.))
                .border_1()
                .border_color(t.border)
                .bg(t.surface)
                .shadow_lg()
                .occlude();
            for (i, (place, uses)) in s.items.iter().enumerate() {
                let selected = s.selected == Some(i);
                list = list.child(
                    h_flex()
                        .id(("edit-suggest-row", i))
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .when(selected, |d| d.bg(t.accent_soft))
                        .hover(|s| s.bg(t.canvas))
                        .child(Icon::new(IconName::Map).xsmall().text_color(t.ink_muted))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(t.ink).child(place.clone()))
                        .child(div().flex_none().text_size(px(12.)).text_color(t.ink_muted).child(i18n::tn!("dialogs.edit.place_used", *uses)))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.pick_suggestion(i, window, cx);
                        })),
                );
            }
            d.child(deferred(list).with_priority(1))
        })
    }

    fn render_events(&self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        let person = self.edit.id.clone();
        let unions: Vec<(String, String)> = self
            .tree
            .unions(&person)
            .iter()
            .map(|(f, p, _)| (f.id.clone(), p.map(display_name).unwrap_or_else(|| i18n::t!("dialogs.edit.events.no_partner"))))
            .collect();
        let add = ui::menu_button(t, "event-add", i18n::t!("dialogs.edit.events.add")).dropdown_menu(move |mut menu, _, _| {
            menu = menu.scrollable(true).max_h(px(380.));
            for (fam, name) in &unions {
                for tag in FAMILY_EVENT_TAGS {
                    let (w, fam) = (weak.clone(), fam.clone());
                    let label = i18n::t!("dialogs.edit.events.with", label = gedcom::tree::event_label(tag), name = name);
                    menu = menu.item(PopupMenuItem::new(label).on_click(move |_, window, cx| {
                        w.update(cx, |this, cx| this.add_fact(fam.clone(), tag, window, cx)).ok();
                    }));
                }
                menu = menu.separator();
            }
            for tag in PERSON_EVENT_TAGS {
                let (w, p) = (weak.clone(), person.clone());
                menu = menu.item(PopupMenuItem::new(gedcom::tree::event_label(tag)).on_click(move |_, window, cx| {
                    w.update(cx, |this, cx| this.add_fact(p.clone(), tag, window, cx)).ok();
                }));
            }
            menu
        });

        let mut list = v_flex();
        let shown: Vec<usize> = (0..self.event_edits.len()).collect();
        if shown.is_empty() {
            list = list.child(empty_state(t, IconName::Calendar, i18n::t!("dialogs.edit.events.empty")));
        }
        for i in shown {
            let f = &self.event_edits[i];
            let open = self.fact_open == Some(i);
            let locked = self.in_data_tab(f);
            let (date, place) = if open { (String::new(), String::new()) } else { (f.date.clone(), f.place.clone()) };
            let year = gedcom::year_of(&date).map(|y| y.to_string()).unwrap_or_else(|| "—".into()); // i18n-ok
            let sub = if f.delete {
                i18n::t!("dialogs.edit.events.deleted")
            } else if locked {
                i18n::t!("dialogs.edit.events.in_data")
            } else if place.is_empty() && !open {
                i18n::t!("dialogs.edit.events.no_place")
            } else {
                place
            };
            let head = h_flex()
                .id(("fact-row", i))
                .gap_3()
                .py_2()
                .cursor_pointer()
                .when(f.delete, |d| d.opacity(0.5))
                .child(div().w(px(44.)).flex_none().text_size(px(12.)).text_color(t.ink_muted).child(if open { String::new() } else { year }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).when(f.delete, |d| d.line_through()).child(self.event_title(f)))
                        .when(!open, |d| d.child(div().text_size(px(12.)).text_color(t.ink_muted).truncate().child(sub))),
                )
                .child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight }).xsmall().text_color(t.ink_muted))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if locked {
                        this.tab = Tab::Data;
                        cx.notify();
                    } else {
                        this.open_fact(if open { None } else { Some(i) }, window, cx);
                    }
                }));
            let mut row = v_flex().border_b_1().border_color(t.border).child(head);
            if open {
                let fi = &self.fact_inputs;
                let custom = matches!(f.tag.as_str(), "EVEN" | "FACT");
                let typed = event_date(&fi.date.read(cx).value());
                let deleted = f.delete;
                row = row.child(
                    v_flex()
                        .gap_3()
                        .pb_3()
                        .pl(px(56.))
                        .when(custom, |d| d.child(field(t, i18n::t!("dialogs.edit.events.kind"), Input::new(&fi.kind).small())))
                        .child(field(t, i18n::t!("dialogs.edit.events.value"), Input::new(&fi.value).small()))
                        .child(
                            h_flex()
                                .gap_3()
                                .items_start()
                                .child(div().w(px(200.)).flex_none().child(field(t, i18n::t!("dialogs.edit.date"), Input::new(&fi.date).small())))
                                .child(field(t, i18n::t!("dialogs.edit.place"), Input::new(&fi.place).small())),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(div().flex_1().text_size(px(12.)).text_color(t.ink_muted).when(!typed.is_empty(), |d| d.child(i18n::t!("dialogs.edit.events.saved_as", date = typed))))
                                .child(
                                    button("fact-delete", if deleted { i18n::t!("dialogs.edit.events.undelete") } else { i18n::t!("dialogs.edit.events.delete") }, false, t).on_click(cx.listener(move |this, _, window, cx| {
                                        this.event_edits = this.current_facts(cx);
                                        if this.event_edits[i].fact.is_none() {
                                            this.event_edits.remove(i);
                                            this.fact_open = None;
                                        } else {
                                            this.event_edits[i].delete = !this.event_edits[i].delete;
                                            this.open_fact(None, window, cx);
                                        }
                                        cx.notify();
                                    })),
                                ),
                        ),
                );
            }
            list = list.child(row);
        }
        v_flex()
            .gap_3()
            .child(h_flex().justify_between().items_center().child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.events.hint"))).child(add))
            .child(list)
            .into_any_element()
    }

    fn render_family(&self, t: &Tokens) -> AnyElement {
        let id = self.edit.id.as_str();
        let parents = self.tree.parents(id);
        let unions = self.tree.unions(id);
        let partners: Vec<_> = unions.iter().filter_map(|(_, p, _)| *p).collect();
        let children: Vec<_> = unions.iter().flat_map(|(_, _, c)| c.iter().copied()).collect();
        if parents.is_empty() && partners.is_empty() && children.is_empty() {
            return empty_state(t, IconName::User, i18n::t!("dialogs.edit.family.empty")).into_any_element();
        }
        let section = |key: &str, people: &[&gedcom::Person]| {
            let mut col = v_flex().gap_1().child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(i18n::tr(key, &[])));
            for p in people {
                let (bg, fg) = t.sex(p.sex);
                col = col.child(
                    h_flex()
                        .gap_2p5()
                        .py_1()
                        .child(avatar(&theme::initials(&p.given, &p.surname), size::AVATAR_ROW, bg, fg))
                        .child(v_flex().min_w_0().child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).truncate().child(display_name(p))).child(div().text_size(px(12.)).text_color(t.ink_muted).child(years(p)))),
                );
            }
            col
        };
        v_flex()
            .gap_4()
            .when(!parents.is_empty(), |d| d.child(section("dialogs.edit.family.parents", &parents)))
            .when(!partners.is_empty(), |d| d.child(section("dialogs.edit.family.partners", &partners)))
            .when(!children.is_empty(), |d| d.child(section("dialogs.edit.family.children", &children)))
            .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.family.hint")))
            .into_any_element()
    }

    fn render_notes(&self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_4()
            .child(field(t, i18n::t!("dialogs.edit.note"), Textarea::new(&self.note).h(px(180.))))
            .child(self.render_sources(t, cx))
            .into_any_element()
    }

    fn render_sources(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mut list = v_flex();
        if self.citations.is_empty() {
            list = list.child(div().text_size(px(12.)).italic().text_color(t.ink_muted).child(i18n::t!("dialogs.edit.sources.empty")));
        }
        for (i, c) in self.citations.iter().enumerate() {
            let detail: Vec<String> = [c.page.clone(), self.fact_label(&c.fact)].into_iter().filter(|s| !s.trim().is_empty()).collect();
            list = list.child(
                h_flex()
                    .gap_2p5()
                    .items_start()
                    .py_2()
                    .border_b_1()
                    .border_color(t.border)
                    .child(div().pt(px(2.)).child(Icon::new(IconName::Inbox).xsmall().text_color(t.ink_muted)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).child(c.title.clone()))
                            .child(div().text_size(px(12.)).text_color(t.ink_muted).child(detail.join(" · "))),
                    )
                    .child(
                        div()
                            .id(("cite-remove", i))
                            .size(px(22.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_color(t.ink_muted)
                            .hover(|s| s.bg(t.canvas).text_color(t.danger))
                            .child(Icon::new(IconName::Close).xsmall())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.citations.remove(i);
                                cx.notify();
                            })),
                    ),
            );
        }

        let mut facts = h_flex().flex_wrap().gap_1p5();
        let options: Vec<(Option<FactRef>, String)> = std::iter::once((None, i18n::t!("dialogs.edit.sources.general")))
            .chain(self.facts.iter().map(|o| (Some(o.fact.clone()), match o.year {
                Some(y) => format!("{} · {y}", o.label),
                None => o.label.clone(),
            })))
            .collect();
        for (i, (fact, label)) in options.into_iter().enumerate() {
            let active = self.cite_fact == fact;
            facts = facts.child(
                div()
                    .id(("cite-fact", i))
                    .px_2()
                    .py_0p5()
                    .rounded(px(999.))
                    .border_1()
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(active, |d| d.border_color(t.accent).bg(t.accent_soft).text_color(t.accent).font_weight(FontWeight::MEDIUM))
                    .when(!active, |d| d.border_color(t.border).text_color(t.ink_muted).hover(|s| s.text_color(t.ink).border_color(t.border_strong)))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.cite_fact = fact.clone();
                        cx.notify();
                    })),
            );
        }

        let query = self.cite_source.read(cx).value().trim().to_string();
        let exact = !query.is_empty() && self.sources.iter().any(|(_, t)| fold(t) == fold(&query));
        let matches: Vec<String> = if self.source_suggest && !query.is_empty() {
            let q = fold(&query);
            self.sources.iter().filter(|(_, t)| fold(t).contains(&q) && fold(t) != q).take(6).map(|(_, t)| t.clone()).collect()
        } else {
            vec![]
        };
        let source_input = div().relative().w_full().child(Input::new(&self.cite_source).small()).when(!matches.is_empty(), |d| {
            let mut menu = v_flex()
                .id("cite-suggest")
                .absolute()
                .top(size::FIELD_H + px(4.))
                .left_0()
                .w_full()
                .p_1()
                .rounded(px(10.))
                .border_1()
                .border_color(t.border)
                .bg(t.surface)
                .shadow_lg()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.source_suggest = false;
                    cx.notify();
                }));
            for (i, title) in matches.into_iter().enumerate() {
                menu = menu.child(
                    h_flex()
                        .id(("cite-suggest-row", i))
                        .gap_2()
                        .px_2()
                        .py_1p5()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(t.canvas))
                        .child(Icon::new(IconName::Inbox).xsmall().text_color(t.ink_muted))
                        .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(t.ink).child(title.clone()))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            let title = title.clone();
                            this.cite_source.update(cx, |s, cx| s.set_value(title, window, cx));
                            this.source_suggest = false;
                            cx.notify();
                        })),
                );
            }
            d.child(deferred(menu).with_priority(1))
        });

        card(t, i18n::t!("dialogs.edit.sources"))
            .child(list)
            .child(div().pt_1().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.sources.add")))
            .child(
                h_flex()
                    .gap_3()
                    .items_start()
                    .child(field(t, i18n::t!("dialogs.edit.sources.source"), source_input))
                    .child(div().w(px(170.)).flex_none().child(field(t, i18n::t!("dialogs.edit.sources.page"), Input::new(&self.cite_page).small()))),
            )
            .when(!query.is_empty() && !exact, |d| d.child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.sources.new", title = query.clone()))))
            .child(field(t, i18n::t!("dialogs.edit.sources.fact"), facts))
            .child(h_flex().justify_end().child(button("cite-add", i18n::t!("dialogs.edit.sources.add_button"), false, t).on_click(cx.listener(|this, _, window, cx| this.add_citation(window, cx)))))
    }

    fn render_footer(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.edit.id.clone();
        let confirming = self.confirm_delete;
        h_flex()
            .gap_3()
            .px_5()
            .py_3()
            .border_t_1()
            .border_color(t.border)
            .child(
                div()
                    .id("edit-delete")
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(t.danger)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(if confirming { i18n::t!("dialogs.edit.delete_confirm") } else { i18n::t!("dialogs.edit.delete") })
                    .on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, _, cx| {
                            if this.confirm_delete {
                                cx.emit(EditDialogEvent::Delete(id.clone()));
                            } else {
                                this.confirm_delete = true;
                                cx.notify();
                            }
                        }
                    })),
            )
            .when(!confirming, |d| {
                d.child(
                    div()
                        .id("edit-gedcom")
                        .text_size(px(13.))
                        .text_color(t.ink_muted)
                        .cursor_pointer()
                        .hover(|s| s.text_color(t.ink).underline())
                        .child(i18n::t!("dialogs.edit.view_gedcom"))
                        .on_click(cx.listener(move |_, _, _, cx| cx.emit(EditDialogEvent::ViewGedcom(id.clone())))),
                )
            })
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.edit.save_hint")))
            .child(button("edit-cancel", i18n::t!("dialogs.edit.cancel"), false, t).on_click(cx.listener(|_, _, _, cx| cx.emit(EditDialogEvent::Cancel))))
            .child(button("edit-save", i18n::t!("dialogs.edit.save"), true, t).on_click(cx.listener(|this, _, _, cx| this.save(cx))))
    }
}

impl DateInputs {
    fn precision_width(&self) -> f32 {
        if self.precision == Precision::Between { 220. } else { 130. }
    }
}

impl Render for EditDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_locale(window, cx);
        let t = theme::tokens(cx);
        let body = match self.tab {
            Tab::Data => self.render_data(&t, cx),
            Tab::Events => self.render_events(&t, cx),
            Tab::Family => self.render_family(&t),
            Tab::Notes => self.render_notes(&t, cx),
            Tab::Photos => empty_state(&t, IconName::Frame, i18n::t!("dialogs.edit.photos_soon")).into_any_element(),
        };
        let max_h = window.viewport_size().height * 0.9;
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_save))
            .on_action(cx.listener(Self::on_escape))
            .on_action(cx.listener(|this, _: &SuggestUp, _, cx| this.on_up_down(-1, cx)))
            .on_action(cx.listener(|this, _: &SuggestDown, _, cx| this.on_up_down(1, cx)))
            .on_action(cx.listener(Self::on_pick))
            .child(
                div()
                    .id("edit-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(t.backdrop())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(EditDialogEvent::Cancel))),
            )
            .child(
                v_flex()
                    .id("edit-modal")
                    .w(size::DIALOG_W)
                    .max_h(max_h)
                    .rounded(size::DIALOG_RADIUS)
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface)
                    .shadow_lg()
                    .occlude()
                    .child(self.render_header(&t, cx))
                    .child(self.render_tabs(&t, cx))
                    .child(div().id("edit-body").flex_1().min_h_0().overflow_y_scroll().px_5().py_4().child(body))
                    .child(self.render_footer(&t, cx)),
            )
    }
}

// ---- small building blocks ----

fn avatar(initials: &str, side: Pixels, bg: Hsla, fg: Hsla) -> Div {
    div()
        .size(side)
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(bg)
        .text_color(fg)
        .text_size(side * 0.38)
        .font_weight(FontWeight::SEMIBOLD)
        .child(initials.to_string())
}

fn field(t: &Tokens, label: String, control: impl IntoElement) -> Div {
    v_flex().flex_1().min_w_0().gap_1().child(div().text_size(px(12.)).text_color(t.ink_muted).child(label)).child(control)
}

fn card(t: &Tokens, title: String) -> Div {
    v_flex().gap_3().p_4().rounded(px(12.)).border_1().border_color(t.border).child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink).child(title))
}

fn button(id: &'static str, label: String, primary: bool, t: &Tokens) -> Stateful<Div> {
    let base = div().id(id).h(size::FIELD_H).px_3p5().flex().items_center().rounded(px(8.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).cursor_pointer().child(label);
    if primary {
        let accent = t.accent;
        base.bg(accent).text_color(white()).hover(move |s| s.bg(accent.opacity(0.9)))
    } else {
        base.border_1().border_color(t.border).text_color(t.ink).hover(|s| s.border_color(t.border_strong))
    }
}

fn empty_state(t: &Tokens, icon: IconName, text: String) -> Div {
    v_flex().items_center().justify_center().gap_2().py_10().text_color(t.ink_muted).child(Icon::new(icon).large()).child(div().text_size(px(13.)).text_center().child(text))
}

/// Event date typed by the user → GEDCOM phrase: «12 mar 1852» → «12 MAR 1852», «abt 1850» or
/// «hacia 1850» → «ABT 1850», «entre 1850 y 1860» → «BET 1850 AND 1860». Anything else is kept.
fn event_date(input: &str) -> String {
    let s = input.trim();
    let lower = fold(s);
    let prefixes: &[(&[&str], &str)] = &[
        (&["abt ", "about ", "hacia ", "circa ", "ca ", "c ", "vers ", "cerca de ", "aprox "], "ABT"),
        (&["bef ", "before ", "antes de ", "avant ", "antes "], "BEF"),
        (&["aft ", "after ", "despues de ", "apres ", "depois de "], "AFT"),
        (&["est ", "estimated "], "EST"),
        (&["cal ", "calculated "], "CAL"),
    ];
    for (words, tag) in prefixes {
        for w in *words {
            if lower.starts_with(w) {
                let rest = &s[s.char_indices().nth(w.chars().count()).map(|(i, _)| i).unwrap_or(s.len())..];
                return format!("{tag} {}", normalize_date(rest));
            }
        }
    }
    for (bet, and) in [("bet ", " and "), ("between ", " and "), ("entre ", " y "), ("entre ", " e "), ("entre ", " et ")] {
        if lower.starts_with(bet) {
            if let Some(k) = lower.find(and) {
                let (a, b) = (&lower[bet.len()..k], &lower[k + and.len()..]);
                return format!("BET {} AND {}", normalize_date(a), normalize_date(b));
            }
        }
    }
    normalize_date(s)
}

fn display_name(p: &gedcom::Person) -> String {
    let n = format!("{} {}", p.given.trim(), p.surname.trim());
    let n = n.trim();
    if n.is_empty() { i18n::t!("dialogs.unnamed") } else { n.to_string() }
}

fn years(p: &gedcom::Person) -> String {
    match (p.birth_year(), p.death_year()) {
        (Some(b), Some(d)) => format!("{b} – {d}"),
        (Some(b), None) => format!("{b} –"),
        (None, Some(d)) => format!("– {d}"),
        (None, None) => i18n::t!("dialogs.no_dates"),
    }
}

/// «I12 · hija de A y B», «I12 · pareja de C», or just the id.
fn subtitle(tree: &Tree, id: &str, sex: Sex) -> String {
    let (f, m) = tree.father_mother(id);
    let parents: Vec<String> = [f, m].into_iter().flatten().map(display_name).collect();
    let relation = match parents.as_slice() {
        [] => tree.unions(id).iter().find_map(|(_, p, _)| *p).map(|p| {
            let name = display_name(p);
            i18n::tv!("dialogs.edit.partner_of", name, name = name)
        }),
        [one] => Some(child_of(sex, one)),
        [a, b, ..] => Some(child_of(sex, &i18n::t!("dialogs.edit.and", a = a, b = b))),
    };
    match relation {
        Some(r) => format!("{id} · {r}"),
        None => id.to_string(),
    }
}

fn child_of(sex: Sex, parents: &str) -> String {
    let key = match sex {
        Sex::Male => "dialogs.edit.son_of",
        Sex::Female => "dialogs.edit.daughter_of",
        Sex::Unknown => "dialogs.edit.child_of",
    };
    i18n::tr_v(key, parents, &[("parents", &parents)])
}

/// Move a selection by `delta` over `len` rows, wrapping; None selects the first/last row.
pub(crate) fn step(selected: Option<usize>, len: usize, delta: isize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let len = len as isize;
    let next = match selected {
        None if delta >= 0 => 0,
        None => len - 1,
        Some(i) => (i as isize + delta).rem_euclid(len),
    };
    Some(next as usize)
}

struct DateFields {
    precision: Precision,
    date: String,
    date2: String,
    place: String,
}

/// GEDCOM phrase for the date fields; free text is kept verbatim, typed dates are normalized.
fn date_phrase(precision: Precision, date: &str, date2: &str) -> String {
    let (date, date2) = match precision {
        Precision::Phrase => (date.trim().to_string(), String::new()),
        Precision::Between => (normalize_date(date), normalize_date(date2)),
        _ => (normalize_date(date), String::new()),
    };
    if date.is_empty() && date2.is_empty() {
        return String::new();
    }
    DateValue { precision, date, date2 }.to_gedcom()
}

fn build_edit(base: &PersonEdit, given: &str, surname: &str, sex: Sex, birth: &DateFields, death: &DateFields, note: &str) -> PersonEdit {
    let event = |base: &EventEdit, f: &DateFields, default_tag: &str| EventEdit {
        tag: if base.tag.is_empty() { default_tag.to_string() } else { base.tag.clone() },
        date: date_phrase(f.precision, &f.date, &f.date2),
        place: f.place.trim().to_string(),
    };
    PersonEdit {
        id: base.id.clone(),
        given: given.trim().to_string(),
        surname: surname.trim().to_string(),
        sex,
        birth: event(&base.birth, birth, "BIRT"),
        death: event(&base.death, death, "DEAT"),
        note: note.trim_end().to_string(),
        citations: base.citations.clone(),
        facts: base.facts.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::{DateFields, build_edit, date_phrase, event_date, step, subtitle};
    use gedcom::{EventEdit, PersonEdit, Precision, Sex, Tree};

    fn fields(date: &str, place: &str) -> DateFields {
        DateFields { precision: Precision::Exact, date: date.into(), date2: String::new(), place: place.into() }
    }

    #[test]
    fn edit_from_fields_keeps_tags_and_trims() {
        let base = PersonEdit { id: "I7".into(), birth: EventEdit { tag: "CHR".into(), ..Default::default() }, ..Default::default() };
        let e = build_edit(&base, "  Ana ", " Ruiz", Sex::Female, &fields("1852", " Villa Norte "), &fields("", ""), "note\n");
        assert_eq!(e.id, "I7");
        assert_eq!((e.given.as_str(), e.surname.as_str()), ("Ana", "Ruiz"));
        assert_eq!(e.sex, Sex::Female);
        assert_eq!(e.birth.tag, "CHR");
        assert_eq!(e.birth.date, "1852");
        assert_eq!(e.birth.place, "Villa Norte");
        assert_eq!(e.death.tag, "DEAT");
        assert_eq!(e.death.date, "");
        assert_eq!(e.note, "note");
    }

    #[test]
    fn empty_dates_stay_empty_for_any_precision() {
        for p in Precision::ALL {
            assert_eq!(date_phrase(p, "  ", ""), "");
        }
        assert_eq!(date_phrase(Precision::Phrase, " circa la guerra ", ""), "circa la guerra");
    }

    #[test]
    fn event_dates_from_free_text() {
        for (inp, out) in [
            ("12 mar 1852", "12 MAR 1852"),
            ("hacia 1850", "ABT 1850"),
            ("abt 1850", "ABT 1850"),
            ("después de marzo 1900", "AFT MAR 1900"),
            ("antes de 1800", "BEF 1800"),
            ("entre 1850 y 1860", "BET 1850 AND 1860"),
            ("BET 1850 AND 1860", "BET 1850 AND 1860"),
            ("ABT 1935", "ABT 1935"),
            ("", ""),
        ] {
            assert_eq!(event_date(inp), out, "{inp}");
        }
    }

    #[test]
    fn selection_wraps() {
        assert_eq!(step(None, 0, 1), None);
        assert_eq!(step(None, 3, 1), Some(0));
        assert_eq!(step(None, 3, -1), Some(2));
        assert_eq!(step(Some(2), 3, 1), Some(0));
        assert_eq!(step(Some(0), 3, -1), Some(2));
    }

    #[test]
    fn subtitle_names_parents() {
        let ged = "0 HEAD\n0 @I1@ INDI\n1 NAME Ana /Ruiz/\n1 SEX F\n1 FAMC @F1@\n0 @I2@ INDI\n1 NAME Luis /Ruiz/\n1 SEX M\n1 FAMS @F1@\n0 @I3@ INDI\n1 NAME Eva /Sol/\n0 @F1@ FAM\n1 HUSB @I2@\n1 CHIL @I1@\n0 @I4@ INDI\n1 NAME Juno /Sol/\n1 FAMC @F2@\n0 @F2@ FAM\n1 WIFE @I3@\n1 CHIL @I4@\n0 TRLR\n";
        let tree = Tree::parse(ged).unwrap();
        i18n::set_locale("es");
        assert_eq!(subtitle(&tree, "I1", Sex::Female), "I1 · hija de Luis Ruiz");
        assert_eq!(subtitle(&tree, "I3", Sex::Unknown), "I3");
        i18n::set_locale("fr");
        assert_eq!(subtitle(&tree, "I1", Sex::Female), "I1 · fille de Luis Ruiz");
        assert_eq!(subtitle(&tree, "I4", Sex::Male), "I4 · fils d'Eva Sol");
        i18n::set_locale("es");
    }
}
