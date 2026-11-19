//! «Calcular parentesco»: how two people are related, with the path drawn as a Λ of mini cards
//! (A's line up to the common ancestor on the left, down to B on the right).
use crate::theme::{self, Tokens, size};
use crate::ui;
use gedcom::{KinKind, Kinship, Link, Tree};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};
use std::rc::Rc;

pub enum KinshipDialogEvent {
    /// a person in the path was clicked: focus it on the canvas
    Focus(String),
    Close,
}

const CONTEXT: &str = "KinshipDialog";
const WIDTH: Pixels = px(600.);
const CARD_W: Pixels = px(230.);

actions!(kinship_dialog, [CloseKinship, SwapKinship]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseKinship, Some(CONTEXT)), KeyBinding::new("tab", SwapKinship, Some(CONTEXT))]);
}

pub struct KinshipDialog {
    tree: Rc<Tree>,
    /// reference person: the result reads «b es … de a»
    a: String,
    b: String,
    focus: FocusHandle,
}

impl EventEmitter<KinshipDialogEvent> for KinshipDialog {}

impl KinshipDialog {
    pub fn new(tree: Rc<Tree>, a: String, b: String, cx: &mut Context<Self>) -> Self {
        KinshipDialog { tree, a, b, focus: cx.focus_handle() }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    fn swap(&mut self, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.a, &mut self.b);
        cx.notify();
    }

    fn card(&self, i: usize, id: &str, apex: bool, t: &Tokens, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.tree.person(id);
        let hover = t.border_strong;
        let target = id.to_string();
        let ends = id == self.a || id == self.b;
        h_flex()
            .id(("kin-card", i))
            .w(CARD_W)
            .gap_2p5()
            .px_3()
            .py_2()
            .rounded(px(12.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .when(apex, |d| d.border_2().border_color(t.accent))
            .when(ends && !apex, |d| d.border_color(t.ink_muted))
            .cursor_pointer()
            .hover(move |s| s.border_color(hover).shadow_sm())
            .on_click(cx.listener(move |_, _, _, cx| cx.emit(KinshipDialogEvent::Focus(target.clone()))))
            .child(ui::avatar(t, p, size::AVATAR_ROW.into()))
            .child(
                v_flex()
                    .min_w_0()
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink).truncate().child(p.map(|p| p.name()).unwrap_or_default()))
                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(p.map(ui::years).unwrap_or_default())),
            )
    }

    /// Connector between two stacked cards with the step's relation («padre», «hija»…).
    fn step(label: String, link: Link, t: &Tokens) -> Div {
        let icon = match link {
            Link::Parent => IconName::ArrowUp,
            Link::Child => IconName::ArrowDown,
            Link::Spouse => IconName::Heart,
        };
        h_flex()
            .w(CARD_W)
            .h(px(30.))
            .justify_center()
            .gap_1()
            .text_size(px(11.))
            .text_color(t.ink_muted)
            .child(div().w(px(1.5)).h_full().bg(t.connector))
            .child(Icon::new(icon).xsmall().text_color(t.connector))
            .child(label)
    }

    fn render_path(&self, k: &Kinship, t: &Tokens, cx: &mut Context<Self>) -> Div {
        let apex = k.apex();
        let mut left = v_flex().items_center();
        // A's side, top (just below the apex) to bottom (A): path[apex-1] … path[0]
        for j in (0..apex).rev() {
            left = left.child(Self::step(k.link_label(&self.tree, j), k.links[j], t)).child(self.card(j, &k.path[j], false, t, cx));
        }
        let mut right = v_flex().items_center();
        for j in apex + 1..k.path.len() {
            right = right.child(Self::step(k.link_label(&self.tree, j - 1), k.links[j - 1], t)).child(self.card(j, &k.path[j], false, t, cx));
        }
        let mut cols = h_flex().items_start().justify_center().gap_6();
        if apex > 0 {
            cols = cols.child(left);
        }
        if apex + 1 < k.path.len() {
            cols = cols.child(right);
        }
        v_flex().items_center().child(self.card(apex, &k.path[apex], true, t, cx)).child(cols)
    }
}

impl Render for KinshipDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        // built every render: labels follow the current language
        let k = self.tree.kinship(&self.a, &self.b);
        let name = |id: &str| self.tree.person(id).map(|p| p.name()).unwrap_or_default();
        let sentence = k.as_ref().map(|k| k.sentence(&self.tree)).unwrap_or_default();
        let mut info = v_flex().gap_1().text_size(px(12.)).text_color(t.ink_muted);
        if let Some(k) = &k {
            if !k.common.is_empty() {
                let names: Vec<String> = k.common.iter().map(|id| name(id)).collect();
                info = info.child(div().child(i18n::tn!("dialogs.kinship.common", names.len(), names = gedcom::insights::join_list(&names))));
            }
            match k.kind {
                KinKind::Blood | KinKind::Marriage => {
                    let via = if k.kind == KinKind::Marriage { i18n::t!("dialogs.kinship.via_marriage") } else { i18n::t!("dialogs.kinship.blood") };
                    info = info.child(div().child(i18n::t!("dialogs.kinship.steps", kind = via, up = k.up, down = k.down)));
                }
                KinKind::None => info = info.child(div().child(i18n::t!("dialogs.kinship.none_hint"))),
                KinKind::Same => {}
            }
            if !k.others.is_empty() {
                let labels: Vec<String> = k.others.iter().map(|o| o.label.clone()).collect();
                info = info.child(div().text_color(t.ink).child(i18n::tn!("dialogs.kinship.others", labels.len(), list = gedcom::insights::join_list(&labels))));
            }
        }
        let path = k.as_ref().filter(|k| k.path.len() > 1).map(|k| self.render_path(k, &t, cx));
        let swap = div()
            .id("kin-swap")
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(t.ink_muted)
            .hover(|s| s.bg(t.canvas).text_color(t.ink))
            .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("dialogs.kinship.swap")).build(window, cx))
            .child(v_flex().child(Icon::new(IconName::ArrowRight).xsmall()).child(Icon::new(IconName::ArrowLeft).xsmall()))
            .on_click(cx.listener(|this, _, _, cx| this.swap(cx)));
        let close = div()
            .id("kin-close")
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(t.ink_muted)
            .hover(|s| s.bg(t.canvas).text_color(t.ink))
            .child(Icon::new(IconName::Close).small())
            .on_click(cx.listener(|_, _, _, cx| cx.emit(KinshipDialogEvent::Close)));
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|_, _: &CloseKinship, _, cx| cx.emit(KinshipDialogEvent::Close)))
            .on_action(cx.listener(|this, _: &SwapKinship, _, cx| this.swap(cx)))
            .child(
                div()
                    .id("kin-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(t.backdrop())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(KinshipDialogEvent::Close))),
            )
            .child(
                v_flex()
                    .id("kin-modal")
                    .w(WIDTH)
                    .max_h(window.viewport_size().height * 0.9)
                    .rounded(size::DIALOG_RADIUS)
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface)
                    .shadow_lg()
                    .occlude()
                    .child(
                        h_flex()
                            .gap_2()
                            .px_5()
                            .pt_4()
                            .child(div().flex_1().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(i18n::t!("dialogs.kinship.title")))
                            .child(swap)
                            .child(close),
                    )
                    .child(
                        v_flex()
                            .id("kin-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px_5()
                            .pb_5()
                            .gap_3()
                            .child(div().font_family(theme::SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(22.)).line_height(px(28.)).text_color(t.ink).child(sentence))
                            .child(info)
                            .when_some(path, |d, p| d.child(div().pt_2().p_4().rounded(px(12.)).bg(t.canvas).child(p))),
                    )
                    .child(
                        h_flex()
                            .px_5()
                            .py_3()
                            .border_t_1()
                            .border_color(t.border)
                            .text_size(px(12.))
                            .text_color(t.ink_muted)
                            .child(i18n::t!("dialogs.kinship.hint")),
                    ),
            )
    }
}
