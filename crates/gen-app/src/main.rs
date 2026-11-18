mod app;
mod edit_dialog;
mod palette;
mod theme;

use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;
use std::path::PathBuf;

actions!(genealogy, [Quit]);

fn main() {
    let arg = std::env::args().nth(1);
    if matches!(arg.as_deref(), Some("-h" | "--help")) {
        println!("usage: gen [FILE.ged]\n\nExplore and edit GEDCOM family trees.");
        return;
    }
    let initial: Option<PathBuf> = arg.map(PathBuf::from);
    gpui_platform::application().with_assets(Assets).run(move |cx| {
        gpui_component::init(cx);
        app::bind_keys(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![Menu { name: "gen".into(), items: vec![MenuItem::action("Quit gen", Quit)], disabled: false }]);
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1440.), px(900.)), cx)),
            titlebar: Some(TitlebarOptions { title: Some("gen".into()), ..Default::default() }),
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
