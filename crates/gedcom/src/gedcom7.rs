//! Export as GEDCOM 7.0. The open tree stays 5.5.1; `Tree::to_gedcom7()` converts a copy.
//!
//! Converted:
//! - HEAD: `1 GEDC / 2 VERS 7.0` first; `CHAR`, `GEDC.FORM` and `HEAD.FILE` dropped.
//! - Lines: UTF-8, LF, no length limit, so no CONC (folded into the value); newlines stay CONT.
//!   Stray CONC/CONT lines the reader could not fold are folded here.
//! - `@`: 5.5.1 `@@` escapes are undone; per 7.0 only a line value that starts with `@` is
//!   written with a leading `@@`.
//! - Xrefs: upper-cased and limited to `A–Z 0–9 _` (kept unique); a pointer to a record that
//!   doesn't exist becomes `@VOID@`.
//! - DATE: upper-cased; `@#DJULIAN@` → `JULIAN` (also GREGORIAN, HEBREW, FRENCH_R), `B.C.` →
//!   `BCE`. `INT date (text)` → `DATE date` + `PHRASE text`; dual years `1648/9` → the later
//!   year + `PHRASE` with the original; anything else that isn't a 7.0 date (free text,
//!   `(text)`) → empty `DATE` + `PHRASE`.
//! - `NOTE @N@` → `SNOTE @N@` and `0 @N@ NOTE` records → `SNOTE`; inline notes stay NOTE.
//! - Inline `SOUR text` citations → a SOUR record (`TITL text`, the citation's TEXT lines move
//!   to it), identical texts share one record; the citation keeps its other sub-lines.
//!   Inline `OBJE` links → an OBJE record pointed to.
//! - `FILE.FORM jpg` → media type (`image/jpeg`…), `FORM.TYPE` → `MEDI`.
//! - Enumerations upper-cased (PEDI, MEDI, NAME.TYPE, ROLE, RESN, ADOP); a value outside the 7.0
//!   set becomes `OTHER` + `PHRASE`. `ASSO.RELA x` → `ROLE OTHER` + `PHRASE x` (or the enum).
//! - `AGE CHILD/INFANT/STILLBORN` → `< 8y` / `< 1y` / `0y` + `PHRASE`.
//! - `LANG English` → `LANG en` for common language names (unknown ones → `_LANG`).
//! - Removed tags: `AFN`/`RFN`/`RIN` → `EXID` + `TYPE https://gedcom.io/terms/v7/…`;
//!   `EMAI` → `EMAIL`; `SUBN`, `FONE`, `ROMN`, `BLOB` → `_SUBN`, `_FONE`, `_ROMN`, `_BLOB`.
//! - Everything else, vendor `_TAG`s (`_MARNM`, `_UID`, …) included, is written as it is:
//!   7.0 allows extension tags.
use crate::node::Node;
use crate::tree::Tree;
use std::collections::{HashMap, HashSet};

impl Tree {
    /// The tree as a GEDCOM 7.0 file (see the module docs).
    pub fn to_gedcom7(&self) -> String {
        let mut recs: Vec<Node> = self.records.clone();
        for r in &mut recs {
            fold_stray(r);
        }
        let map = xref_map(&recs);
        for r in &mut recs {
            if let Some(x) = &r.xref {
                r.xref = Some(map[x].clone());
            }
            pointers(r, &map);
        }
        let mut cx = Conv { used: map.values().cloned().collect(), new_records: vec![], sources: HashMap::new() };
        for r in &mut recs {
            if r.tag == "HEAD" {
                convert_head(r);
            } else {
                if r.tag == "NOTE" {
                    r.tag = "SNOTE".into();
                }
                if r.tag == "SUBN" {
                    r.tag = "_SUBN".into();
                }
                convert_children(r, &mut cx);
            }
        }
        if !recs.first().is_some_and(|r| r.tag == "HEAD") {
            let mut head = Node::new("HEAD", "");
            convert_head(&mut head);
            recs.insert(0, head);
        }
        let at = recs.iter().rposition(|r| r.tag == "TRLR").unwrap_or(recs.len());
        recs.splice(at..at, cx.new_records);
        if !recs.last().is_some_and(|r| r.tag == "TRLR") {
            recs.push(Node::new("TRLR", ""));
        }
        let mut out = String::new();
        for r in &recs {
            write7(&mut out, r, 0);
        }
        out
    }
}

/// CONC/CONT nodes that weren't folded on read (they came after other sub-lines).
fn fold_stray(n: &mut Node) {
    let mut i = 0;
    while i < n.children.len() {
        match n.children[i].tag.as_str() {
            "CONC" | "CONT" => {
                let c = n.children.remove(i);
                if c.tag == "CONT" {
                    n.value.push('\n');
                }
                n.value.push_str(&c.value);
            }
            _ => {
                fold_stray(&mut n.children[i]);
                i += 1;
            }
        }
    }
}

fn xref_map(recs: &[Node]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut used: HashSet<String> = HashSet::new();
    for x in recs.iter().filter_map(|r| r.xref.as_ref()) {
        if map.contains_key(x) {
            continue;
        }
        let mut s: String = x.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c.to_ascii_uppercase() } else { '_' }).collect();
        if s.is_empty() || s == "VOID" || used.contains(&s) {
            let base = if s.is_empty() { "X".to_string() } else { s.clone() };
            let mut n = 1;
            while used.contains(&format!("{base}_{n}")) {
                n += 1;
            }
            s = format!("{base}_{n}");
        }
        used.insert(s.clone());
        map.insert(x.clone(), s);
    }
    map
}

/// Rewrite pointers (unknown → VOID) and undo 5.5.1 `@@` escapes in text values.
fn pointers(n: &mut Node, map: &HashMap<String, String>) {
    match n.pointer() {
        Some(p) => n.value = format!("@{}@", map.get(p).map(String::as_str).unwrap_or("VOID")),
        None => {
            if n.value.contains("@@") {
                n.value = n.value.replace("@@", "@");
            }
        }
    }
    for c in &mut n.children {
        pointers(c, map);
    }
}

fn convert_head(h: &mut Node) {
    h.children.retain(|c| !matches!(c.tag.as_str(), "GEDC" | "CHAR" | "FILE"));
    for c in &mut h.children {
        if c.tag == "SUBN" {
            c.tag = "_SUBN".into();
        }
        convert_leaf(c, "HEAD");
    }
    let gedc = Node { tag: "GEDC".into(), children: vec![Node::new("VERS", "7.0")], ..Default::default() };
    h.children.insert(0, gedc);
}

struct Conv {
    used: HashSet<String>,
    new_records: Vec<Node>,
    /// inline source text → its new SOUR record id
    sources: HashMap<String, String>,
}

impl Conv {
    fn fresh(&mut self, prefix: &str) -> String {
        let mut n = 1;
        while self.used.contains(&format!("{prefix}{n}")) {
            n += 1;
        }
        let id = format!("{prefix}{n}");
        self.used.insert(id.clone());
        id
    }
}

fn convert_children(n: &mut Node, cx: &mut Conv) {
    let parent = n.tag.clone();
    for c in &mut n.children {
        convert(c, &parent, cx);
    }
}

fn convert(n: &mut Node, parent: &str, cx: &mut Conv) {
    if n.tag == "SOUR" && n.pointer().is_none() && n.value.trim() != "@VOID@" {
        inline_source(n, cx);
    } else if n.tag == "OBJE" && n.pointer().is_none() && n.value.trim() != "@VOID@" && !n.children.is_empty() {
        let id = cx.fresh("O");
        let mut rec = Node::new("OBJE", "").with_xref(id.clone());
        rec.children = std::mem::take(&mut n.children);
        // 5.5 links put FORM/TITL next to FILE
        if let Some(fi) = rec.children.iter().position(|c| c.tag == "FILE") {
            let loose: Vec<Node> = rec.children.iter().filter(|c| c.tag == "FORM" || c.tag == "TITL").cloned().collect();
            rec.children.retain(|c| c.tag != "FORM" && c.tag != "TITL");
            let fi = fi.min(rec.children.len().saturating_sub(1));
            if let Some(file) = rec.children.get_mut(fi).filter(|c| c.tag == "FILE") {
                for l in loose {
                    if file.child(&l.tag).is_none() {
                        file.children.push(l);
                    }
                }
            }
        }
        convert_children(&mut rec, cx);
        cx.new_records.push(rec);
        n.value = format!("@{id}@");
        return;
    }
    convert_leaf(n, parent);
    convert_children(n, cx);
}

fn inline_source(n: &mut Node, cx: &mut Conv) {
    let text = n.value.trim().to_string();
    let texts: Vec<Node> = n.children.iter().filter(|c| c.tag == "TEXT").cloned().collect();
    n.children.retain(|c| c.tag != "TEXT");
    let key = format!("{text}\u{0}{}", texts.iter().map(|t| t.value.as_str()).collect::<Vec<_>>().join("\u{0}"));
    let id = match cx.sources.get(&key) {
        Some(id) => id.clone(),
        None => {
            let id = cx.fresh("S");
            let mut rec = Node::new("SOUR", "").with_xref(id.clone());
            if !text.is_empty() {
                rec.children.push(Node::new("TITL", text));
            }
            rec.children.extend(texts);
            cx.sources.insert(key, id.clone());
            cx.new_records.push(rec);
            id
        }
    };
    n.value = format!("@{id}@");
    convert_children(n, cx);
}

const EXID_TAGS: &[&str] = &["AFN", "RFN", "RIN"];

/// Tag/value rewrites that only look at the line itself and its parent's tag.
fn convert_leaf(n: &mut Node, parent: &str) {
    match n.tag.as_str() {
        "NOTE" if n.pointer().is_some() || n.value.trim() == "@VOID@" => n.tag = "SNOTE".into(),
        "EMAI" => n.tag = "EMAIL".into(),
        "SUBN" | "FONE" | "ROMN" | "BLOB" => n.tag = format!("_{}", n.tag),
        t if EXID_TAGS.contains(&t) => {
            n.children.insert(0, Node::new("TYPE", format!("https://gedcom.io/terms/v7/{t}")));
            n.tag = "EXID".into();
        }
        "DATE" => {
            let (value, phrase) = date7(&n.value);
            n.value = value;
            if let Some(p) = phrase {
                n.children.retain(|c| c.tag != "PHRASE");
                n.children.insert(0, Node::new("PHRASE", p));
            }
        }
        "LANG" => match lang_tag(&n.value) {
            Some(l) => n.value = l,
            None => n.tag = "_LANG".into(),
        },
        "FORM" if parent == "FILE" => {
            n.value = media_type(&n.value);
            for c in &mut n.children {
                if c.tag == "TYPE" {
                    c.tag = "MEDI".into();
                }
            }
        }
        "PEDI" => enumerate(n, &["ADOPTED", "BIRTH", "FOSTER", "SEALING", "OTHER"]),
        "MEDI" => enumerate(n, &["AUDIO", "BOOK", "CARD", "ELECTRONIC", "FICHE", "FILM", "MAGAZINE", "MANUSCRIPT", "MAP", "NEWSPAPER", "PHOTO", "TOMBSTONE", "VIDEO", "OTHER"]),
        "TYPE" if parent == "NAME" => enumerate(n, &["AKA", "BIRTH", "IMMIGRANT", "MAIDEN", "MARRIED", "PROFESSIONAL", "OTHER"]),
        "TYPE" if parent == "FORM" => {
            n.tag = "MEDI".into();
            enumerate(n, &["AUDIO", "BOOK", "CARD", "ELECTRONIC", "FICHE", "FILM", "MAGAZINE", "MANUSCRIPT", "MAP", "NEWSPAPER", "PHOTO", "TOMBSTONE", "VIDEO", "OTHER"]);
        }
        "ROLE" | "RELA" if parent == "ASSO" || n.tag == "ROLE" => {
            n.tag = "ROLE".into();
            enumerate(n, &["CHIL", "CLERGY", "FATH", "FRIEND", "GODP", "HUSB", "MOTH", "MULTIPLE", "NGHBR", "OFFICIATOR", "PARENT", "SPOU", "WIFE", "WITN", "OTHER"]);
        }
        "ADOP" if parent == "FAMC" => enumerate(n, &["HUSB", "WIFE", "BOTH"]),
        "RESN" => n.value = n.value.split(',').map(|v| v.trim().to_ascii_uppercase()).filter(|v| !v.is_empty()).collect::<Vec<_>>().join(", "),
        "AGE" => {
            let age = match n.value.trim().to_ascii_uppercase().as_str() {
                "CHILD" => Some("< 8y"),
                "INFANT" => Some("< 1y"),
                "STILLBORN" => Some("0y"),
                _ => None,
            };
            if let Some(a) = age {
                let phrase = std::mem::replace(&mut n.value, a.into());
                n.children.insert(0, Node::new("PHRASE", phrase.trim()));
            }
        }
        _ => {}
    }
}

/// Upper-case an enumeration value; one outside `allowed` becomes OTHER + PHRASE.
fn enumerate(n: &mut Node, allowed: &[&str]) {
    let v = n.value.trim();
    if v.is_empty() {
        return;
    }
    let up = v.to_ascii_uppercase();
    if allowed.contains(&up.as_str()) {
        n.value = up;
    } else if allowed.contains(&"OTHER") {
        // 5.5.1 wraps free-text roles in parentheses: «(padrino)»
        let phrase = v.strip_prefix('(').and_then(|p| p.strip_suffix(')')).unwrap_or(v).trim().to_string();
        n.value = "OTHER".into();
        n.children.insert(0, Node::new("PHRASE", phrase));
    }
}

fn media_type(form: &str) -> String {
    let f = form.trim().trim_start_matches('.').to_ascii_lowercase();
    let mt = match f.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "htm" | "html" => "text/html",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        _ => return form.trim().to_string(),
    };
    mt.into()
}

fn lang_tag(v: &str) -> Option<String> {
    let v = v.trim();
    let tag = match v.to_ascii_lowercase().as_str() {
        "english" => "en",
        "spanish" => "es",
        "french" => "fr",
        "portuguese" => "pt",
        "german" => "de",
        "italian" => "it",
        "dutch" => "nl",
        "catalan" => "ca",
        "polish" => "pl",
        "swedish" => "sv",
        "danish" => "da",
        "norwegian" => "no",
        "finnish" => "fi",
        "czech" => "cs",
        "hungarian" => "hu",
        "russian" => "ru",
        "greek" => "el",
        "latin" => "la",
        "hebrew" => "he",
        "japanese" => "ja",
        "chinese" => "zh",
        _ => {
            // already a BCP 47 tag ("es", "pt-BR")
            let ok = !v.is_empty() && v.split('-').all(|p| (1..=8).contains(&p.len()) && p.chars().all(|c| c.is_ascii_alphanumeric())) && v.split('-').next().is_some_and(|p| p.len() <= 3 && p.chars().all(|c| c.is_ascii_alphabetic()));
            return ok.then(|| v.to_string());
        }
    };
    Some(tag.into())
}

const CALENDARS: &[(&str, &str)] = &[("@#DGREGORIAN@", "GREGORIAN"), ("@#DJULIAN@", "JULIAN"), ("@#DHEBREW@", "HEBREW"), ("@#DFRENCH R@", "FRENCH_R")];
const MONTHS: &[&str] = &["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];
const FRENCH_MONTHS: &[&str] = &["VEND", "BRUM", "FRIM", "NIVO", "PLUV", "VENT", "GERM", "FLOR", "PRAI", "MESS", "THER", "FRUC", "COMP"];
const HEBREW_MONTHS: &[&str] = &["TSH", "CSH", "KSL", "TVT", "SHV", "ADR", "ADS", "NSN", "IYR", "SVN", "TMZ", "AAV", "ELL"];

/// 5.5.1 date value → (7.0 date value, PHRASE).
pub(crate) fn date7(raw: &str) -> (String, Option<String>) {
    let s = raw.trim();
    if s.is_empty() {
        return (String::new(), None);
    }
    let phrase_only = || {
        let p = s.strip_prefix('(').and_then(|p| p.strip_suffix(')')).unwrap_or(s).trim();
        (String::new(), Some(p.to_string()))
    };
    let mut up = s.to_uppercase();
    for (from, to) in CALENDARS {
        up = up.replace(from, to);
    }
    let mut up = up.split_whitespace().collect::<Vec<_>>().join(" ");
    for bc in [" B.C.", " BC", " B.C"] {
        if let Some(x) = up.strip_suffix(bc) {
            up = format!("{x} BCE");
        }
    }
    let up = up.replace(" B.C. ", " BCE ").replace(" BC ", " BCE ");
    if let Some(rest) = up.strip_prefix("INT ") {
        if let Some(open) = rest.find('(') {
            let date = rest[..open].trim();
            let orig_open = s.find('(').unwrap_or(0);
            let phrase = s[orig_open..].trim().trim_start_matches('(').trim_end_matches(')').trim();
            if valid_date(date) {
                return (date.to_string(), Some(phrase.to_string()));
            }
        }
        return phrase_only();
    }
    if valid_date(&up) {
        return (up, None);
    }
    if let Some(fixed) = fix_dual_years(&up)
        && valid_date(&fixed)
    {
        return (fixed, Some(s.to_string()));
    }
    phrase_only()
}

/// `1648/9` → `1649`, `1699/00` → `1700`; None when there is no dual year.
fn fix_dual_years(s: &str) -> Option<String> {
    let mut changed = false;
    let words: Vec<String> = s
        .split(' ')
        .map(|w| {
            if let Some((a, b)) = w.split_once('/')
                && (3..=4).contains(&a.len())
                && (1..=2).contains(&b.len())
                && a.chars().chain(b.chars()).all(|c| c.is_ascii_digit())
            {
                let y: u32 = a.parse().unwrap_or(0);
                let m = 10u32.pow(b.len() as u32);
                let tail: u32 = b.parse().unwrap_or(0);
                let mut later = y - y % m + tail;
                if later <= y {
                    later += m;
                }
                changed = true;
                later.to_string()
            } else {
                w.to_string()
            }
        })
        .collect();
    changed.then(|| words.join(" "))
}

/// The 7.0 DateValue grammar (modifiers, ranges, periods, calendars, BCE).
pub(crate) fn valid_date(s: &str) -> bool {
    let (head, rest) = s.split_once(' ').unwrap_or((s, ""));
    match head {
        "ABT" | "CAL" | "EST" | "BEF" | "AFT" | "TO" => single(rest),
        "BET" => rest.split_once(" AND ").is_some_and(|(a, b)| single(a) && single(b)),
        "FROM" => match rest.split_once(" TO ") {
            Some((a, b)) => single(a) && single(b),
            None => single(rest),
        },
        _ => single(s),
    }
}

fn single(s: &str) -> bool {
    let mut w: Vec<&str> = s.split(' ').filter(|x| !x.is_empty()).collect();
    let months: &[&str] = match w.first() {
        Some(&"HEBREW") => HEBREW_MONTHS,
        Some(&"FRENCH_R") => FRENCH_MONTHS,
        Some(c) if c.starts_with('_') => &[],
        _ => MONTHS,
    };
    let ext = w.first().is_some_and(|c| c.starts_with('_'));
    if w.first().is_some_and(|c| matches!(*c, "GREGORIAN" | "JULIAN" | "HEBREW" | "FRENCH_R") || c.starts_with('_')) {
        w.remove(0);
    }
    if w.last() == Some(&"BCE") {
        w.pop();
    }
    let num = |x: &str| !x.is_empty() && x.len() <= 4 && x.chars().all(|c| c.is_ascii_digit());
    let month = |x: &str| ext || months.contains(&x);
    match w.as_slice() {
        [y] => num(y),
        [m, y] => month(m) && num(y),
        [d, m, y] => num(d) && d.len() <= 2 && month(m) && num(y),
        _ => false,
    }
}

fn write7(out: &mut String, n: &Node, level: usize) {
    let mut lines = n.value.split('\n');
    out.push_str(&level.to_string());
    if let Some(x) = &n.xref {
        out.push_str(&format!(" @{x}@"));
    }
    out.push(' ');
    out.push_str(&n.tag);
    push_value(out, lines.next().unwrap_or(""), n.pointer().is_some());
    for l in lines {
        out.push_str(&format!("{} CONT", level + 1));
        push_value(out, l, false);
    }
    for c in &n.children {
        write7(out, c, level + 1);
    }
}

fn push_value(out: &mut String, v: &str, pointer: bool) {
    if !v.is_empty() {
        out.push(' ');
        if v.starts_with('@') && !pointer {
            out.push('@');
        }
        out.push_str(v);
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = include_str!("../tests/fixtures/legacy551.ged");
    const SAMPLE: &str = include_str!("../tests/fixtures/sample.ged");

    fn lines(s: &str) -> Vec<&str> {
        s.lines().collect()
    }

    #[test]
    fn dates() {
        for (inp, out, phrase) in [
            ("12 JUN 1950", "12 JUN 1950", None),
            ("abt 1790", "ABT 1790", None),
            ("BET 1800 AND 1810", "BET 1800 AND 1810", None),
            ("FROM 1800 TO MAR 1810", "FROM 1800 TO MAR 1810", None),
            ("FROM 1800", "FROM 1800", None),
            ("@#DJULIAN@ 3 FEB 1700", "JULIAN 3 FEB 1700", None),
            ("@#DFRENCH R@ 1 VEND 7", "FRENCH_R 1 VEND 7", None),
            ("44 B.C.", "44 BCE", None),
            ("INT 1850 (dicho por la abuela)", "1850", Some("dicho por la abuela")),
            ("(en la guerra)", "", Some("en la guerra")),
            ("hacia la cosecha", "", Some("hacia la cosecha")),
            ("12/3/1852", "", Some("12/3/1852")),
            ("30 JAN 1648/9", "30 JAN 1649", Some("30 JAN 1648/9")),
            ("1699/00", "1700", Some("1699/00")),
            ("BET 1800", "", Some("BET 1800")),
        ] {
            assert_eq!(date7(inp), (out.to_string(), phrase.map(str::to_string)), "{inp:?}");
        }
    }

    #[test]
    fn exports_valid_gedcom7_that_our_reader_parses() {
        let t = Tree::parse(LEGACY).unwrap();
        let out = t.to_gedcom7();
        let l = lines(&out);
        assert_eq!(&l[..3], ["0 HEAD", "1 GEDC", "2 VERS 7.0"]);
        assert_eq!(l.last(), Some(&"0 TRLR"));
        assert!(!out.contains("CHAR") && !out.contains("LINEAGE-LINKED") && !out.contains(" CONC"), "{out}");
        assert!(!out.contains('\r'));
        for line in &l {
            let level: usize = line.split(' ').next().unwrap().parse().unwrap();
            assert!(level <= 6, "{line}");
        }

        // our own reader takes it back; people, families and links survive
        let back = Tree::parse(&out).unwrap();
        assert_eq!(back.stats(), t.stats());
        assert_eq!(back.father_mother("I3").0.map(|p| p.id.as_str()), Some("I1"));
        assert_eq!(back.person("I1").unwrap().birth.as_ref().unwrap().date.as_deref(), Some("ABT 1820"));

        // long CONC-split note is one value again, CONT kept
        let n = back.record("N1").unwrap();
        assert_eq!(n.tag, "SNOTE");
        assert!(n.value.starts_with("Una nota compartida que sigue"), "{:?}", n.value);
        assert!(n.value.contains("\nSegunda línea"));
        let i1 = back.record("I1").unwrap();
        assert_eq!(i1.child("SNOTE").and_then(|c| c.pointer()), Some("N1"));
        assert!(i1.children_with("NOTE").any(|c| c.pointer().is_none()), "inline note stays NOTE");

        // vendor tags kept
        assert!(out.contains("1 _MARNM Prueba\n") || out.contains("2 _MARNM Prueba\n"), "{out}");
        assert!(out.contains("1 _UID ABC123"));

        // dates
        assert!(out.contains("2 DATE 1850\n3 PHRASE dicho por la abuela\n"), "{out}");
        assert!(out.contains("2 DATE\n3 PHRASE hacia la cosecha\n"), "{out}");
        assert!(out.contains("2 DATE JULIAN 3 FEB 1700\n"));

        // inline source text → SOUR record, shared by identical citations
        let i2 = back.record("I2").unwrap();
        let cites: Vec<&str> = i2.children.iter().filter(|c| c.tag == "SOUR").filter_map(|c| c.pointer()).collect();
        assert_eq!(cites.len(), 2);
        assert_eq!(cites[0], cites[1]);
        let s = back.record(cites[0]).unwrap();
        assert_eq!(s.child_value("TITL"), Some("Libro de bautismos, folio 3"));
        assert_eq!(s.child_value("TEXT"), Some("Bautizada el día tres"));
        assert_ne!(cites[0], "S1", "fresh id, no clash with the existing source");
        assert!(i2.children_with("SOUR").all(|c| c.child("TEXT").is_none()));
        assert_eq!(i2.children_with("SOUR").next().unwrap().child_value("PAGE"), Some("p. 3"));

        // @@ rules: 5.5.1 escapes undone, leading @ doubled
        assert!(out.contains("1 EMAIL ana@example.com\n"), "{out}");
        assert!(out.contains("1 NOTE @@inicio con arroba\n"), "{out}");

        // enums, media, ids, removed tags
        assert!(out.contains("2 PEDI BIRTH\n"));
        assert!(out.contains("2 TYPE OTHER\n3 PHRASE apodo\n") || out.contains("2 TYPE AKA\n"));
        assert!(out.contains("2 ROLE GODP\n"), "{out}");
        assert!(out.contains("2 ROLE OTHER\n3 PHRASE vecina\n"), "{out}");
        assert!(out.contains("2 FORM image/jpeg\n3 MEDI PHOTO\n"), "{out}");
        assert!(out.contains("1 EXID 77\n2 TYPE https://gedcom.io/terms/v7/RIN\n"));
        assert!(out.contains("1 LANG es\n"));
        assert!(out.contains("1 FAMC @VOID@\n"), "dangling pointer: {out}");
        assert!(out.contains("0 @X_1@ INDI\n"), "xref sanitized: {out}");
        assert!(out.contains("1 ASSO @X_1@\n"), "pointer follows: {out}");
        let obj = back.record("I3").unwrap().child("OBJE").and_then(|o| o.pointer()).unwrap();
        assert_eq!(back.record(obj).unwrap().child("FILE").unwrap().child_value("TITL"), Some("Retrato"));
    }

    #[test]
    fn plain_tree_only_changes_header() {
        let t = Tree::parse(SAMPLE).unwrap();
        let out = t.to_gedcom7();
        let five = t.to_gedcom();
        let strip = |s: &str| s.lines().skip_while(|l| !l.starts_with("0 @")).map(str::to_string).collect::<Vec<_>>();
        // sample's only difference past HEAD: the CONC note is folded and the inline note stays
        let a = strip(&out);
        let b = strip(&five);
        assert_eq!(a, b);
        assert!(out.starts_with("0 HEAD\n1 GEDC\n2 VERS 7.0\n1 SOUR TEST\n0 @I1@"), "{out}");
    }
}
