//! «Unir personas» confirmation: both people side by side, a swap to choose who is kept.
//! Like the edit dialog it only emits events; the host applies the merge (and its undo step).
use crate::theme::{self, Tokens, size};
use crate::ui;
use gedcom::Tree;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{Icon, IconName, Sizable, h_flex, v_flex};
use std::rc::Rc;

pub enum MergeDialogEvent {
    /// «Unir personas» / ⌘↵: fold `dup` into `keep`
    Confirm { keep: String, dup: String },
    /// «Cancelar», ×, esc or backdrop click
    Cancel,
}

const CONTEXT: &str = "MergeDialog";
const WIDTH: Pixels = px(640.);

actions!(merge_dialog, [ConfirmMerge, CancelMerge, SwapMerge]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-enter", ConfirmMerge, Some(CONTEXT)),
        KeyBinding::new("escape", CancelMerge, Some(CONTEXT)),
        KeyBinding::new("tab", SwapMerge, Some(CONTEXT)),
    ]);
}

pub struct MergeDialog {
    tree: Rc<Tree>,
    keep: String,
    dup: String,
    focus: FocusHandle,
}

impl EventEmitter<MergeDialogEvent> for MergeDialog {}

impl MergeDialog {
    pub fn new(tree: Rc<Tree>, keep: String, dup: String, cx: &mut Context<Self>) -> Self {
        MergeDialog { tree, keep, dup, focus: cx.focus_handle() }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    fn swap(&mut self, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.keep, &mut self.dup);
        cx.notify();
    }

    fn confirm(&mut self, cx: &mut Context<Self>) {
        if self.tree.can_merge(&self.keep, &self.dup).is_ok() {
            cx.emit(MergeDialogEvent::Confirm { keep: self.keep.clone(), dup: self.dup.clone() });
        }
    }

    fn column(&self, id: &str, kept: bool, t: &Tokens) -> Div {
        let p = self.tree.person(id);
        let name = p.map(|p| p.name()).unwrap_or_default();
        let parents = self.tree.parents(id).len();
        let unions = self.tree.unions(id);
        let partners = unions.iter().filter(|u| u.1.is_some()).count();
        let children: usize = unions.iter().map(|u| u.2.len()).sum();
        let stat = |label: String, n: usize| h_flex().justify_between().text_size(px(12.)).child(div().text_color(t.ink_muted).child(label)).child(div().font_weight(FontWeight::MEDIUM).text_color(t.ink).child(n.to_string()));
        let (badge_bg, badge_fg, badge) = if kept { (t.accent_soft, t.accent, i18n::t!("dialogs.merge.kept")) } else { (t.canvas, t.ink_muted, i18n::t!("dialogs.merge.merged")) };
        v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .p_4()
            .rounded(px(12.))
            .border_1()
            .when(kept, |d| d.border_2().border_color(t.accent))
            .when(!kept, |d| d.border_color(t.border).border_dashed())
            .child(h_flex().justify_between().child(ui::chip(badge, badge_fg, badge_bg)).child(ui::chip(id.to_string(), t.ink_muted, t.canvas)))
            .child(
                v_flex()
                    .items_center()
                    .gap_1()
                    .child(ui::avatar(t, p, 56.))
                    .child(div().font_family(theme::SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(17.)).text_color(t.ink).text_center().child(name))
                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(p.map(ui::years).unwrap_or_default())),
            )
            .child(
                v_flex()
                    .gap_1()
                    .pt_2()
                    .border_t_1()
                    .border_color(t.border)
                    .child(stat(i18n::t!("dialogs.merge.parents"), parents))
                    .child(stat(i18n::t!("dialogs.merge.partners"), partners))
                    .child(stat(i18n::t!("dialogs.merge.children"), children)),
            )
    }
}

fn button(id: &'static str, label: String, primary: bool, enabled: bool, t: &Tokens) -> Stateful<Div> {
    let base = div().id(id).h(size::FIELD_H).px_3p5().flex().items_center().rounded(px(8.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(label);
    let base = if enabled { base.cursor_pointer() } else { base.opacity(0.5) };
    if primary {
        let accent = t.accent;
        base.bg(accent).text_color(white()).when(enabled, |d| d.hover(move |s| s.bg(accent.opacity(0.9))))
    } else {
        let strong = t.border_strong;
        base.border_1().border_color(t.border).text_color(t.ink).hover(move |s| s.border_color(strong))
    }
}

impl Render for MergeDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let name = |id: &str| self.tree.person(id).map(|p| p.name()).unwrap_or_default();
        let (keep_name, dup_name) = (name(&self.keep), name(&self.dup));
        let blocked = self.tree.can_merge(&self.keep, &self.dup).err().map(|e| e.to_string());
        let swap = div()
            .id("merge-swap")
            .flex_none()
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(t.border)
            .bg(t.surface)
            .cursor_pointer()
            .hover(|s| s.border_color(t.border_strong))
            .tooltip(|window, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("dialogs.merge.swap")).build(window, cx))
            .child(v_flex().child(Icon::new(IconName::ArrowRight).xsmall()).child(Icon::new(IconName::ArrowLeft).xsmall()))
            .on_click(cx.listener(|this, _, _, cx| this.swap(cx)));
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|this, _: &ConfirmMerge, _, cx| this.confirm(cx)))
            .on_action(cx.listener(|_, _: &CancelMerge, _, cx| cx.emit(MergeDialogEvent::Cancel)))
            .on_action(cx.listener(|this, _: &SwapMerge, _, cx| this.swap(cx)))
            .child(
                div()
                    .id("merge-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(t.backdrop)
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(MergeDialogEvent::Cancel))),
            )
            .child(
                v_flex()
                    .id("merge-modal")
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
                            .gap_3()
                            .px_5()
                            .pt_5()
                            .pb_3()
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap_0p5()
                                    .child(div().font_family(theme::SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(19.)).text_color(t.ink).child(i18n::t!("dialogs.merge.title")))
                                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.merge.subtitle"))),
                            )
                            .child(
                                div()
                                    .id("merge-close")
                                    .size(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(8.))
                                    .cursor_pointer()
                                    .text_color(t.ink_muted)
                                    .hover(|s| s.bg(t.canvas).text_color(t.ink))
                                    .child(Icon::new(IconName::Close).small())
                                    .on_click(cx.listener(|_, _, _, cx| cx.emit(MergeDialogEvent::Cancel))),
                            ),
                    )
                    .child(
                        v_flex()
                            .id("merge-body")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px_5()
                            .pb_4()
                            .gap_4()
                            .child(h_flex().gap_3().items_center().child(self.column(&self.keep, true, &t)).child(swap).child(self.column(&self.dup, false, &t)))
                            .child(
                                v_flex()
                                    .gap_1()
                                    .p_3()
                                    .rounded(px(10.))
                                    .bg(t.accent_soft)
                                    .text_size(px(13.))
                                    .child(div().text_color(t.ink).child(i18n::t!("dialogs.merge.explain", keep = keep_name.as_str(), dup = dup_name.as_str())))
                                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.merge.detail"))),
                            )
                            .when_some(blocked.clone(), |d, e| d.child(div().text_size(px(12.)).text_color(t.danger).child(e))),
                    )
                    .child(
                        h_flex()
                            .gap_3()
                            .px_5()
                            .py_3()
                            .border_t_1()
                            .border_color(t.border)
                            .child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("dialogs.merge.hint")))
                            .child(div().flex_1())
                            .child(button("merge-cancel", i18n::t!("dialogs.merge.cancel"), false, true, &t).on_click(cx.listener(|_, _, _, cx| cx.emit(MergeDialogEvent::Cancel))))
                            .child(button("merge-confirm", i18n::t!("dialogs.merge.confirm"), true, blocked.is_none(), &t).on_click(cx.listener(|this, _, _, cx| this.confirm(cx)))),
                    ),
            )
    }
}
