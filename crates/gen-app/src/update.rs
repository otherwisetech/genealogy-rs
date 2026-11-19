//! Startup check for a newer GitHub release (opt-out in the globe menu, at most once a day). Only
//! a GET to the public releases API is sent, with our usual `genealogy-rs/<version>` User-Agent.
use crate::map::net::{self, NetError};

const LATEST_URL: &str = "https://api.github.com/repos/otherwisetech/genealogy-rs/releases/latest";
const RELEASES_PREFIX: &str = "https://github.com/otherwisetech/genealogy-rs/releases/";
pub const CHECK_EVERY_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// as published, e.g. `v0.2.0`
    pub tag: String,
    pub url: String,
}

/// `v1.2.3`, `1.2`, `v1.2.3-rc.1` → (1, 2, 3); pre-release/build suffixes are ignored.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix(['v', 'V']).unwrap_or(s);
    let core = s.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    parts.next().is_none().then_some((major, minor, patch))
}

pub fn is_newer(tag: &str, current: &str) -> bool {
    matches!((parse_version(tag), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// The `releases/latest` body; drafts, pre-releases and links outside our repo are ignored.
pub fn parse_latest(json: &str) -> Option<Release> {
    #[derive(serde::Deserialize)]
    struct Body {
        tag_name: String,
        html_url: String,
        #[serde(default)]
        draft: bool,
        #[serde(default)]
        prerelease: bool,
    }
    let b: Body = serde_json::from_str(json).ok()?;
    (!b.draft && !b.prerelease && b.html_url.starts_with(RELEASES_PREFIX) && parse_version(&b.tag_name).is_some()).then_some(Release { tag: b.tag_name, url: b.html_url })
}

/// Whether a check is due, given the last answered one (a clock moved backwards counts as due).
pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|l| now < l || now - l >= CHECK_EVERY_SECS)
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Blocking: run it on the background executor. `Ok(None)` means up to date (or nothing usable).
pub fn fetch_newer() -> Result<Option<Release>, NetError> {
    let mut resp = net::agent().get(LATEST_URL).header("Accept", "application/vnd.github+json").call()?;
    let body = resp.body_mut().with_config().limit(1 << 20).read_to_string()?;
    Ok(parse_latest(&body).filter(|r| is_newer(&r.tag, env!("CARGO_PKG_VERSION"))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("v2.0.0-rc.1+build"), Some((2, 0, 0)));
        assert_eq!(parse_version("nightly"), None);
        assert_eq!(parse_version("v1.2.3.4"), None);
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("v1.0.0", "0.99.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }

    #[test]
    fn latest_body() {
        let ok = r#"{"tag_name":"v0.3.1","html_url":"https://github.com/otherwisetech/genealogy-rs/releases/tag/v0.3.1","draft":false,"prerelease":false,"assets":[]}"#;
        assert_eq!(parse_latest(ok), Some(Release { tag: "v0.3.1".into(), url: "https://github.com/otherwisetech/genealogy-rs/releases/tag/v0.3.1".into() }));
        assert_eq!(parse_latest(&ok.replace("\"prerelease\":false", "\"prerelease\":true")), None);
        assert_eq!(parse_latest(&ok.replace("https://github.com/otherwisetech", "https://evil.example")), None);
        assert_eq!(parse_latest(r#"{"message":"Not Found"}"#), None);
        assert_eq!(parse_latest("not json"), None);
    }

    #[test]
    fn daily() {
        assert!(due(None, 1_000));
        assert!(!due(Some(1_000), 1_000 + CHECK_EVERY_SECS - 1));
        assert!(due(Some(1_000), 1_000 + CHECK_EVERY_SECS));
        assert!(due(Some(5_000), 1_000));
    }
}
