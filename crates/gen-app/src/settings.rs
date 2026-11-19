//! Tiny persisted preferences (UI language, generations shown, map geocoding consent, update
//! check), stored as a flat JSON object.
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub language: Option<String>,
    pub generations: Option<usize>,
    /// the user agreed to send place names to Nominatim (map tab)
    pub geocode_consent: bool,
    /// ask GitHub for the latest release on startup (at most once a day)
    pub update_check: bool,
    /// unix seconds of the last answered update check
    pub last_update_check: Option<u64>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { language: None, generations: None, geocode_consent: false, update_check: true, last_update_check: None }
    }
}

pub(crate) fn dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").filter(|v| !v.is_empty()).map(|d| PathBuf::from(d).join("genealogy-rs"));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Application Support/genealogy-rs"));
    }
    std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.map(|h| h.join(".config"))).map(|d| d.join("genealogy-rs"))
}

/// Per-user cache directory (map tiles); safe to delete.
pub(crate) fn cache_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|d| PathBuf::from(d).join("genealogy-rs"));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Caches/genealogy-rs"));
    }
    std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.map(|h| h.join(".cache"))).map(|d| d.join("genealogy-rs"))
}

fn file() -> Option<PathBuf> {
    dir().map(|d| d.join("settings.json"))
}

impl Settings {
    pub fn load() -> Settings {
        file().and_then(|f| std::fs::read_to_string(f).ok()).map(|s| Settings::parse(&s)).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(f) = file() else { return };
        if let Some(d) = f.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(f, self.to_json());
    }

    fn parse(s: &str) -> Settings {
        Settings {
            language: json_string(s, "language"),
            generations: json_number(s, "generations").and_then(|g| usize::try_from(g).ok()),
            geocode_consent: json_bool(s, "geocode_consent").unwrap_or(false),
            update_check: json_bool(s, "update_check").unwrap_or(true),
            last_update_check: json_number(s, "last_update_check"),
        }
    }

    fn to_json(&self) -> String {
        let mut fields = Vec::new();
        if let Some(l) = &self.language {
            fields.push(format!("  \"language\": \"{}\"", l.replace(['"', '\\'], "")));
        }
        if let Some(g) = self.generations {
            fields.push(format!("  \"generations\": {g}"));
        }
        if self.geocode_consent {
            fields.push("  \"geocode_consent\": true".into());
        }
        if !self.update_check {
            fields.push("  \"update_check\": false".into());
        }
        if let Some(t) = self.last_update_check {
            fields.push(format!("  \"last_update_check\": {t}"));
        }
        if fields.is_empty() { "{}\n".into() } else { format!("{{\n{}\n}}\n", fields.join(",\n")) }
    }
}

/// Value of a top-level string field; enough for our own flat file.
fn json_string(s: &str, key: &str) -> Option<String> {
    let at = s.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = s[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    Some(rest[..rest.find('"')?].to_string())
}

/// Value of a top-level unsigned integer field.
fn json_number(s: &str, key: &str) -> Option<u64> {
    let at = s.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = s[at..].trim_start().strip_prefix(':')?.trim_start();
    rest[..rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len())].parse().ok()
}

fn json_bool(s: &str, key: &str) -> Option<bool> {
    let at = s.find(&format!("\"{key}\""))? + key.len() + 2;
    let rest = s[at..].trim_start().strip_prefix(':')?.trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

/// System UI language tag ("es-CO", "en_US.UTF-8"…), best effort.
pub fn system_locale() -> Option<String> {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() && v != "C" && v != "POSIX" {
                return Some(v);
            }
        }
    }
    if cfg!(target_os = "macos") {
        let out = std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok()?;
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Saved language, else the system one if bundled, else the default.
pub fn initial_language(settings: &Settings) -> &'static str {
    let saved = settings.language.as_deref().and_then(|l| i18n::available().into_iter().map(|(c, _)| c).find(|c| *c == l));
    saved.or_else(|| system_locale().as_deref().and_then(i18n::match_system)).unwrap_or(i18n::DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn roundtrip() {
        let s = Settings { language: Some("en".into()), ..Settings::default() };
        assert_eq!(Settings::parse(&s.to_json()), s);
        let s = Settings { language: Some("fr".into()), generations: Some(12), geocode_consent: true, update_check: false, last_update_check: Some(1_790_000_000) };
        assert_eq!(Settings::parse(&s.to_json()), s);
        let s = Settings { generations: Some(6), last_update_check: Some(42), ..Settings::default() };
        assert_eq!(Settings::parse(&s.to_json()), s);
        assert!(Settings::parse("{}").update_check);
        assert!(!Settings::parse("{ \"update_check\": false }").update_check);
        assert_eq!(Settings::parse("{ \"generations\": \"x\" }").generations, None);
        assert_eq!(Settings::parse("{}"), Settings::default());
        assert!(Settings::parse("{ \"geocode_consent\" : true }").geocode_consent);
        assert!(!Settings::parse("{ \"geocode_consent\": \"yes\" }").geocode_consent);
        assert_eq!(Settings::parse("{ \"language\" : \"es\", \"x\": 1 }").language.as_deref(), Some("es"));
    }
}
