//! «Acerca de»: the family's About (`otherwise_ui::AboutModal`) with genealogy's extras (the
//! GitHub star, other Otherwise apps installed here). Opened from the footer, the native app
//! menu and the globe menu.
use crate::app::GenApp;
use crate::theme::{self, Tokens};
use gpui::*;
use otherwise_ui::tokens::{space, text};
use std::sync::{Arc, LazyLock};

pub enum AboutEvent {
    Close,
}

const CONTEXT: &str = "AboutDialog";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const WEBSITE_HOST: &str = "otherwisetech.com"; // i18n-ok: domain
const LOGO_PNG: &[u8] = include_bytes!("../assets/logo/gen-icon-1024.png");

static LOGO: LazyLock<Arc<Image>> = LazyLock::new(|| Arc::new(Image::from_bytes(ImageFormat::Png, LOGO_PNG.to_vec())));

actions!(about, [CloseAbout]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", CloseAbout, Some(CONTEXT))]);
}

/// «Acerca de genealogy-rs», for menus.
pub(crate) fn menu_label() -> String {
    i18n::t!("app.about.menu", app = crate::APP_NAME)
}


pub struct AboutDialog {
    focus: FocusHandle,
    /// Other Otherwise apps installed here (found in the background).
    other_apps: Vec<(otherwise_link::apps::AppSpec, otherwise_link::apps::Installed)>,
}

impl EventEmitter<AboutEvent> for AboutDialog {}

impl AboutDialog {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let task = cx.background_executor().spawn(async { crate::otherwise::other_apps(&otherwise_link::apps::Locations::system()) });
        cx.spawn(async move |this, cx| {
            let found = task.await;
            this.update(cx, |this, cx| {
                this.other_apps = found;
                cx.notify();
            })
            .ok();
        })
        .detach();
        AboutDialog { focus: cx.focus_handle(), other_apps: Vec::new() }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }
}

/// "More Otherwise apps": the ones installed here, each opening on click.
fn other_apps(t: &Tokens, apps: &[(otherwise_link::apps::AppSpec, otherwise_link::apps::Installed)]) -> Option<Div> {
    if apps.is_empty() {
        return None;
    }
    // compact chips that wrap, so About keeps its height however many apps are installed
    let chips = apps.iter().enumerate().map(|(i, (spec, installed))| {
        let installed = installed.clone();
        let hover = t.border_strong;
        div()
            .id(("about-app", i))
            .flex()
            .items_center()
            .gap(px(space::S2))
            .px(px(space::S4))
            .py(px(space::S1))
            .rounded(px(otherwise_ui::tokens::radius::CHIP))
            .border_1()
            .border_color(t.border)
            .text_size(px(text::SMALL))
            .cursor_pointer()
            .hover(move |s| s.border_color(hover))
            .child(div().text_color(t.ink).child(spec.name.clone()))
            .child(div().text_color(t.ink_muted).child(installed.version.clone().unwrap_or_default()))
            .on_click(move |_, _, _| {
                let app = installed.clone();
                std::thread::spawn(move || crate::otherwise::launch(&app));
            })
    });
    Some(
        div()
            .w_full()
            .pt(px(space::S2))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(space::S3))
            .child(otherwise_ui::section_title(t, i18n::t!("app.about.more_apps")))
            .child(div().flex().flex_wrap().justify_center().gap(px(space::S2)).children(chips)),
    )
}

impl Render for AboutDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let star = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(space::S3))
            .child(crate::star::about_button(&t, cx))
            .child(div().text_size(px(text::SMALL)).text_color(t.ink_muted).child(i18n::t!("app.star.ask")));
        let about = otherwise_ui::AboutModal::new(crate::APP_NAME, VERSION, i18n::t!("app.about.developed_by", company = crate::DEVELOPER))
            .logo(LOGO.clone())
            .description(i18n::t!("app.about.description"))
            .place(i18n::t!("app.about.location"))
            .website(i18n::t!("app.about.website", site = WEBSITE_HOST), crate::DEVELOPER_URL)
            .vcity(i18n::t!("app.vcity.pitch"), crate::vcity::menu_label(), crate::vcity::url(crate::otherwise::APP_ID))
            .source(i18n::t!("app.about.source"), crate::star::repo_url())
            .extra(star);
        let about = match other_apps(&t, &self.other_apps) {
            Some(apps) => about.extra(apps),
            None => about,
        };
        div()
            .absolute()
            .inset_0()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|_, _: &CloseAbout, _, cx| cx.emit(AboutEvent::Close)))
            .child(about.license(i18n::t!("app.about.license")).on_dismiss(cx.listener(|_, _: &otherwise_ui::Dismiss, _, cx| cx.emit(AboutEvent::Close))))
    }
}

impl GenApp {
    /// The family's status line under the main area on every screen: the privacy note on the
    /// left, the GitHub star and «genealogy-rs 0.8 · Otherwise Technologies LLC · Acerca de»
    /// (opens About) on the right.
    pub(crate) fn render_footer(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        otherwise_ui::AppFooter::new(crate::APP_NAME, i18n::t!("app.about.footer_link"))
            .version(VERSION)
            .status(otherwise_ui::privacy_note(t, i18n::t!("app.privacy.local"), None))
            .item(crate::star::footer_link(t, cx))
            .on_about(cx.listener(|this, _, window, cx| this.open_about(window, cx)))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn logo_is_embedded_png() {
        assert!(super::LOGO_PNG.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn footer_names_version_and_company() {
        // «genealogy-rs 0.1.0 · Otherwise Technologies LLC · Acerca de», as AppFooter draws it
        let s = otherwise_ui::footer_credit(crate::APP_NAME, Some(super::VERSION), &i18n::t!("app.about.footer_link"));
        assert!(s.contains(super::VERSION) && s.contains(crate::DEVELOPER) && s.contains(crate::APP_NAME), "{s}");
    }
}
