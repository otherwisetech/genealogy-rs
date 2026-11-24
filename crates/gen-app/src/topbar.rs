//! The family's top bar (`otherwise_ui::AppHeader`): logo, tree title, view tabs, search
//! look-alike, undo/redo, open file, import/export menu, «+ Persona», then the AI panel and the
//! preferences (language) menu last.
use crate::app::{GenApp, Tab};
use crate::theme::{Tokens, size, tokens};
use crate::ui;
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

        let search = otherwise_ui::search_frame(&t)
            .id("search")
            .w(px(300.))
            .flex_shrink_1()
            .min_w(px(160.))
            .cursor_text()
            .child(div().flex_1().min_w_0().truncate().text_color(t.ink_muted).child(i18n::t!("app.topbar.search")))
            .child(div().px(px(6.)).py(px(1.)).rounded(px(5.)).border_1().border_color(t.border).bg(t.surface).text_size(px(otherwise_ui::tokens::text::CAPTION)).text_color(t.ink_muted).child("⌘K"))
            .on_click(cx.listener(|this, _, window, cx| this.open_palette(window, cx)));

        let (can_undo, can_redo) = (self.can_undo(false), self.can_undo(true));
        let current = i18n::locale();
        let weak = cx.entity().downgrade();
        let update_check = self.update_check;
        let (share_people, today_on) = (self.share_people, self.today_on);
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
                .item(PopupMenuItem::new(i18n::t!("app.shared.share_people")).checked(share_people).on_click({
                    let w = weak.clone();
                    move |_, _, cx| {
                        w.update(cx, |this, cx| this.set_share_people(!share_people, cx)).ok();
                    }
                }))
                .label(i18n::t!("app.shared.share_people_hint"))
                .item(PopupMenuItem::new(i18n::t!("app.shared.today")).checked(today_on).on_click({
                    let w = weak.clone();
                    move |_, _, cx| {
                        w.update(cx, |this, cx| this.set_today(!today_on, cx)).ok();
                    }
                }))
                .separator()
                .item(PopupMenuItem::new(crate::about::menu_label()).on_click(move |_, window, cx| {
                    about.update(cx, |this, cx| this.open_about(window, cx)).ok();
                }))
                .item(PopupMenuItem::new(crate::vcity::menu_label()).on_click(|_, _, cx| crate::vcity::open(cx)))
        });

        let file_menu = ui::menu_button(&t, "file-menu", i18n::t!("app.topbar.file_menu")).dropdown_menu_with_anchor(Anchor::TopRight, {
            let weak = cx.entity().downgrade();
            let has_file = self.path.is_some();
            move |menu, _, _| {
                let (w1, w2, w3) = (weak.clone(), weak.clone(), weak.clone());
                let menu = menu.item(PopupMenuItem::new(i18n::t!("app.topbar.save_as")).on_click(move |_, window, cx| {
                    w1.update(cx, |this, cx| this.save(true, window, cx)).ok();
                }))
                .item(PopupMenuItem::new(i18n::t!("app.topbar.export7")).on_click(move |_, window, cx| {
                    w2.update(cx, |this, cx| this.export_gedcom7(window, cx)).ok();
                }))
                .separator()
                .item(PopupMenuItem::new(i18n::t!("app.topbar.import")).on_click(move |_, window, cx| {
                    w3.update(cx, |this, cx| this.import_dialog(window, cx)).ok();
                }));
                let (w4, w5) = (weak.clone(), weak.clone());
                let mut menu = menu
                    .separator()
                    .item(PopupMenuItem::new(i18n::t!("app.file.move_to")).disabled(!has_file).on_click(move |_, window, cx| {
                        w4.update(cx, |this, cx| this.move_dialog(window, cx)).ok();
                    }))
                    .item(PopupMenuItem::new(crate::storage::reveal_label()).disabled(!has_file).on_click(move |_, _, cx| {
                        w5.update(cx, |this, cx| this.reveal_file(cx)).ok();
                    }));
                let recent = crate::storage::Recent::load().files;
                if !recent.is_empty() {
                    menu = menu.separator().label(i18n::t!("app.file.recent"));
                    for p in recent {
                        let p = std::path::PathBuf::from(p);
                        let w = weak.clone();
                        menu = menu.item(PopupMenuItem::new(crate::files::recent_label(&p)).on_click(move |_, window, cx| {
                            let p = p.clone();
                            w.update(cx, |this, cx| this.open_recent(p, window, cx)).ok();
                        }));
                    }
                }
                menu
            }
        });

        otherwise_ui::AppHeader::new(otherwise_ui::logo_mark(crate::LOGO_SVG), self.tree.title())
            .subtitle(subtitle)
            .center(self.render_tabs(&t, cx))
            .action(search)
            .action(self.render_review_button(&t, cx))
            .action(
                div()
                    .flex()
                    .child(ui::icon_button(&t, "undo", IconName::Undo, can_undo).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.undo")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.undo(false, window, cx))))
                    .child(ui::icon_button(&t, "redo", IconName::Redo, can_redo).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.redo")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.undo(true, window, cx)))),
            )
            .action(ui::icon_button(&t, "open", IconName::FolderOpen, true).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.topbar.open")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))))
            .action(file_menu)
            .action(
                ui::filled_button(&t, "new-person", t.accent)
                    .child(i18n::t!("app.topbar.new_person"))
                    .on_click(cx.listener(|this, _, window, cx| this.create_person("", "", window, cx))),
            )
            .action(ui::icon_button(&t, "assistant", IconName::Bot, true).tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.agent.toggle")).build(w, cx)).on_click(cx.listener(|this, _, window, cx| this.toggle_agent_panel(window, cx))))
            .action(lang)
    }

    fn render_tabs(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = [(Tab::Pedigree, "app.tab.pedigree"), (Tab::Descendants, "app.tab.descendants"), (Tab::Fan, "app.tab.fan"), (Tab::List, "app.tab.list"), (Tab::Map, "app.tab.map")];
        let mut seg = otherwise_ui::segmented(t);
        for (i, (tab, key)) in tabs.into_iter().enumerate() {
            seg = seg.child(
                otherwise_ui::segment(t, ("tab", i), self.tab == tab)
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
