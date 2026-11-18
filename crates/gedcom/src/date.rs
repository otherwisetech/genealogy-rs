//! GEDCOM date phrases ↔ the dialog's (precision, date) fields.
use crate::search::fold;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    #[default]
    Exact,
    /// ABT / CAL / EST
    About,
    /// BEF
    Before,
    /// AFT
    After,
    /// BET x AND y (also FROM x TO y)
    Between,
    /// anything we cannot structure; `date` holds the raw text
    Phrase,
}

impl Precision {
    pub const ALL: [Precision; 5] = [Precision::Exact, Precision::About, Precision::Before, Precision::After, Precision::Between];
    /// Localized UI label (es: Exacta, Aproximada, Antes de, Después de, Entre, Texto libre).
    pub fn label(self) -> String {
        match self {
            Precision::Exact => i18n::t!("core.precision.exact"),
            Precision::About => i18n::t!("core.precision.about"),
            Precision::Before => i18n::t!("core.precision.before"),
            Precision::After => i18n::t!("core.precision.after"),
            Precision::Between => i18n::t!("core.precision.between"),
            Precision::Phrase => i18n::t!("core.precision.phrase"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DateValue {
    pub precision: Precision,
    /// normalized GEDCOM date ("12 MAR 1852", "MAR 1852", "1852") or raw text for Phrase
    pub date: String,
    /// second date for Between
    pub date2: String,
}

impl DateValue {
    pub fn parse(gedcom: &str) -> DateValue {
        let raw = gedcom.trim();
        if raw.is_empty() {
            return DateValue::default();
        }
        let phrase = || DateValue { precision: Precision::Phrase, date: raw.to_string(), date2: String::new() };
        let (head, rest) = raw.split_once(char::is_whitespace).unwrap_or((raw, ""));
        let head = head.to_ascii_uppercase();
        let one = |p: Precision| match structured(rest) {
            Some(d) => DateValue { precision: p, date: d, date2: String::new() },
            None => phrase(),
        };
        let two = |sep: &str| {
            let up = rest.to_ascii_uppercase();
            let ix = up.find(sep)?;
            let (a, b) = (structured(&rest[..ix])?, structured(&rest[ix + sep.len()..])?);
            Some(DateValue { precision: Precision::Between, date: a, date2: b })
        };
        match head.as_str() {
            "ABT" | "CAL" | "EST" | "ABOUT" | "CIRCA" => one(Precision::About),
            "BEF" | "BEFORE" => one(Precision::Before),
            "AFT" | "AFTER" => one(Precision::After),
            "BET" => two(" AND ").unwrap_or_else(phrase),
            "FROM" => two(" TO ").unwrap_or_else(phrase),
            _ => match structured(raw) {
                Some(d) => DateValue { precision: Precision::Exact, date: d, date2: String::new() },
                None => phrase(),
            },
        }
    }

    /// "ABT 1790", "BET 1800 AND 1810", "" when empty.
    pub fn to_gedcom(&self) -> String {
        let (a, b) = (normalize_date(&self.date), normalize_date(&self.date2));
        let pre = |p: &str| if a.is_empty() { String::new() } else { format!("{p} {a}") };
        match self.precision {
            Precision::Phrase => self.date.trim().to_string(),
            Precision::Exact => a,
            Precision::About => pre("ABT"),
            Precision::Before => pre("BEF"),
            Precision::After => pre("AFT"),
            Precision::Between => match (a.is_empty(), b.is_empty()) {
                (false, false) => format!("BET {a} AND {b}"),
                (false, true) => format!("AFT {a}"),
                (true, false) => format!("BEF {b}"),
                (true, true) => String::new(),
            },
        }
    }
}

const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

/// Month number for an (already folded) English/Spanish/Portuguese/French name or abbreviation.
fn month_of(word: &str) -> Option<usize> {
    const NAMES: &[(&str, usize)] = &[
        ("jan", 1), ("january", 1), ("ene", 1), ("enero", 1), ("janeiro", 1), ("janv", 1), ("janvier", 1),
        ("feb", 2), ("february", 2), ("febrero", 2), ("fev", 2), ("fevereiro", 2), ("fevr", 2), ("fevrier", 2),
        ("mar", 3), ("march", 3), ("marzo", 3), ("marco", 3), ("mars", 3),
        ("apr", 4), ("april", 4), ("abr", 4), ("abril", 4), ("avr", 4), ("avril", 4),
        ("may", 5), ("mayo", 5), ("mai", 5), ("maio", 5),
        ("jun", 6), ("june", 6), ("junio", 6), ("junho", 6), ("juin", 6),
        ("jul", 7), ("july", 7), ("julio", 7), ("julho", 7), ("juil", 7), ("juillet", 7),
        ("aug", 8), ("august", 8), ("ago", 8), ("agosto", 8), ("aout", 8),
        ("sep", 9), ("sept", 9), ("september", 9), ("septiembre", 9), ("setiembre", 9), ("set", 9), ("setembro", 9), ("septembre", 9),
        ("oct", 10), ("october", 10), ("octubre", 10), ("out", 10), ("outubro", 10), ("octobre", 10),
        ("nov", 11), ("november", 11), ("noviembre", 11), ("novembro", 11), ("novembre", 11),
        ("dec", 12), ("december", 12), ("dic", 12), ("diciembre", 12), ("dez", 12), ("dezembro", 12), ("decembre", 12),
    ];
    NAMES.iter().find(|(n, _)| *n == word).map(|(_, m)| *m)
}

enum Tok {
    Num(String),
    Month(usize),
}

/// Lenient input → GEDCOM date, or None when it isn't a recognizable day/month/year.
fn structured(input: &str) -> Option<String> {
    let f = fold(input.trim());
    if f.is_empty() {
        return None;
    }
    let mut toks = Vec::new();
    for w in f.split(|c: char| c.is_whitespace() || matches!(c, '/' | '-' | '.' | ',')).filter(|w| !w.is_empty()) {
        if matches!(w, "de" | "del" | "of" | "le") {
            continue;
        }
        if w.chars().all(|c| c.is_ascii_digit()) {
            toks.push(Tok::Num(w.to_string()));
        } else {
            toks.push(Tok::Month(month_of(w)?));
        }
    }
    let day = |s: &str| s.parse::<u32>().ok().filter(|d| (1..=31).contains(d));
    let mon = |s: &str| s.parse::<usize>().ok().filter(|m| (1..=12).contains(m));
    let year = |s: &str| (s.len() <= 4).then(|| s.parse::<u32>().ok()).flatten().filter(|y| *y > 0);
    let ymd = |d: Option<u32>, m: usize, y: u32| match d {
        Some(d) => format!("{d} {} {y}", MONTHS[m - 1]),
        None => format!("{} {y}", MONTHS[m - 1]),
    };
    use Tok::*;
    Some(match toks.as_slice() {
        [Num(y)] => year(y)?.to_string(),
        [Month(m), Num(y)] => ymd(None, *m, year(y)?),
        [Num(d), Month(m), Num(y)] | [Month(m), Num(d), Num(y)] => ymd(Some(day(d)?), *m, year(y)?),
        [Num(m), Num(y)] if y.len() >= 3 => ymd(None, mon(m)?, year(y)?),
        [Num(a), Num(b), Num(c)] if a.len() == 4 => ymd(Some(day(c)?), mon(b)?, year(a)?),
        [Num(d), Num(m), Num(y)] if y.len() >= 3 => ymd(Some(day(d)?), mon(m)?, year(y)?),
        _ => return None,
    })
}

/// Lenient user input → GEDCOM date: "12/3/1852", "12 mar 1852", "12 marzo 1852", "1852-03-12"
/// → "12 MAR 1852". Unrecognized input is returned trimmed.
pub fn normalize_date(input: &str) -> String {
    structured(input).unwrap_or_else(|| input.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dv(p: Precision, a: &str, b: &str) -> DateValue {
        DateValue { precision: p, date: a.into(), date2: b.into() }
    }

    #[test]
    fn normalizes_lenient_input() {
        for (inp, out) in [
            ("12/3/1852", "12 MAR 1852"),
            ("1852-03-12", "12 MAR 1852"),
            ("12 mar 1852", "12 MAR 1852"),
            ("12 marzo 1852", "12 MAR 1852"),
            ("12 de marzo de 1852", "12 MAR 1852"),
            ("marzo 1852", "MAR 1852"),
            ("March 12, 1852", "12 MAR 1852"),
            ("3 août 1901", "3 AUG 1901"),
            ("5 setembro 1900", "5 SEP 1900"),
            ("01 dic. 1799", "1 DEC 1799"),
            ("3/1852", "MAR 1852"),
            (" 1852 ", "1852"),
            ("12 MAR 1852", "12 MAR 1852"),
            ("  quién sabe ", "quién sabe"),
            ("32/1/1850", "32/1/1850"),
            ("", ""),
        ] {
            assert_eq!(normalize_date(inp), out, "{inp:?}");
        }
    }

    #[test]
    fn parses_qualifiers() {
        assert_eq!(DateValue::parse("ABT 1790"), dv(Precision::About, "1790", ""));
        assert_eq!(DateValue::parse("cal 1790"), dv(Precision::About, "1790", ""));
        assert_eq!(DateValue::parse("EST MAR 1790"), dv(Precision::About, "MAR 1790", ""));
        assert_eq!(DateValue::parse("BEF 3 JAN 1800"), dv(Precision::Before, "3 JAN 1800", ""));
        assert_eq!(DateValue::parse("aft 1800"), dv(Precision::After, "1800", ""));
        assert_eq!(DateValue::parse("BET 1800 AND 1810"), dv(Precision::Between, "1800", "1810"));
        assert_eq!(DateValue::parse("from 1800 to jun 1810"), dv(Precision::Between, "1800", "JUN 1810"));
        assert_eq!(DateValue::parse("12 jun 1950"), dv(Precision::Exact, "12 JUN 1950", ""));
        assert_eq!(DateValue::parse(""), DateValue::default());
        assert_eq!(DateValue::parse("INT 1850 (dicho por la abuela)"), dv(Precision::Phrase, "INT 1850 (dicho por la abuela)", ""));
        assert_eq!(DateValue::parse("(en la guerra)"), dv(Precision::Phrase, "(en la guerra)", ""));
        assert_eq!(DateValue::parse("BET 1800"), dv(Precision::Phrase, "BET 1800", ""));
    }

    #[test]
    fn to_gedcom_inverse() {
        for s in ["ABT 1790", "BEF 3 JAN 1800", "AFT MAR 1800", "BET 1800 AND 1810", "12 JUN 1950", "1950", "INT 1850 (x)"] {
            assert_eq!(DateValue::parse(s).to_gedcom(), s);
        }
        assert_eq!(DateValue::parse("FROM 1800 TO 1810").to_gedcom(), "BET 1800 AND 1810");
        assert_eq!(dv(Precision::About, "12/3/1852", "").to_gedcom(), "ABT 12 MAR 1852");
        assert_eq!(dv(Precision::About, " ", "").to_gedcom(), "");
        assert_eq!(dv(Precision::Between, "1800", "").to_gedcom(), "AFT 1800");
        assert_eq!(DateValue::default().to_gedcom(), "");
    }

    #[test]
    fn precision_labels_are_localized() {
        let _l = crate::locale_lock();
        assert_eq!(Precision::About.label(), i18n::t!("core.precision.about"));
        assert_ne!(Precision::About.label(), "core.precision.about");
    }
}
