//! 56 px top bar: logo, tree title, view tabs, search look-alike, undo/redo, import/export menu, «+ Persona»,
//! open file and the language switcher.
use crate::app::{GenApp, Tab};
use crate::theme::{SERIF, Tokens, size, tokens};
use crate::ui::{self, icon};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_component::{IconName, Sizable as _};

impl GenApp {
    pub(crate) fn render_topbar(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens(cx);
        let (np, nf) = self.tree.stats();
        let state = if self.dirty { i18n::t!("app.topbar.unsaved") } else { i18n::t!("app.topbar.saved") };
        let subtitle = format!("{} · {} · {state}", i18n::tn!("app.topbar.people", np), i18n::tn!("app.topbar.families", nf));

        let logo = div().flex().flex_none().items_center().justify_center().size(px(32.)).rounded(px(9.)).bg(t.accent).child(svg().path(crate::LOGO_SVG).size(px(20.)).text_color(t.on_fill()));
        let title = div()
            .flex()
            .flex_col()
            .min_w_0()
            .child(div().font_family(SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(16.)).line_height(px(20.)).truncate().child(self.tree.title()))
            .child(div().text_size(px(12.)).text_color(t.ink_muted).truncate().child(subtitle));

        let search = div()
            .id("search")
            .flex()
            .items_center()
            .gap(px(8.))
            .h(size::CONTROL_H)
            .w(px(300.))
            .flex_shrink_1()
            .min_w(px(160.))
            .px(px(12.))
            .rounded(px(10.))
            .bg(t.canvas)
            .border_1()
            .border_color(t.border)
            .cursor_text()
            .child(icon(IconName::Search, 14., t.ink_muted))
            .child(div().flex_1().min_w_0().truncate().text_color(t.ink_muted).child(i18n::t!("app.topbar.search")))
            .child(div().px(px(6.)).py(px(1.)).rounded(px(5.)).border_1().border_color(t.border).bg(t.surface).text_size(px(11.)).text_color(t.ink_muted).child("⌘K"))
            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx)));

        let (can_undo, can_redo) = (self.can_undo(false), self.can_undo(true));
        let current = i18n::locale();
        let weak = cx.entity().downgrade();
        let update_check = self.update_check;
        let lang = Button::new("lang").ghost().icon(IconName::Globe).with_size(size::CONTROL_H).tooltip(i18n::t!("app.topbar.preferences")).dropdown_menu_with_anchor(Anchor::TopRight, move |mut menu, _, _| {
            menu = menu.label(i18n::t!("common.language"));
            for (code, name) in i18n::available() {
                let w = weak.clone();
                menu = menu.item(PopupMenuItem::new(name).checked(code == current).on_click(move |_, window, cx| {
                    w.update(cx, |this, cx| this.set_language(code, window, cx)).ok();
                }));
            }
            let w = weak.clone();
            let about = weak.clone();
            menu.separator()
                .item(PopupMenuItem::new(i18n::t!("app.update.setting")).checked(update_check).on_click(move |_, _, cx| {
                    w.update(cx, |this, cx| this.set_update_check(!update_check, cx)).ok();
                }))
                .label(i18n::t!("app.update.setting_hint"))
                .separator()
                .item(PopupMenuItem::new(crate::about::menu_label()).on_click(move |_, window, cx| {
                    about.update(cx, |this, cx| this.open_about(window, cx)).ok();
                }))
        });

        let file_menu = ui::menu_button(&t, "file-menu", i18n::t!("app.topbar.file_menu")).dropdown_menu_with_anchor(Anchor::TopRight, {
            let weak = cx.entity().downgrade();
            move |menu, _, _| {
                let (w1, w2, w3) = (weak.clone(), weak.clone(), weak.clone());
                menu.item(PopupMenuItem::new(i18n::t!("app.topbar.save_as")).on_click(move |_, window, cx| {
                    w1.update(cx, |this, cx| this.save(true, window, cx)).ok();
                }))
                .item(PopupMenuItem::new(i18n::t!("app.topbar.export7")).on_click(move |_, window, cx| {
                    w2.update(cx, |this, cx| this.export_gedcom7(window, cx)).ok();
                }))
                .separator()
                .item(PopupMenuItem::new(i18n::t!("app.topbar.import")).on_click(move |_, window, cx| {
                    w3.update(cx, |this, cx| this.import_dialog(window, cx)).ok();
                }))
            }
        });

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(12.))
            .h(size::TOPBAR_H)
            .px(px(16.))
            .bg(t.surface)
            .border_b_1()
            .border_color(t.border)
            .child(logo)
            .child(div().w(px(220.)).flex_shrink_1().min_w(px(120.)).child(title))
            .child(self.render_tabs(&t, cx))
            .child(div().flex_1())
            .child(search)
            .child(self.render_review_button(&t, cx))
            .child(
                div()
                    .flex()
                    .child(ui::icon_button(&t, "undo", IconName::Undo, can_undo).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.undo")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.undo(false, window, cx))))
                    .child(ui::icon_button(&t, "redo", IconName::Redo, can_redo).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.redo")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.undo(true, window, cx)))),
            )
            .child(ui::icon_button(&t, "open", IconName::FolderOpen, true).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.open")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))))
            .child(file_menu)
            .child(
                ui::filled_button(&t, "new-person", t.accent)
                    .child(i18n::t!("app.topbar.new_person"))
                    .on_click(cx.listener(|this, _, window, cx| this.create_person("", "", window, cx))),
            )
            .child(lang)
    }

    fn render_tabs(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = [(Tab::Pedigree, "app.tab.pedigree"), (Tab::Descendants, "app.tab.descendants"), (Tab::Fan, "app.tab.fan"), (Tab::List, "app.tab.list"), (Tab::Map, "app.tab.map")];
        let mut seg = div().flex().flex_none().items_center().p(px(3.)).gap(px(2.)).h(size::CONTROL_H).rounded(px(10.)).bg(t.canvas);
        for (i, (tab, key)) in tabs.into_iter().enumerate() {
            let sel = self.tab == tab;
            let ink = t.ink;
            seg = seg.child(
                div()
                    .id(("tab", i))
                    .flex()
                    .items_center()
                    .h_full()
                    .px(px(12.))
                    .rounded(px(8.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .map(|d| if sel { d.bg(t.surface).text_color(t.ink).shadow_xs() } else { d.text_color(t.ink_muted).hover(move |s| s.text_color(ink)) })
                    .child(i18n::t!(key))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.tab != tab {
                            this.tab = tab;
                            this.hovered = None;
                            this.restore_focus(window, cx);
                            this.recenter(false, window);
                            if tab == Tab::List
                                && let Some(ix) = this.focus_id.as_ref().and_then(|f| this.tree.order.iter().position(|o| o == f))
                            {
                                this.list_scroll.scroll_to_item(ix, ScrollStrategy::Center);
                            }
                            cx.notify();
                        }
                    })),
            );
        }
        seg
    }
}
