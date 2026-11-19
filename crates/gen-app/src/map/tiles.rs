//! OpenStreetMap raster tiles, cached on disk under the user cache dir. Only tiles on screen are
//! requested, at most `MAX_INFLIGHT` at a time (OSM tile usage policy); the cache is capped by
//! deleting the oldest files.
use super::geo::TileKey;
use super::net::{self, NetError};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const MAX_INFLIGHT: usize = 2;
pub const CACHE_CAP: u64 = 200 * 1024 * 1024;
/// pruning goes down to this, so it doesn't run again after every download
const CACHE_LOW: u64 = 160 * 1024 * 1024;
/// older tiles are shown but fetched again
pub const MAX_AGE: Duration = Duration::from_secs(30 * 24 * 3600);

pub fn dir() -> Option<PathBuf> {
    crate::settings::cache_dir().map(|d| d.join("tiles"))
}

pub fn path(root: &Path, k: TileKey) -> PathBuf {
    root.join(k.z.to_string()).join(k.x.to_string()).join(format!("{}.png", k.y))
}

pub fn url(k: TileKey) -> String {
    format!("https://tile.openstreetmap.org/{}/{}/{}.png", k.z, k.x, k.y)
}

/// Download one tile to its cache file (blocking).
pub fn fetch(root: &Path, k: TileKey) -> Result<PathBuf, NetError> {
    let bytes = net::get_bytes(&url(k), 2 << 20)?;
    if !bytes.starts_with(b"\x89PNG") {
        return Err(NetError::Transport("not a png".into())); // i18n-ok: log only
    }
    let p = path(root, k);
    let io = |e: std::io::Error| NetError::Transport(e.to_string());
    std::fs::create_dir_all(p.parent().unwrap_or(root)).map_err(io)?;
    let tmp = p.with_extension("png.part");
    std::fs::write(&tmp, &bytes).map_err(io)?;
    std::fs::rename(&tmp, &p).map_err(io)?;
    Ok(p)
}

/// Is the cached file there, and is it fresh?
pub fn cached(p: &Path) -> Option<bool> {
    let m = std::fs::metadata(p).ok()?;
    let age = m.modified().ok().and_then(|t| SystemTime::now().duration_since(t).ok()).unwrap_or_default();
    Some(age < MAX_AGE)
}

/// Delete the oldest tiles while the cache is over `cap` (down to `low`). Returns bytes freed.
pub fn prune(root: &Path, cap: u64, low: u64) -> u64 {
    let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                stack.push(e.path());
            } else {
                files.push((m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), e.path()));
            }
        }
    }
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    if total <= cap {
        return 0;
    }
    files.sort();
    let mut freed = 0;
    for (_, len, p) in files {
        if total <= low {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= len;
            freed += len;
        }
    }
    freed
}

pub fn prune_default(root: &Path) -> u64 {
    prune(root, CACHE_CAP, CACHE_LOW)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_urls() {
        let k = TileKey { z: 3, x: 5, y: 2 };
        assert_eq!(url(k), "https://tile.openstreetmap.org/3/5/2.png");
        assert_eq!(path(Path::new("/c"), k), PathBuf::from("/c/3/5/2.png"));
    }

    #[test]
    fn prune_drops_oldest_first() {
        let root = std::env::temp_dir().join(format!("gen-tiles-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (i, y) in [0u32, 1, 2, 3].iter().enumerate() {
            let p = path(&root, TileKey { z: 1, x: 0, y: *y });
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, vec![0u8; 100]).unwrap();
            let t = SystemTime::now() - Duration::from_secs(1000 - i as u64 * 100);
            std::fs::File::options().write(true).open(&p).unwrap().set_modified(t).unwrap();
        }
        assert_eq!(prune(&root, 1000, 500), 0, "under the cap");
        assert_eq!(prune(&root, 300, 200), 200);
        assert!(!path(&root, TileKey { z: 1, x: 0, y: 0 }).exists() && !path(&root, TileKey { z: 1, x: 0, y: 1 }).exists());
        assert_eq!(cached(&path(&root, TileKey { z: 1, x: 0, y: 3 })), Some(true));
        assert_eq!(cached(&path(&root, TileKey { z: 1, x: 0, y: 0 })), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
