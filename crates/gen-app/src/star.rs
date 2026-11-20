//! «Star on GitHub» in the footer and the About dialog, with the repo's stargazer count. The count
//! is an unauthenticated GET to the public repos API (nothing about the user or their trees is
//! sent), at most once a day, cached in the cache dir; any failure just hides it. Follows the
//! «check for updates» preference, our only switch for talking to GitHub.
use crate::map::net;
use crate::theme::Tokens;
use gpui::*;
use gpui::prelude::FluentBuilder;
use gpui_component::IconName;
use std::path::PathBuf;

const REPO: &str = "otherwisetech/genealogy-rs";
pub const REFRESH_SECS: u64 = 24 * 60 * 60;

pub fn repo_url() -> String {
    format!("https://github.com/{REPO}")
}

pub fn api_url() -> String {
    format!("https://api.github.com/repos/{REPO}")
}

pub fn user_agent() -> String {
    format!("{}/{} (+https://otherwisetech.com)", crate::APP_NAME, env!("CARGO_PKG_VERSION")) // i18n-ok: HTTP header
}

/// `stargazers_count` of a `repos/{owner}/{repo}` body.
pub fn parse_stars(json: &str) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct Body {
        stargazers_count: u64,
    }
    serde_json::from_str::<Body>(json).ok().map(|b| b.stargazers_count)
}

/// 12 → «12», 1234 → «1.2k», 12345 → «12k».
pub fn short_count(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..10_000 => format!("{:.1}k", n as f64 / 1_000.0).replace(".0k", "k"), // i18n-ok: number
        _ => format!("{}k", n / 1_000), // i18n-ok: number
    }
}

/// Last attempt (unix seconds) and its answer; `stars: None` when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cache {
    pub checked: u64,
    pub stars: Option<u64>,
}

impl Cache {
    /// A clock moved backwards counts as stale.
    pub fn fresh(&self, now: u64) -> bool {
        now >= self.checked && now - self.checked < REFRESH_SECS
    }

    pub fn parse(s: &str) -> Option<Cache> {
        #[derive(serde::Deserialize)]
        struct Raw {
            checked: u64,
            stars: Option<u64>,
        }
        serde_json::from_str::<Raw>(s).ok().map(|r| Cache { checked: r.checked, stars: r.stars })
    }

    pub fn to_json(self) -> String {
        match self.stars {
            Some(n) => format!("{{\"checked\": {}, \"stars\": {n}}}\n", self.checked),
            None => format!("{{\"checked\": {}, \"stars\": null}}\n", self.checked),
        }
    }
}

fn cache_file() -> Option<PathBuf> {
    crate::settings::cache_dir().map(|d| d.join("github-stars.json"))
}

fn load_cache() -> Option<Cache> {
    Cache::parse(&std::fs::read_to_string(cache_file()?).ok()?)
}

fn save_cache(c: Cache) {
    let Some(f) = cache_file() else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, c.to_json());
}

/// Blocking: run it on the background executor.
fn fetch() -> Option<u64> {
    let mut resp = net::agent().get(&api_url()).header("User-Agent", &user_agent()).header("Accept", "application/vnd.github+json").call().ok()?;
    parse_stars(&resp.body_mut().with_config().limit(1 << 20).read_to_string().ok()?)
}

/// Count shown next to the star, if known.
#[derive(Default)]
struct Stars(Option<u64>);

impl Global for Stars {}

pub fn count(cx: &App) -> Option<u64> {
    cx.try_global::<Stars>().and_then(|s| s.0)
}

/// Shows the cached count and refreshes it when a day old. `enabled: false` hides it.
pub fn refresh<V: 'static>(enabled: bool, cx: &mut Context<V>) {
    if !enabled {
        cx.set_global(Stars(None));
        return;
    }
    let now = crate::update::now_secs();
    let cached = load_cache();
    cx.set_global(Stars(cached.and_then(|c| c.stars)));
    if cached.is_some_and(|c| c.fresh(now)) {
        return;
    }
    let task = cx.background_executor().spawn(async move {
        let stars = fetch();
        save_cache(Cache { checked: now, stars });
        stars
    });
    cx.spawn(async move |this, cx| {
        let stars = task.await;
        this.update(cx, |_, cx| {
            cx.set_global(Stars(stars));
            cx.notify();
        })
        .ok();
    })
    .detach();
}

fn open_repo(_: &ClickEvent, _: &mut Window, cx: &mut App) {
    cx.open_url(&repo_url());
}

fn star_count(color: Hsla, size: f32, n: Option<u64>) -> Div {
    div().flex().items_center().gap(px(3.)).child(crate::ui::icon(IconName::Star, size, color)).when_some(n, |d, n| d.child(short_count(n)))
}

/// Small footer link: GitHub mark, «Star on GitHub», ★ count.
pub fn footer_link(t: &Tokens, cx: &App) -> Stateful<Div> {
    let (muted, hover) = (t.ink_muted, t.ink);
    div()
        .id("star-footer")
        .flex()
        .items_center()
        .gap(px(5.))
        .text_size(px(11.))
        .text_color(muted)
        .cursor_pointer()
        .hover(move |s| s.text_color(hover))
        .child(crate::ui::icon(IconName::Github, 12., muted))
        .child(i18n::t!("app.star.label"))
        .child(star_count(muted, 11., count(cx)))
        .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new(i18n::t!("app.star.tooltip")).build(w, cx))
        .on_click(open_repo)
}

/// About dialog button: outlined, GitHub mark, «Star on GitHub», ★ count.
pub fn about_button(t: &Tokens, cx: &App) -> Stateful<Div> {
    let hover = t.border_strong;
    div()
        .id("about-star")
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(30.))
        .px(px(12.))
        .rounded(px(8.))
        .border_1()
        .border_color(t.border)
        .bg(t.surface)
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t.ink)
        .cursor_pointer()
        .hover(move |s| s.border_color(hover))
        .child(crate::ui::icon(IconName::Github, 14., t.ink))
        .child(i18n::t!("app.star.label"))
        .child(div().pl(px(6.)).border_l_1().border_color(t.border).text_color(t.ink_muted).child(star_count(t.warning, 13., count(cx))))
        .on_click(open_repo)
}

#[cfg(test)]
mod tests {
    use super::{Cache, REFRESH_SECS, api_url, parse_stars, repo_url, short_count, user_agent};

    #[test]
    fn urls() {
        assert_eq!(repo_url(), "https://github.com/otherwisetech/genealogy-rs");
        assert_eq!(api_url(), "https://api.github.com/repos/otherwisetech/genealogy-rs");
        assert_eq!(user_agent(), format!("genealogy-rs/{} (+https://otherwisetech.com)", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn stars_body() {
        assert_eq!(parse_stars(r#"{"id":1,"full_name":"otherwisetech/genealogy-rs","stargazers_count":12,"watchers_count":12}"#), Some(12));
        assert_eq!(parse_stars(r#"{"stargazers_count":0}"#), Some(0));
        assert_eq!(parse_stars(r#"{"message":"API rate limit exceeded"}"#), None);
        assert_eq!(parse_stars(r#"{"stargazers_count":-1}"#), None);
        assert_eq!(parse_stars("<html>"), None);
    }

    #[test]
    fn freshness() {
        let c = Cache { checked: 1_000, stars: Some(3) };
        assert!(c.fresh(1_000));
        assert!(c.fresh(1_000 + REFRESH_SECS - 1));
        assert!(!c.fresh(1_000 + REFRESH_SECS));
        assert!(!c.fresh(999));
    }

    #[test]
    fn cache_roundtrip() {
        for c in [Cache { checked: 1_790_000_000, stars: Some(42) }, Cache { checked: 5, stars: None }] {
            assert_eq!(Cache::parse(&c.to_json()), Some(c));
        }
        assert_eq!(Cache::parse("{}"), None);
        assert_eq!(Cache::parse("garbage"), None);
    }

    #[test]
    fn counts() {
        assert_eq!(short_count(0), "0");
        assert_eq!(short_count(999), "999");
        assert_eq!(short_count(1_000), "1k");
        assert_eq!(short_count(1_234), "1.2k");
        assert_eq!(short_count(12_345), "12k");
    }
}
