//! Read-only person sheet («ficha», right panel, 360 px). Editing goes through the dialog.
use crate::app::{GenApp, Rel};
use crate::theme::{SERIF, Tokens, size, tokens};
use crate::ui::{self, avatar, chip, section_title};
use gedcom::{Person, Sex};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::IconName;

const SIBLING_AVATARS: usize = 5;
/// notes shown before «Ver todas», and their length when collapsed
const NOTES_COLLAPSED: usize = 3;
const NOTE_CHARS: usize = 320;

impl GenApp {
    pub(crate) fn render_sheet(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens(cx);
        let panel = div()
            .id("sheet")
            .w(size::SHEET_W)
            .flex_none()
            .h_full()
            .bg(t.surface)
            .border_l_1()
            .border_color(t.border)
            .overflow_y_scroll()
            .track_scroll(&self.sheet_scroll);
        let Some(p) = self.focus_id.as_deref().and_then(|f| self.tree.person(f)).cloned() else {
            return panel.child(div().p(size::SHEET_PAD).text_color(t.ink_muted).child(i18n::t!("app.sheet.nobody")));
        };
        panel.child(
            div()
                .flex()
                .flex_col()
                .gap(px(20.))
                .p(size::SHEET_PAD)
                .child(self.sheet_header(&t, &p, cx))
                .child(self.sheet_completeness(&t, &p, cx))
                .child(self.sheet_timeline(&t, &p, cx))
                .child(self.sheet_notes(&t, &p, cx))
                .child(self.sheet_parents(&t, &p, cx))
                .child(self.sheet_siblings(&t, &p, cx))
                .child(self.sheet_children(&t, &p, cx)),
        )
    }

    fn sheet_header(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let life = match p.age_at_death() {
            Some(a) if a >= 0 => format!("{} · {}", ui::years(p), i18n::tn!("app.sheet.lived", a)),
            _ => ui::years(p),
        };
        let (sbg, sfg) = t.sex(p.sex);
        let sex = match p.sex {
            Sex::Male => i18n::t!("app.sex.male"),
            Sex::Female => i18n::t!("app.sex.female"),
            Sex::Unknown => i18n::t!("app.sex.unknown"),
        };
        let id = p.id.clone();
        let weak = cx.entity().downgrade();
        let add_menu = ui::menu_button(t, "add-relative", i18n::t!("app.sheet.add_relative")).dropdown_menu({
            let (id, weak) = (id.clone(), weak.clone());
            move |mut menu, _, _| {
                for (key, rel) in [("app.rel.father", Rel::Father), ("app.rel.mother", Rel::Mother), ("app.rel.partner", Rel::Partner), ("app.rel.son", Rel::Son), ("app.rel.daughter", Rel::Daughter)] {
                    let (id, w) = (id.clone(), weak.clone());
                    menu = menu.item(PopupMenuItem::new(i18n::t!(key)).on_click(move |_, window, cx| {
                        w.update(cx, |this, cx| this.add_and_edit(&id, rel, None, window, cx)).ok();
                    }));
                }
                menu
            }
        });
        let vigia = self.vigia_installed;
        let more = ui::menu_icon_button(t, "more", IconName::Ellipsis).dropdown_menu_with_anchor(Anchor::TopRight, {
            let (id, weak) = (id.clone(), weak.clone());
            move |menu, _, _| {
                let (id5, w5) = (id.clone(), weak.clone());
                let menu = if vigia {
                    menu.item(PopupMenuItem::new(i18n::t!("app.sheet.track_vigia")).on_click(move |_, _, cx| {
                        w5.update(cx, |this, cx| this.track_in_vigia(id5.clone(), cx)).ok();
                    }))
                } else {
                    menu
                };
                let (id1, w1, id2, w2) = (id.clone(), weak.clone(), id.clone(), weak.clone());
                let (id3, w3) = (id.clone(), weak.clone());
                let (id4, w4) = (id.clone(), weak.clone());
                menu.item(PopupMenuItem::new(i18n::t!("app.sheet.view_gedcom")).on_click(move |_, window, cx| {
                    w1.update(cx, |this, cx| this.show_raw(id1.clone(), window, cx)).ok();
                }))
                .item(PopupMenuItem::new(i18n::t!("app.sheet.merge")).on_click(move |_, window, cx| {
                    w3.update(cx, |this, cx| this.open_merge_picker(id3.clone(), window, cx)).ok();
                }))
                .item(PopupMenuItem::new(i18n::t!("app.sheet.kinship")).on_click(move |_, window, cx| {
                    w4.update(cx, |this, cx| this.open_kinship_picker(id4.clone(), window, cx)).ok();
                }))
                .item(PopupMenuItem::new(i18n::t!("app.sheet.delete")).on_click(move |_, _, cx| {
                    w2.update(cx, |this, cx| {
                        this.armed_delete = Some(id2.clone());
                        cx.notify();
                    })
                    .ok();
                }))
            }
        });
        let armed = self.armed_delete.as_deref() == Some(p.id.as_str());
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(avatar(t, Some(p), size::AVATAR_SHEET.into()))
            .child(div().font_family(SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(24.)).line_height(px(30.)).text_center().child(p.name()))
            .child(div().text_size(px(13.)).text_color(t.ink_muted).child(life))
            .child(div().flex().gap(px(6.)).child(chip(sbg, sfg, sex)).child(chip(t.canvas, t.ink_muted, p.id.clone())))
            .children(self.review_hint(t, &p.id, cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .mt(px(4.))
                    .child(ui::filled_button(t, "edit", t.ink).child(i18n::t!("app.sheet.edit")).on_click(cx.listener({
                        let id = id.clone();
                        move |this, _, window, cx| this.open_edit(id.clone(), window, cx)
                    })))
                    .child(add_menu)
                    .child(more),
            )
            .when(armed, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().text_size(px(12.)).text_color(t.danger).child(i18n::t!("app.sheet.delete_confirm_hint")))
                        .child(ui::filled_button(t, "delete-confirm", t.danger).h(px(30.)).child(i18n::t!("app.sheet.delete_confirm")).on_click(cx.listener({
                            let id = id.clone();
                            move |this, _, window, cx| this.delete_person(id.clone(), window, cx)
                        })))
                        .child(ui::outline_button(t, "delete-cancel").h(px(30.)).child(i18n::t!("app.sheet.cancel")).on_click(cx.listener(|this, _, _, cx| {
                            this.armed_delete = None;
                            cx.notify();
                        }))),
                )
            })
    }

    fn sheet_completeness(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.tree.completeness(&p.id);
        let pct = c.percent.min(100);
        let id = p.id.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(14.))
            .rounded(px(12.))
            .bg(t.accent_soft)
            .border_1()
            .border_color(t.border)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(i18n::t!("app.sheet.complete", n = pct)))
                    .when(pct < 100, |d| d.child(link(t, "complete-link", i18n::t!("app.sheet.complete_action")).on_click(cx.listener(move |this, _, window, cx| this.open_edit(id.clone(), window, cx))))),
            )
            .child(div().h(px(6.)).w_full().rounded_full().bg(t.border).child(div().h_full().w(relative(pct as f32 / 100.)).rounded_full().bg(t.accent)))
            .when(!c.missing.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(t.ink_muted).child(c.sentence())))
    }

    fn sheet_timeline(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.tree.timeline(&p.id);
        let history = if self.show_history { self.tree.history_for(&p.id, gedcom::current_year()) } else { vec![] };
        let has_history = !self.tree.history_for(&p.id, gedcom::current_year()).is_empty();
        // life events in order, each historical event just before the first life event after it
        enum Row {
            Life(usize, gedcom::TimelineItem),
            Hist(gedcom::HistEvent),
        }
        let mut rows: Vec<Row> = Vec::new();
        let mut pending = history.into_iter().peekable();
        for (i, it) in items.into_iter().enumerate() {
            if let Some(y) = it.year {
                while pending.peek().is_some_and(|h| h.year < y) {
                    rows.push(Row::Hist(pending.next().unwrap()));
                }
            }
            rows.push(Row::Life(i, it));
        }
        rows.extend(pending.map(Row::Hist));

        let mut list = div().flex().flex_col();
        let n = rows.len();
        for (r, row) in rows.into_iter().enumerate() {
            let last = r + 1 == n;
            let rail = |dot: Div| div().flex().flex_col().items_center().w(px(10.)).child(dot).when(!last, |d| d.child(div().w(px(1.)).flex_1().bg(t.border)));
            match row {
                Row::Hist(h) => {
                    list = list.child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .child(rail(div().mt(px(5.)).size(px(7.)).rounded_full().border_1().border_color(t.ink_muted).bg(t.surface)))
                            .child(div().pb(px(12.)).min_w_0().text_size(px(12.)).italic().text_color(t.ink_muted).child(format!("{} · {}", h.years(), h.label()))),
                    );
                }
                Row::Life(i, it) => {
                    let head = match it.year {
                        Some(y) => format!("{y} · {}", it.label),
                        None => it.label.clone(),
                    };
                    let detail: AnyElement = if matches!(it.tag.as_str(), "MARR" | "DIV") && it.partner.is_none() {
                        let id = p.id.clone();
                        link(t, ("tl-partner", i), i18n::t!("app.sheet.add_partner")).on_click(cx.listener(move |this, _, window, cx| this.add_and_edit(&id, Rel::Partner, None, window, cx))).into_any_element()
                    } else {
                        let partner = it.partner.as_deref().and_then(|x| self.tree.person(x)).map(|pp| pp.name());
                        let text = match (partner, &it.place) {
                            (Some(pp), Some(pl)) => Some(format!("{pp} · {pl}")),
                            (Some(pp), None) => Some(pp),
                            (None, Some(pl)) => Some(pl.clone()),
                            (None, None) => None,
                        };
                        match text {
                            Some(s) => div().text_size(px(12.)).text_color(t.ink_muted).child(s).into_any_element(),
                            None => div().text_size(px(12.)).italic().text_color(t.ink_muted.opacity(0.75)).child(i18n::t!("app.sheet.no_place")).into_any_element(),
                        }
                    };
                    list = list.child(
                        div()
                            .flex()
                            .gap(px(12.))
                            .child(rail(div().mt(px(4.)).size(px(9.)).rounded_full().bg(if i == 0 { t.accent } else { t.connector })))
                            .child(div().flex().flex_col().gap(px(2.)).pb(px(14.)).min_w_0().child(div().font_weight(FontWeight::MEDIUM).child(head)).child(detail)),
                    );
                }
            }
        }
        let body: AnyElement = if n == 0 { div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.sheet.no_events")).into_any_element() } else { list.into_any_element() };
        let mut head = div().flex().justify_between().items_center().child(section_title(t, i18n::t!("app.sheet.timeline")));
        if has_history {
            let label = if self.show_history { i18n::t!("app.sheet.history_hide") } else { i18n::t!("app.sheet.history_show") };
            head = head.child(link(t, "history-toggle", label).on_click(cx.listener(|this, _, _, cx| {
                this.show_history = !this.show_history;
                cx.notify();
            })));
        }
        div().flex().flex_col().gap(px(10.)).child(head).child(body)
    }

    fn person_row(&self, t: &Tokens, key: impl Into<ElementId>, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let id = p.id.clone();
        let hover = t.canvas;
        div()
            .id(key)
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(8.))
            .py(px(6.))
            .mx(px(-8.))
            .rounded(px(10.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .on_click(cx.listener(move |this, _, window, cx| this.set_focus(id.clone(), true, window, cx)))
            .child(avatar(t, Some(p), size::AVATAR_ROW.into()))
            .child(div().flex().flex_col().min_w_0().child(div().font_weight(FontWeight::MEDIUM).truncate().child(p.name())).child(div().text_size(px(12.)).text_color(t.ink_muted).child(ui::years(p))))
    }

    fn sheet_parents(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let (f, m) = self.tree.father_mother(&p.id);
        let mut d = div().flex().flex_col().gap(px(4.)).child(section_title(t, i18n::t!("app.sheet.parents")));
        for (i, (pp, rel, key)) in [(f, Rel::Father, "app.canvas.add_father"), (m, Rel::Mother, "app.canvas.add_mother")].into_iter().enumerate() {
            d = match pp {
                Some(pp) => d.child(self.person_row(t, ("parent", i), pp, cx)),
                None => {
                    let id = p.id.clone();
                    d.child(dashed_button(t, ("add-parent", i), i18n::t!(key)).on_click(cx.listener(move |this, _, window, cx| this.add_and_edit(&id, rel, None, window, cx))))
                }
            };
        }
        d
    }

    fn sheet_siblings(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> AnyElement {
        let sibs = self.tree.siblings(&p.id);
        if sibs.is_empty() {
            return div().into_any_element();
        }
        let expanded = self.siblings_expanded;
        let mut head = div().flex().justify_between().items_center().child(section_title(t, i18n::t!("app.sheet.siblings", n = sibs.len())));
        if sibs.len() > 1 || expanded {
            head = head.child(link(t, "siblings-all", if expanded { i18n::t!("app.sheet.see_less") } else { i18n::t!("app.sheet.see_all") }).on_click(cx.listener(|this, _, _, cx| {
                this.siblings_expanded = !this.siblings_expanded;
                cx.notify();
            })));
        }
        let mut d = div().flex().flex_col().gap(px(8.)).child(head);
        if expanded {
            for (i, s) in sibs.iter().enumerate() {
                d = d.child(self.person_row(t, ("sib", i), s, cx));
            }
        } else {
            let mut row = div().flex().items_center();
            for (i, s) in sibs.iter().take(SIBLING_AVATARS).enumerate() {
                let id = s.id.clone();
                row = row.child(
                    div()
                        .id(("sib-av", i))
                        .when(i > 0, |d| d.ml(px(-8.)))
                        .rounded_full()
                        .border_2()
                        .border_color(t.surface)
                        .cursor_pointer()
                        .tooltip({
                            let name = s.name();
                            move |w, cx| gpui_component::tooltip::Tooltip::new(name.clone()).build(w, cx)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| this.set_focus(id.clone(), true, window, cx)))
                        .child(avatar(t, Some(s), 32.)),
                );
            }
            if sibs.len() > SIBLING_AVATARS {
                row = row.child(div().ml(px(-8.)).size(px(36.)).rounded_full().border_2().border_color(t.surface).bg(t.canvas).flex().items_center().justify_center().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(format!("+{}", sibs.len() - SIBLING_AVATARS)));
            }
            d = d.child(row);
        }
        d.into_any_element()
    }

    fn sheet_notes(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> AnyElement {
        if p.notes.is_empty() {
            return div().into_any_element();
        }
        let expanded = self.notes_expanded;
        let long = p.notes.len() > NOTES_COLLAPSED || p.notes.iter().any(|n| n.chars().count() > NOTE_CHARS);
        let mut head = div().flex().justify_between().items_center().child(section_title(t, i18n::t!("app.sheet.notes", n = p.notes.len())));
        if long {
            head = head.child(link(t, "notes-all", if expanded { i18n::t!("app.sheet.see_less") } else { i18n::t!("app.sheet.see_all") }).on_click(cx.listener(|this, _, _, cx| {
                this.notes_expanded = !this.notes_expanded;
                cx.notify();
            })));
        }
        let mut d = div().flex().flex_col().gap(px(10.)).child(head);
        let shown = if expanded { p.notes.len() } else { NOTES_COLLAPSED };
        for note in p.notes.iter().take(shown) {
            let text = if expanded || note.chars().count() <= NOTE_CHARS {
                note.clone()
            } else {
                let cut: String = note.chars().take(NOTE_CHARS).collect();
                format!("{}…", cut.trim_end())
            };
            d = d.child(div().pl(px(12.)).border_l_2().border_color(t.border).text_size(px(13.)).line_height(px(19.)).text_color(t.ink).child(text));
        }
        if !expanded && p.notes.len() > shown {
            d = d.child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.sheet.notes_more", n = p.notes.len() - shown)));
        }
        d.into_any_element()
    }

    fn sheet_children(&self, t: &Tokens, p: &Person, cx: &mut Context<Self>) -> impl IntoElement {
        let kids: Vec<Person> = self.tree.unions(&p.id).into_iter().flat_map(|(_, _, k)| k.into_iter().cloned()).collect();
        let mut d = div().flex().flex_col().gap(px(4.)).child(section_title(t, i18n::t!("app.sheet.children", n = kids.len())));
        for (i, k) in kids.iter().enumerate() {
            d = d.child(self.person_row(t, ("kid", i), k, cx));
        }
        let id = p.id.clone();
        d.child(dashed_button(t, "add-child", i18n::t!("app.sheet.add_child")).when(!kids.is_empty(), |b| b.mt(px(4.))).on_click(cx.listener(move |this, _, window, cx| this.add_and_edit(&id, Rel::Child, None, window, cx))))
    }
}

fn link(t: &Tokens, id: impl Into<ElementId>, text: String) -> Stateful<Div> {
    let ink = t.ink;
    div().id(id).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(t.accent).cursor_pointer().hover(move |s| s.text_color(ink)).child(text)
}

fn dashed_button(t: &Tokens, id: impl Into<ElementId>, text: String) -> Stateful<Div> {
    let (accent, soft) = (t.accent, t.accent_soft);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .h(px(36.))
        .w_full()
        .rounded(px(10.))
        .border_1()
        .border_dashed()
        .border_color(t.slot_dashed)
        .text_color(t.ink_muted)
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.border_color(accent).text_color(accent).bg(soft))
        .child(text)
}
