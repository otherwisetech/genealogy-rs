//! Derived facts for the person sheet and palette. CONTRACT STUB — the core agent implements/tests.
use crate::tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    BirthDate,
    BirthPlace,
    DeathPlace,
    Parents,
    Partner,
    Sources,
}

impl Missing {
    /// Spanish fragment for «Faltan …»: "fecha de nacimiento", "lugar de defunción", "padres",
    /// "pareja", "fuentes"…
    pub fn label(self) -> &'static str {
        match self {
            Missing::BirthDate => "fecha de nacimiento",
            Missing::BirthPlace => "lugar de nacimiento",
            Missing::DeathPlace => "lugar de defunción",
            Missing::Parents => "padres",
            Missing::Partner => "pareja",
            Missing::Sources => "fuentes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Completeness {
    pub percent: u8,
    pub missing: Vec<Missing>,
}

impl Completeness {
    /// «Faltan lugar de defunción, pareja y fuentes.» or "" when complete.
    pub fn sentence(&self) -> String {
        String::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimelineItem {
    pub year: Option<i32>,
    pub date: Option<String>,
    pub tag: String,
    /// Spanish: Nacimiento, Bautizo, Matrimonio, Defunción, Entierro, Residencia…
    pub label: String,
    pub place: Option<String>,
    /// for MARR/DIV: the family and the other partner
    pub family: Option<String>,
    pub partner: Option<String>,
}

impl Tree {
    /// «Familia Ordóñez»: most common first surname word; «Árbol familiar» when empty.
    pub fn title(&self) -> String {
        "Árbol familiar".into()
    }

    pub fn completeness(&self, _id: &str) -> Completeness {
        Completeness::default()
    }

    /// Person events + family events (marriage…) sorted by year; undated last.
    pub fn timeline(&self, _id: &str) -> Vec<TimelineItem> {
        vec![]
    }

    /// Places used in events, prefix/word match (accent-insensitive), most used first.
    pub fn place_suggestions(&self, _query: &str, _limit: usize) -> Vec<(String, usize)> {
        vec![]
    }

    /// Spanish gendered relationship of `other` seen from `focus`: "padre", "madre", "abuela",
    /// "bisabuelo", "hijo", "nieta", "hermano", "tía", "sobrino", "prima", "esposa", "pareja",
    /// "persona en foco"… None when unrelated within 4 steps.
    pub fn relationship(&self, _focus: &str, _other: &str) -> Option<String> {
        None
    }
}

impl crate::tree::Person {
    /// Whole years between birth and death when both have a year.
    pub fn age_at_death(&self) -> Option<i32> {
        Some(self.death_year()? - self.birth_year()?)
    }
}
