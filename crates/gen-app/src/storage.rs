//! Where genealogy-rs keeps its own files, and the bookkeeping around the trees the user opens.
//!
//! The app's settings, sync state, caches and recent-files list live in the Otherwise data
//! folder, `<data root>/genealogy/` (`~/.otherwise/genealogy/` unless the user moved the root in
//! Otherwise Apps). Before 0.5 they were in the platform config folder ([`legacy_dir`]);
//! [`migrate`] moves them over once, never overwriting anything.
//!
//! Family trees stay wherever the user saves them. New trees are offered the last folder used,
//! else `<data root>/genealogy/` — never a temp folder.
use otherwise_link::storage;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Left in the old folder once its files moved (names the new folder).
pub const MARKER: &str = ".migrated-to-otherwise";
pub const RECENT_MAX: usize = 8;

/// `<data root>/genealogy/` (not created here).
pub fn app_dir() -> PathBuf {
    otherwise_link::paths::app_dir(crate::otherwise::APP_ID)
}

/// Map tiles and the like; safe to delete.
pub fn cache_dir() -> PathBuf {
    app_dir().join("cache")
}

/// The settings folder before 0.5.
pub fn legacy_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return std::env::var_os("APPDATA").filter(|v| !v.is_empty()).map(|d| PathBuf::from(d).join("genealogy-rs"));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|h| h.join("Library/Application Support/genealogy-rs"));
    }
    std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(|| home.map(|h| h.join(".config"))).map(|d| d.join("genealogy-rs"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migrated {
    /// No old folder (or it is the new one).
    Nothing,
    AlreadyDone,
    /// Moved these (relative paths); `kept` were already in the new folder and stay behind.
    Done { moved: Vec<String>, kept: Vec<String> },
}

/// Moves every file of `old` into `new` once (rename, or verified copy then delete), leaving
/// [`MARKER`] behind. A file that already exists in `new` is never overwritten: it stays in
/// `old` and is reported in `kept`.
pub fn migrate(old: &Path, new: &Path) -> io::Result<Migrated> {
    if !old.is_dir() || old == new {
        return Ok(Migrated::Nothing);
    }
    if old.join(MARKER).exists() {
        return Ok(Migrated::AlreadyDone);
    }
    let (mut moved, mut kept) = (Vec::new(), Vec::new());
    move_files(old, new, "", &mut moved, &mut kept)?;
    otherwise_link::fsutil::write_atomic(&old.join(MARKER), format!("{}\n", new.display()).as_bytes())?;
    Ok(Migrated::Done { moved, kept })
}

fn move_files(from: &Path, to: &Path, rel: &str, moved: &mut Vec<String>, kept: &mut Vec<String>) -> io::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(from)?.collect::<io::Result<_>>()?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let t = e.file_type()?;
        if t.is_dir() {
            move_files(&e.path(), &to.join(&name), &r, moved, kept)?;
            let _ = std::fs::remove_dir(e.path());
        } else if t.is_file() && name != MARKER && !storage::skipped_in_copy(&name) {
            let dest = to.join(&name);
            if dest.exists() {
                kept.push(r);
            } else {
                storage::move_any(&e.path(), &dest)?;
                moved.push(r);
            }
        }
    }
    Ok(())
}

/// [`migrate`] from the real old folder, once at startup. Skipped when `OTHERWISE_DATA_DIR` is
/// set (tests, portable setups).
pub fn migrate_on_startup() {
    if std::env::var_os(otherwise_link::paths::ENV).is_some_and(|v| !v.is_empty()) {
        return;
    }
    let Some(old) = legacy_dir() else { return };
    match migrate(&old, &app_dir()) {
        Ok(Migrated::Done { kept, .. }) if !kept.is_empty() => eprintln!("genealogy-rs: left in {}: {}", old.display(), kept.join(", ")), // i18n-ok: log only
        Ok(_) => {}
        Err(e) => eprintln!("genealogy-rs: moving {} failed: {e}", old.display()), // i18n-ok: log only
    }
}

/// Temp folders are never offered as the place to save a tree.
pub fn is_temp(p: &Path) -> bool {
    let tmp = std::env::temp_dir();
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (p, tmp) = (canon(p), canon(&tmp));
    p.starts_with(&tmp) || ["/tmp", "/private/tmp", "/var/folders", "/private/var/folders"].iter().any(|t| p.starts_with(t)) // i18n-ok: paths
}

/// The open tree's folder, else the last one used, else `<data root>/genealogy/` (created).
pub fn save_dir(current: Option<&Path>, last: Option<&Path>) -> PathBuf {
    [current.and_then(Path::parent), last].into_iter().flatten().find(|d| d.is_dir() && !is_temp(d)).map(Path::to_path_buf).unwrap_or_else(|| {
        let d = app_dir();
        let _ = std::fs::create_dir_all(&d);
        d
    })
}

/// `recent.json`: the trees opened lately (most recent first) and the last folder used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recent {
    pub files: Vec<String>,
    pub last_dir: Option<String>,
}

impl Recent {
    pub fn file() -> PathBuf {
        app_dir().join("recent.json")
    }

    pub fn load_from(file: &Path) -> Recent {
        std::fs::read_to_string(file).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_to(&self, file: &Path) -> io::Result<()> {
        otherwise_link::fsutil::write_atomic(file, serde_json::to_string_pretty(self).map_err(io::Error::other)?.as_bytes())
    }

    pub fn load() -> Recent {
        Recent::load_from(&Recent::file())
    }

    pub fn update(f: impl FnOnce(&mut Recent)) {
        let mut r = Recent::load();
        f(&mut r);
        if let Err(e) = r.save_to(&Recent::file()) {
            eprintln!("genealogy-rs: recent.json: {e}"); // i18n-ok: log only
        }
    }

    /// Puts `path` first (once) and remembers its folder, unless it is a temp folder.
    pub fn push(&mut self, path: &Path) {
        let p = path.display().to_string();
        self.files.retain(|x| *x != p);
        self.files.insert(0, p);
        self.files.truncate(RECENT_MAX);
        if let Some(d) = path.parent().filter(|d| !d.as_os_str().is_empty() && !is_temp(d)) {
            self.last_dir = Some(d.display().to_string());
        }
    }

    pub fn remove(&mut self, path: &Path) {
        let p = path.display().to_string();
        self.files.retain(|x| *x != p);
    }

    pub fn last_dir(&self) -> Option<PathBuf> {
        self.last_dir.as_deref().map(PathBuf::from)
    }
}

/// «Show in Finder» / «Show in Explorer» / «Show in file manager».
pub fn reveal_label() -> String {
    if cfg!(target_os = "macos") {
        i18n::t!("app.file.reveal_macos")
    } else if cfg!(windows) {
        i18n::t!("app.file.reveal_windows")
    } else {
        i18n::t!("app.file.reveal_linux")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_moves_once_and_never_overwrites() {
        let t = tempfile::tempdir().unwrap();
        let (old, new) = (t.path().join("old"), t.path().join("root").join("genealogy"));
        assert_eq!(migrate(&old, &new).unwrap(), Migrated::Nothing);
        std::fs::create_dir_all(old.join("sub")).unwrap();
        std::fs::write(old.join("settings.json"), b"{\"language\": \"fr\"}").unwrap();
        std::fs::write(old.join("otherwise-sync.json"), b"{}").unwrap();
        std::fs::write(old.join("sub").join("x.json"), b"x").unwrap();
        std::fs::write(old.join("a.lock.json"), b"{}").unwrap();
        // already at the new place: left alone on both sides
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("otherwise-sync.json"), b"{\"newer\": 1}").unwrap();
        let m = migrate(&old, &new).unwrap();
        assert_eq!(m, Migrated::Done { moved: vec!["settings.json".into(), "sub/x.json".into()], kept: vec!["otherwise-sync.json".into()] });
        assert_eq!(std::fs::read(new.join("settings.json")).unwrap(), b"{\"language\": \"fr\"}");
        assert_eq!(std::fs::read(new.join("sub").join("x.json")).unwrap(), b"x");
        assert_eq!(std::fs::read(new.join("otherwise-sync.json")).unwrap(), b"{\"newer\": 1}");
        assert!(old.join("otherwise-sync.json").exists() && !old.join("settings.json").exists());
        assert!(!new.join("a.lock.json").exists());
        assert!(old.join(MARKER).exists());
        assert_eq!(migrate(&old, &new).unwrap(), Migrated::AlreadyDone);
        assert_eq!(migrate(&new, &new).unwrap(), Migrated::Nothing);
    }

    #[test]
    fn recent_list_and_last_folder() {
        let t = tempfile::tempdir().unwrap();
        let file = t.path().join("recent.json");
        let mut r = Recent::load_from(&file);
        assert_eq!(r, Recent::default());
        for i in 0..10 {
            r.push(Path::new(&format!("/Users/ana/Trees/{i}.ged")));
        }
        r.push(Path::new("/Users/ana/Trees/5.ged"));
        assert_eq!(r.files.len(), RECENT_MAX);
        assert_eq!(r.files[0], "/Users/ana/Trees/5.ged");
        assert_eq!(r.files.iter().filter(|x| *x == "/Users/ana/Trees/5.ged").count(), 1);
        assert_eq!(r.last_dir.as_deref(), Some("/Users/ana/Trees"));
        // a file in a temp folder is listed, but its folder isn't remembered
        r.push(&std::env::temp_dir().join("x.ged"));
        assert_eq!(r.last_dir.as_deref(), Some("/Users/ana/Trees"));
        r.remove(Path::new("/Users/ana/Trees/5.ged"));
        assert!(!r.files.iter().any(|x| x == "/Users/ana/Trees/5.ged"));
        r.save_to(&file).unwrap();
        assert_eq!(Recent::load_from(&file), r);
    }

    #[test]
    fn save_folder_is_never_temp() {
        let t = tempfile::tempdir().unwrap();
        assert!(is_temp(t.path()));
        let d = save_dir(Some(&t.path().join("a.ged")), Some(t.path()));
        assert!(!is_temp(&d), "{}", d.display());
        let home = std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_dir() && !is_temp(h));
        if let Some(h) = home {
            assert_eq!(save_dir(None, Some(&h)), h);
        }
    }
}
