//! «Lista» tab: everyone, virtualised.
use crate::app::{GenApp, Tab};
use crate::theme::{Tokens, tokens};
use crate::ui::{self, avatar};
use gpui::prelude::FluentBuilder as _;
use gpui::*;

const ROW_H: f32 = 48.;
const YEARS_W: f32 = 140.;
const PLACE_W: f32 = 280.;

fn header(t: &Tokens) -> impl IntoElement {
    let cell = |s: String| div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(s);
    div()
        .flex()
        .items_center()
        .h(px(40.))
        .px(px(24.))
        .gap(px(16.))
        .border_b_1()
        .border_color(t.border)
        .child(div().flex_1().child(cell(i18n::t!("app.list.name"))))
        .child(div().w(px(YEARS_W)).child(cell(i18n::t!("app.list.years"))))
        .child(div().w(px(PLACE_W)).child(cell(i18n::t!("app.list.birth_place"))))
}

impl GenApp {
    pub(crate) fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens(cx);
        let count = self.tree.order.len();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(t.surface)
            .child(header(&t))
            .child(
                uniform_list(
                    "people-list",
                    count,
                    cx.processor(|this, range: std::ops::Range<usize>, _window, cx| {
                        let t = tokens(cx);
                        range
                            .filter_map(|ix| {
                                let id = this.tree.order.get(ix)?.clone();
                                let p = this.tree.person(&id)?;
                                let sel = this.focus_id.as_deref() == Some(id.as_str());
                                let hover = t.canvas;
                                let place = p.birth.as_ref().and_then(|e| e.place.clone()).unwrap_or_default();
                                Some(
                                    div()
                                        .id(("row", ix))
                                        .flex()
                                        .items_center()
                                        .gap(px(16.))
                                        .h(px(ROW_H))
                                        .px(px(24.))
                                        .border_b_1()
                                        .border_color(t.border)
                                        .cursor_pointer()
                                        .when(sel, |d| d.bg(t.accent_soft))
                                        .hover(move |s| s.bg(hover))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.tab = Tab::Pedigree;
                                            this.restore_focus(window, cx);
                                            this.set_focus(id.clone(), true, window, cx);
                                        }))
                                        .child(div().flex_1().min_w_0().flex().items_center().gap(px(10.)).child(avatar(&t, Some(p), 30.)).child(div().font_weight(FontWeight::MEDIUM).truncate().child(p.name())))
                                        .child(div().w(px(YEARS_W)).text_size(px(12.)).text_color(t.ink_muted).child(ui::years(p)))
                                        .child(div().w(px(PLACE_W)).text_size(px(12.)).text_color(t.ink_muted).truncate().child(place)),
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.list_scroll)
                .flex_1(),
            )
    }
}
