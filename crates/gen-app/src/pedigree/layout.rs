//! Pure geometry for the tree canvases: tidy layout (pedigree and descendants, both flowing
//! left → right), connector curves, viewport (pan/zoom) math and the minimap transform.
//! World units are unscaled pixels; the focus card is centred vertically on y = 0 at x = 0.
use gedcom::Tree;

pub const CARD_W: f32 = 220.;
pub const CARD_H: f32 = 64.;
pub const FOCUS_H: f32 = 88.;
pub const STEP: f32 = crate::theme::size::GEN_STEP;
pub const VGAP: f32 = crate::theme::size::MIN_VGAP;
/// room above the first child of a union group in the descendants view, for its label
pub const UNION_LABEL_H: f32 = 22.;
/// extra space between two union groups of the same parent
pub const UNION_GAP: f32 = 12.;
pub const ZOOM_MIN: f32 = 0.25;
pub const ZOOM_MAX: f32 = 2.0;
/// hard cap so a huge view (or a pedigree with heavy implex) stays interactive
pub const MAX_NODES: usize = 3000;
/// generations shown (focus column included): stepper range and step
pub const GEN_MIN: usize = 2;
pub const GEN_MAX: usize = 20;
pub const GEN_STEP: usize = 2;
pub const GEN_DEFAULT: usize = 4;

/// Generations after one stepper click (`up` = more), kept within `GEN_MIN..=GEN_MAX`.
pub fn step_generations(g: usize, up: bool) -> usize {
    let g = g.clamp(GEN_MIN, GEN_MAX);
    if up { (g + GEN_STEP).min(GEN_MAX) } else { g.saturating_sub(GEN_STEP).max(GEN_MIN) }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V2 {
    pub x: f32,
    pub y: f32,
}

pub const fn v2(x: f32, y: f32) -> V2 {
    V2 { x, y }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub min: V2,
    pub max: V2,
}

impl Rect {
    pub fn w(&self) -> f32 {
        self.max.x - self.min.x
    }
    pub fn h(&self) -> f32 {
        self.max.y - self.min.y
    }
    pub fn center(&self) -> V2 {
        v2((self.min.x + self.max.x) / 2., (self.min.y + self.max.y) / 2.)
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.min.x < o.max.x && o.min.x < self.max.x && self.min.y < o.max.y && o.min.y < self.max.y
    }
    fn union(self, o: Rect) -> Rect {
        Rect { min: v2(self.min.x.min(o.min.x), self.min.y.min(o.min.y)), max: v2(self.max.x.max(o.max.x), self.max.y.max(o.max.y)) }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    Person(String),
    /// dashed «+ Añadir padre/madre» for `child`
    AddParent { child: String, father: bool },
    /// dashed «+ Añadir hijo» for a focus without children (pedigree children column)
    AddChild { parent: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LNode {
    pub slot: Slot,
    pub depth: usize,
    /// the node this one hangs from (towards the focus)
    pub parent: Option<usize>,
    /// top-left corner
    pub pos: V2,
    pub w: f32,
    pub h: f32,
    /// descendants view: first child of a union group carries the group's label (other partner)
    pub union_label: Option<Option<String>>,
    /// pedigree: one of the focus's children, in the column left of the focus (x = −STEP); its
    /// connector runs from its right edge to the focus's left edge
    pub child_col: bool,
}

impl LNode {
    pub fn rect(&self) -> Rect {
        Rect { min: self.pos, max: v2(self.pos.x + self.w, self.pos.y + self.h) }
    }
    pub fn person(&self) -> Option<&str> {
        match &self.slot {
            Slot::Person(id) => Some(id),
            Slot::AddParent { .. } | Slot::AddChild { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub nodes: Vec<LNode>,
    /// number of columns (focus included)
    pub columns: usize,
    /// y of the column headers (world)
    pub header_y: f32,
    pub truncated: bool,
    /// pedigree: the children column left of the focus is present
    pub child_col: bool,
}

/// Spec of the tree before positions are known.
struct Spec {
    slot: Slot,
    depth: usize,
    parent: Option<usize>,
    h: f32,
    extra_top: f32,
    gap_before: f32,
    children: Vec<usize>,
    union_label: Option<Option<String>>,
}

impl Spec {
    fn new(slot: Slot, depth: usize, parent: Option<usize>) -> Spec {
        Spec { slot, depth, parent, h: if depth == 0 { FOCUS_H } else { CARD_H }, extra_top: 0., gap_before: 0., children: vec![], union_label: None }
    }
}

/// Ancestors of `focus` for `generations` columns (focus included). Every shown person outside
/// the last column gets both parent slots: the real parent or an empty «add» slot. Only known
/// people are expanded, so the size follows the data, not 2^generations (capped at `MAX_NODES`
/// for trees with heavy implex). The focus's children (all unions, in order) are appended as a
/// column left of the focus, or a single «+ Añadir hijo» slot when there are none.
pub fn pedigree(tree: &Tree, focus: &str, generations: usize) -> Layout {
    let mut specs: Vec<Spec> = Vec::new();
    if tree.person(focus).is_none() {
        return Layout::default();
    }
    specs.push(Spec::new(Slot::Person(focus.to_string()), 0, None));
    let mut truncated = false;
    let mut i = 0;
    while i < specs.len() {
        let (depth, id) = match &specs[i].slot {
            Slot::Person(id) => (specs[i].depth, id.clone()),
            _ => {
                i += 1;
                continue;
            }
        };
        if depth + 1 < generations {
            if specs.len() + 2 > MAX_NODES {
                truncated = true;
                break;
            }
            let (f, m) = tree.father_mother(&id);
            for (p, father) in [(f, true), (m, false)] {
                let slot = match p {
                    Some(p) => Slot::Person(p.id.clone()),
                    None => Slot::AddParent { child: id.clone(), father },
                };
                specs.push(Spec::new(slot, depth + 1, Some(i)));
                let n = specs.len() - 1;
                specs[i].children.push(n);
            }
        }
        i += 1;
    }
    let mut l = finish(specs);
    l.truncated = truncated;
    add_children_column(&mut l, tree, focus);
    l
}

/// Children of the focus stacked at x = −STEP, centred on the focus (y = 0).
fn add_children_column(l: &mut Layout, tree: &Tree, focus: &str) {
    let mut kids: Vec<Slot> = Vec::new();
    for (_, _, ks) in tree.unions(focus) {
        for k in ks {
            let s = Slot::Person(k.id.clone());
            if !kids.contains(&s) {
                kids.push(s);
            }
        }
    }
    if kids.is_empty() {
        kids.push(Slot::AddChild { parent: focus.to_string() });
    }
    let n = kids.len() as f32;
    let top = -(n * CARD_H + (n - 1.) * VGAP) / 2.;
    let mut y = top;
    for slot in kids {
        l.nodes.push(LNode { slot, depth: 1, parent: Some(0), pos: v2(-STEP, y), w: CARD_W, h: CARD_H, union_label: None, child_col: true });
        y += CARD_H + VGAP;
    }
    l.header_y = l.header_y.min(top - 36.);
    l.child_col = true;
}

/// Descendants of `focus` for `generations` columns, children grouped by union.
pub fn descendants(tree: &Tree, focus: &str, generations: usize) -> Layout {
    let mut specs: Vec<Spec> = Vec::new();
    if tree.person(focus).is_none() {
        return Layout::default();
    }
    specs.push(Spec::new(Slot::Person(focus.to_string()), 0, None));
    let mut truncated = false;
    let mut i = 0;
    while i < specs.len() {
        let depth = specs[i].depth;
        let Slot::Person(id) = specs[i].slot.clone() else { unreachable!() };
        if depth + 1 < generations {
            let unions = tree.unions(&id);
            let labelled = unions.len() > 1 || unions.iter().any(|(_, p, _)| p.is_some());
            for (ui, (_, partner, kids)) in unions.iter().enumerate() {
                for (ki, k) in kids.iter().enumerate() {
                    if specs.len() >= MAX_NODES {
                        truncated = true;
                        break;
                    }
                    let mut s = Spec::new(Slot::Person(k.id.clone()), depth + 1, Some(i));
                    if ki == 0 && labelled {
                        s.extra_top = UNION_LABEL_H;
                        s.union_label = Some(partner.map(|p| p.id.clone()));
                        if ui > 0 {
                            s.gap_before = UNION_GAP;
                        }
                    }
                    specs.push(s);
                    let n = specs.len() - 1;
                    specs[i].children.push(n);
                }
            }
        }
        i += 1;
    }
    let mut l = finish(specs);
    l.truncated = truncated;
    l
}

fn finish(specs: Vec<Spec>) -> Layout {
    let mut rel = vec![0f32; specs.len()];
    tidy(&specs, 0, &mut rel);
    let mut centre = vec![0f32; specs.len()];
    // specs are in BFS order, so a parent is always positioned before its children
    for i in 1..specs.len() {
        centre[i] = centre[specs[i].parent.unwrap()] + rel[i];
    }
    let nodes: Vec<LNode> = specs
        .into_iter()
        .zip(centre)
        .map(|(s, cy)| LNode { pos: v2(s.depth as f32 * STEP, cy - s.h / 2.), w: CARD_W, h: s.h, slot: s.slot, depth: s.depth, parent: s.parent, union_label: s.union_label, child_col: false })
        .collect();
    let columns = nodes.iter().map(|n| n.depth + 1).max().unwrap_or(0);
    let top = nodes.iter().map(|n| n.pos.y - if n.union_label.is_some() { UNION_LABEL_H } else { 0. }).fold(f32::INFINITY, f32::min);
    Layout { nodes, columns, header_y: if top.is_finite() { top - 36. } else { -80. }, truncated: false, child_col: false }
}

/// Reingold–Tilford style: each subtree returns its contour (top, bottom) per depth relative to
/// its own centre; siblings are pushed apart until every shared depth keeps `VGAP`, then the
/// parent is centred between its first and last child.
fn tidy(specs: &[Spec], n: usize, rel: &mut [f32]) -> Vec<(f32, f32)> {
    let s = &specs[n];
    let own = (-s.h / 2. - s.extra_top, s.h / 2.);
    if s.children.is_empty() {
        return vec![own];
    }
    let mut merged: Vec<(f32, f32)> = Vec::new();
    let mut offs = Vec::with_capacity(s.children.len());
    for (i, &c) in s.children.iter().enumerate() {
        let cc = tidy(specs, c, rel);
        let off = if i == 0 {
            0.
        } else {
            let common = merged.len().min(cc.len());
            (0..common).map(|d| merged[d].1 + VGAP - cc[d].0).fold(f32::MIN, f32::max) + specs[c].gap_before
        };
        for (d, (t, b)) in cc.iter().enumerate() {
            let (t, b) = (t + off, b + off);
            if let Some(m) = merged.get_mut(d) {
                m.0 = m.0.min(t);
                m.1 = m.1.max(b);
            } else {
                merged.push((t, b));
            }
        }
        offs.push(off);
    }
    // centre on the cards themselves (not on the label space above the first one)
    let first_c = offs[0];
    let last_c = *offs.last().unwrap();
    let mid = (first_c + last_c) / 2.;
    for (i, &c) in s.children.iter().enumerate() {
        rel[c] = offs[i] - mid;
    }
    let mut out = vec![own];
    out.extend(merged.into_iter().map(|(t, b)| (t - mid, b - mid)));
    out
}

impl Layout {
    pub fn bounds(&self) -> Rect {
        let mut r: Option<Rect> = None;
        for n in &self.nodes {
            let mut nr = n.rect();
            if n.union_label.is_some() {
                nr.min.y -= UNION_LABEL_H;
            }
            r = Some(r.map_or(nr, |r| r.union(nr)));
        }
        let mut r = r.unwrap_or_default();
        if !self.nodes.is_empty() {
            r.min.y = r.min.y.min(self.header_y);
        }
        r
    }

    pub fn column_x(&self, depth: usize) -> f32 {
        depth as f32 * STEP
    }

    /// Indices from `idx` up to the focus (inclusive both ends).
    pub fn path_to_root(&self, idx: usize) -> Vec<usize> {
        let mut out = vec![idx];
        let mut cur = idx;
        while let Some(p) = self.nodes.get(cur).and_then(|n| n.parent) {
            out.push(p);
            cur = p;
        }
        out
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.person() == Some(id))
    }

    /// Cubic connector between `node` and its parent, always left → right: from the parent's right
    /// edge to `node`'s left edge, or, in the children column, from `node`'s right edge to the
    /// focus's left edge. Start, ctrl1, ctrl2, end.
    pub fn connector(&self, node: usize) -> Option<[V2; 4]> {
        let n = self.nodes.get(node)?;
        let p = &self.nodes[n.parent?];
        let (l, r) = if n.child_col { (n, p) } else { (p, n) };
        let a = v2(l.pos.x + l.w, l.pos.y + l.h / 2.);
        let b = v2(r.pos.x, r.pos.y + r.h / 2.);
        let mx = (a.x + b.x) / 2.;
        Some([a, v2(mx, a.y), v2(mx, b.y), b])
    }
}

/// Pan/zoom: screen = pan + world * zoom (screen relative to the canvas origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub pan: V2,
    pub zoom: f32,
}

impl Default for View {
    fn default() -> Self {
        View { pan: V2::default(), zoom: 1. }
    }
}

impl View {
    pub fn to_screen(self, w: V2) -> V2 {
        v2(self.pan.x + w.x * self.zoom, self.pan.y + w.y * self.zoom)
    }
    pub fn to_world(self, s: V2) -> V2 {
        v2((s.x - self.pan.x) / self.zoom, (s.y - self.pan.y) / self.zoom)
    }
    /// Zoom keeping the world point under `anchor` (screen) fixed.
    pub fn zoom_around(&mut self, anchor: V2, zoom: f32) {
        let w = self.to_world(anchor);
        self.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        self.pan = v2(anchor.x - w.x * self.zoom, anchor.y - w.y * self.zoom);
    }
    /// Put world point `w` at the centre of a viewport of `size`.
    pub fn centered_on(self, w: V2, size: V2) -> View {
        View { pan: v2(size.x / 2. - w.x * self.zoom, size.y / 2. - w.y * self.zoom), zoom: self.zoom }
    }
    /// Whole `bounds` visible with `margin` px around, never zooming in past 100 %.
    pub fn fit(bounds: Rect, size: V2, margin: f32) -> View {
        let (aw, ah) = ((size.x - 2. * margin).max(1.), (size.y - 2. * margin).max(1.));
        let zoom = (aw / bounds.w().max(1.)).min(ah / bounds.h().max(1.)).clamp(ZOOM_MIN, 1.);
        View { zoom, ..View::default() }.centered_on(bounds.center(), size)
    }
    /// Visible world rectangle for a viewport of `size`.
    pub fn visible(self, size: V2) -> Rect {
        Rect { min: self.to_world(V2::default()), max: self.to_world(size) }
    }
    pub fn lerp(a: View, b: View, t: f32) -> View {
        let l = |x: f32, y: f32| x + (y - x) * t;
        View { pan: v2(l(a.pan.x, b.pan.x), l(a.pan.y, b.pan.y)), zoom: l(a.zoom, b.zoom) }
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t).powi(3)
}

/// World → minimap: (scale, offset) so that `bounds` fits centred in `size`.
pub fn minimap_transform(bounds: Rect, size: V2, pad: f32) -> (f32, V2) {
    let s = ((size.x - 2. * pad) / bounds.w().max(1.)).min((size.y - 2. * pad) / bounds.h().max(1.));
    let off = v2((size.x - bounds.w() * s) / 2. - bounds.min.x * s, (size.y - bounds.h() * s) / 2. - bounds.min.y * s);
    (s, off)
}

#[cfg(test)]
mod tests {
    use super::{CARD_H, CARD_W, FOCUS_H, GEN_MAX, GEN_MIN, Layout, MAX_NODES, Rect, STEP, Slot, VGAP, View, descendants, ease_out_cubic, minimap_transform, pedigree, step_generations, v2};
    use gedcom::{Sex, Tree};

    /// focus F with parents P1/P2; P1 has father G1 only.
    fn small() -> (Tree, String) {
        let mut t = Tree::new_empty();
        let f = t.add_person("F", "X", Sex::Female);
        let p1 = t.add_person("P1", "X", Sex::Male);
        let p2 = t.add_person("P2", "Y", Sex::Female);
        let g1 = t.add_person("G1", "X", Sex::Male);
        t.add_parent_link(&f, &p1).unwrap();
        t.add_parent_link(&f, &p2).unwrap();
        t.add_parent_link(&p1, &g1).unwrap();
        (t, f)
    }

    fn centre_y(l: &Layout, i: usize) -> f32 {
        l.nodes[i].pos.y + l.nodes[i].h / 2.
    }

    #[test]
    fn pedigree_slots_and_columns() {
        let (t, f) = small();
        let l = pedigree(&t, &f, 3);
        assert_eq!(l.columns, 3);
        // focus + 2 parents + 4 grandparent slots + the «add child» slot
        assert_eq!(l.nodes.len(), 8);
        assert_eq!(l.nodes[0].h, FOCUS_H);
        assert_eq!(centre_y(&l, 0), 0.);
        assert_eq!(l.nodes[1].pos.x, STEP);
        assert_eq!(l.nodes[3].pos.x, 2. * STEP);
        let empties = l.nodes.iter().filter(|n| matches!(n.slot, Slot::AddParent { .. })).count();
        assert_eq!(empties, 3);
        assert!(matches!(&l.nodes[4].slot, Slot::AddParent { father: false, .. }));
    }

    #[test]
    fn parents_centred_on_child_with_min_gap() {
        let (t, f) = small();
        let l = pedigree(&t, &f, 4);
        for (i, n) in l.nodes.iter().enumerate() {
            let kids: Vec<usize> = l.nodes.iter().enumerate().filter(|(_, c)| c.parent == Some(i) && !c.child_col).map(|(j, _)| j).collect();
            if kids.len() == 2 {
                let mid = (centre_y(&l, kids[0]) + centre_y(&l, kids[1])) / 2.;
                assert!((mid - (n.pos.y + n.h / 2.)).abs() < 0.01);
                assert!(l.nodes[kids[0]].pos.y < l.nodes[kids[1]].pos.y, "father above mother");
            }
        }
        for d in 0..l.columns {
            let mut col: Vec<_> = l.nodes.iter().filter(|n| n.depth == d && !n.child_col).collect();
            col.sort_by(|a, b| a.pos.y.total_cmp(&b.pos.y));
            for w in col.windows(2) {
                assert!(w[1].pos.y - (w[0].pos.y + w[0].h) >= VGAP - 0.01);
            }
        }
    }

    #[test]
    fn connector_endpoints() {
        let (t, f) = small();
        let l = pedigree(&t, &f, 2);
        let [a, c1, c2, b] = l.connector(1).unwrap();
        assert_eq!(a, v2(CARD_W, 0.));
        assert_eq!(b.x, STEP);
        assert_eq!(b.y, l.nodes[1].pos.y + CARD_H / 2.);
        assert_eq!(c1, v2((CARD_W + STEP) / 2., 0.));
        assert_eq!(c2.y, b.y);
        assert!(l.connector(0).is_none());
        assert_eq!(l.path_to_root(2), vec![2, 0]);
    }

    #[test]
    fn pedigree_children_column() {
        let (mut t, f) = small();
        let l = pedigree(&t, &f, 2);
        let kids: Vec<_> = l.nodes.iter().filter(|n| n.child_col).collect();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].slot, Slot::AddChild { parent: f.clone() });
        assert_eq!(kids[0].pos.x, -STEP);
        assert!(l.child_col);

        // two unions: children in union order, stacked and centred on the focus
        let a = t.add_person("A", "Z", Sex::Male);
        let b = t.add_person("B", "W", Sex::Male);
        let f1 = t.add_partner_link(&f, &a).unwrap();
        let f2 = t.add_partner_link(&f, &b).unwrap();
        let k1 = t.add_person("K1", "Z", Sex::Male);
        let k2 = t.add_person("K2", "Z", Sex::Female);
        let k3 = t.add_person("K3", "W", Sex::Female);
        t.add_child_link(&f, &k1, Some(&f1)).unwrap();
        t.add_child_link(&f, &k2, Some(&f1)).unwrap();
        t.add_child_link(&f, &k3, Some(&f2)).unwrap();
        let l = pedigree(&t, &f, 3);
        let ix: Vec<usize> = (0..l.nodes.len()).filter(|&i| l.nodes[i].child_col).collect();
        let ids: Vec<_> = ix.iter().map(|&i| l.nodes[i].person().unwrap().to_string()).collect();
        assert_eq!(ids, vec![k1.clone(), k2, k3]);
        assert_eq!(l.columns, 3, "the children column is not a generation");
        assert!((centre_y(&l, ix[0]) + centre_y(&l, ix[2])).abs() < 0.01);
        for w in ix.windows(2) {
            let (a, b) = (&l.nodes[w[0]], &l.nodes[w[1]]);
            assert!((b.pos.y - (a.pos.y + a.h) - VGAP).abs() < 0.01);
        }
        assert!(l.header_y < l.nodes[ix[0]].pos.y);
        // ancestors keep their indices; the children come last
        assert_eq!(l.nodes[1].person(), t.father_mother(&f).0.map(|p| p.id.as_str()));
        assert_eq!(l.index_of(&k1), Some(ix[0]));

        // connector: child's right edge → focus's left edge, still left to right
        let [a, c1, c2, b] = l.connector(ix[0]).unwrap();
        assert_eq!(a, v2(-STEP + CARD_W, centre_y(&l, ix[0])));
        assert_eq!(b, v2(0., 0.));
        assert_eq!((c1.x, c1.y), ((a.x + b.x) / 2., a.y));
        assert_eq!((c2.x, c2.y), ((a.x + b.x) / 2., 0.));
        assert_eq!(l.path_to_root(ix[1]), vec![ix[1], 0]);
        let bb = l.bounds();
        assert_eq!(bb.min.x, -STEP);
        assert!(bb.min.y <= l.nodes[ix[0]].pos.y);
    }

    #[test]
    fn generations_stepper() {
        assert_eq!(step_generations(4, true), 6);
        assert_eq!(step_generations(4, false), 2);
        assert_eq!(step_generations(GEN_MIN, false), GEN_MIN);
        assert_eq!(step_generations(GEN_MAX, true), GEN_MAX);
        assert_eq!(step_generations(19, true), GEN_MAX);
        assert_eq!(step_generations(99, false), GEN_MAX - 2);
        let mut g = GEN_MIN;
        let mut seen = vec![g];
        while g < GEN_MAX {
            g = step_generations(g, true);
            seen.push(g);
        }
        assert_eq!(seen, (GEN_MIN..=GEN_MAX).step_by(2).collect::<Vec<_>>());
    }

    #[test]
    fn deep_pedigree_only_expands_known_people() {
        // a straight paternal line of 30 men: 20 generations show 20 people + their empty slots
        let mut t = Tree::new_empty();
        let mut cur = t.add_person("L0", "X", Sex::Male);
        let focus = cur.clone();
        for i in 1..30 {
            let p = t.add_person(&format!("L{i}"), "X", Sex::Male);
            t.add_parent_link(&cur, &p).unwrap();
            cur = p;
        }
        let l = pedigree(&t, &focus, GEN_MAX);
        assert_eq!(l.columns, GEN_MAX);
        let people = l.nodes.iter().filter(|n| n.person().is_some()).count();
        assert_eq!(people, GEN_MAX);
        // 19 empty mothers + 1 «add child»
        assert_eq!(l.nodes.len(), 2 * GEN_MAX);
        assert!(!l.truncated);
    }

    #[test]
    fn implex_pedigree_is_capped() {
        // every generation is one couple, parents of the couple below: each person is shown
        // 2^depth times, so 20 generations would be ~1M cards without the cap
        let mut t = Tree::new_empty();
        let focus = t.add_person("F", "X", Sex::Female);
        let (mut f, mut m) = (t.add_person("A0", "X", Sex::Male), t.add_person("B0", "Y", Sex::Female));
        t.add_parent_link(&focus, &f).unwrap();
        t.add_parent_link(&focus, &m).unwrap();
        for i in 1..GEN_MAX {
            let (nf, nm) = (t.add_person(&format!("A{i}"), "X", Sex::Male), t.add_person(&format!("B{i}"), "Y", Sex::Female));
            for c in [&f, &m] {
                t.add_parent_link(c, &nf).unwrap();
                t.add_parent_link(c, &nm).unwrap();
            }
            (f, m) = (nf, nm);
        }
        let l = pedigree(&t, &focus, GEN_MAX);
        assert!(l.truncated);
        assert!(l.nodes.len() <= MAX_NODES + 1);
    }

    #[test]
    fn descendants_grouped_by_union() {
        let mut t = Tree::new_empty();
        let a = t.add_person("A", "X", Sex::Male);
        let b = t.add_person("B", "Y", Sex::Female);
        let c = t.add_person("C", "Z", Sex::Female);
        let f1 = t.add_partner_link(&a, &b).unwrap();
        let f2 = t.add_partner_link(&a, &c).unwrap();
        for (fam, n) in [(&f1, 2), (&f2, 1)] {
            for i in 0..n {
                let k = t.add_person(&format!("K{i}"), "X", Sex::Male);
                t.add_child_link(&a, &k, Some(fam)).unwrap();
            }
        }
        let l = descendants(&t, &a, 3);
        assert_eq!(l.nodes.len(), 4);
        let labels: Vec<_> = l.nodes.iter().filter_map(|n| n.union_label.clone()).collect();
        assert_eq!(labels, vec![Some(b.clone()), Some(c.clone())]);
        let mid = (centre_y(&l, 1) + centre_y(&l, 3)) / 2.;
        assert!(mid.abs() < 0.01);
    }

    #[test]
    fn sizes_match_theme() {
        use crate::theme::size;
        assert_eq!(CARD_W, f32::from(size::CARD_W));
        assert_eq!(CARD_H, f32::from(size::CARD_H));
        assert_eq!(FOCUS_H, f32::from(size::CARD_FOCUS_H));
    }

    #[test]
    fn zoom_pan_fit() {
        let mut v = View { pan: v2(100., 50.), ..View::default() };
        let anchor = v2(300., 200.);
        let before = v.to_world(anchor);
        v.zoom_around(anchor, 1.5);
        assert!((v.to_world(anchor).x - before.x).abs() < 1e-3 && (v.to_world(anchor).y - before.y).abs() < 1e-3);
        v.zoom_around(anchor, 10.);
        assert_eq!(v.zoom, 2.);
        v.zoom_around(anchor, 0.01);
        assert_eq!(v.zoom, 0.25);

        let c = View::default().centered_on(v2(110., 0.), v2(800., 600.));
        assert_eq!(c.to_screen(v2(110., 0.)), v2(400., 300.));

        let b = Rect { min: v2(0., -500.), max: v2(1000., 500.) };
        let f = View::fit(b, v2(600., 600.), 50.);
        assert!((f.zoom - 0.5).abs() < 1e-4);
        assert_eq!(f.to_screen(b.center()), v2(300., 300.));
        let small = Rect { min: v2(0., 0.), max: v2(10., 10.) };
        assert_eq!(View::fit(small, v2(600., 600.), 0.).zoom, 1.);
    }

    #[test]
    fn easing_and_minimap() {
        assert_eq!(ease_out_cubic(0.), 0.);
        assert_eq!(ease_out_cubic(1.), 1.);
        assert!(ease_out_cubic(0.5) > 0.5);
        let b = Rect { min: v2(-100., -100.), max: v2(100., 100.) };
        let (s, off) = minimap_transform(b, v2(200., 100.), 0.);
        assert_eq!(s, 0.5);
        assert_eq!(v2(b.min.x * s + off.x, b.min.y * s + off.y), v2(50., 0.));
    }
}
