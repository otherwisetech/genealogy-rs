//! Lossless line-level GEDCOM tree. Every tag — standard or vendor (`_UID`, `_UPD`, …) — is kept,
//! so a load → save without edits only changes CONC/CONT line splits and whitespace at EOL.

/// One GEDCOM line and its subordinate lines. `value` has CONC/CONT already folded in
/// (CONT becomes `\n`); they are re-split on write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Node {
    pub xref: Option<String>,
    pub tag: String,
    pub value: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(tag: impl Into<String>, value: impl Into<String>) -> Self {
        Node { xref: None, tag: tag.into(), value: value.into(), children: vec![] }
    }

    pub fn with_xref(mut self, xref: impl Into<String>) -> Self {
        self.xref = Some(xref.into());
        self
    }

    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.tag == tag)
    }

    pub fn child_mut(&mut self, tag: &str) -> Option<&mut Node> {
        self.children.iter_mut().find(|c| c.tag == tag)
    }

    pub fn children_with<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.tag == tag)
    }

    pub fn child_value(&self, tag: &str) -> Option<&str> {
        self.child(tag).map(|c| c.value.as_str()).filter(|v| !v.is_empty())
    }

    /// Value of a pointer line (`@X1@` → `X1`).
    pub fn pointer(&self) -> Option<&str> {
        let v = self.value.trim();
        (v.len() > 2 && v.starts_with('@') && v.ends_with('@') && !v.starts_with("@@")).then(|| &v[1..v.len() - 1])
    }

    /// First child with `tag`, created (appended) if missing.
    pub fn ensure_child(&mut self, tag: &str) -> &mut Node {
        let ix = match self.children.iter().position(|c| c.tag == tag) {
            Some(ix) => ix,
            None => {
                self.children.push(Node::new(tag, ""));
                self.children.len() - 1
            }
        };
        &mut self.children[ix]
    }

    /// Set (or remove, when `value` is empty and the child has no children) a sub-value.
    pub fn set_child_value(&mut self, tag: &str, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            if let Some(ix) = self.children.iter().position(|c| c.tag == tag) {
                if self.children[ix].children.is_empty() {
                    self.children.remove(ix);
                } else {
                    self.children[ix].value.clear();
                }
            }
            return;
        }
        self.ensure_child(tag).value = value.to_string();
    }

    pub fn remove_children_where(&mut self, f: impl Fn(&Node) -> bool) {
        self.children.retain(|c| !f(c));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("{}", i18n::t!("core.error.bad_level", line = .line, text = .text))]
    BadLevel { line: usize, text: String },
    #[error("{}", i18n::t!("core.error.missing_tag", line = .line))]
    MissingTag { line: usize },
    #[error("{}", i18n::t!("core.error.orphan_line", line = .line, level = .level))]
    OrphanLine { line: usize, level: usize },
}

/// Source formatting we reproduce on write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Format {
    pub bom: bool,
    pub crlf: bool,
}

/// Parse a whole file into level-0 records. Tolerant: blank lines are skipped, a level jump
/// (e.g. 1 → 3) attaches to the nearest shallower node, and an unparseable line is an error.
pub fn parse(text: &str) -> Result<(Vec<Node>, Format), ParseError> {
    let bom = text.starts_with('\u{feff}');
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let crlf = text.contains("\r\n");
    let mut roots: Vec<Node> = Vec::new();
    // stack of (level, path into the tree)
    let mut stack: Vec<(usize, Node)> = Vec::new();

    fn pop_to(stack: &mut Vec<(usize, Node)>, roots: &mut Vec<Node>, level: usize) {
        while let Some((l, _)) = stack.last() {
            if *l < level {
                break;
            }
            let (_, node) = stack.pop().unwrap();
            match stack.last_mut() {
                Some((_, parent)) => parent.children.push(node),
                None => roots.push(node),
            }
        }
    }

    for (i, raw) in text.split('\n').enumerate() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        let lineno = i + 1;
        let (level_s, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
        let Ok(level) = level_s.parse::<usize>() else {
            // Some exporters (MyHeritage) leave raw newlines inside values: treat a line without
            // a level number as a continuation of the previous line.
            match stack.last_mut() {
                // the stack top is always the most recently read line
                Some((_, n)) => {
                    n.value.push('\n');
                    n.value.push_str(line);
                    continue;
                }
                None => return Err(ParseError::BadLevel { line: lineno, text: level_s.to_string() }),
            }
        };
        let (xref, rest) = if rest.starts_with('@') {
            let (x, r) = rest.split_once(' ').unwrap_or((rest, ""));
            if x.len() > 2 && x.ends_with('@') {
                (Some(x[1..x.len() - 1].to_string()), r)
            } else {
                (None, rest)
            }
        } else {
            (None, rest)
        };
        let (tag, value) = match rest.split_once(' ') {
            Some((t, v)) => (t, v),
            None => (rest, ""),
        };
        if tag.is_empty() {
            return Err(ParseError::MissingTag { line: lineno });
        }
        if (tag == "CONC" || tag == "CONT") && level > 0 {
            if let Some((pl, parent)) = stack.last_mut() {
                if *pl == level - 1 && parent.children.is_empty() {
                    if tag == "CONT" {
                        parent.value.push('\n');
                    }
                    parent.value.push_str(value);
                    continue;
                }
            }
        }
        if level == 0 {
            pop_to(&mut stack, &mut roots, 0);
        } else {
            pop_to(&mut stack, &mut roots, level);
            if stack.is_empty() {
                return Err(ParseError::OrphanLine { line: lineno, level });
            }
        }
        let node = Node { xref, tag: tag.to_string(), value: value.to_string(), children: vec![] };
        stack.push((level, node));
    }
    pop_to(&mut stack, &mut roots, 0);
    Ok((roots, Format { bom, crlf }))
}

/// Physical line length cap (GEDCOM 5.5.1 says 255 incl. level/tag).
const MAX_VALUE: usize = 200;

pub fn write(records: &[Node], fmt: Format) -> String {
    let mut out = String::new();
    if fmt.bom {
        out.push('\u{feff}');
    }
    let nl = if fmt.crlf { "\r\n" } else { "\n" };
    for r in records {
        write_node(&mut out, r, 0, nl);
    }
    out
}

fn write_node(out: &mut String, n: &Node, level: usize, nl: &str) {
    let mut lines = n.value.split('\n');
    let first = lines.next().unwrap_or("");
    let mut head = format!("{level}");
    if let Some(x) = &n.xref {
        head.push_str(&format!(" @{x}@"));
    }
    head.push(' ');
    head.push_str(&n.tag);
    let chunks = split_conc(first);
    push_line(out, &head, chunks[0], nl);
    for c in &chunks[1..] {
        push_line(out, &format!("{} CONC", level + 1), c, nl);
    }
    for l in lines {
        let chunks = split_conc(l);
        push_line(out, &format!("{} CONT", level + 1), chunks[0], nl);
        for c in &chunks[1..] {
            push_line(out, &format!("{} CONC", level + 1), c, nl);
        }
    }
    for c in &n.children {
        write_node(out, c, level + 1, nl);
    }
}

fn push_line(out: &mut String, head: &str, value: &str, nl: &str) {
    out.push_str(head);
    if !value.is_empty() {
        out.push(' ');
        out.push_str(value);
    }
    out.push_str(nl);
}

/// Split into ≤MAX_VALUE-byte pieces on char boundaries, never leaving a space at a split edge
/// (many readers trim those, which would eat the space).
fn split_conc(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = s;
    while rest.len() > MAX_VALUE {
        let mut cut = MAX_VALUE;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let mut c = cut;
        while c > MAX_VALUE / 2 && (rest.as_bytes()[c - 1] == b' ' || rest.as_bytes()[c] == b' ' || !rest.is_char_boundary(c)) {
            c -= 1;
        }
        if c > MAX_VALUE / 2 {
            cut = c;
        }
        out.push(&rest[..cut]);
        rest = &rest[cut..];
    }
    out.push(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_conc_cont_and_round_trips() {
        let src = "0 HEAD\n1 CHAR UTF-8\n0 @N1@ NOTE First\n1 CONC  part\n1 CONT second line\n0 TRLR\n";
        let (recs, fmt) = parse(src).unwrap();
        assert_eq!(recs.len(), 3);
        assert_eq!(recs[1].xref.as_deref(), Some("N1"));
        assert_eq!(recs[1].value, "First part\nsecond line");
        let out = write(&recs, fmt);
        assert_eq!(out, "0 HEAD\n1 CHAR UTF-8\n0 @N1@ NOTE First part\n1 CONT second line\n0 TRLR\n");
        assert_eq!(parse(&out).unwrap().0, recs);
    }

    #[test]
    fn keeps_bom_crlf_and_vendor_tags() {
        let src = "\u{feff}0 @I1@ INDI\r\n1 _UID ABC\r\n2 _X y\r\n0 TRLR\r\n";
        let (recs, fmt) = parse(src).unwrap();
        assert!(fmt.bom && fmt.crlf);
        assert_eq!(write(&recs, fmt), src);
    }

    #[test]
    fn long_values_split_without_space_at_edges() {
        let long = "palabra ".repeat(80);
        let recs = vec![Node::new("NOTE", long.trim_end())];
        let out = write(&recs, Format::default());
        for l in out.lines() {
            assert!(l.len() <= 255);
            assert!(!l.ends_with(' '));
        }
        assert_eq!(parse(&out).unwrap().0, recs);
    }

    #[test]
    fn tolerates_level_jumps_and_blank_lines() {
        let (recs, _) = parse("0 @I1@ INDI\n\n1 BIRT\n3 DATE 1900\n0 TRLR").unwrap();
        assert_eq!(recs[0].children[0].children[0].tag, "DATE");
    }

    #[test]
    fn raw_newline_inside_value_is_continuation() {
        let (recs, _) = parse("0 @I1@ INDI\r\n1 NOTE <p>uno\n<p>dos\r\n2 CONC  tres\r\n1 SEX M\r\n").unwrap();
        assert_eq!(recs[0].children[0].value, "<p>uno\n<p>dos tres");
        assert_eq!(recs[0].children[1].tag, "SEX");
    }

    #[test]
    fn pointer_and_escaped_at() {
        assert_eq!(Node::new("FAMC", "@F1@").pointer(), Some("F1"));
        assert_eq!(Node::new("EMAIL", "a@@b.c").pointer(), None);
    }

    #[test]
    fn errors_are_localized() {
        let _l = crate::locale_lock();
        assert!(matches!(parse("x INDI"), Err(ParseError::BadLevel { line: 1, .. })));
        assert!(matches!(parse("0 HEAD\n1 \n"), Err(ParseError::MissingTag { line: 2 })));
        let orphan = parse("1 NAME A").unwrap_err();
        i18n::set_locale("en");
        assert_eq!(orphan.to_string(), "line 1: level 1 line outside any record");
        i18n::set_locale("fr");
        assert_eq!(orphan.to_string(), "ligne 1 : ligne de niveau 1 hors de tout enregistrement");
        i18n::set_locale("es");
    }
}
