//! GEDCOM date phrases ↔ the dialog's (precision, date) fields.
//! CONTRACT STUB — the core agent implements and tests this.

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
    /// Spanish UI label: Exacta, Aproximada, Antes de, Después de, Entre, Texto libre.
    pub fn label(self) -> &'static str {
        match self {
            Precision::Exact => "Exacta",
            Precision::About => "Aproximada",
            Precision::Before => "Antes de",
            Precision::After => "Después de",
            Precision::Between => "Entre",
            Precision::Phrase => "Texto libre",
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
        DateValue { precision: Precision::Exact, date: gedcom.trim().to_string(), date2: String::new() }
    }

    /// "ABT 1790", "BET 1800 AND 1810", "" when empty.
    pub fn to_gedcom(&self) -> String {
        self.date.trim().to_string()
    }
}

/// Lenient user input → GEDCOM date: "12/3/1852", "12 mar 1852", "12 marzo 1852", "1852-03-12"
/// → "12 MAR 1852". Unrecognized input is returned trimmed.
pub fn normalize_date(input: &str) -> String {
    input.trim().to_string()
}
