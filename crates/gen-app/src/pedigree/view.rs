//! Pan/zoom tree canvas (Pedigrí and Descendientes): dotted background and Bézier connectors
//! painted in a `canvas`, cards as absolutely positioned divs scaled by the zoom, plus the
//! floating breadcrumb / generations / zoom / minimap overlays.
use super::layout::{self, Layout, LNode, Rect, Slot, V2, View, v2};
use crate::app::{CANVAS_CONTEXT, Drag, GenApp, HOVER_DELAY, Rel, Tab};
use crate::theme::{Tokens, size, tokens};
use crate::ui::{self, avatar, icon, pill};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::IconName;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

const QUICK_H: f32 = 30.;
const MINI_W: f32 = 200.;
const MINI_H: f32 = 132.;

fn p(v: V2) -> Point<Pixels> {
    point(px(v.x), px(v.y))
}

#[derive(Clone, Copy, PartialEq)]
enum Stroke {
    Normal,
    Dashed,
    Hi,
}

impl GenApp {
    pub(crate) fn render_canvas(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens(cx);
        if self.focus_id.is_none() {
            return self.render_empty(&t, cx);
        }
        let layout = self.layout();
        let view = self.view;
        let z = view.zoom;
        let size = self.canvas_size();
        let mut visible = view.visible(size);
        visible.min = v2(visible.min.x - 60., visible.min.y - 60.);
        visible.max = v2(visible.max.x + 60., visible.max.y + 60.);

        let hovered = self.hovered.filter(|(i, _)| *i < layout.nodes.len());
        let hi: HashSet<usize> = hovered.map(|(i, _)| layout.path_to_root(i).into_iter().collect()).unwrap_or_default();
        let mut curves: Vec<([V2; 4], Stroke)> = Vec::new();
        for (i, n) in layout.nodes.iter().enumerate() {
            let Some(c) = layout.connector(i) else { continue };
            let r = Rect { min: v2(c[0].x, c[0].y.min(c[3].y)), max: v2(c[3].x, c[0].y.max(c[3].y) + 1.) };
            if !r.intersects(&visible) {
                continue;
            }
            let kind = if hi.contains(&i) {
                Stroke::Hi
            } else if matches!(n.slot, Slot::AddParent { .. }) {
                Stroke::Dashed
            } else {
                Stroke::Normal
            };
            curves.push((c.map(|w| view.to_screen(w)), kind));
        }
        // highlighted curves last so they sit on top
        curves.sort_by_key(|(_, k)| *k == Stroke::Hi);

        let bounds_cell = self.canvas_bounds.clone();
        let weak = cx.entity().downgrade();
        let dragging = self.drag.is_some();
        let bg = canvas(
            move |b, _, _| bounds_cell.set(b),
            move |b, _, window, _| {
                paint_dots(b, view, t.canvas_dots, window);
                paint_curves(b, &curves, &t, z, window);
                if dragging {
                    let w = weak.clone();
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        w.update(cx, |this, cx| {
                            if let Some(d) = &this.drag {
                                if e.pressed_button == Some(MouseButton::Left) {
                                    let dx: f32 = (e.position.x - d.start.x).into();
                                    let dy: f32 = (e.position.y - d.start.y).into();
                                    this.view.pan = v2(d.view.pan.x + dx, d.view.pan.y + dy);
                                } else {
                                    this.drag = None;
                                }
                                cx.notify();
                            }
                        })
                        .ok();
                    });
                    let w = weak.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble {
                            w.update(cx, |this, cx| {
                                this.drag = None;
                                cx.notify();
                            })
                            .ok();
                        }
                    });
                }
            },
        )
        .absolute()
        .inset_0();

        let mut cards: Vec<AnyElement> = Vec::new();
        for (i, n) in layout.nodes.iter().enumerate() {
            let mut r = n.rect();
            r.min.y -= QUICK_H;
            if !r.intersects(&visible) {
                continue;
            }
            cards.push(self.render_node(&t, i, n, hovered, cx));
            if let Some(label) = &n.union_label {
                cards.push(self.render_union_label(&t, i, n, label.clone(), cx));
            }
        }
        let headers = self.render_headers(&t, &layout);
        let quick_rect = hovered.filter(|(_, at)| at.elapsed() >= HOVER_DELAY).map(|(i, _)| layout.nodes[i].rect());
        let hit_layout = layout.clone();

        div()
            .id("tree-canvas")
            .key_context(CANVAS_CONTEXT)
            .track_focus(&self.canvas_focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.canvas)
            .when(dragging, |d| d.cursor(CursorStyle::ClosedHand))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    this.canvas_focus.focus(window, cx);
                    let local = this.local(e.position);
                    let w = this.view.to_world(local);
                    let on_card = hit_layout.nodes.iter().any(|n| contains(n.rect(), w))
                        || quick_rect.is_some_and(|mut r| {
                            r.max.y = r.min.y;
                            r.min.y -= QUICK_H;
                            contains(r, w)
                        });
                    if !on_card {
                        this.stop_anim();
                        this.drag = Some(Drag { start: e.position, view: this.view });
                        cx.notify();
                    }
                }),
            )
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, window, cx| {
                let d = e.delta.pixel_delta(px(20.));
                let (dx, dy): (f32, f32) = (d.x.into(), d.y.into());
                this.stop_anim();
                if e.modifiers.platform || e.modifiers.control {
                    let anchor = this.local(e.position);
                    this.zoom_by((dy * 0.01).exp(), Some(anchor), window);
                } else {
                    this.view.pan = v2(this.view.pan.x + dx, this.view.pan.y + dy);
                }
                cx.notify();
            }))
            .on_pinch(cx.listener(|this, e: &PinchEvent, window, cx| {
                let anchor = this.local(e.position);
                this.stop_anim();
                this.zoom_by(1. + e.delta, Some(anchor), window);
                cx.notify();
            }))
            .child(bg)
            .children(headers)
            .children(cards)
            .child(self.render_top_pills(&t, cx))
            .child(self.render_zoom_control(&t, cx))
            .child(self.render_minimap(&t, &layout, size, cx))
            .into_any_element()
    }

    /// Window position → canvas-local.
    pub(crate) fn local(&self, pos: Point<Pixels>) -> V2 {
        let b = self.canvas_bounds.get();
        v2((pos.x - b.origin.x).into(), (pos.y - b.origin.y).into())
    }

    fn render_empty(&self, t: &Tokens, cx: &mut Context<Self>) -> AnyElement {
        let msg = if self.loading { i18n::t!("app.canvas.loading") } else { i18n::t!("app.canvas.empty") };
        div()
            .id("tree-canvas")
            .track_focus(&self.canvas_focus)
            .key_context(CANVAS_CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(16.))
            .child(div().text_color(t.ink_muted).text_size(px(14.)).child(msg))
            .when(!self.loading, |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(ui::outline_button(t, "empty-open").child(icon(IconName::FolderOpen, 14., t.ink)).child(i18n::t!("app.topbar.open")).on_click(cx.listener(|this, _, window, cx| this.open_dialog(window, cx))))
                        .child(ui::filled_button(t, "empty-new", t.accent).child(i18n::t!("app.topbar.new_person")).on_click(cx.listener(|this, _, window, cx| this.new_tree(window, cx)))),
                )
            })
            .into_any_element()
    }

    fn render_headers(&self, t: &Tokens, l: &Layout) -> Vec<AnyElement> {
        let desc = self.tab == Tab::Descendants;
        (0..l.columns)
            .map(|d| {
                let s = self.view.to_screen(v2(l.column_x(d), l.header_y));
                div()
                    .absolute()
                    .left(px(s.x))
                    .top(px(s.y))
                    .w(px(layout::CARD_W * self.view.zoom))
                    .text_size(px((12. * self.view.zoom).max(7.)))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.ink_muted)
                    .truncate()
                    .child(column_title(d, desc))
                    .into_any_element()
            })
            .collect()
    }

    fn render_union_label(&self, t: &Tokens, i: usize, n: &LNode, partner: Option<String>, cx: &mut Context<Self>) -> AnyElement {
        let z = self.view.zoom;
        let s = self.view.to_screen(v2(n.pos.x, n.pos.y - layout::UNION_LABEL_H));
        let text = match partner.as_deref().and_then(|p| self.tree.person(p)) {
            Some(pp) => i18n::t!("app.canvas.union_with", name = pp.name()),
            None => i18n::t!("app.canvas.union_unknown"),
        };
        let accent = t.accent;
        div()
            .id(("union", i))
            .absolute()
            .left(px(s.x))
            .top(px(s.y))
            .h(px(layout::UNION_LABEL_H * z))
            .max_w(px(layout::CARD_W * z))
            .text_size(px((11. * z).max(6.)))
            .text_color(t.ink_muted)
            .truncate()
            .child(text)
            .when_some(partner, |d, pid| {
                d.cursor_pointer().hover(move |s| s.text_color(accent)).on_click(cx.listener(move |this, _, window, cx| this.focus_by_click(pid.clone(), window, cx)))
            })
            .into_any_element()
    }

    fn render_node(&self, t: &Tokens, i: usize, n: &LNode, hovered: Option<(usize, Instant)>, cx: &mut Context<Self>) -> AnyElement {
        let z = self.view.zoom;
        let s = self.view.to_screen(n.pos);
        let h = if n.depth == 0 { size::CARD_FOCUS_H } else { size::CARD_H };
        let (w, h) = (f32::from(size::CARD_W) * z, f32::from(h) * z);
        let show_quick = hovered.is_some_and(|(h, at)| h == i && at.elapsed() >= HOVER_DELAY) && n.person().is_some();
        let card = match &n.slot {
            Slot::Person(id) => self.person_card(t, i, id, n.depth == 0, z, cx),
            Slot::AddParent { child, father } => empty_slot(t, i, *father, z, w, h).on_click({
                let (child, father) = (child.clone(), *father);
                cx.listener(move |this, _, window, cx| this.add_and_edit(&child, if father { Rel::Father } else { Rel::Mother }, None, window, cx))
            }),
        };
        let wrapper = div()
            .id(("node", i))
            .absolute()
            .left(px(s.x))
            .top(px(s.y - QUICK_H))
            .w(px(w))
            .h(px(h + QUICK_H))
            .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                if *inside {
                    if this.hovered.map(|(h, _)| h) != Some(i) {
                        this.hovered = Some((i, Instant::now()));
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(HOVER_DELAY).await;
                            this.update(cx, |_, cx| cx.notify()).ok();
                        })
                        .detach();
                    }
                } else if this.hovered.map(|(h, _)| h) == Some(i) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .child(div().absolute().left_0().top(px(QUICK_H)).w(px(w)).h(px(h)).child(card));
        match n.person().filter(|_| show_quick) {
            Some(id) => wrapper.child(self.quick_actions(t, i, id, cx)).into_any_element(),
            None => wrapper.into_any_element(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn person_card(&self, t: &Tokens, i: usize, id: &str, focus: bool, z: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let person = self.tree.person(id);
        let name = person.map(|p| p.name()).unwrap_or_default();
        let years = person.map(ui::years).unwrap_or_default();
        let hover_border = t.border_strong;
        let pid = id.to_string();
        let halo = BoxShadow { color: t.accent.opacity(0.18), offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(4. * z), inset: false };
        let lift = BoxShadow { color: hsla(0., 0., 0., 0.10), offset: point(px(0.), px(4. * z)), blur_radius: px(14. * z), spread_radius: px(0.), inset: false };
        div()
            .id(("card", i))
            .size_full()
            .flex()
            .items_center()
            .gap(px(10. * z))
            .px(size::CARD_PAD * z)
            .rounded(size::CARD_RADIUS * z)
            .bg(t.surface)
            .cursor_pointer()
            .map(|d| if focus { d.border_2().border_color(t.accent).shadow(vec![halo]) } else { d.border_1().border_color(t.border).hover(move |s| s.border_color(hover_border).shadow(vec![lift])) })
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                if e.click_count() >= 2 {
                    this.open_edit(pid.clone(), window, cx);
                } else {
                    this.focus_by_click(pid.clone(), window, cx);
                }
            }))
            .child(avatar(t, person, f32::from(size::AVATAR_CARD) * z))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .flex_1()
                    .gap(px(2. * z))
                    .child(div().text_size(px(13. * z)).line_height(px(16. * z)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink).line_clamp(2).child(name))
                    .child(div().text_size(px(12. * z)).line_height(px(15. * z)).text_color(t.ink_muted).truncate().child(years)),
            )
    }

    fn quick_actions(&self, t: &Tokens, i: usize, id: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let mk = |key: &'static str, rel: Rel, n: usize, cx: &mut Context<Self>| {
            let id = id.to_string();
            let accent = t.accent;
            div()
                .id(("quick", i * 4 + n))
                .px(px(8.))
                .h(px(24.))
                .flex()
                .items_center()
                .rounded_full()
                .bg(t.surface)
                .border_1()
                .border_color(t.border)
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(t.ink)
                .cursor_pointer()
                .hover(move |s| s.border_color(accent).text_color(accent))
                .on_click(cx.listener(move |this, _, window, cx| this.add_and_edit(&id, rel, None, window, cx)))
                .child(i18n::t!(key))
        };
        let parent = match self.tree.father_mother(id) {
            (None, _) => Some(("app.quick.parent", Rel::Father)),
            (Some(_), None) => Some(("app.quick.mother", Rel::Mother)),
            _ => None,
        };
        div()
            .absolute()
            .left_0()
            .top_0()
            .h(px(QUICK_H))
            .flex()
            .items_center()
            .gap(px(4.))
            .children(parent.map(|(k, r)| mk(k, r, 0, cx)))
            .child(mk("app.quick.partner", Rel::Partner, 1, cx))
            .child(mk("app.quick.child", Rel::Child, 2, cx))
    }

    pub(crate) fn render_top_pills(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let mut crumbs = pill(t).id("crumbs").px(px(14.)).occlude();
        let start = self.trail.len().saturating_sub(3);
        if start > 0 {
            crumbs = crumbs.child(div().text_color(t.ink_muted).child("…")).child(icon(IconName::ChevronRight, 12., t.ink_muted));
        }
        for (ix, id) in self.trail.iter().enumerate().skip(start) {
            let name = self.tree.person(id).map(|p| p.given.split_whitespace().next().map(str::to_string).filter(|s| !s.is_empty()).unwrap_or_else(|| p.name())).unwrap_or_default();
            let accent = t.accent;
            crumbs = crumbs
                .child(div().id(("crumb", ix)).text_color(t.ink_muted).cursor_pointer().hover(move |s| s.text_color(accent)).child(name).on_click(cx.listener(move |this, _, window, cx| this.jump_trail(ix, window, cx))))
                .child(icon(IconName::ChevronRight, 12., t.ink_muted));
        }
        let cur = self.focus_id.as_deref().and_then(|f| self.tree.person(f)).map(|p| p.name()).unwrap_or_default();
        crumbs = crumbs.child(div().font_weight(FontWeight::SEMIBOLD).max_w(px(220.)).truncate().child(cur));

        let mut gens = pill(t).id("gens").pl(px(14.)).occlude().child(div().text_color(t.ink_muted).mr(px(4.)).child(i18n::t!("app.canvas.generations")));
        for g in 3..=6usize {
            let sel = self.generations == g;
            let canvas_bg = t.canvas;
            gens = gens.child(
                div()
                    .id(("gen", g))
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .map(|d| if sel { d.bg(t.ink).text_color(t.on_fill()) } else { d.text_color(t.ink).hover(move |s| s.bg(canvas_bg)) })
                    .child(g.to_string())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.generations = g;
                        this.recenter(true, window);
                        cx.notify();
                    })),
            );
        }
        div().absolute().top(px(16.)).left(px(16.)).right(px(16.)).flex().flex_wrap().gap(px(8.)).child(crumbs).child(gens)
    }

    fn render_zoom_control(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let btn = |id: &'static str, name: IconName| ui::icon_button(t, id, name, true).size(px(30.)).rounded_full();
        pill(t)
            .id("zoom")
            .occlude()
            .absolute()
            .left(px(16.))
            .bottom(px(16.))
            .child(btn("zoom-out", IconName::Minus).on_click(cx.listener(|this, _, window, cx| {
                this.zoom_by(0.8, None, window);
                cx.notify();
            })))
            .child(
                div()
                    .id("zoom-pct")
                    .w(px(48.))
                    .text_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .child(format!("{}%", (self.view.zoom * 100.).round()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let f = 1. / this.view.zoom;
                        this.zoom_by(f, None, window);
                        cx.notify();
                    })),
            )
            .child(btn("zoom-in", IconName::Plus).on_click(cx.listener(|this, _, window, cx| {
                this.zoom_by(1.25, None, window);
                cx.notify();
            })))
            .child(div().w(px(1.)).h(px(18.)).bg(t.border))
            .child(btn("zoom-fit", IconName::Maximize).on_click(cx.listener(|this, _, window, cx| {
                this.fit_view(window);
                cx.notify();
            })))
            .child(btn("zoom-center", IconName::CircleUser).on_click(cx.listener(|this, _, window, cx| {
                this.recenter(true, window);
                cx.notify();
            })))
    }

    fn render_minimap(&self, t: &Tokens, l: &Rc<Layout>, viewport: V2, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = l.bounds();
        let (s, off) = layout::minimap_transform(bounds, v2(MINI_W, MINI_H), 8.);
        let vis = self.view.visible(viewport);
        let rects: Vec<(V2, V2, bool, bool)> = l.nodes.iter().map(|n| (v2(n.pos.x * s + off.x, n.pos.y * s + off.y), v2(n.w * s, n.h * s), n.depth == 0, n.person().is_none())).collect();
        let vr = (v2(vis.min.x * s + off.x, vis.min.y * s + off.y), v2(vis.w() * s, vis.h() * s));
        let t2 = *t;
        div()
            .id("minimap")
            .occlude()
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
            .w(px(MINI_W))
            .h(px(MINI_H))
            .rounded(px(12.))
            .bg(t.surface)
            .border_1()
            .border_color(t.border)
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    let b = this.canvas_bounds.get();
                    let mini_origin = v2(f32::from(b.origin.x + b.size.width) - 16. - MINI_W, f32::from(b.origin.y + b.size.height) - 16. - MINI_H);
                    let m = v2(f32::from(e.position.x) - mini_origin.x, f32::from(e.position.y) - mini_origin.y);
                    let world = v2((m.x - off.x) / s, (m.y - off.y) / s);
                    let target = this.view.centered_on(world, this.canvas_size());
                    this.animate_to(target, true, window);
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let o = b.origin;
                        for (pos, sz, focus, empty) in &rects {
                            let r = Bounds::new(o + p(*pos), size(px(sz.x.max(1.)), px(sz.y.max(1.))));
                            let c = if *focus {
                                t2.accent
                            } else if *empty {
                                t2.slot_dashed.opacity(0.5)
                            } else {
                                t2.connector.opacity(0.7)
                            };
                            window.paint_quad(fill(r, c).corner_radii(px(1.5)));
                        }
                        let r = Bounds::new(o + p(vr.0), size(px(vr.1.x), px(vr.1.y)));
                        window.paint_quad(quad(r, px(3.), t2.accent.opacity(0.06), px(1.), t2.accent, BorderStyle::Solid));
                    },
                )
                .size_full(),
            )
    }
}

fn contains(r: Rect, w: V2) -> bool {
    w.x >= r.min.x && w.x <= r.max.x && w.y >= r.min.y && w.y <= r.max.y
}

fn empty_slot(t: &Tokens, i: usize, father: bool, z: f32, _w: f32, _h: f32) -> Stateful<Div> {
    let (accent, soft) = (t.accent, t.accent_soft);
    div()
        .id(("slot", i))
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6. * z))
        .rounded(px(14. * z))
        .border_1()
        .border_dashed()
        .border_color(t.slot_dashed)
        .text_color(t.ink_muted)
        .text_size(px(13. * z))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.border_color(accent).text_color(accent).bg(soft))
        .child(if father { i18n::t!("app.canvas.add_father") } else { i18n::t!("app.canvas.add_mother") })
}

fn column_title(d: usize, desc: bool) -> String {
    let keys_up = ["app.column.focus", "app.column.parents", "app.column.grandparents", "app.column.great_grandparents", "app.column.great2_grandparents"];
    let keys_down = ["app.column.focus", "app.column.children", "app.column.grandchildren", "app.column.great_grandchildren", "app.column.great2_grandchildren"];
    let keys = if desc { &keys_down } else { &keys_up };
    match keys.get(d) {
        Some(k) => i18n::t!(k),
        None => i18n::t!("app.column.generation", n = d + 1),
    }
}

fn paint_dots(b: Bounds<Pixels>, view: View, color: Hsla, window: &mut Window) {
    let mut step = crate::theme::size::DOT_SPACING * view.zoom;
    while step < 10. {
        step *= 2.;
    }
    let (w, h): (f32, f32) = (b.size.width.into(), b.size.height.into());
    let x0 = view.pan.x.rem_euclid(step);
    let y0 = view.pan.y.rem_euclid(step);
    let mut y = y0;
    while y < h {
        let mut x = x0;
        while x < w {
            window.paint_quad(fill(Bounds::new(b.origin + point(px(x), px(y)), size(px(1.), px(1.))), color));
            x += step;
        }
        y += step;
    }
}

fn paint_curves(b: Bounds<Pixels>, curves: &[([V2; 4], Stroke)], t: &Tokens, z: f32, window: &mut Window) {
    for kind in [Stroke::Normal, Stroke::Dashed, Stroke::Hi] {
        let width = if kind == Stroke::Hi { 2. } else { 1.5 };
        let mut pb = PathBuilder::stroke(px(width));
        if kind == Stroke::Dashed {
            pb = pb.dash_array(&[px(4. * z.max(0.5)), px(4. * z.max(0.5))]);
        }
        let mut any = false;
        for (c, k) in curves {
            if *k != kind {
                continue;
            }
            any = true;
            pb.move_to(b.origin + p(c[0]));
            pb.cubic_bezier_to(b.origin + p(c[3]), b.origin + p(c[1]), b.origin + p(c[2]));
        }
        if !any {
            continue;
        }
        let color = match kind {
            Stroke::Normal => t.connector,
            Stroke::Dashed => t.slot_dashed,
            Stroke::Hi => t.accent,
        };
        if let Ok(path) = pb.build() {
            window.paint_path(path, color);
        }
    }
}
