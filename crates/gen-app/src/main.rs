mod app;
mod edit_dialog;
mod fan;
mod list;
mod palette;
mod pedigree;
mod settings;
mod sheet;
mod theme;
mod topbar;
mod ui;

use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;
use std::borrow::Cow;
use std::path::PathBuf;

actions!(genealogy, [Quit]);

pub(crate) const APP_NAME: &str = "genealogy-rs"; // i18n-ok: product name

/// The native menu bar holds plain strings, so it is rebuilt on every language switch.
pub(crate) fn set_app_menus(cx: &mut App) {
    cx.set_menus(vec![Menu { name: APP_NAME.into(), items: vec![MenuItem::action(i18n::t!("app.menu.quit"), Quit)], disabled: false }]);
}

/// gpui-component's icon set plus our own `icons/*.svg`.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match path {
            LOGO_SVG => Ok(Some(Cow::Borrowed(include_bytes!("../assets/icons/gen-logo.svg")))),
            _ => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Assets.list(path)
    }
}

pub(crate) const LOGO_SVG: &str = "icons/gen-logo.svg";

const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Fraunces72pt-SemiBold.ttf"),
];

fn main() {
    i18n::set_locale(settings::initial_language(&settings::Settings::load()));
    let arg = std::env::args().nth(1);
    if matches!(arg.as_deref(), Some("-h" | "--help")) {
        println!("{}", i18n::t!("app.cli.usage"));
        return;
    }
    let initial: Option<PathBuf> = arg.map(PathBuf::from);
    gpui_platform::application().with_assets(AppAssets).run(move |cx| {
        if let Err(e) = cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()) {
            eprintln!("fonts: {e}");
        }
        gpui_component::init(cx);
        app::sync_theme(None, cx);
        app::bind_keys(cx);
        edit_dialog::bind_keys(cx);
        palette::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        gpui_component::set_locale(i18n::locale());
        set_app_menus(cx);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..Default::default() }),
            ..Default::default()
        };
        let initial = initial.clone();
        cx.spawn(async move |cx| {
            cx.open_window(window_options, |window, cx| {
                let view = cx.new(|cx| app::GenApp::new(initial, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("open window");
        })
        .detach();
    });
}
