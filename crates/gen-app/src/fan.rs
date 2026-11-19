//! «Abanico»: half-circle ancestor fan. Slot numbering follows `Tree::pedigree` (Ahnentafel,
//! 0 = focus, father before mother), drawn left → right over the top.
use crate::app::GenApp;
use crate::theme::tokens;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use std::f32::consts::PI;

const R0: f32 = 72.;
const RING: f32 = 92.;
/// rings beyond this are slivers (and 2^n wedges), so the fan stops here whatever the stepper says
pub const FAN_MAX: usize = 8;

/// (inner radius, outer radius, start angle, end angle) of Ahnentafel slot `ix` (ix ≥ 1).
pub fn wedge(ix: usize) -> (f32, f32, f32, f32) {
    let g = (usize::BITS - (ix + 1).leading_zeros() - 1) as usize;
    let k = ix + 1 - (1 << g);
    let span = PI / (1 << g) as f32;
    let a0 = PI + k as f32 * span;
    (R0 + (g - 1) as f32 * RING, R0 + g as f32 * RING, a0, a0 + span)
}

pub fn radius(generations: usize) -> f32 {
    R0 + (generations.saturating_sub(1)) as f32 * RING
}

/// Slot under a point relative to the fan centre (y down), if any.
pub fn hit(x: f32, y: f32, generations: usize) -> Option<usize> {
    let r = (x * x + y * y).sqrt();
    if r <= R0 {
        return Some(0);
    }
    if y > 0. || r > radius(generations) {
        return None;
    }
    let g = ((r - R0) / RING) as usize + 1;
    let a = y.atan2(x).rem_euclid(2. * PI);
    let k = (((a - PI) / (PI / (1 << g) as f32)) as usize).min((1 << g) - 1);
    Some((1 << g) - 1 + k)
}

fn polar(r: f32, a: f32) -> (f32, f32) {
    (r * a.cos(), r * a.sin())
}

impl GenApp {
    pub(crate) fn render_fan(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = tokens(cx);
        let Some(focus) = self.focus_id.clone() else { return div().size_full().into_any_element() };
        let gens = self.generations.min(FAN_MAX);
        let clamped = self.generations > FAN_MAX;
        let slots = self.tree.pedigree(&focus, gens as u32);
        let b = self.canvas_bounds.get();
        let (w, h): (f32, f32) = (b.size.width.into(), b.size.height.into());
        let rmax = radius(gens);
        let scale = ((w / 2. - 24.) / rmax).min((h - 90.) / rmax).clamp(0.2, 1.4);
        let centre = (w / 2., h - 40.);
        let sex: Vec<Option<gedcom::Sex>> = slots.iter().map(|s| s.as_deref().and_then(|id| self.tree.person(id)).map(|p| p.sex)).collect();

        let mut labels: Vec<AnyElement> = Vec::new();
        for (ix, slot) in slots.iter().enumerate() {
            let Some(p) = slot.as_deref().and_then(|id| self.tree.person(id)) else { continue };
            let (cx_, cy_, width, text, size_) = if ix == 0 {
                (centre.0, centre.1 - R0 * scale * 0.45, R0 * 2. * scale * 0.9, p.name(), 13.)
            } else {
                let (r0, r1, a0, a1) = wedge(ix);
                let (x, y) = polar((r0 + r1) / 2. * scale, (a0 + a1) / 2.);
                let chord = ((r0 + r1) / 2. * (a1 - a0)).min(r1 - r0) * scale;
                let text = if ix < 7 { p.name() } else { p.given.split_whitespace().next().unwrap_or("").to_string() };
                (centre.0 + x, centre.1 + y, chord.max(24.), text, if ix < 7 { 12. } else { 10. })
            };
            if width < 28. {
                continue;
            }
            labels.push(
                div()
                    .absolute()
                    .left(px(cx_ - width / 2.))
                    .top(px(cy_ - 14.))
                    .w(px(width))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .text_size(px(size_ * scale.clamp(0.8, 1.2)))
                    .line_height(px(13.))
                    .font_weight(if ix == 0 { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
                    .text_color(t.ink)
                    .line_clamp(2)
                    .child(text)
                    .into_any_element(),
            );
        }

        let bounds_cell = self.canvas_bounds.clone();
        let canvas_el = canvas(
            move |b, _, _| bounds_cell.set(b),
            move |b, _, window, _| {
                let o = b.origin;
                let at = |x: f32, y: f32| o + point(px(centre.0 + x * scale), px(centre.1 + y * scale));
                for ix in (1..sex.len()).rev() {
                    let (r0, r1, a0, a1) = wedge(ix);
                    let steps = 16;
                    let mut pts = Vec::with_capacity(2 * steps + 2);
                    for i in 0..=steps {
                        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
                        let (x, y) = polar(r1, a);
                        pts.push(at(x, y));
                    }
                    for i in (0..=steps).rev() {
                        let a = a0 + (a1 - a0) * i as f32 / steps as f32;
                        let (x, y) = polar(r0, a);
                        pts.push(at(x, y));
                    }
                    let fill_c = match sex[ix] {
                        Some(s) => t.sex(s).0,
                        None => t.surface.opacity(0.6),
                    };
                    let mut pb = PathBuilder::fill();
                    pb.add_polygon(&pts, true);
                    if let Ok(path) = pb.build() {
                        window.paint_path(path, fill_c);
                    }
                    let mut st = PathBuilder::stroke(px(1.));
                    if sex[ix].is_none() {
                        st = st.dash_array(&[px(4.), px(4.)]);
                    }
                    st.add_polygon(&pts, true);
                    if let Ok(path) = st.build() {
                        window.paint_path(path, if sex[ix].is_some() { t.border_strong } else { t.slot_dashed });
                    }
                }
                let mut pts = Vec::new();
                for i in 0..=32 {
                    let (x, y) = polar(R0, PI + PI * i as f32 / 32.);
                    pts.push(at(x, y));
                }
                let mut pb = PathBuilder::fill();
                pb.add_polygon(&pts, true);
                if let Ok(path) = pb.build() {
                    window.paint_path(path, t.accent_soft);
                }
                let mut st = PathBuilder::stroke(px(2.));
                st.add_polygon(&pts, true);
                if let Ok(path) = st.build() {
                    window.paint_path(path, t.accent);
                }
            },
        )
        .absolute()
        .inset_0();

        div()
            .id("fan")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.canvas)
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                let l = this.local(e.position());
                let Some(ix) = hit((l.x - centre.0) / scale, (l.y - centre.1) / scale, gens) else { return };
                if let Some(id) = this.tree.pedigree(&focus, gens as u32).get(ix).cloned().flatten() {
                    if e.click_count() >= 2 {
                        this.open_edit(id, window, cx);
                    } else {
                        this.set_focus(id, true, window, cx);
                    }
                }
            }))
            .child(canvas_el)
            .children(labels)
            .child(self.render_top_pills(&t, cx))
            .when(clamped, |d| {
                d.child(div().absolute().left_0().right_0().bottom(px(12.)).flex().justify_center().child(div().text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.fan.clamped", n = FAN_MAX))))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{R0, RING, hit, radius, wedge};
    use std::f32::consts::PI;

    #[test]
    fn wedges_and_hits() {
        assert_eq!(wedge(1), (R0, R0 + RING, PI, 1.5 * PI));
        assert_eq!(wedge(2).2, 1.5 * PI);
        let (r0, r1, a0, a1) = wedge(4);
        assert_eq!((r0, r1), (R0 + RING, R0 + 2. * RING));
        assert!((a1 - a0 - PI / 4.).abs() < 1e-5);
        assert_eq!(hit(0., 0., 4), Some(0));
        // upper-left of ring 1 → father; upper-right → mother
        assert_eq!(hit(-80., -10., 4), Some(1));
        assert_eq!(hit(80., -10., 4), Some(2));
        assert_eq!(hit(0., 50. + R0, 4), None);
        assert_eq!(hit(radius(3) + 1., -1., 3), None);
        for ix in 1..15 {
            let (r0, r1, a0, a1) = wedge(ix);
            let (r, a) = ((r0 + r1) / 2., (a0 + a1) / 2.);
            assert_eq!(hit(r * a.cos(), r * a.sin(), 4), Some(ix));
        }
    }
}
