//! Note text for display: many programs (and online trees) export notes as HTML —
//! `<p>Sobre la descendencia&hellip;</p>`, `Pe&ntilde;aranda`, `<a href=…>`. The file keeps the
//! original; the app shows and searches plain text.

/// Plain text of a note. Text without markup or entities is returned unchanged (trimmed).
pub fn note_text(raw: &str) -> String {
    if !looks_like_html(raw) {
        return raw.trim().to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find(['<', '&']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with('<') {
            match rest.find('>') {
                Some(end) => {
                    let tag = rest[1..end].trim_start_matches('/').split(|c: char| c.is_whitespace() || c == '/').next().unwrap_or("").to_ascii_lowercase();
                    match tag.as_str() {
                        "br" => out.push('\n'),
                        "p" | "div" | "li" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4" | "tr" | "table" | "blockquote" => out.push_str("\n\n"),
                        _ => {}
                    }
                    if tag == "li" && !rest[1..].starts_with('/') {
                        out.push_str("• ");
                    }
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push('<');
                    rest = &rest[1..];
                }
            }
        } else {
            match rest[1..].find(';').filter(|&n| n <= 10).and_then(|n| entity(&rest[1..=n]).map(|c| (c, n))) {
                Some((c, n)) => {
                    out.push(c);
                    rest = &rest[n + 2..];
                }
                None => {
                    out.push('&');
                    rest = &rest[1..];
                }
            }
        }
    }
    out.push_str(rest);
    tidy(&out)
}

fn looks_like_html(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    ["<p", "</", "<br", "<div", "<a ", "<span", "<li", "<b>", "<i>", "<strong", "<em>"].iter().any(|t| l.contains(t))
        || (l.contains('&') && l.contains(';') && l.split('&').skip(1).any(|p| p.find(';').is_some_and(|n| n <= 10 && entity(&p[..n]).is_some())))
}

fn entity(name: &str) -> Option<char> {
    if let Some(num) = name.strip_prefix('#') {
        let code = match num.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => num.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "nbsp" => ' ',
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "hellip" => '…',
        "ndash" => '–',
        "mdash" => '—',
        "laquo" => '«',
        "raquo" => '»',
        "ldquo" => '“',
        "rdquo" => '”',
        "lsquo" => '‘',
        "rsquo" => '’',
        "iexcl" => '¡',
        "iquest" => '¿',
        "ordf" => 'ª',
        "ordm" => 'º',
        "deg" => '°',
        "middot" => '·',
        "copy" => '©',
        "szlig" => 'ß',
        _ => return accented(name),
    })
}

/// `&aacute;` `&Ntilde;` `&ccedil;` `&uuml;` … built from base letter + accent name.
fn accented(name: &str) -> Option<char> {
    const TABLE: &[(&str, &str, &str)] = &[
        ("acute", "aeiouyAEIOUY", "áéíóúýÁÉÍÓÚÝ"),
        ("grave", "aeiouAEIOU", "àèìòùÀÈÌÒÙ"),
        ("circ", "aeiouAEIOU", "âêîôûÂÊÎÔÛ"),
        ("uml", "aeiouyAEIOU", "äëïöüÿÄËÏÖÜ"),
        ("tilde", "anoANO", "ãñõÃÑÕ"),
        ("cedil", "cC", "çÇ"),
        ("ring", "aA", "åÅ"),
        ("slash", "oO", "øØ"),
        ("lig", "", ""),
    ];
    let mut chars = name.chars();
    let base = chars.next()?;
    let accent = chars.as_str();
    match name {
        "aelig" => return Some('æ'),
        "AElig" => return Some('Æ'),
        "oelig" => return Some('œ'),
        "OElig" => return Some('Œ'),
        _ => {}
    }
    let (_, bases, out) = TABLE.iter().find(|(a, _, _)| *a == accent)?;
    let ix = bases.chars().position(|c| c == base)?;
    out.chars().nth(ix)
}

/// Collapse runs of spaces, trim each line, at most one blank line between paragraphs.
fn tidy(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in s.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            blank += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank > 0 { "\n\n" } else { "\n" });
        }
        blank = 0;
        out.push_str(&line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_and_decodes_entities() {
        let raw = "<p>Sobre la descendencia de Balthasar trabaj&oacute; Hel&iacute; Pe&ntilde;aranda Vermi&egrave;re, en &quot;Mil abuelos&quot;.</p>\n<p>&nbsp;</p><p>Ver <a href=\"https://example.org/x\">el grupo</a><br/>y m&aacute;s &#233; &#xE9; &amp; fin</p>";
        assert_eq!(note_text(raw), "Sobre la descendencia de Balthasar trabajó Helí Peñaranda Vermière, en \"Mil abuelos\".\n\nVer el grupo\ny más é é & fin");
    }

    #[test]
    fn plain_text_is_untouched() {
        assert_eq!(note_text("  Nació en 1850 & murió <joven> (p. 4)  "), "Nació en 1850 & murió <joven> (p. 4)");
        assert_eq!(note_text("Línea uno\nLínea dos"), "Línea uno\nLínea dos");
        assert_eq!(note_text("a &unknown; b <p>c</p>"), "a &unknown; b\n\nc");
    }

    #[test]
    fn lists() {
        assert_eq!(note_text("<ul><li>uno</li><li>dos</li></ul>"), "• uno\n\n• dos");
    }
}
