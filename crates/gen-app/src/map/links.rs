//! «Conexiones familiares» in the place panel: branches, couples, parent→child pairs and
//! migrations from `Tree::place_links`, plus the relationship of each listed person to the focus
//! (computed a few per frame and cached).
use super::{MapFilter, POPOVER_ROWS};
use crate::app::GenApp;
use crate::theme::Tokens;
use crate::ui::{icon, section_title};
use gedcom::{KinKind, MoveDir, PlaceGroup, PlaceLinks};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::IconName;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// kinship lookups per frame (each walks both ancestries)
const KIN_PER_FRAME: usize = 12;
const MAX_BRANCHES: usize = 8;
const MAX_PAIRS: usize = 12;
const MAX_MOVES: usize = 10;
const LINE_HEAD: usize = 3;
const LINE_TAIL: usize = 2;

/// What `links` was computed for.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct LinksKey {
    rev: u64,
    place: String,
    focus: Option<String>,
    filter: MapFilter,
}

/// Relationship labels to the focus, valid for one (rev, focus, locale).
#[derive(Default)]
pub(super) struct KinCache {
    stamp: Option<(u64, String, &'static str)>,
    labels: HashMap<String, Option<String>>,
}

fn year_span(y: Option<(i32, i32)>) -> Option<String> {
    y.map(|(a, b)| if a == b { a.to_string() } else { format!("{a}–{b}") }) // i18n-ok: year range
}

impl GenApp {
    /// Connections of the single selected place (cached by rev, place, focus and filter).
    pub(super) fn map_links(&mut self, groups: &[PlaceGroup]) -> Option<Rc<PlaceLinks>> {
        let place = match self.map.selected.as_deref() {
            Some([k]) => k.clone(),
            _ => return None,
        };
        let key = LinksKey { rev: self.rev, place, focus: self.focus_id.clone(), filter: self.map.filter };
        if let Some((k, l)) = &self.map.links
            && *k == key
        {
            return l.clone();
        }
        let set = self.map_filter_set();
        let l = self.tree.place_links(groups, &key.place, key.focus.as_deref(), set.as_deref()).map(Rc::new);
        self.map.links = Some((key, l.clone()));
        l
    }

    /// Fill the focus-relationship cache for `people` (in display order), a few per frame.
    pub(super) fn map_fill_kin(&mut self, people: &[String], window: &mut Window) {
        let Some(focus) = self.focus_id.clone() else { return };
        let stamp = (self.rev, focus.clone(), i18n::locale());
        if self.map.kin.stamp.as_ref() != Some(&stamp) {
            self.map.kin = KinCache { stamp: Some(stamp), labels: HashMap::new() };
        }
        let mut done = 0;
        for p in people {
            if self.map.kin.labels.contains_key(p) {
                continue;
            }
            if done == KIN_PER_FRAME {
                window.request_animation_frame();
                return;
            }
            let label = self.tree.kinship(&focus, p).filter(|k| !matches!(k.kind, KinKind::None | KinKind::Same)).map(|k| k.label);
            self.map.kin.labels.insert(p.clone(), label);
            done += 1;
        }
    }

    /// How `pid` is related to the focus, if computed and related.
    pub(super) fn map_kin(&self, pid: &str) -> Option<&str> {
        self.map.kin.labels.get(pid)?.as_deref()
    }

    pub(super) fn render_map_links(&self, t: &Tokens, l: &PlaceLinks, cx: &mut Context<Self>) -> Div {
        let ids = Rc::new(Cell::new(0usize));
        let name_of = |id: &str| self.tree.person(id).map(|p| p.name()).unwrap_or_default();
        // a clickable name that focuses the person
        let link = |pid: &str, cx: &mut Context<Self>| {
            let n = ids.get();
            ids.set(n + 1);
            let id = pid.to_string();
            let accent = t.accent;
            div()
                .id(("map-link", n))
                .text_color(t.ink)
                .cursor_pointer()
                .hover(move |s| s.text_color(accent).underline())
                .child(name_of(pid))
                .on_click(cx.listener(move |this, _, window, cx| this.set_focus(id.clone(), true, window, cx)))
        };
        let sep = |s: &'static str| div().text_color(t.ink_muted).child(s);
        let muted = |text: String| div().text_size(px(12.)).text_color(t.ink_muted).child(text);
        let sub_title = |text: String| div().pt(px(10.)).pb(px(2.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(text);
        let more = |n: usize| (n > 0).then(|| muted(i18n::t!("app.map.more", n = n)).px(px(8.)));

        let mut d = div().flex().flex_col().gap(px(4.)).px(px(8.)).pt(px(6.)).child(section_title(t, i18n::t!("app.map.links.title")));
        let mut summary = vec![i18n::tn!("app.map.links.people", l.people.len())];
        if !l.branches.is_empty() {
            summary.push(i18n::tn!("app.map.links.branches", l.branches.len()));
        }
        summary.extend(year_span(l.years));
        d = d.child(div().text_size(px(13.)).text_color(t.ink).child(summary.join(" · "))); // i18n-ok: separator

        if l.people.len() == 1 {
            return d.child(muted(i18n::t!("app.map.links.only_one")));
        }
        if l.branches.is_empty() {
            d = d.child(muted(i18n::t!("app.map.links.no_kin")));
        }

        // family branches
        for b in l.branches.iter().take(MAX_BRANCHES) {
            let title = if b.surname.is_empty() { i18n::t!("app.map.links.family_unnamed") } else { i18n::t!("app.map.links.family", surname = b.surname.clone()) };
            let mut facts = vec![i18n::tn!("app.map.links.people", b.members.len()), i18n::tn!("app.map.links.generations", b.generations)];
            facts.extend(year_span(b.years));
            let mut names = div().flex().flex_wrap().items_center().gap_x(px(4.)).text_size(px(12.));
            if b.line.len() >= 2 {
                let n = b.line.len();
                for (i, pid) in b.line.iter().enumerate() {
                    if n > LINE_HEAD + LINE_TAIL && i >= LINE_HEAD && i < n - LINE_TAIL {
                        if i == LINE_HEAD {
                            names = names.child(sep("→ … →")); // i18n-ok: symbols
                        }
                        continue;
                    }
                    if i > 0 && !(n > LINE_HEAD + LINE_TAIL && i == n - LINE_TAIL) {
                        names = names.child(sep("→")); // i18n-ok: symbol
                    }
                    names = names.child(link(pid, cx));
                }
            } else {
                let shown = 5.min(b.members.len());
                for (i, pid) in b.members.iter().take(shown).enumerate() {
                    if i > 0 {
                        names = names.child(sep("·")); // i18n-ok: separator
                    }
                    names = names.child(link(pid, cx));
                }
                names = names.children(more(b.members.len() - shown));
            }
            d = d.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .mt(px(4.))
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(if b.has_focus { t.accent } else { t.border })
                    .when(b.has_focus, |d| d.bg(t.accent_soft))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).truncate().child(title))
                    .child(muted(facts.join(" · "))) // i18n-ok: separator
                    .child(names),
            );
        }
        d = d.children(more(l.branches.len().saturating_sub(MAX_BRANCHES)));
        if !l.branches.is_empty() && !l.alone.is_empty() {
            d = d.child(muted(i18n::tn!("app.map.links.alone", l.alone.len())));
        }

        // couples
        if !l.couples.is_empty() {
            d = d.child(sub_title(i18n::t!("app.map.links.couples")));
            for c in l.couples.iter().take(MAX_PAIRS) {
                let label = gedcom::tree::event_label(&c.tag);
                let sub = c.year.map_or(label.clone(), |y| i18n::t!("app.map.event_year", label = label, year = y));
                d = d.child(
                    div()
                        .flex()
                        .flex_col()
                        .px(px(2.))
                        .child(div().flex().flex_wrap().items_center().gap_x(px(4.)).text_size(px(13.)).child(link(&c.partners[0], cx)).child(icon(IconName::Heart, 12., t.ink_muted)).child(link(&c.partners[1], cx)))
                        .child(muted(sub)),
                );
            }
            d = d.children(more(l.couples.len().saturating_sub(MAX_PAIRS)));
        }

        // parent → child pairs
        if !l.parent_child.is_empty() {
            d = d.child(sub_title(i18n::t!("app.map.links.parents_children")));
            for pc in l.parent_child.iter().take(MAX_PAIRS) {
                let mut sub = pc.label(&self.tree);
                if pc.both_born {
                    sub = i18n::t!("app.map.links.both_born", pair = sub);
                }
                d = d.child(
                    div()
                        .flex()
                        .flex_col()
                        .px(px(2.))
                        .child(div().flex().flex_wrap().items_center().gap_x(px(4.)).text_size(px(13.)).child(link(&pc.parent, cx)).child(sep("→")).child(link(&pc.child, cx))) // i18n-ok: symbol
                        .child(muted(sub)),
                );
            }
            d = d.children(more(l.parent_child.len().saturating_sub(MAX_PAIRS)));
        }

        // migrations
        if !l.migrations.is_empty() {
            d = d.child(sub_title(i18n::t!("app.map.links.migrations")));
            for (i, m) in l.migrations.iter().take(MAX_MOVES).enumerate() {
                let (text, ic) = match m.dir {
                    MoveDir::From => (i18n::tn!("app.map.links.came_from", m.people.len(), place = m.name.clone()), IconName::ArrowLeft),
                    MoveDir::To => (i18n::tn!("app.map.links.went_to", m.people.len(), place = m.name.clone()), IconName::ArrowRight),
                };
                let shown: Vec<String> = m.people.iter().take(3).map(|p| name_of(p)).collect();
                let mut who = gedcom::insights::join_list(&shown);
                if m.people.len() > shown.len() {
                    who = i18n::t!("app.map.links.and_more", names = who, n = m.people.len() - shown.len());
                }
                let key = m.key.clone();
                let hover = t.canvas;
                d = d.child(
                    div()
                        .id(("map-move", i))
                        .flex()
                        .items_start()
                        .gap(px(8.))
                        .px(px(6.))
                        .py(px(5.))
                        .rounded(px(8.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .child(div().pt(px(2.)).child(icon(ic, 14., t.accent)))
                        .child(div().flex().flex_col().min_w_0().child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).line_clamp(2).child(text)).child(muted(who).line_clamp(2)))
                        .on_click(cx.listener(move |this, _, _, cx| this.map_show_place(&key, cx))),
                );
            }
            d = d.children(more(l.migrations.len().saturating_sub(MAX_MOVES)));
        }
        d.child(div().h(px(4.)))
    }

    /// People of the selected place in the panel's row order (for the kinship cache).
    pub(super) fn map_row_people(groups: &[PlaceGroup], keys: &[String], set: Option<&std::collections::HashSet<String>>) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for g in keys.iter().filter_map(|k| groups.iter().find(|g| g.key == *k)) {
            for e in g.events.iter().filter(|e| super::passes(set, &e.people)) {
                for p in &e.people {
                    if out.len() >= POPOVER_ROWS {
                        return out;
                    }
                    if seen.insert(p.clone()) {
                        out.push(p.clone());
                    }
                }
            }
        }
        out
    }
}
