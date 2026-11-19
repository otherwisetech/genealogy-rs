//! Place-name geocoding with Nominatim, opt-in (see the map consent banner). Usage policy:
//! one request at a time, at most one per second, identifying User-Agent, and every answer —
//! «not found» included — cached forever so a place is never asked twice. Only place names are
//! sent. Coordinates are never written into the GEDCOM.
use super::net::{self, NetError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ENDPOINT: &str = "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=1&q=";
pub const MIN_INTERVAL: Duration = Duration::from_millis(1100);

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
struct CacheFile {
    version: u32,
    /// place_key → [lat, lon], or null when Nominatim had no match
    places: HashMap<String, Option<[f64; 2]>>,
}

#[derive(Debug, Default)]
pub struct GeoCache {
    path: Option<PathBuf>,
    places: HashMap<String, Option<[f64; 2]>>,
}

impl GeoCache {
    pub fn default_path() -> Option<PathBuf> {
        crate::settings::dir().map(|d| d.join("geocode-cache.json"))
    }

    pub fn load(path: Option<PathBuf>) -> GeoCache {
        let places = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str::<CacheFile>(&s).ok()).map(|f| f.places).unwrap_or_default();
        GeoCache { path, places }
    }

    /// `None` = never asked; `Some(None)` = asked, not found.
    pub fn get(&self, key: &str) -> Option<Option<(f64, f64)>> {
        self.places.get(key).map(|v| v.map(|[a, b]| (a, b)))
    }

    pub fn insert(&mut self, key: String, v: Option<(f64, f64)>) {
        self.places.insert(key, v.map(|(a, b)| [a, b]));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.places.len()
    }

    pub fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let file = CacheFile { version: 1, places: self.places.clone() };
        if let Ok(s) = serde_json::to_string(&file) {
            let tmp = p.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, p);
            }
        }
    }
}

pub fn query_url(place: &str) -> String {
    format!("{ENDPOINT}{}", net::encode(place))
}

#[derive(Deserialize)]
struct Hit {
    lat: String,
    lon: String,
}

/// First hit of a `format=jsonv2` answer; `Err` when the body isn't one.
pub fn parse_response(body: &str) -> Result<Option<(f64, f64)>, ()> {
    let hits: Vec<Hit> = serde_json::from_str(body).map_err(|_| ())?;
    Ok(hits.first().and_then(|h| Some((h.lat.trim().parse().ok()?, h.lon.trim().parse().ok()?))))
}

/// Blocking lookup; serialized and spaced by `MIN_INTERVAL` process-wide.
pub fn lookup(place: &str, language: &str) -> Result<Option<(f64, f64)>, NetError> {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(at) = *last {
        let wait = MIN_INTERVAL.saturating_sub(at.elapsed());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
    let res = net::get_text(&query_url(place), language);
    *last = Some(Instant::now());
    parse_response(&res?).map_err(|_| NetError::Transport("bad response".into())) // i18n-ok: log only
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nominatim() {
        let body = r#"[{"place_id":1,"lat":"10.25","lon":"-20.5","display_name":"Villa Uno"}]"#;
        assert_eq!(parse_response(body), Ok(Some((10.25, -20.5))));
        assert_eq!(parse_response("[]"), Ok(None));
        assert_eq!(parse_response(r#"[{"lat":"x","lon":"1"}]"#), Ok(None));
        assert!(parse_response("<html>").is_err());
        assert!(query_url("Villa Uno, Comarca").ends_with("&limit=1&q=Villa%20Uno%2C%20Comarca"));
    }

    #[test]
    fn cache_round_trip_keeps_not_found() {
        let dir = std::env::temp_dir().join(format!("gen-geocode-test-{}", std::process::id()));
        let path = dir.join("cache.json");
        let mut c = GeoCache::load(Some(path.clone()));
        assert_eq!(c.get("villa uno"), None);
        c.insert("villa uno".into(), Some((1.5, -2.)));
        c.insert("nowhere".into(), None);
        c.save();
        let c = GeoCache::load(Some(path));
        assert_eq!(c.get("villa uno"), Some(Some((1.5, -2.))));
        assert_eq!(c.get("nowhere"), Some(None), "a miss is remembered too");
        assert_eq!(c.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }
}
