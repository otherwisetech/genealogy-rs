// No console window behind the app on Windows release builds.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod about;
mod app;
mod edit_dialog;
mod fan;
mod files;
mod kinship_dialog;
mod list;
mod map;
mod merge_dialog;
mod open_files;
mod otherwise;
mod palette;
mod pedigree;
mod review;
mod settings;
mod shared;
mod sheet;
mod star;
mod storage;
mod vcity;
mod theme;
mod topbar;
mod ui;
mod update;

use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;
use std::borrow::Cow;
use std::path::PathBuf;

actions!(genealogy, [Quit, OpenDeveloperSite, ShowAbout, OpenVcity]);

pub(crate) const APP_NAME: &str = "genealogy-rs"; // i18n-ok: product name
pub(crate) const DEVELOPER: &str = "Otherwise Technologies LLC"; // i18n-ok: company name
pub(crate) const DEVELOPER_URL: &str = "https://otherwisetech.com";

/// «Developed by Otherwise Technologies LLC · Wyoming, USA» in the current language.
pub(crate) fn developer_credit() -> String {
    i18n::t!("app.credit", company = DEVELOPER)
}

/// The native menu bar holds plain strings, so it is rebuilt on every language switch.
pub(crate) fn set_app_menus(cx: &mut App) {
    let items = vec![MenuItem::action(about::menu_label(), ShowAbout), MenuItem::action(developer_credit(), OpenDeveloperSite), MenuItem::separator(), MenuItem::action(i18n::t!("app.menu.quit"), Quit)];
    let file = vec![
        MenuItem::action(i18n::t!("app.menu.open"), app::Open),
        MenuItem::action(i18n::t!("app.menu.save"), app::Save),
        MenuItem::action(i18n::t!("app.menu.save_as"), app::SaveAs),
        MenuItem::action(i18n::t!("app.file.move_to"), app::MoveTo),
        MenuItem::action(storage::reveal_label(), app::RevealFile),
    ];
    let help = vec![MenuItem::action(vcity::menu_label(), OpenVcity)];
    cx.set_menus(vec![
        Menu { name: APP_NAME.into(), items, disabled: false },
        Menu { name: i18n::t!("app.menu.file").into(), items: file, disabled: false },
        Menu { name: i18n::t!("app.menu.help").into(), items: help, disabled: false },
    ]);
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

/// Sets the Dock icon at runtime: covers `cargo run` (no bundle) and stale Dock icon caches.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // objc 0.2 macros check a `cargo-clippy` feature
fn set_dock_icon() {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    static PNG: &[u8] = include_bytes!("../assets/logo/gen-icon-1024.png");
    // SAFETY: plain AppKit calls on the main thread with a valid byte buffer that NSData copies.
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), dataWithBytes: PNG.as_ptr() length: PNG.len()];
        let image: *mut Object = msg_send![class!(NSImage), alloc];
        let image: *mut Object = msg_send![image, initWithData: data];
        if image.is_null() {
            return;
        }
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, setApplicationIconImage: image];
        let _: () = msg_send![image, release];
    }
}

const FONTS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/Inter-Regular.ttf"),
    include_bytes!("../assets/fonts/Inter-Medium.ttf"),
    include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Fraunces72pt-SemiBold.ttf"),
];

fn main() {
    storage::migrate_on_startup();
    i18n::set_locale(settings::initial_language(&settings::Settings::load()));
    let initial: Option<PathBuf> = open_files::path_from_args(std::env::args().skip(1));
    if matches!(initial.as_deref().and_then(|p| p.to_str()), Some("-h" | "--help")) {
        println!("{}", i18n::t!("app.cli.usage"));
        return;
    }
    // Linux `%u` / the Windows URL handler pass our links as the argument
    let initial = match initial.as_deref().and_then(|p| p.to_str()).and_then(otherwise::url_action) {
        Some(otherwise::Action::Open(p)) => Some(p),
        Some(a) => {
            otherwise::push(a);
            None
        }
        None => initial,
    };
    otherwise::register_scheme();
    // Finder files and otherwise-genealogy:// links arrive as Apple Events, possibly before the
    // window exists: queued, and the window drains them
    let application = gpui_platform::application().with_assets(AppAssets);
    application.on_open_urls(|urls| {
        for a in urls.iter().filter_map(|u| otherwise::url_action(u)) {
            otherwise::push(a);
        }
    });
    application.run(move |cx| {
        if let Err(e) = cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()) {
            eprintln!("fonts: {e}");
        }
        #[cfg(target_os = "macos")]
        set_dock_icon();
        gpui_component::init(cx);
        app::sync_theme(None, cx);
        app::bind_keys(cx);
        edit_dialog::bind_keys(cx);
        palette::bind_keys(cx);
        merge_dialog::bind_keys(cx);
        review::bind_keys(cx);
        kinship_dialog::bind_keys(cx);
        about::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &OpenDeveloperSite, cx| cx.open_url(DEVELOPER_URL));
        cx.on_action(|_: &OpenVcity, cx| vcity::open(cx));
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
                let v = cx.new(|cx| app::GenApp::new(initial, window, cx));
                cx.new(|cx| Root::new(v, window, cx))
            })
            .expect("open window");
        })
        .detach();
    });
}
