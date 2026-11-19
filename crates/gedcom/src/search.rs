//! Accent/case-insensitive person search: every query token must prefix-match some word of the
//! person's search key (name, married names, years, places, id).
use crate::tree::Tree;
use std::ops::Range;

/// Lowercase and strip Latin diacritics (á→a, ñ→n, ç→c, ß→ss…). Not full Unicode NFD on purpose:
/// no extra dependency, and genealogy data is overwhelmingly Latin-script.
pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let mapped: &str = match c {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ą' | 'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' | 'Ą' => "a",
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' | 'É' | 'È' | 'Ê' | 'Ë' | 'Ē' | 'Ę' | 'Ě' => "e",
            'í' | 'ì' | 'î' | 'ï' | 'ī' | 'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' => "i",
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ő' | 'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ø' | 'Ō' | 'Ő' => "o",
            'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' | 'Ů' | 'Ű' => "u",
            'ñ' | 'ń' | 'ň' | 'Ñ' | 'Ń' | 'Ň' => "n",
            'ç' | 'ć' | 'č' | 'Ç' | 'Ć' | 'Č' => "c",
            'ý' | 'ÿ' | 'Ý' => "y",
            'š' | 'ś' | 'Š' | 'Ś' => "s",
            'ž' | 'ź' | 'ż' | 'Ž' | 'Ź' | 'Ż' => "z",
            'ł' | 'Ł' => "l",
            'ř' | 'Ř' => "r",
            'ď' | 'Ď' => "d",
            'ť' | 'Ť' => "t",
            'ğ' | 'Ğ' => "g",
            'ß' => "ss",
            'æ' | 'Æ' => "ae",
            'œ' | 'Œ' => "oe",
            _ => {
                for l in c.to_lowercase() {
                    out.push(l);
                }
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

pub(crate) fn tokens(folded: &str) -> Vec<&str> {
    folded.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect()
}

/// Every token prefix-matches some word of the (folded) key.
pub(crate) fn words_match(key: &str, tokens: &[&str]) -> bool {
    tokens.iter().all(|t| key.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(t)))
}

/// Person ids matching `query`, in the tree's display order. Empty query → everyone.
pub fn search<'a>(tree: &'a Tree, query: &str) -> Vec<&'a str> {
    let q = fold(query);
    let tokens = tokens(&q);
    tree.order.iter().filter(|id| words_match(&tree.people[*id].search_key, &tokens)).map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_strips_accents() {
        assert_eq!(fold("Muñoz Ramírez"), "munoz ramirez");
        assert_eq!(fold("Straße"), "strasse");
    }

    #[test]
    fn search_tokens_prefix_any_order() {
        let t = Tree::parse(include_str!("../tests/fixtures/sample.ged")).unwrap();
        assert_eq!(search(&t, "lopez ana"), vec!["I3"]);
        assert_eq!(search(&t, "GARCIA").len(), 4);
        assert_eq!(search(&t, "1950"), vec!["I3"]);
        assert_eq!(search(&t, "sevilla"), vec!["I1"]);
        assert_eq!(search(&t, "").len(), 5);
    }
}

// ---- ⌘K palette search. CONTRACT STUB — the core agent implements and tests. ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchFilter {
    #[default]
    All,
    People,
    Places,
    /// people with no birth and no death date
    NoDates,
    /// people with no parents
    NoParents,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonHit {
    pub id: String,
    /// display name ("Given Surname")
    pub name: String,
    /// byte ranges in `name` to highlight
    pub name_hl: Vec<std::ops::Range<usize>>,
    /// «1758 – 1821 · padre de Ramona»
    pub sub: String,
    pub sub_hl: Vec<std::ops::Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceHit {
    pub place: String,
    pub hl: Vec<std::ops::Range<usize>>,
    /// number of events at this place
    pub events: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PaletteResults {
    pub people: Vec<PersonHit>,
    pub places: Vec<PlaceHit>,
}

/// `fold` that remembers, for every folded byte, the original char's byte span.
struct Folded {
    text: String,
    spans: Vec<(usize, usize)>,
}

impl Folded {
    fn new(s: &str) -> Folded {
        let (mut text, mut spans) = (String::with_capacity(s.len()), Vec::with_capacity(s.len()));
        for (i, c) in s.char_indices() {
            let f = fold(c.encode_utf8(&mut [0; 4]));
            spans.extend(std::iter::repeat_n((i, i + c.len_utf8()), f.len()));
            text.push_str(&f);
        }
        Folded { text, spans }
    }

    /// Original byte ranges of the words that start with any token (prefix part only).
    fn highlights(&self, tokens: &[&str]) -> Vec<Range<usize>> {
        let mut out: Vec<Range<usize>> = Vec::new();
        let mut start = None;
        let bytes = self.text.char_indices().map(|(i, c)| (i, c.is_alphanumeric())).chain(std::iter::once((self.text.len(), false)));
        for (i, alnum) in bytes {
            match (alnum, start) {
                (true, None) => start = Some(i),
                (false, Some(s)) => {
                    let word = &self.text[s..i];
                    if let Some(len) = tokens.iter().filter(|t| word.starts_with(**t)).map(|t| t.len()).max() {
                        out.push(self.spans[s].0..self.spans[s + len - 1].1);
                    }
                    start = None;
                }
                _ => {}
            }
        }
        out
    }
}

/// Ranked results for the palette. `focus` drives the relationship text in `sub`.
/// People rank: whole-name equal > name prefix > every token in the name > matched elsewhere
/// (years, places, id); ties keep the tree order.
pub fn palette_search(tree: &Tree, query: &str, filter: SearchFilter, focus: Option<&str>, limit: usize) -> PaletteResults {
    let q = fold(query);
    let toks = tokens(&q);
    let joined = toks.join(" ");
    let years: Vec<&str> = toks.iter().copied().filter(|t| t.chars().all(|c| c.is_ascii_digit())).collect();
    let mut res = PaletteResults::default();

    if filter != SearchFilter::Places {
        let focus_name = focus.and_then(|f| tree.person(f)).map(|p| p.given.split_whitespace().next().map(str::to_string).unwrap_or_else(|| p.name()));
        let mut ranked: Vec<(u8, usize, &str)> = search(tree, query)
            .into_iter()
            .enumerate()
            .filter(|(_, id)| {
                let p = &tree.people[*id];
                match filter {
                    SearchFilter::NoDates => p.birth.as_ref().and_then(|e| e.date.as_ref()).is_none() && p.death.as_ref().and_then(|e| e.date.as_ref()).is_none(),
                    SearchFilter::NoParents => tree.parents(id).is_empty(),
                    _ => true,
                }
            })
            .map(|(i, id)| {
                let name = tokens(&fold(&tree.people[id].name())).join(" ");
                let tier = if joined.is_empty() || name == joined {
                    0
                } else if name.starts_with(&joined) {
                    1
                } else if words_match(&name, &toks) {
                    2
                } else {
                    3
                };
                (tier, i, id)
            })
            .collect();
        ranked.sort();
        res.people = ranked.into_iter().take(limit).map(|(_, _, id)| person_hit(tree, id, &toks, &years, focus, focus_name.as_deref())).collect();
    }

    let places_allowed = match filter {
        SearchFilter::All => !toks.is_empty(),
        SearchFilter::Places => true,
        _ => false,
    };
    if places_allowed {
        res.places = tree
            .place_counts()
            .into_iter()
            .filter(|(p, _)| words_match(&fold(p), &toks))
            .take(limit)
            .map(|(place, events)| PlaceHit { hl: Folded::new(&place).highlights(&toks), place, events })
            .collect();
    }
    res
}

fn person_hit(tree: &Tree, id: &str, toks: &[&str], years: &[&str], focus: Option<&str>, focus_name: Option<&str>) -> PersonHit {
    let p = &tree.people[id];
    let name = p.name();
    let name_hl = Folded::new(&name).highlights(toks);
    let mut sub = p.lifespan();
    let rel = match (focus, focus_name) {
        (Some(f), _) if f == id => tree.relationship(f, id),
        (Some(f), Some(n)) => tree.relationship(f, id).map(|r| i18n::tv!("core.palette.relation", n, rel = r, name = n)),
        _ => None,
    };
    if let Some(r) = rel {
        if !sub.is_empty() {
            sub.push_str(" · ");
        }
        sub.push_str(&r);
    }
    let sub_hl = Folded::new(&sub).highlights(years);
    PersonHit { id: id.to_string(), name, name_hl, sub, sub_hl }
}

/// Picker results when choosing whom to merge `source` with: likely duplicates first (matching
/// the query, if any), then the other search hits. People `source` can't be merged with
/// (itself, partners, parents, children) are left out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeResults {
    pub candidates: Vec<PersonHit>,
    pub people: Vec<PersonHit>,
}

pub fn merge_search(tree: &Tree, source: &str, query: &str, limit: usize) -> MergeResults {
    let q = fold(query);
    let toks = tokens(&q);
    let years: Vec<&str> = toks.iter().copied().filter(|t| t.chars().all(|c| c.is_ascii_digit())).collect();
    let focus_name = tree.person(source).map(|p| p.given.split_whitespace().next().map(str::to_string).unwrap_or_else(|| p.name()));
    let hit = |id: &str| person_hit(tree, id, &toks, &years, Some(source), focus_name.as_deref());
    let cands: Vec<String> = tree.duplicate_candidates(source).into_iter().map(|(id, _)| id).filter(|id| words_match(&tree.people[id].search_key, &toks)).take(limit).collect();
    let people = search(tree, query).into_iter().filter(|id| !cands.iter().any(|c| c == id) && tree.can_merge(source, id).is_ok()).take(limit).map(&hit).collect();
    MergeResults { candidates: cands.iter().map(|id| hit(id)).collect(), people }
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    const FAMILY: &str = include_str!("../tests/fixtures/family.ged");

    fn hl<'a>(s: &'a str, r: &[Range<usize>]) -> Vec<&'a str> {
        r.iter().map(|r| &s[r.clone()]).collect()
    }

    #[test]
    fn folded_highlights_map_to_original_bytes() {
        let f = Folded::new("Straße Ñuñoa Ærø");
        assert_eq!(hl("Straße Ñuñoa Ærø", &f.highlights(&["strass", "nun", "aer"])), vec!["Straß", "Ñuñ", "Ær"]);
        let f = Folded::new("Ordóñez");
        assert_eq!(hl("Ordóñez", &f.highlights(&["ordon"])), vec!["Ordóñ"]);
    }

    #[test]
    fn ranks_filters_and_places() {
        let t = Tree::parse(FAMILY).unwrap();
        let r = palette_search(&t, "ordonez", SearchFilter::All, None, 10);
        let ids: Vec<&str> = r.people.iter().map(|h| h.id.as_str()).collect();
        // name-word matches first; "Valdés Ordóñez" rank the same as other word matches, tree order
        assert!(ids.contains(&"I10") && ids.contains(&"I20"));
        let r = palette_search(&t, "tomas ordonez", SearchFilter::All, None, 10);
        assert_eq!(r.people[0].id, "I10");
        assert_eq!(hl(&r.people[0].name, &r.people[0].name_hl), vec!["Tomás", "Ordóñez"]);
        // prefix of the whole name beats a word match elsewhere in the name
        let r = palette_search(&t, "ro", SearchFilter::All, None, 10);
        assert_eq!(r.people[0].id, "I12", "Rosa… starts with ro");

        let r = palette_search(&t, "1790", SearchFilter::All, None, 10);
        assert_eq!(r.people.len(), 1);
        assert_eq!(hl(&r.people[0].sub, &r.people[0].sub_hl), vec!["1790"]);

        let r = palette_search(&t, "bogota", SearchFilter::All, None, 10);
        assert_eq!(r.places.len(), 1);
        assert_eq!((r.places[0].events, hl(&r.places[0].place, &r.places[0].hl)), (3, vec!["Bogotá"]));
        assert!(palette_search(&t, "bogota", SearchFilter::People, None, 10).places.is_empty());
        let r = palette_search(&t, "bogota", SearchFilter::Places, None, 10);
        assert!(r.people.is_empty() && r.places.len() == 1);
        assert_eq!(palette_search(&t, "", SearchFilter::Places, None, 10).places.len(), 2);

        let r = palette_search(&t, "", SearchFilter::NoDates, None, 100);
        assert!(r.places.is_empty());
        assert!(r.people.iter().all(|h| !["I10", "I12", "I20"].contains(&h.id.as_str())));
        assert_eq!(r.people.len(), t.people.len() - 3);
        let r = palette_search(&t, "", SearchFilter::NoParents, None, 100);
        assert!(r.people.iter().all(|h| !["I10", "I12", "I14", "I20", "I21", "I22"].contains(&h.id.as_str())));
        assert_eq!(palette_search(&t, "", SearchFilter::All, None, 3).people.len(), 3);
    }

    /// Text assertions together: the locale is global.
    #[test]
    fn sub_line_text() {
        let t = Tree::parse(FAMILY).unwrap();
        let _l = crate::locale_lock();
        i18n::set_locale("es");
        let r = palette_search(&t, "tomas", SearchFilter::All, Some("I12"), 10);
        assert_eq!(r.people[0].sub, "1790 – 1852 · padre de Rosa");
        let r = palette_search(&t, "rosa", SearchFilter::All, Some("I12"), 10);
        assert_eq!(r.people[0].sub, "n. 1815 · persona en foco");
        let r = palette_search(&t, "nadie", SearchFilter::All, Some("I12"), 10);
        assert_eq!(r.people[0].sub, "");
        i18n::set_locale("en");
        let r = palette_search(&t, "tomas 1852", SearchFilter::All, Some("I12"), 10);
        assert_eq!(r.people[0].sub, "1790 – 1852 · father of Rosa");
        assert_eq!(hl(&r.people[0].sub, &r.people[0].sub_hl), vec!["1852"]);
        i18n::set_locale("es");
    }

    #[test]
    fn merge_picker_results() {
        let src = "0 HEAD\n0 @I1@ INDI\n1 NAME Elena /Varela/\n1 FAMS @F1@\n0 @I2@ INDI\n1 NAME Helena /Varela/\n0 @I3@ INDI\n1 NAME Marco /Soto/\n1 FAMS @F1@\n0 @I4@ INDI\n1 NAME Elena /Soto/\n0 @F1@ FAM\n1 HUSB @I3@\n1 WIFE @I1@\n0 TRLR\n";
        let t = Tree::parse(src).unwrap();
        let ids = |v: &[PersonHit]| v.iter().map(|h| h.id.clone()).collect::<Vec<_>>();
        let r = merge_search(&t, "I1", "", 10);
        assert_eq!(ids(&r.candidates), vec!["I2"]);
        assert_eq!(ids(&r.people), vec!["I4"], "self, candidates and partner left out");
        let r = merge_search(&t, "I1", "soto", 10);
        assert!(r.candidates.is_empty());
        assert_eq!(ids(&r.people), vec!["I4"]);
    }
}
