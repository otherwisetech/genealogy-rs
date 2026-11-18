//! ⌘K palette (docs/DESIGN.md, artboard 3).
use crate::theme::{self, Tokens, size};
use gedcom::{PaletteResults, SearchFilter, Tree, palette_search};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};
use std::ops::Range;
use std::rc::Rc;

pub enum PaletteEvent {
    /// ↵ on a person: make them the focus of the tree
    Focus(String),
    /// ⌘↵ on a person: open the edit dialog
    Edit(String),
    /// ↵ on a place: host may filter/highlight; at minimum focus the first person with an event there
    Place(String),
    /// «Crear persona «query»» / ⌘N — the raw query text, host splits it into names
    CreatePerson(String),
    /// esc or backdrop click
    Dismiss,
}

const CONTEXT: &str = "Palette";
const LIMIT: usize = 8;

actions!(palette, [SelectPrev, SelectNext, Confirm, ConfirmSecondary, Create, Dismiss, NextFilter, PrevFilter, Filter1, Filter2, Filter3, Filter4, Filter5]);

/// Key bindings of the palette; call once at startup.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrev, Some(CONTEXT)),
        KeyBinding::new("down", SelectNext, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm, Some(CONTEXT)),
        KeyBinding::new("secondary-enter", ConfirmSecondary, Some(CONTEXT)),
        KeyBinding::new("secondary-n", Create, Some(CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(CONTEXT)),
        KeyBinding::new("tab", NextFilter, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PrevFilter, Some(CONTEXT)),
        KeyBinding::new("secondary-1", Filter1, Some(CONTEXT)),
        KeyBinding::new("secondary-2", Filter2, Some(CONTEXT)),
        KeyBinding::new("secondary-3", Filter3, Some(CONTEXT)),
        KeyBinding::new("secondary-4", Filter4, Some(CONTEXT)),
        KeyBinding::new("secondary-5", Filter5, Some(CONTEXT)),
    ]);
}

pub(crate) const FILTERS: [(SearchFilter, &str); 5] = [
    (SearchFilter::All, "dialogs.palette.filter.all"),
    (SearchFilter::People, "dialogs.palette.filter.people"),
    (SearchFilter::Places, "dialogs.palette.filter.places"),
    (SearchFilter::NoDates, "dialogs.palette.filter.no_dates"),
    (SearchFilter::NoParents, "dialogs.palette.filter.no_parents"),
];

/// Filter `delta` chips away from `current`, wrapping.
pub(crate) fn cycle_filter(current: SearchFilter, delta: isize) -> SearchFilter {
    let i = FILTERS.iter().position(|(f, _)| *f == current).unwrap_or(0) as isize;
    FILTERS[(i + delta).rem_euclid(FILTERS.len() as isize) as usize].0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Row {
    Person(usize),
    Place(usize),
    Create,
}

/// Selectable rows in display order: people, places, then «Crear persona» for a non-empty query.
pub(crate) fn rows(results: &PaletteResults, query: &str) -> Vec<Row> {
    let mut out: Vec<Row> = (0..results.people.len()).map(Row::Person).collect();
    out.extend((0..results.places.len()).map(Row::Place));
    if !query.trim().is_empty() {
        out.push(Row::Create);
    }
    out
}

pub(crate) fn wrap(selected: usize, len: usize, delta: isize) -> usize {
    if len == 0 { 0 } else { (selected as isize + delta).rem_euclid(len as isize) as usize }
}

/// Sorted, merged, in-bounds highlight ranges snapped to char boundaries.
pub(crate) fn hl_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<Range<usize>> {
    let snap_down = |mut i: usize| {
        i = i.min(text.len());
        while !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    };
    let snap_up = |mut i: usize| {
        i = i.min(text.len());
        while !text.is_char_boundary(i) {
            i += 1;
        }
        i
    };
    let mut rs: Vec<Range<usize>> = ranges.iter().map(|r| snap_down(r.start)..snap_up(r.end)).filter(|r| r.start < r.end).collect();
    rs.sort_by_key(|r| r.start);
    let mut out: Vec<Range<usize>> = Vec::with_capacity(rs.len());
    for r in rs {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

fn highlighted(text: &str, ranges: &[Range<usize>], bg: Hsla) -> StyledText {
    let style = HighlightStyle { background_color: Some(bg), ..Default::default() };
    StyledText::new(text.to_string()).with_highlights(hl_ranges(text, ranges).into_iter().map(|r| (r, style)))
}

pub struct Palette {
    tree: Rc<Tree>,
    focus_person: Option<String>,
    focus: FocusHandle,
    input: Entity<InputState>,
    filter: SearchFilter,
    results: PaletteResults,
    selected: usize,
    synced_locale: &'static str,
    _subs: Vec<Subscription>,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Palette {
    pub fn new(tree: Rc<Tree>, focus_person: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.refresh(cx);
            }
        })];
        let mut palette = Palette {
            tree,
            focus_person,
            focus: cx.focus_handle(),
            input,
            filter: SearchFilter::All,
            results: PaletteResults::default(),
            selected: 0,
            synced_locale: "",
            _subs: subs,
        };
        palette.refresh(cx);
        palette
    }

    /// Focus the query input; the host calls this right after opening.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| s.focus(window, cx));
    }

    fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx);
        self.results = palette_search(&self.tree, query.trim(), self.filter, self.focus_person.as_deref(), LIMIT);
        self.selected = 0;
        cx.notify();
    }

    fn set_filter(&mut self, filter: SearchFilter, cx: &mut Context<Self>) {
        if self.filter != filter {
            self.filter = filter;
            self.refresh(cx);
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let len = rows(&self.results, &self.query(cx)).len();
        self.selected = wrap(self.selected, len, delta);
        cx.notify();
    }

    fn activate(&mut self, row: Row, secondary: bool, cx: &mut Context<Self>) {
        let ev = match row {
            Row::Person(i) => {
                let Some(hit) = self.results.people.get(i) else { return };
                if secondary { PaletteEvent::Edit(hit.id.clone()) } else { PaletteEvent::Focus(hit.id.clone()) }
            }
            Row::Place(i) => {
                let Some(hit) = self.results.places.get(i) else { return };
                PaletteEvent::Place(hit.place.clone())
            }
            Row::Create => PaletteEvent::CreatePerson(self.query(cx).trim().to_string()),
        };
        cx.emit(ev);
    }

    fn confirm(&mut self, secondary: bool, cx: &mut Context<Self>) {
        let rows = rows(&self.results, &self.query(cx));
        if let Some(row) = rows.get(self.selected).copied() {
            self.activate(row, secondary, cx);
        }
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let query = self.query(cx).trim().to_string();
        if !query.is_empty() {
            cx.emit(PaletteEvent::CreatePerson(query));
        }
    }

    /// The placeholder and the result subtitles are stored text: rebuilt when the language changes.
    fn sync_locale(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = i18n::locale();
        if self.synced_locale != locale {
            let first = self.synced_locale.is_empty();
            self.synced_locale = locale;
            let text = i18n::t!("dialogs.palette.placeholder");
            self.input.update(cx, |s, cx| s.set_placeholder(text, window, cx));
            if !first {
                let selected = self.selected;
                self.refresh(cx);
                self.selected = selected;
            }
        }
    }

    fn render_chips(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = h_flex().gap_1p5().px_4().py_2p5().border_b_1().border_color(t.border).flex_wrap();
        for (i, (filter, key)) in FILTERS.iter().copied().enumerate() {
            let active = self.filter == filter;
            row = row.child(
                div()
                    .id(("palette-chip", i))
                    .px_2p5()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .rounded_full()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .when(active, |d| d.bg(t.ink).text_color(t.on_fill()))
                    .when(!active, |d| d.border_1().border_color(t.border).text_color(t.ink_muted).hover(|s| s.text_color(t.ink).border_color(t.border_strong)))
                    .child(i18n::tr(key, &[]))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx))),
            );
        }
        row
    }

    fn row_shell(&self, index: usize, row: Row, t: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.selected == index;
        h_flex()
            .id(("palette-row", index))
            .gap_2p5()
            .px_2()
            .py_1p5()
            .mx_2()
            .rounded(px(8.))
            .cursor_pointer()
            .when(selected, |d| d.bg(t.accent_soft))
            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                if this.selected != index {
                    this.selected = index;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| this.activate(row, ev.modifiers().secondary(), cx)))
    }

    fn enter_hint(&self, index: usize, t: &Tokens) -> impl IntoElement {
        div().flex_none().text_size(px(12.)).text_color(t.ink_muted).when(self.selected != index, |d| d.invisible()).child(i18n::t!("dialogs.key.enter"))
    }

    fn render_results(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.query(cx);
        let all = rows(&self.results, &query);
        let section = |text: String| div().px_4().pt_2p5().pb_1().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(text);
        let mut list = v_flex().id("palette-results").max_h(px(420.)).overflow_y_scroll().pb_2();

        if all.is_empty() {
            return list.child(div().px_4().py_6().text_center().text_size(px(13.)).text_color(t.ink_muted).child(i18n::t!("dialogs.palette.empty")));
        }
        if !self.results.people.is_empty() {
            list = list.child(section(i18n::t!("dialogs.palette.section.people", n = self.results.people.len())));
        }
        for (index, row) in all.iter().copied().enumerate() {
            match row {
                Row::Person(i) => {
                    let hit = &self.results.people[i];
                    let person = self.tree.person(&hit.id);
                    let (bg, fg) = t.sex(person.map(|p| p.sex).unwrap_or_default());
                    let initials = person.map(|p| theme::initials(&p.given, &p.surname)).unwrap_or_else(|| "?".into());
                    list = list.child(
                        self.row_shell(index, row, t, cx)
                            .child(
                                div()
                                    .size(size::AVATAR_ROW)
                                    .flex_none()
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(bg)
                                    .text_color(fg)
                                    .text_size(px(11.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(initials),
                            )
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).truncate().child(highlighted(&hit.name, &hit.name_hl, t.highlight)))
                                    .when(!hit.sub.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(t.ink_muted).truncate().child(highlighted(&hit.sub, &hit.sub_hl, t.highlight)))),
                            )
                            .child(self.enter_hint(index, t)),
                    );
                }
                Row::Place(i) => {
                    if i == 0 {
                        list = list.child(section(i18n::t!("dialogs.palette.section.places", n = self.results.places.len())));
                    }
                    let hit = &self.results.places[i];
                    list = list.child(
                        self.row_shell(index, row, t, cx)
                            .child(div().size(size::AVATAR_ROW).flex_none().rounded_full().flex().items_center().justify_center().bg(t.canvas).text_color(t.ink_muted).child(Icon::new(IconName::Map).small()))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).truncate().child(highlighted(&hit.place, &hit.hl, t.highlight)))
                                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::tn!("dialogs.palette.place_events", hit.events))),
                            )
                            .child(self.enter_hint(index, t)),
                    );
                }
                Row::Create => {
                    list = list.child(section(i18n::t!("dialogs.palette.section.actions"))).child(
                        self.row_shell(index, row, t, cx)
                            .child(div().size(size::AVATAR_ROW).flex_none().rounded_full().flex().items_center().justify_center().bg(t.accent_soft).text_color(t.accent).child(Icon::new(IconName::Plus).small()))
                            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).child(i18n::t!("dialogs.palette.create", query = query.trim())))
                            .child(kbd(i18n::t!("dialogs.key.cmd_n"), t)),
                    );
                }
            }
        }
        list
    }
}

fn kbd(text: String, t: &Tokens) -> Div {
    div().flex_none().px_1p5().py_0p5().rounded(px(5.)).border_1().border_color(t.border).bg(t.canvas).text_size(px(11.)).text_color(t.ink_muted).child(text)
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_locale(window, cx);
        let t = theme::tokens(cx);
        let footer = h_flex()
            .gap_3()
            .px_4()
            .py_2p5()
            .border_t_1()
            .border_color(t.border)
            .text_size(px(12.))
            .text_color(t.ink_muted)
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.up_down"), &t)).child(i18n::t!("dialogs.palette.footer.move")))
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.enter"), &t)).child(i18n::t!("dialogs.palette.footer.focus")))
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.cmd_enter"), &t)).child(i18n::t!("dialogs.palette.footer.edit")))
            .child(div().flex_1())
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.tab"), &t)).child(i18n::t!("dialogs.palette.footer.filters")));

        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &SelectPrev, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &Confirm, _, cx| this.confirm(false, cx)))
            .on_action(cx.listener(|this, _: &ConfirmSecondary, _, cx| this.confirm(true, cx)))
            .on_action(cx.listener(|this, _: &Create, _, cx| this.create(cx)))
            .on_action(cx.listener(|_, _: &Dismiss, _, cx| cx.emit(PaletteEvent::Dismiss)))
            .on_action(cx.listener(|this, _: &NextFilter, _, cx| this.set_filter(cycle_filter(this.filter, 1), cx)))
            .on_action(cx.listener(|this, _: &PrevFilter, _, cx| this.set_filter(cycle_filter(this.filter, -1), cx)))
            .on_action(cx.listener(|this, _: &Filter1, _, cx| this.set_filter(FILTERS[0].0, cx)))
            .on_action(cx.listener(|this, _: &Filter2, _, cx| this.set_filter(FILTERS[1].0, cx)))
            .on_action(cx.listener(|this, _: &Filter3, _, cx| this.set_filter(FILTERS[2].0, cx)))
            .on_action(cx.listener(|this, _: &Filter4, _, cx| this.set_filter(FILTERS[3].0, cx)))
            .on_action(cx.listener(|this, _: &Filter5, _, cx| this.set_filter(FILTERS[4].0, cx)))
            .child(
                div()
                    .id("palette-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(t.backdrop())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PaletteEvent::Dismiss))),
            )
            .child(div().h(window.viewport_size().height * 0.14).flex_none())
            .child(
                v_flex()
                    .id("palette-modal")
                    .w(size::PALETTE_W)
                    .rounded(size::DIALOG_RADIUS)
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface)
                    .shadow_lg()
                    .occlude()
                    .child(
                        h_flex()
                            .gap_2()
                            .px_4()
                            .py_2()
                            .border_b_1()
                            .border_color(t.border)
                            .child(Icon::new(IconName::Search).small().text_color(t.ink_muted))
                            .child(div().flex_1().child(Input::new(&self.input).appearance(false)))
                            .child(kbd(i18n::t!("dialogs.key.esc"), &t)),
                    )
                    .child(self.render_chips(&t, cx))
                    .child(self.render_results(&t, cx))
                    .child(footer),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Row, cycle_filter, hl_ranges, rows, wrap};
    use gedcom::{PaletteResults, PersonHit, PlaceHit, SearchFilter};

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn highlight_ranges_are_clean() {
        let s = "Ramón Ruiz";
        // 'ó' is two bytes (3..5); a range ending inside it snaps outward
        assert_eq!(hl_ranges(s, &[0..4]), vec![0..5]);
        assert_eq!(hl_ranges(s, &[7..9, 0..3, 2..4, 50..60, 3..3]), vec![0..5, 7..9]);
        assert_eq!(hl_ranges(s, &[8..99]), vec![8..s.len()]);
    }

    #[test]
    fn rows_and_wrapping() {
        let hit = |id: &str| PersonHit { id: id.into(), name: id.into(), name_hl: vec![], sub: String::new(), sub_hl: vec![] };
        let res = PaletteResults { people: vec![hit("I1"), hit("I2")], places: vec![PlaceHit { place: "X".into(), hl: vec![], events: 3 }] };
        assert_eq!(rows(&res, "an"), vec![Row::Person(0), Row::Person(1), Row::Place(0), Row::Create]);
        assert_eq!(rows(&res, "  "), vec![Row::Person(0), Row::Person(1), Row::Place(0)]);
        assert_eq!(wrap(3, 4, 1), 0);
        assert_eq!(wrap(0, 4, -1), 3);
        assert_eq!(wrap(0, 0, 1), 0);
    }

    #[test]
    fn filters_cycle() {
        assert_eq!(cycle_filter(SearchFilter::All, 1), SearchFilter::People);
        assert_eq!(cycle_filter(SearchFilter::All, -1), SearchFilter::NoParents);
        assert_eq!(cycle_filter(SearchFilter::NoParents, 1), SearchFilter::All);
    }
}
