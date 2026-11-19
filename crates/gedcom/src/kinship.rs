//! Relationship calculator: how any two people are related, with the path between them.
//!
//! Blood kinship comes from the lowest common ancestors over FAMC links: `up` generations from A
//! and `down` from the ancestor to B name the relation (parent, cousin, «primo segundo, con una
//! generación de diferencia»…). Without a blood link we try one marriage at either end (spouse,
//! «suegro», «yerno», «cuñado», step-relations, «primo político»…).
use crate::tree::{Sex, Tree};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KinKind {
    Same,
    Blood,
    /// through one marriage/partnership
    Marriage,
    None,
}

/// `path[i + 1]` is the `Link` of `path[i]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Link {
    Parent,
    Child,
    Spouse,
}

/// Another blood relationship between the same two people (pedigree collapse).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KinAlt {
    pub label: String,
    pub common: Vec<String>,
    pub up: usize,
    pub down: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kinship {
    /// the reference person (A)
    pub from: String,
    /// the person described (B): `label` is what B is to A, gendered by B's sex
    pub to: String,
    pub kind: KinKind,
    pub label: String,
    /// closest common ancestor(s): one person, or the couple both lines descend from
    pub common: Vec<String>,
    /// A → … → common ancestor → … → B (in-law paths include the spouse step)
    pub path: Vec<String>,
    pub links: Vec<Link>,
    /// generations from A (or A's spouse) up to the common ancestor, and from it down to B (or B's spouse)
    pub up: usize,
    pub down: usize,
    /// collateral relation through only one of the ancestor couple
    pub half: bool,
    /// a second, unrelated ancestor couple gives the same relation (double cousins)
    pub double: bool,
    /// other blood relationships, closest first
    pub others: Vec<KinAlt>,
}

struct Group<'a> {
    members: Vec<&'a str>,
    up: usize,
    down: usize,
    half: bool,
}

struct Blood<'a> {
    groups: Vec<Group<'a>>,
    path: Vec<String>,
    links: Vec<Link>,
}

fn gender(sex: Sex) -> &'static str {
    match sex {
        Sex::Male => "m",
        Sex::Female => "f",
        Sex::Unknown => "u",
    }
}

/// Localized name of B for `up`/`down` generations through the common ancestor.
fn blood_label(up: usize, down: usize, half: bool, double: bool, affinal: bool, sex: Sex) -> String {
    let g = gender(sex);
    let rel = |stem: &str| i18n::tr(&format!("core.rel.{stem}.{g}"), &[]);
    let kin_with = |stem: &str, args: &[(&str, &dyn std::fmt::Display)]| i18n::tr(&format!("core.kin.{stem}.{g}"), args);
    let kin = |stem: &str| kin_with(stem, &[]);
    // «5.º abuelo» (es counts from the grandparent), «4× great-grandfather» (en counts greats)
    let offset: i64 = i18n::t!("core.kin.gen_offset").trim().parse().unwrap_or(1);
    let nth = |stem: &str, gens: usize| kin_with(stem, &[("k", &(gens as i64 - offset))]);
    let mut removed = 0;
    let mut label = match (up, down) {
        (0, 0) => i18n::t!("core.kin.same"),
        (1, 0) => rel("parent"),
        (2, 0) => rel("grandparent"),
        (3, 0) => rel("great_grandparent"),
        (4, 0) => rel("great2_grandparent"),
        (5, 0) => kin("great3_grandparent"),
        (m, 0) => nth("ancestor_n", m),
        (0, 1) => rel("child"),
        (0, 2) => rel("grandchild"),
        (0, 3) => rel("great_grandchild"),
        (0, 4) => rel("great2_grandchild"),
        (0, 5) => kin("great3_grandchild"),
        (0, n) => nth("descendant_n", n),
        (1, 1) if half => kin("half_sibling"),
        (1, 1) => rel("sibling"),
        (2, 1) => rel("uncle"),
        (3, 1) => kin("grand_uncle"),
        (4, 1) => kin("great_grand_uncle"),
        (_, 1) => kin("collateral"),
        (1, 2) => rel("nephew"),
        (1, 3) => kin("grand_nephew"),
        (1, 4) => kin("great_grand_nephew"),
        (1, _) => kin("collateral"),
        (m, n) => {
            let degree = m.min(n) - 1;
            removed = m.abs_diff(n);
            if degree <= 6 { kin(&format!("cousin.{degree}")) } else { kin_with("cousin_n", &[("d", &degree)]) }
        }
    };
    if double {
        label = kin_with("double", &[("rel", &label)]);
    }
    if affinal {
        label = kin_with("affinal", &[("rel", &label)]);
    }
    if removed > 0 {
        label = i18n::tn!("core.kin.removed", removed, rel = label);
    }
    label
}

impl Tree {
    /// Every ancestor of `start` (itself included) → (generations up, the child it was reached from).
    fn ancestry(&self, start: &str) -> HashMap<&str, (usize, Option<&str>)> {
        let mut out = HashMap::new();
        let Some((k, _)) = self.people.get_key_value(start) else { return out };
        out.insert(k.as_str(), (0, None));
        let mut queue = VecDeque::from([k.as_str()]);
        while let Some(id) = queue.pop_front() {
            let d = out[id].0;
            for p in self.parents(id) {
                if !out.contains_key(p.id.as_str()) {
                    out.insert(p.id.as_str(), (d + 1, Some(id)));
                    queue.push_back(p.id.as_str());
                }
            }
        }
        out
    }

    fn children_of(&self, id: &str) -> impl Iterator<Item = &str> {
        self.person(id).into_iter().flat_map(|p| p.fams.iter()).filter_map(|f| self.family(f)).flat_map(|f| f.children.iter().map(String::as_str))
    }

    fn partners_of(&self, id: &str) -> Vec<&str> {
        let Some(p) = self.person(id) else { return vec![] };
        p.fams.iter().filter_map(|f| self.family(f)).filter_map(|f| f.other_partner(id)).map(String::as_str).filter(|x| self.person(x).is_some()).collect()
    }

    fn blood(&self, a: &str, b: &str) -> Option<Blood<'_>> {
        let (anc_a, anc_b) = (self.ancestry(a), self.ancestry(b));
        let common = |x: &str| anc_a.contains_key(x) && anc_b.contains_key(x);
        let rank: HashMap<&str, usize> = self.order.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();
        let mut lcas: Vec<&str> = anc_a.keys().copied().filter(|c| anc_b.contains_key(c) && !self.children_of(c).any(common)).collect();
        if lcas.is_empty() {
            return None;
        }
        lcas.sort_by_key(|c| (anc_a[c].0 + anc_b[c].0, rank.get(c).copied().unwrap_or(usize::MAX)));
        let mut used: HashSet<&str> = HashSet::new();
        let mut groups = Vec::new();
        for &c in &lcas {
            if !used.insert(c) {
                continue;
            }
            let ((up, ca), (down, cb)) = (anc_a[c], anc_b[c]);
            let mut members = vec![c];
            let mut half = false;
            if let (Some(ca), Some(cb)) = (ca, cb) {
                let fam = self.person(c).into_iter().flat_map(|p| p.fams.iter()).filter_map(|f| self.family(f)).find(|f| f.children.iter().any(|x| x == ca) && f.children.iter().any(|x| x == cb));
                match fam {
                    None => half = true,
                    Some(f) => {
                        if let Some(p) = f.other_partner(c).map(String::as_str) {
                            if lcas.contains(&p) && anc_a[p].0 == up && anc_b[p].0 == down && used.insert(p) {
                                members.push(p);
                            }
                        }
                    }
                }
            }
            groups.push(Group { members, up, down, half });
        }
        groups.sort_by_key(|g| (g.up + g.down, g.half));
        let c = groups[0].members[0];
        let chain = |anc: &HashMap<&str, (usize, Option<&str>)>| {
            let mut v = vec![c.to_string()];
            let mut cur = c;
            while let Some(next) = anc[cur].1 {
                v.push(next.to_string());
                cur = next;
            }
            v
        };
        let mut path = chain(&anc_a);
        path.reverse();
        path.extend(chain(&anc_b).into_iter().skip(1));
        let links = std::iter::repeat_n(Link::Parent, groups[0].up).chain(std::iter::repeat_n(Link::Child, groups[0].down)).collect();
        Some(Blood { groups, path, links })
    }

    /// How `b` is related to `a` («primo segundo», «suegra», «tía abuela»…). None if either id is
    /// unknown; `KinKind::None` when nothing links them within one marriage.
    pub fn kinship(&self, a: &str, b: &str) -> Option<Kinship> {
        self.person(a)?;
        let sex = self.person(b)?.sex;
        let mut k = Kinship {
            from: a.to_string(),
            to: b.to_string(),
            kind: KinKind::None,
            label: i18n::t!("core.kin.unrelated"),
            common: vec![],
            path: vec![],
            links: vec![],
            up: 0,
            down: 0,
            half: false,
            double: false,
            others: vec![],
        };
        if a == b {
            k.kind = KinKind::Same;
            k.label = i18n::t!("core.kin.same");
            k.path = vec![a.to_string()];
            return Some(k);
        }
        if let Some(bl) = self.blood(a, b) {
            let p = &bl.groups[0];
            let double = bl.groups[1..].iter().any(|g| g.up == p.up && g.down == p.down && !g.half && !p.half && p.up > 0 && p.down > 0);
            k.kind = KinKind::Blood;
            k.label = blood_label(p.up, p.down, p.half, double, false, sex);
            k.common = p.members.iter().map(|s| s.to_string()).collect();
            (k.up, k.down, k.half, k.double) = (p.up, p.down, p.half, double);
            k.others = bl.groups[1..].iter().map(|g| KinAlt { label: blood_label(g.up, g.down, g.half, false, false, sex), common: g.members.iter().map(|s| s.to_string()).collect(), up: g.up, down: g.down }).collect();
            k.path = bl.path;
            k.links = bl.links;
            return Some(k);
        }
        if let Some(m) = self.affinal(a, b, sex) {
            return Some(Kinship { from: a.to_string(), to: b.to_string(), ..m });
        }
        Some(k)
    }

    /// Closest relation through one partnership: B is a blood relative of A's partner, or the
    /// partner of A's blood relative.
    fn affinal(&self, a: &str, b: &str, sex: Sex) -> Option<Kinship> {
        let mut best: Option<Kinship> = None;
        let mut consider = |k: Kinship| {
            if best.as_ref().is_none_or(|x| k.path.len() < x.path.len()) {
                best = Some(k);
            }
        };
        let base = |label: String, path: Vec<String>, links: Vec<Link>| Kinship {
            from: String::new(),
            to: String::new(),
            kind: KinKind::Marriage,
            label,
            common: vec![],
            path,
            links,
            up: 0,
            down: 0,
            half: false,
            double: false,
            others: vec![],
        };
        let g = gender(sex);
        let rel = |stem: &str| i18n::tr(&format!("core.rel.{stem}.{g}"), &[]);
        let kin = |stem: &str| i18n::tr(&format!("core.kin.{stem}.{g}"), &[]);
        for s in self.partners_of(a) {
            if s == b {
                consider(base(self.rel_label("spouse", a, b, sex), vec![a.into(), b.into()], vec![Link::Spouse]));
                continue;
            }
            let Some(bl) = self.blood(s, b) else { continue };
            let g = &bl.groups[0];
            let label = match (g.up, g.down) {
                (1, 0) => rel("parent_in_law"),
                (1, 1) => rel("sibling_in_law"),
                (0, 1) => kin("step_child"),
                (m, n) => blood_label(m, n, g.half, false, true, sex),
            };
            let mut k = base(label, std::iter::once(a.to_string()).chain(bl.path).collect(), std::iter::once(Link::Spouse).chain(bl.links).collect());
            (k.up, k.down, k.half, k.common) = (g.up, g.down, g.half, g.members.iter().map(|s| s.to_string()).collect());
            consider(k);
        }
        for r in self.partners_of(b) {
            if r == a {
                continue;
            }
            let Some(bl) = self.blood(a, r) else { continue };
            let g = &bl.groups[0];
            let label = match (g.up, g.down) {
                (0, 1) => rel("child_in_law"),
                (1, 1) => rel("sibling_in_law"),
                (1, 0) => kin("step_parent"),
                (m, n) => blood_label(m, n, g.half, false, true, sex),
            };
            let mut path = bl.path;
            path.push(b.to_string());
            let mut links = bl.links;
            links.push(Link::Spouse);
            let mut k = base(label, path, links);
            (k.up, k.down, k.half, k.common) = (g.up, g.down, g.half, g.members.iter().map(|s| s.to_string()).collect());
            consider(k);
        }
        best
    }
}

impl Kinship {
    /// «Bruno es primo segundo de Ana» / «No se encontró parentesco entre Ana y Bruno».
    pub fn sentence(&self, tree: &Tree) -> String {
        let name = |id: &str| tree.person(id).map(|p| p.name()).unwrap_or_default();
        let (a, b) = (name(&self.from), name(&self.to));
        match self.kind {
            KinKind::None => i18n::t!("core.kin.none", a = a, b = b),
            KinKind::Same => i18n::t!("core.kin.same_sentence", a = a),
            _ => i18n::tv!("core.kin.sentence", a, a = a, b = b, rel = self.label),
        }
    }

    /// Label of the step `path[i] → path[i + 1]`: what `path[i + 1]` is to `path[i]` («madre», «hijo», «esposa»).
    pub fn link_label(&self, tree: &Tree, i: usize) -> String {
        let (Some(link), Some(from), Some(to)) = (self.links.get(i), self.path.get(i), self.path.get(i + 1)) else { return String::new() };
        let sex = tree.person(to).map(|p| p.sex).unwrap_or_default();
        let stem = match link {
            Link::Parent => "parent",
            Link::Child => "child",
            Link::Spouse => return tree.rel_label("spouse", from, to, sex),
        };
        i18n::tr(&format!("core.rel.{stem}.{}", gender(sex)), &[])
    }

    /// Index in `path` of the common ancestor (top of the chain); 0 for a pure descent.
    pub fn apex(&self) -> usize {
        self.links.iter().take_while(|l| **l != Link::Child).filter(|l| **l == Link::Parent).count() + usize::from(self.links.first() == Some(&Link::Spouse))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builder for synthetic trees (made-up names, surname «Prueba»).
    struct T {
        t: Tree,
    }

    impl T {
        fn p(&mut self, given: &str, sex: Sex) -> String {
            self.t.add_person(given, "Prueba", sex)
        }
        fn couple(&mut self, a: &str, b: &str, kids: &[&str]) -> String {
            let f = self.t.add_partner_link(a, b).unwrap();
            for k in kids {
                self.t.add_child_link(a, k, Some(&f)).unwrap();
            }
            f
        }
        fn single(&mut self, parent: &str, kids: &[&str]) {
            let f = self.t.add_child_link(parent, kids[0], None).unwrap();
            for k in &kids[1..] {
                self.t.add_child_link(parent, k, Some(&f)).unwrap();
            }
        }
        fn k(&self, a: &str, b: &str) -> Kinship {
            self.t.kinship(a, b).unwrap()
        }
        fn label(&self, a: &str, b: &str) -> String {
            self.k(a, b).label
        }
    }

    /// Abel+Berta → Elio, Hebe. Elio+Fina → Ivo, Kira; Elio+Olga → Noel (half-sibling).
    /// Gil+Hebe → Juno (sex unknown), Pia. Kira+Luis → Mar → Sol → Tito → Ugo → Zoe (single
    /// parents). Pia → Rui → Vito. Ana and Bea are unrelated to everyone.
    fn sample() -> (T, HashMap<&'static str, String>) {
        let mut s = T { t: Tree::new_empty() };
        let mut ids = HashMap::new();
        use Sex::*;
        for (k, sex) in [
            ("abel", Male), ("berta", Female), ("elio", Male), ("hebe", Female), ("fina", Female), ("olga", Female), ("gil", Male),
            ("ivo", Male), ("kira", Female), ("noel", Male), ("juno", Unknown), ("pia", Female), ("rui", Male), ("mar", Female),
            ("luis", Male), ("sol", Female), ("tito", Male), ("ugo", Male), ("vito", Male), ("zoe", Female), ("ana", Female), ("bea", Female),
        ] {
            let given = k[..1].to_uppercase() + &k[1..];
            ids.insert(k, s.p(&given, sex));
        }
        let i = |k: &str| ids[k].clone();
        s.couple(&i("abel"), &i("berta"), &[&i("elio"), &i("hebe")]);
        s.couple(&i("elio"), &i("fina"), &[&i("ivo"), &i("kira")]);
        s.couple(&i("elio"), &i("olga"), &[&i("noel")]); // half-siblings of ivo/kira
        s.couple(&i("gil"), &i("hebe"), &[&i("juno"), &i("pia")]);
        s.couple(&i("kira"), &i("luis"), &[&i("mar")]);
        s.single(&i("pia"), &[&i("rui")]);
        s.single(&i("mar"), &[&i("sol")]);
        s.single(&i("sol"), &[&i("tito")]);
        s.single(&i("tito"), &[&i("ugo")]);
        s.single(&i("rui"), &[&i("vito")]);
        s.single(&i("ugo"), &[&i("zoe")]);
        (s, ids)
    }

    #[test]
    fn blood_structure_and_paths() {
        let (s, id) = sample();
        let i = |k: &str| id[k].clone();
        let k = s.k(&i("ivo"), &i("juno"));
        assert_eq!((k.kind, k.up, k.down, k.half), (KinKind::Blood, 2, 2, false));
        let mut common = k.common.clone();
        common.sort();
        let mut exp = vec![i("abel"), i("berta")];
        exp.sort();
        assert_eq!(common, exp);
        assert_eq!(k.path, vec![i("ivo"), i("elio"), i("abel"), i("hebe"), i("juno")]);
        assert_eq!(k.links, vec![Link::Parent, Link::Parent, Link::Child, Link::Child]);
        assert_eq!(k.apex(), 2);
        // half-siblings share only Elio
        let h = s.k(&i("ivo"), &i("noel"));
        assert!(h.half);
        assert_eq!(h.common, vec![i("elio")]);
        let full = s.k(&i("ivo"), &i("kira"));
        assert!(!full.half && full.common.len() == 2);
        // lineal: zoe is 7 generations below abel
        let z = s.k(&i("abel"), &i("zoe"));
        assert_eq!((z.up, z.down, z.path.len()), (0, 7, 8));
        assert_eq!(z.common, vec![i("abel")]);
        assert_eq!(z.apex(), 0);
        // unrelated / unknown ids
        assert_eq!(s.k(&i("ana"), &i("ivo")).kind, KinKind::None);
        assert!(s.t.kinship("NOPE", &i("ivo")).is_none());
        assert_eq!(s.k(&i("ivo"), &i("ivo")).kind, KinKind::Same);
    }

    #[test]
    fn double_cousins_and_collapse() {
        let mut s = T { t: Tree::new_empty() };
        use Sex::*;
        let [a1, a2, b1, b2] = [s.p("Aro", Male), s.p("Ada", Female), s.p("Bal", Male), s.p("Bea", Female)];
        let [x1, x2, y1, y2] = [s.p("Xan", Male), s.p("Xia", Female), s.p("Yago", Male), s.p("Yara", Female)];
        // two brothers of family A marry two sisters of family B
        s.couple(&a1, &a2, &[&x1, &y1]);
        s.couple(&b1, &b2, &[&x2, &y2]);
        let [c1, c2] = [s.p("Cai", Male), s.p("Cora", Female)];
        s.couple(&x1, &x2, &[&c1]);
        s.couple(&y1, &y2, &[&c2]);
        let k = s.k(&c1, &c2);
        assert!(k.double);
        assert_eq!(k.others.len(), 1);
        assert_eq!((k.others[0].up, k.others[0].down), (2, 2));
        // first cousins Cai + Cora have a child: their grandparents are reached two ways
        let kid = s.p("Dino", Male);
        s.couple(&c1, &c2, &[&kid]);
        let k = s.k(&kid, &c1);
        assert_eq!((k.up, k.down), (1, 0));
        assert!(k.others.is_empty());
    }

    #[test]
    fn in_laws() {
        let (mut s, id) = sample();
        let i = |k: &str| id[k].clone();
        let quim = s.p("Quim", Sex::Male);
        s.couple(&quim, &i("pia"), &[]);
        let k = s.k(&i("gil"), &quim);
        assert_eq!(k.kind, KinKind::Marriage);
        assert_eq!(k.path, vec![i("gil"), i("pia"), quim.clone()]);
        assert_eq!(k.links, vec![Link::Child, Link::Spouse]);
        // Rui is Pia's son (single-parent family), so Quim is his stepfather
        let k = s.k(&i("rui"), &quim);
        assert_eq!((k.kind, k.links.clone()), (KinKind::Marriage, vec![Link::Parent, Link::Spouse]));
        assert_eq!(s.k(&quim, &i("rui")).links, vec![Link::Spouse, Link::Child]);
        assert_eq!(s.k(&i("ana"), &quim).kind, KinKind::None);
    }

    /// All text assertions in one test: the locale is global.
    #[test]
    fn localized_labels() {
        let (mut s, id) = sample();
        let i = |k: &str| id[k].clone();
        let quim = s.p("Quim", Sex::Male);
        s.couple(&quim, &i("pia"), &[]);
        let _l = crate::locale_lock();
        i18n::set_locale("es");
        let l = |a: &str, b: &str| s.label(&i(a), &i(b));
        assert_eq!(l("ivo", "elio"), "padre");
        assert_eq!(l("ivo", "abel"), "abuelo");
        assert_eq!(l("mar", "abel"), "bisabuelo");
        assert_eq!(l("sol", "berta"), "tatarabuela");
        assert_eq!(l("tito", "abel"), "trastatarabuelo");
        assert_eq!(l("ugo", "berta"), "5.ª abuela");
        assert_eq!(l("zoe", "abel"), "6.º abuelo");
        assert_eq!(l("abel", "tito"), "trastataranieto");
        assert_eq!(l("abel", "zoe"), "6.ª nieta");
        assert_eq!(l("ivo", "kira"), "hermana");
        assert_eq!(l("kira", "noel"), "medio hermano");
        assert_eq!(l("noel", "kira"), "media hermana");
        assert_eq!(l("ivo", "hebe"), "tía");
        assert_eq!(l("mar", "hebe"), "tía abuela");
        assert_eq!(l("sol", "hebe"), "tía bisabuela");
        assert_eq!(l("hebe", "ivo"), "sobrino");
        assert_eq!(l("hebe", "mar"), "sobrina nieta");
        assert_eq!(l("hebe", "sol"), "sobrina bisnieta");
        assert_eq!(l("ivo", "juno"), "primo/a hermano/a");
        assert_eq!(l("ivo", "pia"), "prima hermana");
        assert_eq!(l("mar", "rui"), "primo segundo");
        assert_eq!(l("sol", "vito"), "primo tercero");
        assert_eq!(l("mar", "pia"), "prima hermana, con una generación de diferencia");
        assert_eq!(l("tito", "vito"), "primo tercero, con una generación de diferencia");
        assert_eq!(l("ugo", "vito"), "primo tercero, con 2 generaciones de diferencia");
        // two marriages apart (Hebe's husband and her brother's wife) is out of reach
        assert_eq!(s.k(&i("gil"), &i("fina")).kind, KinKind::None);
        assert_eq!(s.label(&i("gil"), &quim), "yerno");
        assert_eq!(s.label(&quim, &i("gil")), "suegro");
        assert_eq!(s.label(&quim, &i("rui")), "hijastro");
        assert_eq!(s.label(&i("rui"), &quim), "padrastro");
        assert_eq!(s.label(&quim, &i("juno")), "cuñado/a");
        assert_eq!(l("luis", "ivo"), "cuñado");
        assert_eq!(l("luis", "hebe"), "tía política");
        assert_eq!(s.label(&quim, &i("ivo")), "primo hermano político");
        assert_eq!(l("ana", "ivo"), "sin parentesco");
        let k = s.k(&i("mar"), &i("rui"));
        assert_eq!(k.sentence(&s.t), "Rui Prueba es primo segundo de Mar Prueba");
        assert_eq!(k.link_label(&s.t, 0), "madre");
        assert_eq!(k.link_label(&s.t, 4), "hija");
        assert_eq!(s.k(&i("ana"), &i("ivo")).sentence(&s.t), "No se encontró parentesco entre Ana Prueba y Ivo Prueba");
        assert_eq!(s.k(&i("gil"), &i("hebe")).link_label(&s.t, 0), "pareja");

        i18n::set_locale("en");
        assert_eq!(l("ivo", "pia"), "first cousin");
        assert_eq!(l("mar", "pia"), "first cousin once removed");
        assert_eq!(l("ugo", "vito"), "third cousin 2 times removed");
        assert_eq!(l("tito", "abel"), "great-great-great-grandfather");
        assert_eq!(l("ugo", "berta"), "4× great-grandmother");
        assert_eq!(l("kira", "noel"), "half-brother");
        assert_eq!(l("mar", "hebe"), "great-aunt");
        assert_eq!(l("luis", "hebe"), "aunt-in-law");
        assert_eq!(s.label(&quim, &i("rui")), "stepson");
        // labels are built in the current language: recompute after switching
        assert_eq!(s.k(&i("mar"), &i("rui")).sentence(&s.t), "Rui Prueba is Mar Prueba's second cousin");

        i18n::set_locale("fr");
        assert_eq!(l("ivo", "pia"), "cousine germaine");
        assert_eq!(l("mar", "rui"), "cousin issu de germain");
        assert_eq!(l("noel", "kira"), "demi-sœur");
        assert_eq!(l("ugo", "berta"), "aïeule de la 6e génération");
        assert_eq!(s.k(&i("ivo"), &i("elio")).sentence(&s.t), "Elio Prueba est père d'Ivo Prueba");

        i18n::set_locale("pt");
        assert_eq!(l("ivo", "pia"), "prima-irmã");
        assert_eq!(l("mar", "rui"), "primo em segundo grau");
        assert_eq!(l("tito", "abel"), "tetravô");
        assert_eq!(l("noel", "kira"), "meia-irmã");
        assert_eq!(l("mar", "hebe"), "tia-avó");
        assert_eq!(l("luis", "hebe"), "tia por afinidade");
        i18n::set_locale("es");
    }
}
