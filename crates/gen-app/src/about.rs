//! «Acerca de»: logo, version, what the app does and who makes it. Opened from the footer, the
//! native app menu and the globe menu.
use crate::app::GenApp;
use crate::theme::{self, Tokens, size};
use crate::ui;
use gpui::*;
use gpui_component::{IconName, v_flex};
use std::sync::{Arc, LazyLock};

pub enum AboutEvent {
    Close,
}

const CONTEXT: &str = "AboutDialog";
const WIDTH: Pixels = px(420.);
const LOGO_SIZE: Pixels = px(88.);
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

/// «genealogy-rs 0.1.0 · Otherwise Technologies LLC · Acerca de»
fn footer_text() -> String {
    i18n::t!("app.about.footer", app = crate::APP_NAME, version = VERSION, company = crate::DEVELOPER)
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
    let rows = apps.iter().enumerate().map(|(i, (spec, installed))| {
        let installed = installed.clone();
        let hover = t.canvas;
        div()
            .id(("about-app", i))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .px(px(10.))
            .py(px(5.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(div().text_size(px(13.)).text_color(t.ink).child(spec.name.clone()))
            .child(div().text_size(px(11.)).text_color(t.ink_muted).child(installed.version.clone().unwrap_or_default()))
            .on_click(move |_, _, _| {
                let app = installed.clone();
                std::thread::spawn(move || crate::otherwise::launch(&app));
            })
    });
    Some(div().w_full().pt(px(14.)).flex().flex_col().gap(px(2.)).text_left().child(div().px(px(10.)).pb(px(2.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).child(i18n::t!("app.about.more_apps"))).children(rows))
}

fn link(t: &Tokens, id: &'static str, label: String) -> Stateful<Div> {
    div().id(id).text_size(px(13.)).text_color(t.accent).cursor_pointer().hover(|s| s.underline()).child(label).on_click(|_, _, cx| cx.open_url(&crate::star::repo_url()))
}

impl Render for AboutDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::tokens(cx);
        let muted = |text: String| div().text_size(px(12.)).text_color(t.ink_muted).child(text);
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .font_family(theme::SANS)
            .track_focus(&self.focus)
            .key_context(CONTEXT)
            .on_action(cx.listener(|_, _: &CloseAbout, _, cx| cx.emit(AboutEvent::Close)))
            .child(
                div()
                    .id("about-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(t.backdrop())
                    .occlude()
                    .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(AboutEvent::Close))),
            )
            .child(
                div()
                    .id("about-modal")
                    .relative()
                    .w(WIDTH)
                    .rounded(size::DIALOG_RADIUS)
                    .border_1()
                    .border_color(t.border)
                    .bg(t.surface)
                    .shadow_lg()
                    .occlude()
                    .child(
                        v_flex()
                            .items_center()
                            .text_center()
                            .px(px(32.))
                            .pt(px(32.))
                            .pb(px(24.))
                            .gap(px(4.))
                            .child(img(LOGO.clone()).size(LOGO_SIZE).mb(px(8.)))
                            .child(div().font_family(theme::SERIF).font_weight(FontWeight::SEMIBOLD).text_size(px(22.)).text_color(t.ink).child(crate::APP_NAME))
                            .child(muted(i18n::t!("app.about.version", version = VERSION)))
                            .child(div().pt(px(12.)).text_size(px(13.)).line_height(px(19.)).text_color(t.ink).child(i18n::t!("app.about.description")))
                            .child(div().pt(px(16.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(t.ink).child(i18n::t!("app.about.developed_by", company = crate::DEVELOPER)))
                            .child(muted(i18n::t!("app.about.location")))
                            .child(div().pt(px(20.)).child(ui::filled_button(&t, "about-website", t.accent).child(i18n::t!("app.about.website", site = WEBSITE_HOST)).on_click(|_, _, cx| cx.open_url(crate::DEVELOPER_URL))))
                            .child(div().pt(px(12.)).flex().items_center().gap(px(14.)).child(link(&t, "about-source", i18n::t!("app.about.source"))).child(crate::star::about_button(&t, cx)))
                            .child(div().pt(px(6.)).text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.star.ask")))
                            .child(crate::vcity::about_block(&t))
                            .children(other_apps(&t, &self.other_apps))
                            .child(div().pt(px(14.)).text_size(px(11.)).text_color(t.ink_muted).child(i18n::t!("app.about.license"))),
                    )
                    // after the body so it paints, and takes clicks, on top
                    .child(div().absolute().top(px(10.)).right(px(10.)).child(ui::icon_button(&t, "about-close", IconName::Close, true).on_click(cx.listener(|_, _, _, cx| cx.emit(AboutEvent::Close))))),
            )
    }
}

impl GenApp {
    /// Slim bar under the main area on every screen; clicking it opens «Acerca de».
    pub(crate) fn render_footer(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let hover = t.ink;
        div().flex().flex_none().items_center().justify_end().gap(px(12.)).h(px(24.)).px(px(12.)).border_t_1().border_color(t.border).bg(t.surface).child(crate::star::footer_link(t, cx)).child(
            div()
                .id("about-footer")
                .text_size(px(11.))
                .text_color(t.ink_muted)
                .cursor_pointer()
                .hover(move |s| s.text_color(hover).underline())
                .child(footer_text())
                .on_click(cx.listener(|this, _, window, cx| this.open_about(window, cx))),
        )
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
        let s = super::footer_text();
        assert!(s.contains(super::VERSION) && s.contains(crate::DEVELOPER) && s.contains(crate::APP_NAME), "{s}");
    }
}
