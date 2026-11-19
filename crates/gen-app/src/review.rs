//! «Revisión» side panel: the tree's data-quality issues (`Tree::quality_issues`) grouped by
//! severity, then by kind. Non-modal: it sits between the canvas and the ficha so focusing a
//! person keeps the panel open. Like the dialogs it only emits events; the host acts on them.
use crate::app::GenApp;
use crate::theme::{self, Tokens, size};
use crate::ui;
use gedcom::{Issue, IssueKind, Severity, Tree};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{IconName, h_flex, v_flex};
use std::collections::HashSet;
use std::rc::Rc;

pub enum ReviewEvent {
    /// row click / ↵: focus this person on the canvas
    Focus(String),
    /// «Editar» / ⌘↵
    Edit(String),
    /// «Unir» on a possible duplicate
    Merge(String, String),
    /// «Ignorar» / ⌘⌫: hide this issue (by `Issue::key`) for the session
    Ignore(String),
    /// esc or ×
    Close,
}

const CONTEXT: &str = "Review";
/// rows shown per kind before «Ver N más»
const PER_KIND: usize = 30;

actions!(review, [ReviewPrev, ReviewNext, ReviewConfirm, ReviewEdit, ReviewIgnore, ReviewClose]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", ReviewPrev, Some(CONTEXT)),
        KeyBinding::new("down", ReviewNext, Some(CONTEXT)),
        KeyBinding::new("enter", ReviewConfirm, Some(CONTEXT)),
        KeyBinding::new("secondary-enter", ReviewEdit, Some(CONTEXT)),
        KeyBinding::new("secondary-backspace", ReviewIgnore, Some(CONTEXT)),
        KeyBinding::new("escape", ReviewClose, Some(CONTEXT)),
        KeyBinding::new("cmd-shift-r", ToggleReview, Some(crate::app::KEY_CONTEXT)),
    ]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Entry {
    Severity(Severity, usize),
    Kind(IssueKind, usize),
    /// index into the issue list
    Row(usize),
    /// «Ver N más» for a collapsed kind
    More(IssueKind, usize),
}

impl Entry {
    fn selectable(self) -> bool {
        matches!(self, Entry::Row(_) | Entry::More(..))
    }
}

/// Issues not ignored and matching `filter`.
pub(crate) fn visible<'a>(issues: &'a [Issue], ignored: &'a HashSet<String>, filter: Option<Severity>) -> impl Iterator<Item = (usize, &'a Issue)> + 'a {
    issues.iter().enumerate().filter(move |(_, i)| filter.is_none_or(|f| i.severity == f) && !ignored.contains(&i.key()))
}

/// Display list: a header per severity and per kind (issues arrive sorted by both), at most
/// `PER_KIND` rows per kind unless expanded.
pub(crate) fn entries(issues: &[Issue], ignored: &HashSet<String>, filter: Option<Severity>, expanded: &HashSet<IssueKind>) -> Vec<Entry> {
    let vis: Vec<(usize, &Issue)> = visible(issues, ignored, filter).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < vis.len() {
        let sev = vis[i].1.severity;
        let n_sev = vis[i..].iter().take_while(|(_, x)| x.severity == sev).count();
        out.push(Entry::Severity(sev, n_sev));
        let end = i + n_sev;
        while i < end {
            let kind = vis[i].1.kind;
            let n = vis[i..end].iter().take_while(|(_, x)| x.kind == kind).count();
            out.push(Entry::Kind(kind, n));
            let shown = if expanded.contains(&kind) { n } else { n.min(PER_KIND) };
            out.extend(vis[i..i + shown].iter().map(|(ix, _)| Entry::Row(*ix)));
            if shown < n {
                out.push(Entry::More(kind, n - shown));
            }
            i += n;
        }
    }
    out
}

/// Next selectable entry from `from` in direction `delta` (wrapping); `from` itself if none.
pub(crate) fn step(entries: &[Entry], from: usize, delta: isize) -> usize {
    let n = entries.len() as isize;
    if n == 0 {
        return 0;
    }
    let mut i = from.min(entries.len() - 1) as isize;
    for _ in 0..n {
        i = (i + delta).rem_euclid(n);
        if entries[i as usize].selectable() {
            return i as usize;
        }
    }
    from
}

pub(crate) const FILTERS: [Option<Severity>; 4] = [None, Some(Severity::Error), Some(Severity::Warning), Some(Severity::Info)];

pub struct ReviewPanel {
    tree: Rc<Tree>,
    issues: Rc<Vec<Issue>>,
    ignored: HashSet<String>,
    filter: Option<Severity>,
    expanded: HashSet<IssueKind>,
    /// index into `entries()`
    selected: usize,
    /// whether the tree's issues have been computed at least once
    ready: bool,
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl EventEmitter<ReviewEvent> for ReviewPanel {}

pub(crate) fn severity_icon(t: &Tokens, s: Severity) -> (IconName, Hsla) {
    match s {
        Severity::Error => (IconName::CircleX, t.danger),
        Severity::Warning => (IconName::TriangleAlert, t.warning),
        Severity::Info => (IconName::Info, t.ink_muted),
    }
}

impl ReviewPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ReviewPanel {
            tree: Rc::new(Tree::new_empty()),
            issues: Rc::new(vec![]),
            ignored: HashSet::new(),
            filter: None,
            expanded: HashSet::new(),
            selected: 0,
            ready: false,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    /// New tree / issues / ignore set from the host; keeps the selection on the same issue.
    pub fn set_data(&mut self, tree: Rc<Tree>, issues: Option<Rc<Vec<Issue>>>, ignored: HashSet<String>, cx: &mut Context<Self>) {
        let current = self.selected_issue().map(Issue::key);
        self.tree = tree;
        if let Some(i) = issues {
            self.issues = i;
            self.ready = true;
        }
        self.ignored = ignored;
        let entries = self.entries();
        let again = current.and_then(|k| entries.iter().position(|e| matches!(e, Entry::Row(ix) if self.issues[*ix].key() == k)));
        self.selected = again.unwrap_or_else(|| self.selected.min(entries.len().saturating_sub(1)));
        if entries.get(self.selected).is_some_and(|e| !e.selectable()) {
            self.selected = step(&entries, self.selected, 1);
        }
        cx.notify();
    }

    /// Select the first issue about `id` (from the ficha's hint).
    pub fn select_person(&mut self, id: &str, cx: &mut Context<Self>) {
        self.filter = None;
        let entries = self.entries();
        if let Some(ix) = entries.iter().position(|e| matches!(e, Entry::Row(i) if self.issues[*i].people.first().is_some_and(|p| p == id))) {
            self.selected = ix;
            self.scroll.scroll_to_item(ix);
        }
        cx.notify();
    }

    fn entries(&self) -> Vec<Entry> {
        entries(&self.issues, &self.ignored, self.filter, &self.expanded)
    }

    fn selected_issue(&self) -> Option<&Issue> {
        match self.entries().get(self.selected) {
            Some(Entry::Row(i)) => self.issues.get(*i),
            _ => None,
        }
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let entries = self.entries();
        self.selected = step(&entries, self.selected, delta);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn set_filter(&mut self, f: Option<Severity>, cx: &mut Context<Self>) {
        self.filter = f;
        let entries = self.entries();
        self.selected = step(&entries, entries.len().saturating_sub(1), 1);
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn activate(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selected = ix;
        match self.entries().get(ix).copied() {
            Some(Entry::Row(i)) => {
                if let Some(p) = self.issues[i].people.first() {
                    cx.emit(ReviewEvent::Focus(p.clone()));
                }
            }
            Some(Entry::More(kind, _)) => {
                self.expanded.insert(kind);
            }
            _ => {}
        }
        cx.notify();
    }

    fn edit_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.selected_issue().and_then(|i| i.people.first()).cloned() {
            cx.emit(ReviewEvent::Edit(p));
        }
    }

    fn ignore_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(i) = self.selected_issue() {
            cx.emit(ReviewEvent::Ignore(i.key()));
        }
    }

    fn render_chips(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let count = |f: Option<Severity>| visible(&self.issues, &self.ignored, f).count();
        let mut row = h_flex().gap_1p5().px_4().py_2p5().border_b_1().border_color(t.border).flex_wrap();
        for (i, f) in FILTERS.into_iter().enumerate() {
            let active = self.filter == f;
            let n = count(f);
            let label = match f {
                None => i18n::t!("app.review.filter.all", n = n),
                Some(Severity::Error) => i18n::t!("app.review.filter.errors", n = n),
                Some(Severity::Warning) => i18n::t!("app.review.filter.warnings", n = n),
                Some(Severity::Info) => i18n::t!("app.review.filter.info", n = n),
            };
            row = row.child(
                div()
                    .id(("review-chip", i))
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
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_filter(f, cx))),
            );
        }
        row
    }

    fn link(t: &Tokens, id: impl Into<ElementId>, text: String) -> Stateful<Div> {
        let accent = t.accent;
        div().id(id).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(accent).cursor_pointer().hover(|s| s.underline()).child(text)
    }

    fn render_row(&self, ix: usize, issue_ix: usize, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let issue = &self.issues[issue_ix];
        let selected = self.selected == ix;
        let person = issue.people.first().and_then(|p| self.tree.person(p));
        let (icon, color) = severity_icon(t, issue.severity);
        let title = match (person, &issue.family) {
            (Some(p), _) => p.name(),
            (None, Some(f)) => i18n::t!("app.review.family", id = f),
            (None, None) => String::new(),
        };
        let lead = match person {
            Some(p) => ui::avatar(t, Some(p), size::AVATAR_ROW.into()),
            None => div().size(size::AVATAR_ROW).flex_none().rounded_full().flex().items_center().justify_center().bg(t.canvas).child(ui::icon(IconName::Inbox, 14., t.ink_muted)),
        };
        let actions = selected.then(|| {
            let mut a = h_flex().gap_3().pt_1();
            if let Some(p) = issue.people.first() {
                let p = p.clone();
                a = a.child(Self::link(t, ("review-edit", ix), i18n::t!("app.review.edit")).on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ReviewEvent::Edit(p.clone()));
                })));
            }
            if issue.kind == IssueKind::PossibleDuplicate
                && let [a_id, b_id, ..] = issue.people.as_slice()
            {
                let (x, y) = (a_id.clone(), b_id.clone());
                a = a.child(Self::link(t, ("review-merge", ix), i18n::t!("app.review.merge")).on_click(cx.listener(move |_, _, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ReviewEvent::Merge(x.clone(), y.clone()));
                })));
            }
            let key = issue.key();
            a.child(Self::link(t, ("review-ignore", ix), i18n::t!("app.review.ignore")).on_click(cx.listener(move |_, _, _, cx| {
                cx.stop_propagation();
                cx.emit(ReviewEvent::Ignore(key.clone()));
            })))
        });
        h_flex()
            .id(("review-row", ix))
            .items_start()
            .gap_2p5()
            .px_2()
            .py_2()
            .mx_2()
            .rounded(px(8.))
            .cursor_pointer()
            .when(selected, |d| d.bg(t.accent_soft))
            .when(!selected, |d| d.hover(|s| s.bg(t.canvas)))
            .on_click(cx.listener(move |this, _, _, cx| this.activate(ix, cx)))
            .child(div().pt(px(8.)).flex_none().child(ui::icon(icon, 14., color)))
            .child(lead)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).truncate().child(title))
                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(issue.message()))
                    .children(actions),
            )
            .into_any_element()
    }

    fn render_list(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.entries();
        let mut list = v_flex().id("review-list").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll).pb_2();
        if entries.is_empty() {
            let text = if !self.ready {
                i18n::t!("app.review.loading")
            } else if self.filter.is_some() {
                i18n::t!("app.review.empty_filter")
            } else {
                i18n::t!("app.review.empty")
            };
            return list.child(div().px_4().py_6().text_center().text_size(px(13.)).text_color(t.ink_muted).child(text));
        }
        for (ix, e) in entries.into_iter().enumerate() {
            let el = match e {
                Entry::Severity(s, n) => {
                    let (icon, color) = severity_icon(t, s);
                    h_flex()
                        .gap_1p5()
                        .px_4()
                        .pt_3()
                        .pb_1()
                        .child(ui::icon(icon, 13., color))
                        .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink).child(s.label()))
                        .child(div().text_size(px(12.)).text_color(t.ink_muted).child(n.to_string()))
                        .into_any_element()
                }
                Entry::Kind(k, n) => div().px_4().pt_1p5().pb_0p5().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(i18n::t!("app.review.kind", kind = k.label(), n = n)).into_any_element(),
                Entry::Row(i) => self.render_row(ix, i, t, cx),
                Entry::More(_, n) => {
                    let selected = self.selected == ix;
                    div()
                        .id(("review-more", ix))
                        .mx_2()
                        .px_2()
                        .py_1p5()
                        .rounded(px(8.))
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(t.accent)
                        .cursor_pointer()
                        .when(selected, |d| d.bg(t.accent_soft))
                        .child(i18n::tn!("app.review.more", n))
                        .on_click(cx.listener(move |this, _, _, cx| this.activate(ix, cx)))
                        .into_any_element()
                }
            };
            list = list.child(el);
        }
        list
    }
}

fn kbd(text: String, t: &Tokens) -> Div {
    div().flex_none().px_1p5().py_0p5().rounded(px(5.)).border_1().border_color(t.border).bg(t.canvas).text_size(px(11.)).text_color(t.ink_muted).child(text)
}

impl Render for ReviewPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let footer = h_flex()
            .gap_3()
            .px_4()
            .py_2p5()
            .flex_wrap()
            .border_t_1()
            .border_color(t.border)
            .text_size(px(12.))
            .text_color(t.ink_muted)
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.up_down"), &t)).child(i18n::t!("dialogs.palette.footer.move")))
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.enter"), &t)).child(i18n::t!("app.review.footer.focus")))
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.cmd_enter"), &t)).child(i18n::t!("dialogs.palette.footer.edit")))
            .child(h_flex().gap_1().child(kbd(i18n::t!("dialogs.key.esc"), &t)).child(i18n::t!("app.review.footer.close")));
        v_flex()
            .id("review")
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &ReviewPrev, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &ReviewNext, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &ReviewConfirm, _, cx| this.activate(this.selected, cx)))
            .on_action(cx.listener(|this, _: &ReviewEdit, _, cx| this.edit_selected(cx)))
            .on_action(cx.listener(|this, _: &ReviewIgnore, _, cx| this.ignore_selected(cx)))
            .on_action(cx.listener(|_, _: &ReviewClose, _, cx| cx.emit(ReviewEvent::Close)))
            .w(size::REVIEW_W)
            .flex_none()
            .h_full()
            .bg(t.surface)
            .border_l_1()
            .border_color(t.border)
            .font_family(theme::SANS)
            .child(
                h_flex()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(t.border)
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink).child(i18n::t!("app.review.title")))
                    .child(ui::icon_button(&t, "review-close", IconName::Close, true).on_click(cx.listener(|_, _, _, cx| cx.emit(ReviewEvent::Close)))),
            )
            .child(self.render_chips(&t, cx))
            .child(self.render_list(&t, cx))
            .child(footer)
    }
}

// ---- host side ----

actions!(gen_app, [ToggleReview]);

/// Host state: issues cached by tree revision (computed off the UI thread) and the session's
/// ignore set.
#[derive(Default)]
pub(crate) struct ReviewState {
    issues: Option<(u64, Rc<Vec<Issue>>)>,
    pending: Option<u64>,
    ignored: HashSet<String>,
    sub: Option<Subscription>,
}

impl ReviewState {
    /// A different tree was opened: ids in the ignore set no longer mean the same people.
    pub(crate) fn forget_ignored(&mut self) {
        self.ignored.clear();
    }
}

impl GenApp {
    /// Recompute the issues in the background when the tree revision changed; called from render.
    pub(crate) fn ensure_quality(&mut self, cx: &mut Context<Self>) {
        let rev = self.rev;
        let st = &mut self.review_state;
        if st.issues.as_ref().is_some_and(|(r, _)| *r == rev) || st.pending == Some(rev) {
            return;
        }
        st.pending = Some(rev);
        let tree: Tree = (*self.tree).clone();
        let task = cx.background_executor().spawn(async move { tree.quality_issues() });
        cx.spawn(async move |this, cx| {
            let issues = task.await;
            this.update(cx, |this, cx| {
                let st = &mut this.review_state;
                if st.pending == Some(rev) {
                    st.pending = None;
                }
                // an older run finishing late must not replace a newer result
                if st.issues.as_ref().is_none_or(|(r, _)| *r < rev) {
                    st.issues = Some((rev, Rc::new(issues)));
                    this.push_review(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn push_review(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.review.clone() {
            let issues = self.review_state.issues.as_ref().map(|(_, i)| i.clone());
            let (tree, ignored) = (self.tree.clone(), self.review_state.ignored.clone());
            r.update(cx, |r, cx| r.set_data(tree, issues, ignored, cx));
        }
    }

    fn live_issues(&self) -> impl Iterator<Item = &Issue> {
        let ignored = &self.review_state.ignored;
        self.review_state.issues.iter().flat_map(|(_, i)| i.iter()).filter(move |i| ignored.is_empty() || !ignored.contains(&i.key()))
    }

    /// Errors + warnings not ignored (the top-bar badge).
    pub(crate) fn review_badge(&self) -> (usize, bool) {
        let mut n = 0;
        let mut errors = false;
        for i in self.live_issues().filter(|i| i.severity != Severity::Info) {
            n += 1;
            errors |= i.severity == Severity::Error;
        }
        (n, errors)
    }

    pub(crate) fn toggle_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.review.is_some() {
            self.close_review(window, cx);
        } else {
            self.open_review(window, cx);
        }
    }

    pub(crate) fn open_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(r) = &self.review {
            r.update(cx, |r, cx| r.focus(window, cx));
            return;
        }
        let panel = cx.new(ReviewPanel::new);
        self.review_state.sub = Some(cx.subscribe_in(&panel, window, |this, panel, ev: &ReviewEvent, window, cx| match ev {
            ReviewEvent::Focus(id) => {
                this.focus_from_outside(id.clone(), window, cx);
                panel.update(cx, |p, cx| p.focus(window, cx));
            }
            ReviewEvent::Edit(id) => {
                this.focus_from_outside(id.clone(), window, cx);
                this.open_edit(id.clone(), window, cx);
            }
            ReviewEvent::Merge(a, b) => this.open_merge_confirm(a.clone(), b.clone(), window, cx),
            ReviewEvent::Ignore(key) => {
                this.review_state.ignored.insert(key.clone());
                this.push_review(cx);
                cx.notify();
            }
            ReviewEvent::Close => this.close_review(window, cx),
        }));
        panel.update(cx, |p, cx| p.focus(window, cx));
        self.review = Some(panel);
        self.push_review(cx);
        cx.notify();
    }

    fn close_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review = None;
        self.review_state.sub = None;
        self.restore_focus(window, cx);
        cx.notify();
    }

    /// Top-bar «Revisión» button with the errors + warnings badge.
    pub(crate) fn render_review_button(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let (n, errors) = self.review_badge();
        let open = self.review.is_some();
        let badge = (n > 0).then(|| {
            let text = if n > 99 { "99+".to_string() } else { n.to_string() }; // i18n-ok
            div()
                .absolute()
                .top(px(2.))
                .right(px(0.))
                .min_w(px(16.))
                .h(px(16.))
                .px(px(4.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(if errors { t.danger } else { t.warning })
                .text_color(t.on_fill())
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(text)
        });
        let icon = if n > 0 { IconName::TriangleAlert } else { IconName::CircleCheck };
        div()
            .relative()
            .flex_none()
            .child(
                ui::icon_button(t, "review", icon, true)
                    .when(open, |d| d.bg(t.canvas))
                    .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.review")).build(w, cx))
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_review(window, cx))),
            )
            .children(badge)
    }

    /// Ficha line «2 problemas en la revisión» for a person with errors or warnings.
    pub(crate) fn review_hint(&self, t: &Tokens, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mine: Vec<&Issue> = self.live_issues().filter(|i| i.severity != Severity::Info && i.people.iter().any(|p| p == id)).collect();
        let worst = mine.iter().map(|i| i.severity).min()?;
        let (icon, color) = severity_icon(t, worst);
        let id = id.to_string();
        Some(
            h_flex()
                .id("review-hint")
                .gap_1p5()
                .text_size(px(12.))
                .text_color(color)
                .cursor_pointer()
                .hover(|s| s.underline())
                .child(ui::icon(icon, 13., color))
                .child(i18n::tn!("app.sheet.review_hint", mine.len()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_review(window, cx);
                    if let Some(r) = this.review.clone() {
                        r.update(cx, |r, cx| r.select_person(&id, cx));
                    }
                }))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, PER_KIND, entries, step};
    use gedcom::{IssueKind, Severity, Tree};
    use std::collections::HashSet;

    fn tree() -> Tree {
        // child born before its mother, a nameless person, two isolated people
        Tree::parse(
            "0 HEAD\n0 @I1@ INDI\n1 NAME Ana /Norte/\n1 SEX F\n1 BIRT\n2 DATE 1900\n1 FAMS @F1@\n0 @I2@ INDI\n1 NAME Blas /Norte/\n1 BIRT\n2 DATE 1890\n1 FAMC @F1@\n\
             0 @I3@ INDI\n0 @I4@ INDI\n1 NAME Ciro /Sur/\n0 @F1@ FAM\n1 WIFE @I1@\n1 CHIL @I2@\n0 TRLR\n",
        )
        .unwrap()
    }

    #[test]
    fn groups_by_severity_then_kind() {
        let issues = tree().quality_issues();
        let e = entries(&issues, &HashSet::new(), None, &HashSet::new());
        assert!(matches!(e[0], Entry::Severity(Severity::Error, 1)));
        assert!(matches!(e[1], Entry::Kind(IssueKind::BornBeforeMother, 1)));
        assert!(matches!(e[2], Entry::Row(_)));
        assert!(e.iter().any(|x| matches!(x, Entry::Kind(IssueKind::Isolated, 2))), "{e:?}");
        let only_info = entries(&issues, &HashSet::new(), Some(Severity::Info), &HashSet::new());
        assert!(matches!(only_info[0], Entry::Severity(Severity::Info, _)));
        // ignoring the only error drops its section
        let ignored: HashSet<String> = [issues[0].key()].into();
        let e2 = entries(&issues, &ignored, None, &HashSet::new());
        assert!(!e2.iter().any(|x| matches!(x, Entry::Severity(Severity::Error, _))));
    }

    #[test]
    fn collapses_long_kinds_and_steps_over_headers() {
        let mut text = String::from("0 HEAD\n");
        for i in 0..40 {
            text += &format!("0 @I{i}@ INDI\n1 NAME P{i} /X{i}/\n");
        }
        let issues = Tree::parse(&(text + "0 TRLR\n")).unwrap().quality_issues();
        let e = entries(&issues, &HashSet::new(), None, &HashSet::new());
        assert_eq!(e.iter().filter(|x| matches!(x, Entry::Row(_))).count(), PER_KIND);
        assert!(matches!(e.last(), Some(Entry::More(IssueKind::Isolated, 10))));
        let all = entries(&issues, &HashSet::new(), None, &[IssueKind::Isolated].into());
        assert_eq!(all.iter().filter(|x| matches!(x, Entry::Row(_))).count(), 40);
        assert_eq!(step(&e, 0, 1), 2, "skips both headers");
        assert_eq!(step(&e, 2, -1), e.len() - 1, "wraps to «Ver más»");
        assert_eq!(step(&[], 0, 1), 0);
    }
}
