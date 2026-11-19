//! Files handed to the app by the OS: argv, and on macOS the "open documents" Apple Event
//! (Finder double-click, Open With, `open file.ged`), which GPUI delivers as `file://` URLs.

use std::path::PathBuf;

/// First argument that names a file. Old macOS launchers pass `-psn_…` process serial numbers.
pub(crate) fn path_from_args(args: impl IntoIterator<Item = String>) -> Option<PathBuf> {
    args.into_iter().find(|a| !a.starts_with("-psn_")).map(PathBuf::from)
}

/// The local path behind a `file://` URL, or `None` for any other URL.
pub(crate) fn path_from_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file:///p` and `file://localhost/p` are local; another host is not ours to open
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let decoded = percent_decode(rest)?;
    #[cfg(windows)]
    let decoded = decoded.strip_prefix('/').filter(|d| d.as_bytes().get(1) == Some(&b':')).unwrap_or(&decoded).to_string();
    Some(PathBuf::from(decoded))
}

/// The paths among `urls` this app can open, in order. A plain path (no scheme) is taken as is.
pub(crate) fn paths_from_urls(urls: &[String]) -> Vec<PathBuf> {
    urls.iter().filter_map(|u| if u.contains("://") { path_from_url(u) } else { Some(PathBuf::from(u)) }).collect()
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn decodes_file_urls() {
        assert_eq!(path_from_url("file:///Users/ana/tree.ged").as_deref(), Some(Path::new("/Users/ana/tree.ged")));
        assert_eq!(path_from_url("file://localhost/tmp/a.ged").as_deref(), Some(Path::new("/tmp/a.ged")));
        // NSURL.absoluteString: spaces and accents arrive percent-encoded (NFD from APFS or NFC)
        assert_eq!(path_from_url("file:///Users/ana/Documents/Arbol%20Geneal%C3%B3gico.ged").as_deref(), Some(Path::new("/Users/ana/Documents/Arbol Genealógico.ged")));
        assert_eq!(path_from_url("file:///tmp/Familia%20Pe%CC%81rez%20%26%20co.GED").as_deref(), Some(Path::new("/tmp/Familia Pe\u{301}rez & co.GED")));
    }

    #[test]
    fn rejects_other_urls() {
        assert_eq!(path_from_url("https://example.com/a.ged"), None);
        assert_eq!(path_from_url("file://server/share/a.ged"), None);
        assert_eq!(path_from_url("file:///bad%zz.ged"), None);
        assert_eq!(path_from_url("file:///bad%ff.ged"), None);
        assert_eq!(paths_from_urls(&["genealogy://x".into(), "file:///a.ged".into(), "/b.ged".into()]), vec![PathBuf::from("/a.ged"), PathBuf::from("/b.ged")]);
    }

    #[test]
    fn args_skip_process_serial_number() {
        let args = |v: &[&str]| path_from_args(v.iter().map(|s| s.to_string()));
        assert_eq!(args(&[]), None);
        assert_eq!(args(&["-psn_0_12345"]), None);
        assert_eq!(args(&["-psn_0_1", "a b.ged"]), Some(PathBuf::from("a b.ged")));
    }
}
