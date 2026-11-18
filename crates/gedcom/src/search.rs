//! Accent/case-insensitive person search: every query token must prefix-match some word of the
//! person's search key (name, married names, years, places, id).
use crate::tree::Tree;

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

/// Person ids matching `query`, in the tree's display order. Empty query → everyone.
pub fn search<'a>(tree: &'a Tree, query: &str) -> Vec<&'a str> {
    let q = fold(query);
    let tokens: Vec<&str> = q.split(|c: char| !c.is_alphanumeric()).filter(|t| !t.is_empty()).collect();
    tree.order
        .iter()
        .filter(|id| {
            let key = &tree.people[*id].search_key;
            tokens.iter().all(|t| key.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(t)))
        })
        .map(String::as_str)
        .collect()
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
