//! «Mapa»: the tree's places on OpenStreetMap. Coordinates come from `PLAC.MAP` in the file or,
//! with the user's consent, from Nominatim (cached forever, see `geocode`). Tiles are OSM raster
//! tiles cached on disk (`tiles`). Markers are clustered in screen space; clicking one lists the
//! events there and clicking a person focuses them (the ficha stays visible).
pub mod geo;
pub mod geocode;
mod links;
pub mod net;
pub mod tiles;

use crate::app::GenApp;
use crate::settings::Settings;
use crate::theme::{Tokens, tokens};
use crate::ui::{self, avatar, icon, pill};
use gedcom::{PlaceGroup, PlaceLinks};
use geo::{Camera, TileKey};
use geocode::GeoCache;
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::IconName;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

const COPYRIGHT_URL: &str = "https://www.openstreetmap.org/copyright";
const ATTRIBUTION: &str = "© OpenStreetMap contributors"; // i18n-ok: OSM's required attribution
const FIT_PAD: f64 = 64.;
const FIT_MAX_ZOOM: f64 = 11.;
const TILE_RETRY: Duration = Duration::from_secs(30);
const OFFLINE_PAUSE: Duration = Duration::from_secs(20);
const BACKOFF_PAUSE: Duration = Duration::from_secs(300);
/// decoded tiles kept in memory before the image cache is flushed
const IMAGE_CACHE_MAX: usize = 300;
const POPOVER_ROWS: usize = 200;
const MISSING_ROWS: usize = 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum MapFilter {
    #[default]
    All,
    Ancestors,
}

enum TileState {
    Ready(PathBuf),
    /// on disk but older than `tiles::MAX_AGE`: drawn while it is fetched again
    Stale(PathBuf),
    /// downloading; keeps the stale copy to draw meanwhile
    Loading(Option<PathBuf>),
    Failed(Instant),
}

pub struct MapState {
    camera: Camera,
    /// fit the markers on the next render
    needs_fit: bool,
    /// the user panned/zoomed: new geocoding results no longer refit the view
    user_moved: bool,
    groups: Option<(u64, Rc<Vec<PlaceGroup>>)>,
    ancestors: Option<(u64, String, Rc<HashSet<String>>)>,
    filter: MapFilter,
    cache: Option<GeoCache>,
    consent: bool,
    /// «Ahora no» for this session
    declined: bool,
    /// (done, total) while geocoding
    progress: Option<(usize, usize)>,
    net_error: bool,
    tiles: HashMap<TileKey, TileState>,
    inflight: usize,
    tiles_paused_until: Option<Instant>,
    downloads: usize,
    image_cache: Option<Entity<RetainAllImageCache>>,
    /// place key of the hovered marker (first member of its cluster)
    hovered: Option<String>,
    /// place keys whose events are listed
    selected: Option<Vec<String>>,
    show_missing: bool,
    drag: Option<(Point<Pixels>, Camera, bool)>,
    /// «Conexiones familiares» of the selected place
    links: Option<(links::LinksKey, Option<Rc<PlaceLinks>>)>,
    kin: links::KinCache,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl MapState {
    pub fn new(settings: &Settings) -> Self {
        MapState {
            camera: Camera::default(),
            needs_fit: true,
            user_moved: false,
            groups: None,
            ancestors: None,
            filter: MapFilter::All,
            cache: None,
            consent: settings.geocode_consent,
            declined: false,
            progress: None,
            net_error: false,
            tiles: HashMap::new(),
            inflight: 0,
            tiles_paused_until: None,
            downloads: 0,
            image_cache: None,
            hovered: None,
            selected: None,
            show_missing: false,
            drag: None,
            links: None,
            kin: links::KinCache::default(),
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// A different file was opened.
    pub fn reset(&mut self) {
        self.needs_fit = true;
        self.user_moved = false;
        self.selected = None;
        self.hovered = None;
        self.show_missing = false;
        self.declined = false;
        self.links = None;
    }

    fn cache(&mut self) -> &mut GeoCache {
        self.cache.get_or_insert_with(|| GeoCache::load(GeoCache::default_path()))
    }

    fn coords(&mut self, g: &PlaceGroup) -> Option<(f64, f64)> {
        g.coords.or_else(|| self.cache().get(&g.key).flatten())
    }

    fn size(&self) -> (f64, f64) {
        let b = self.bounds.get();
        (f32::from(b.size.width) as f64, f32::from(b.size.height) as f64)
    }

    fn local(&self, p: Point<Pixels>) -> (f64, f64) {
        let b = self.bounds.get();
        (f32::from(p.x - b.origin.x) as f64, f32::from(p.y - b.origin.y) as f64)
    }

    fn zoom_to(&mut self, zoom: f64, anchor: Option<(f64, f64)>) {
        let size = self.size();
        let a = anchor.unwrap_or((size.0 / 2., size.1 / 2.));
        self.camera.zoom_around(a, size, zoom);
        self.user_moved = true;
    }
}

/// A place on screen: group index, world position, events that pass the filter.
struct Spot {
    gi: usize,
    world: (f64, f64),
    count: usize,
}

fn passes(set: Option<&HashSet<String>>, people: &[String]) -> bool {
    set.is_none_or(|s| people.iter().any(|p| s.contains(p)))
}

impl GenApp {
    fn map_groups(&mut self) -> Rc<Vec<PlaceGroup>> {
        if let Some((rev, g)) = &self.map.groups
            && *rev == self.rev
        {
            return g.clone();
        }
        let g = Rc::new(self.tree.place_groups());
        self.map.groups = Some((self.rev, g.clone()));
        g
    }

    /// People whose events pass the filter; `None` = everybody.
    fn map_filter_set(&mut self) -> Option<Rc<HashSet<String>>> {
        let focus = self.focus_id.clone().filter(|_| self.map.filter == MapFilter::Ancestors)?;
        if let Some((rev, f, s)) = &self.map.ancestors
            && *rev == self.rev
            && *f == focus
        {
            return Some(s.clone());
        }
        let s = Rc::new(self.tree.ancestors_and_self(&focus));
        self.map.ancestors = Some((self.rev, focus, s.clone()));
        Some(s)
    }

    fn map_spots(&mut self, groups: &[PlaceGroup], set: Option<&HashSet<String>>) -> Vec<Spot> {
        let mut out = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            let count = g.events.iter().filter(|e| passes(set, &e.people)).count();
            if count == 0 {
                continue;
            }
            if let Some((lat, lon)) = self.map.coords(g) {
                out.push(Spot { gi, world: geo::to_world(lat, lon), count });
            }
        }
        out
    }

    fn map_fit(&mut self, spots: &[Spot]) {
        let pts: Vec<(f64, f64)> = spots.iter().map(|s| s.world).collect();
        let size = self.map.size();
        if size.0 > 0. && !pts.is_empty() {
            self.map.camera = Camera::fit(&pts, size, FIT_PAD, FIT_MAX_ZOOM);
        }
    }

    /// Palette «Lugares» hit while on the map: select that place and centre on it.
    pub(crate) fn map_show_place(&mut self, place: &str, cx: &mut Context<Self>) {
        let key = gedcom::place_key(place);
        let groups = self.map_groups();
        if let Some(g) = groups.iter().find(|g| g.key == key) {
            if let Some((lat, lon)) = self.map.coords(g) {
                self.map.camera.center = geo::to_world(lat, lon);
                self.map.camera.zoom = self.map.camera.zoom.max(8.);
                self.map.user_moved = true;
            }
            self.map.selected = Some(vec![key]);
        }
        cx.notify();
    }

    fn give_consent(&mut self, cx: &mut Context<Self>) {
        self.map.consent = true;
        self.map.net_error = false;
        let mut s = Settings::load();
        s.geocode_consent = true;
        s.save();
        cx.notify();
    }

    fn start_geocoding(&mut self, todo: Vec<(String, String)>, cx: &mut Context<Self>) {
        let total = todo.len();
        self.map.progress = Some((0, total));
        self.map.net_error = false;
        let lang = i18n::locale().to_string();
        cx.spawn(async move |this, cx| {
            for (i, (key, name)) in todo.into_iter().enumerate() {
                let lang = lang.clone();
                let res = cx.background_executor().spawn(async move { geocode::lookup(&name, &lang) }).await;
                let go = this
                    .update(cx, |this, cx| {
                        let ok = match res {
                            Ok(v) => {
                                let c = this.map.cache();
                                c.insert(key, v);
                                c.save();
                                this.map.progress = Some((i + 1, total));
                                if !this.map.user_moved {
                                    this.map.needs_fit = true;
                                }
                                true
                            }
                            Err(e) => {
                                eprintln!("nominatim: {e}");
                                this.map.net_error = true;
                                false
                            }
                        };
                        cx.notify();
                        ok
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
            this.update(cx, |this, cx| {
                this.map.progress = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Start downloads for the wanted tiles (centre first), at most `MAX_INFLIGHT` at a time.
    fn pump_tiles(&mut self, wanted: &[TileKey], cx: &mut Context<Self>) {
        let Some(root) = tiles::dir() else { return };
        if self.map.tiles_paused_until.is_some_and(|t| Instant::now() < t) {
            return;
        }
        for &k in wanted {
            if self.map.inflight >= tiles::MAX_INFLIGHT {
                break;
            }
            let stale = match self.map.tiles.remove(&k) {
                Some(TileState::Stale(p)) => Some(p),
                _ => None,
            };
            self.map.tiles.insert(k, TileState::Loading(stale));
            self.map.inflight += 1;
            let root = root.clone();
            let task = cx.background_executor().spawn(async move { tiles::fetch(&root, k) });
            cx.spawn(async move |this, cx| {
                let res = task.await;
                this.update(cx, |this, cx| {
                    this.map.inflight -= 1;
                    match res {
                        Ok(p) => {
                            this.map.tiles.insert(k, TileState::Ready(p));
                            this.map.downloads += 1;
                            if this.map.downloads % 200 == 1
                                && let Some(root) = tiles::dir()
                            {
                                cx.background_executor().spawn(async move { tiles::prune_default(&root) }).detach();
                            }
                        }
                        Err(e) => {
                            eprintln!("tile {k:?}: {e}");
                            // offline, or the tile server asks us to back off
                            let pause = match e {
                                net::NetError::Transport(_) => Some(OFFLINE_PAUSE),
                                net::NetError::Status(403 | 429) => Some(BACKOFF_PAUSE),
                                net::NetError::Status(_) => None,
                            };
                            if let Some(d) = pause {
                                this.map.tiles_paused_until = Some(Instant::now() + d);
                            }
                            let prev = this.map.tiles.remove(&k);
                            // keep showing an old copy if there is one
                            this.map.tiles.insert(
                                k,
                                match prev {
                                    Some(TileState::Loading(Some(p))) => TileState::Ready(p),
                                    _ => TileState::Failed(Instant::now()),
                                },
                            );
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    /// Image of tile `k` if we have it (fresh or stale), else checks the disk once.
    fn tile_image(&mut self, k: TileKey, root: Option<&PathBuf>, wanted: &mut Vec<TileKey>) -> Option<PathBuf> {
        match self.map.tiles.get(&k) {
            Some(TileState::Ready(p)) => return Some(p.clone()),
            Some(TileState::Stale(p)) => {
                wanted.push(k);
                return Some(p.clone());
            }
            Some(TileState::Loading(stale)) => return stale.clone(),
            Some(TileState::Failed(at)) if at.elapsed() < TILE_RETRY => return None,
            _ => {}
        }
        let root = root?;
        let p = tiles::path(root, k);
        match tiles::cached(&p) {
            Some(true) => {
                self.map.tiles.insert(k, TileState::Ready(p.clone()));
                Some(p)
            }
            Some(false) => {
                self.map.tiles.insert(k, TileState::Stale(p.clone()));
                wanted.push(k);
                Some(p)
            }
            None => {
                wanted.push(k);
                None
            }
        }
    }

    /// Ready lower-zoom tile covering `k`, as (path, levels up).
    fn tile_parent(&self, k: TileKey) -> Option<(PathBuf, u8)> {
        (1..=4u8).filter(|d| *d <= k.z).find_map(|d| match self.map.tiles.get(&TileKey { z: k.z - d, x: k.x >> d, y: k.y >> d }) {
            Some(TileState::Ready(p) | TileState::Stale(p) | TileState::Loading(Some(p))) => Some((p.clone(), d)),
            _ => None,
        })
    }

    pub(crate) fn render_map(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = tokens(cx);
        let groups = self.map_groups();
        let set = self.map_filter_set();
        let spots = self.map_spots(&groups, set.as_deref());
        let size = self.map.size();
        if size.0 <= 0. {
            window.request_animation_frame();
        } else if self.map.needs_fit {
            self.map.needs_fit = false;
            self.map_fit(&spots);
        }

        // geocoding bookkeeping
        let mut pending: Vec<(String, String)> = Vec::new();
        let mut missing: Vec<usize> = Vec::new();
        for (gi, g) in groups.iter().enumerate() {
            if g.coords.is_some() {
                continue;
            }
            match self.map.cache().get(&g.key) {
                None => pending.push((g.key.clone(), g.name.clone())),
                Some(None) => missing.push(gi),
                Some(Some(_)) => {}
            }
        }
        if self.map.consent && !pending.is_empty() && self.map.progress.is_none() && !self.map.net_error {
            self.start_geocoding(pending.clone(), cx);
        }

        // tiles
        let tile_cache = self.map.image_cache.get_or_insert_with(|| RetainAllImageCache::new(cx)).clone();
        if tile_cache.read(cx).len() > IMAGE_CACHE_MAX {
            tile_cache.update(cx, |c, cx| c.clear(window, cx));
        }
        let root = tiles::dir();
        let cam = self.map.camera;
        let dim = if t.dark { 0.78 } else { 1. };
        let mut wanted = Vec::new();
        let mut tile_els: Vec<AnyElement> = Vec::new();
        if size.0 > 0. {
            for pl in geo::visible_tiles(&cam, size) {
                let (l, tp, s) = (pl.left as f32, pl.top as f32, pl.side as f32);
                if let Some(p) = self.tile_image(pl.key, root.as_ref(), &mut wanted) {
                    tile_els.push(img(p).absolute().left(px(l)).top(px(tp)).size(px(s + 0.5)).opacity(dim).into_any_element());
                } else if let Some((p, d)) = self.tile_parent(pl.key) {
                    let n = 1u32 << d;
                    let (ox, oy) = ((pl.key.x % n) as f32 * s, (pl.key.y % n) as f32 * s);
                    tile_els.push(
                        div()
                            .absolute()
                            .left(px(l))
                            .top(px(tp))
                            .size(px(s + 0.5))
                            .overflow_hidden()
                            .child(img(p).absolute().left(px(-ox)).top(px(-oy)).size(px(s * n as f32)).opacity(dim))
                            .into_any_element(),
                    );
                }
            }
        }
        self.pump_tiles(&wanted, cx);

        // family connections of the selected place, and thin arcs to where people came from / went
        let place_links = self.map_links(&groups);
        if let Some(keys) = self.map.selected.clone() {
            let rows = Self::map_row_people(&groups, &keys, set.as_deref());
            self.map_fill_kin(&rows, window);
        }
        let mut arcs: Vec<((f64, f64), (f64, f64))> = Vec::new();
        if let (Some(l), Some(here)) = (&place_links, self.map.selected.as_ref().and_then(|k| groups.iter().find(|g| Some(&g.key) == k.first()))) {
            if let Some((lat, lon)) = self.map.coords(here) {
                let a = cam.to_screen(geo::to_world(lat, lon), size);
                for m in &l.migrations {
                    if let Some(g) = groups.iter().find(|g| g.key == m.key)
                        && let Some((lat, lon)) = self.map.coords(g)
                    {
                        arcs.push((a, cam.to_screen(geo::to_world(lat, lon), size)));
                    }
                }
            }
        }
        let arc_color = t.accent.opacity(0.55);
        let arcs_el = (!arcs.is_empty()).then(|| {
            canvas(
                |_, _, _| {},
                move |b, _, window, _| {
                    for ((x0, y0), (x1, y1)) in &arcs {
                        let (dx, dy) = (x1 - x0, y1 - y0);
                        let len = (dx * dx + dy * dy).sqrt();
                        if len < 4. {
                            continue;
                        }
                        // bow to one side by a fifth of the length
                        let (nx, ny) = (-dy * 0.2, dx * 0.2);
                        let p = |x: f64, y: f64| b.origin + point(px(x as f32), px(y as f32));
                        let mut pb = PathBuilder::stroke(px(1.5));
                        pb.move_to(p(*x0, *y0));
                        pb.cubic_bezier_to(p(*x1, *y1), p(x0 + dx / 3. + nx, y0 + dy / 3. + ny), p(x0 + 2. * dx / 3. + nx, y0 + 2. * dy / 3. + ny));
                        if let Ok(path) = pb.build() {
                            window.paint_path(path, arc_color);
                        }
                    }
                },
            )
            .absolute()
            .inset_0()
        });

        // markers
        let margin = 40.;
        let on_screen: Vec<(usize, (f64, f64))> = spots
            .iter()
            .enumerate()
            .map(|(i, s)| (i, cam.to_screen(s.world, size)))
            .filter(|(_, (x, y))| *x > -margin && *y > -margin && *x < size.0 + margin && *y < size.1 + margin)
            .collect();
        let clusters = geo::cluster(&on_screen.iter().map(|(i, (x, y))| (*x, *y, spots[*i].count)).collect::<Vec<_>>());
        let selected: HashSet<String> = self.map.selected.iter().flatten().cloned().collect();
        let mut markers: Vec<AnyElement> = Vec::new();
        let mut tooltip: Option<AnyElement> = None;
        for (ci, c) in clusters.iter().enumerate() {
            let keys: Vec<String> = c.members.iter().map(|m| groups[spots[on_screen[*m].0].gi].key.clone()).collect();
            let r = geo::marker_radius(c.count) as f32;
            let (x, y) = (c.x as f32, c.y as f32);
            let is_sel = keys.iter().any(|k| selected.contains(k));
            let hovered = self.map.hovered.as_deref() == keys.first().map(String::as_str);
            let first = keys[0].clone();
            let world = spots[on_screen[c.members[0]].0].world;
            let multi = keys.len() > 1;
            let click_keys = keys.clone();
            markers.push(
                div()
                    .id(("map-marker", ci))
                    .absolute()
                    .left(px(x - r))
                    .top(px(y - r))
                    .size(px(2. * r))
                    .rounded_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if hovered { t.accent.opacity(0.9) } else { t.accent })
                    .border_2()
                    .border_color(if is_sel { t.ink } else { t.surface })
                    .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.22), offset: point(px(0.), px(1.)), blur_radius: px(4.), spread_radius: px(0.), inset: false }])
                    .text_color(t.on_fill())
                    .text_size(px(if r < 12. { 10. } else { 11. }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .occlude()
                    .child(c.count.to_string())
                    .on_hover(cx.listener(move |this, inside: &bool, _, cx| {
                        if *inside {
                            this.map.hovered = Some(first.clone());
                        } else if this.map.hovered.as_deref() == Some(first.as_str()) {
                            this.map.hovered = None;
                        }
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if multi && this.map.camera.zoom < geo::MAX_ZOOM - 1. {
                            // a cluster: zoom in on it rather than listing everything
                            this.map.camera.center = world;
                            let z = this.map.camera.zoom.round() + 2.;
                            this.map.zoom_to(z, None);
                        } else {
                            this.map.selected = Some(click_keys.clone());
                            this.map.show_missing = false;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if hovered {
                let title = if multi { i18n::tn!("app.map.places", keys.len()) } else { groups[spots[on_screen[c.members[0]].0].gi].name.clone() };
                tooltip = Some(
                    div()
                        .absolute()
                        .left(px(x - 160.))
                        .top(px(y - r - 58.))
                        .w(px(320.))
                        .h(px(52.))
                        .flex()
                        .items_end()
                        .justify_center()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .max_w(px(320.))
                                .px(px(10.))
                                .py(px(6.))
                                .rounded(px(8.))
                                .bg(t.ink)
                                .text_color(t.on_fill())
                                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).truncate().child(title))
                                .child(div().text_size(px(11.)).opacity(0.8).child(i18n::tn!("app.map.events", c.count))),
                        )
                        .into_any_element(),
                );
            }
        }

        let bounds_cell = self.map.bounds.clone();
        let weak = cx.entity().downgrade();
        let dragging = self.map.drag.is_some();
        let input = canvas(
            move |b, _, _| bounds_cell.set(b),
            move |_, _, window, _| {
                if !dragging {
                    return;
                }
                let w = weak.clone();
                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    w.update(cx, |this, cx| {
                        let Some((start, cam, moved)) = &mut this.map.drag else { return };
                        if e.pressed_button != Some(MouseButton::Left) {
                            this.map.drag = None;
                        } else {
                            let dx = f32::from(e.position.x - start.x) as f64;
                            let dy = f32::from(e.position.y - start.y) as f64;
                            *moved |= dx.abs() + dy.abs() > 3.;
                            let mut c = *cam;
                            c.pan_px(dx, dy);
                            this.map.camera = c;
                            this.map.user_moved = true;
                        }
                        cx.notify();
                    })
                    .ok();
                });
                let w = weak.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    w.update(cx, |this, cx| {
                        if let Some((_, _, false)) = this.map.drag {
                            // a plain click on the map closes the lists
                            this.map.selected = None;
                            this.map.show_missing = false;
                        }
                        this.map.drag = None;
                        cx.notify();
                    })
                    .ok();
                });
            },
        )
        .absolute()
        .inset_0();

        let empty = groups.is_empty().then(|| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(div().px(px(16.)).py(px(10.)).rounded(px(12.)).bg(t.surface).border_1().border_color(t.border).text_color(t.ink_muted).child(if self.loading { i18n::t!("app.canvas.loading") } else { i18n::t!("app.map.no_places") }))
        });

        div()
            .id("map")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.canvas)
            .when(dragging, |d| d.cursor(CursorStyle::ClosedHand))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, e: &MouseDownEvent, window, cx| {
                    this.restore_focus(window, cx);
                    this.map.drag = Some((e.position, this.map.camera, false));
                    cx.notify();
                }),
            )
            .on_scroll_wheel(cx.listener(|this, e: &ScrollWheelEvent, _, cx| {
                let d = e.delta.pixel_delta(px(20.));
                let (dx, dy) = (f32::from(d.x) as f64, f32::from(d.y) as f64);
                if e.modifiers.platform || e.modifiers.control {
                    let a = this.map.local(e.position);
                    let z = this.map.camera.zoom + dy * 0.01 / std::f64::consts::LN_2;
                    this.map.zoom_to(z, Some(a));
                } else {
                    this.map.camera.pan_px(dx, dy);
                    this.map.user_moved = true;
                }
                cx.notify();
            }))
            .on_pinch(cx.listener(|this, e: &PinchEvent, _, cx| {
                let a = this.map.local(e.position);
                let z = this.map.camera.zoom + (1. + e.delta as f64).max(0.1).log2();
                this.map.zoom_to(z, Some(a));
                cx.notify();
            }))
            .child(image_cache(tile_cache).absolute().inset_0().children(tile_els))
            .children(arcs_el)
            .child(input)
            .children(markers)
            .children(tooltip)
            .children(empty)
            .child(self.render_map_filter(&t, cx))
            .children(self.render_map_banner(&t, pending.len(), cx))
            .child(self.render_map_controls(&t, &spots, pending.len(), &missing, cx))
            .children(self.render_map_popover(&t, &groups, set.as_deref(), place_links.as_deref(), cx))
            .children(self.render_map_missing(&t, &groups, &missing, cx))
            .child(
                div()
                    .id("map-attribution")
                    .occlude()
                    .absolute()
                    .right_0()
                    .bottom_0()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded_tl(px(6.))
                    .bg(t.surface.opacity(0.85))
                    .text_size(px(11.))
                    .text_color(t.ink_muted)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(ATTRIBUTION)
                    .on_click(|_, _, cx| cx.open_url(COPYRIGHT_URL)),
            )
            .into_any_element()
    }

    fn render_map_filter(&self, t: &Tokens, cx: &mut Context<Self>) -> impl IntoElement {
        let focus_name = self.focus_id.as_deref().and_then(|f| self.tree.person(f)).map(|p| p.name());
        let seg = |id: &'static str, label: String, f: MapFilter, cx: &mut Context<Self>| {
            let sel = self.map.filter == f;
            let ink = t.ink;
            div()
                .id(id)
                .flex()
                .items_center()
                .h(px(28.))
                .px(px(12.))
                .rounded_full()
                .max_w(px(280.))
                .truncate()
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .map(|d| if sel { d.bg(t.ink).text_color(t.on_fill()) } else { d.text_color(t.ink_muted).hover(move |s| s.text_color(ink)) })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if this.map.filter != f {
                        this.map.filter = f;
                        this.map.selected = None;
                        this.map.needs_fit = true;
                        cx.notify();
                    }
                }))
        };
        pill(t)
            .id("map-filter")
            .occlude()
            .absolute()
            .left(px(16.))
            .top(px(16.))
            .child(seg("map-all", i18n::t!("app.map.filter_all"), MapFilter::All, cx))
            .when_some(focus_name, |d, name| d.child(seg("map-anc", i18n::t!("app.map.filter_ancestors", name = name), MapFilter::Ancestors, cx)))
    }

    fn render_map_banner(&self, t: &Tokens, pending: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (text, primary, secondary): (String, String, Option<String>) = if self.map.net_error && self.map.consent {
            (i18n::t!("app.map.net_error"), i18n::t!("app.map.retry"), None)
        } else if !self.map.consent && !self.map.declined && pending > 0 {
            (i18n::tn!("app.map.consent", pending), i18n::t!("app.map.consent_yes"), Some(i18n::t!("app.map.consent_no")))
        } else {
            return None;
        };
        Some(
            div()
                .absolute()
                .top(px(16.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("map-banner")
                        .occlude()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .max_w(px(620.))
                        .mx(px(16.))
                        .mt(px(48.))
                        .pl(px(14.))
                        .pr(px(8.))
                        .py(px(8.))
                        .rounded(px(12.))
                        .bg(t.surface)
                        .border_1()
                        .border_color(t.border)
                        .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.08), offset: point(px(0.), px(2.)), blur_radius: px(10.), spread_radius: px(0.), inset: false }])
                        .child(icon(if self.map.net_error { IconName::TriangleAlert } else { IconName::Map }, 16., if self.map.net_error { t.warning } else { t.accent }))
                        .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(t.ink).child(text))
                        .when_some(secondary, |d, s| {
                            d.child(ui::outline_button(t, "map-decline").h(px(32.)).child(s).on_click(cx.listener(|this, _, _, cx| {
                                this.map.declined = true;
                                cx.notify();
                            })))
                        })
                        .child(ui::filled_button(t, "map-consent", t.accent).h(px(32.)).child(primary).on_click(cx.listener(|this, _, _, cx| this.give_consent(cx)))),
                )
                .into_any_element(),
        )
    }

    fn render_map_controls(&self, t: &Tokens, spots: &[Spot], pending: usize, missing: &[usize], cx: &mut Context<Self>) -> impl IntoElement {
        let btn = |id: &'static str, name: IconName| ui::icon_button(t, id, name, true).size(px(30.)).rounded_full();
        let tip = |key: &'static str| move |w: &mut Window, cx: &mut App| gpui_component::tooltip::Tooltip::new(i18n::t!(key)).build(w, cx);
        let worlds: Vec<(f64, f64)> = spots.iter().map(|s| s.world).collect();
        let zoom = pill(t)
            .id("map-zoom")
            .occlude()
            .child(btn("map-zoom-out", IconName::Minus).tooltip(tip("app.map.zoom_out")).on_click(cx.listener(|this, _, _, cx| {
                let z = this.map.camera.zoom.round() - 1.;
                this.map.zoom_to(z, None);
                cx.notify();
            })))
            .child(btn("map-zoom-in", IconName::Plus).tooltip(tip("app.map.zoom_in")).on_click(cx.listener(|this, _, _, cx| {
                let z = this.map.camera.zoom.round() + 1.;
                this.map.zoom_to(z, None);
                cx.notify();
            })))
            .child(div().w(px(1.)).h(px(18.)).bg(t.border))
            .child(btn("map-fit", IconName::Maximize).tooltip(tip("app.map.fit")).on_click(cx.listener(move |this, _, _, cx| {
                let size = this.map.size();
                if !worlds.is_empty() {
                    this.map.camera = Camera::fit(&worlds, size, FIT_PAD, FIT_MAX_ZOOM);
                }
                this.map.user_moved = false;
                cx.notify();
            })));
        let status = |id: &'static str, text: String| pill(t).id(id).occlude().px(px(14.)).text_size(px(12.)).text_color(t.ink_muted).child(text);
        let progress = self.map.progress.map(|(done, total)| status("map-progress", i18n::t!("app.map.progress", done = (done + 1).min(total), total = total)).child(icon(IconName::LoaderCircle, 14., t.accent)));
        // declined for now: a quiet way back to the consent banner
        let unlocated = (!self.map.consent && self.map.declined && pending > 0).then(|| {
            let accent = t.accent;
            status("map-unlocated", i18n::tn!("app.map.unlocated", pending)).cursor_pointer().hover(move |s| s.text_color(accent)).on_click(cx.listener(|this, _, _, cx| {
                this.map.declined = false;
                cx.notify();
            }))
        });
        let not_found = (!missing.is_empty() && self.map.progress.is_none()).then(|| {
            let accent = t.accent;
            status("map-missing", i18n::tn!("app.map.not_found", missing.len())).cursor_pointer().hover(move |s| s.text_color(accent)).on_click(cx.listener(|this, _, _, cx| {
                this.map.show_missing = !this.map.show_missing;
                this.map.selected = None;
                cx.notify();
            }))
        });
        div().absolute().left(px(16.)).bottom(px(16.)).flex().items_center().gap(px(8.)).child(zoom).children(progress).children(unlocated).children(not_found)
    }

    fn render_map_popover(&self, t: &Tokens, groups: &[PlaceGroup], set: Option<&HashSet<String>>, links: Option<&PlaceLinks>, cx: &mut Context<Self>) -> Option<AnyElement> {
        let keys = self.map.selected.as_ref()?;
        let chosen: Vec<&PlaceGroup> = keys.iter().filter_map(|k| groups.iter().find(|g| g.key == *k)).collect();
        if chosen.is_empty() {
            return None;
        }
        let focus = self.focus_id.clone();
        let mut body = div().flex().flex_col().gap(px(2.));
        if let Some(l) = links {
            body = body.child(self.render_map_links(t, l, cx)).child(div().px(px(8.)).pt(px(12.)).pb(px(4.)).child(ui::section_title(t, i18n::t!("app.map.links.events"))));
        }
        let mut rows = 0usize;
        let mut hidden = 0usize;
        for g in &chosen {
            let events: Vec<&gedcom::PlaceEvent> = g.events.iter().filter(|e| passes(set, &e.people)).collect();
            if chosen.len() > 1 {
                body = body.child(div().pt(px(10.)).pb(px(4.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(t.ink_muted).truncate().child(format!("{} · {}", g.name, events.len())));
            }
            for e in events {
                let sub = match e.year {
                    Some(y) => i18n::t!("app.map.event_year", label = e.event.label(), year = y),
                    None => e.event.label(),
                };
                for pid in &e.people {
                    if rows >= POPOVER_ROWS {
                        hidden += 1;
                        continue;
                    }
                    let Some(p) = self.tree.person(pid) else { continue };
                    let id = pid.clone();
                    let hover = t.canvas;
                    let is_focus = focus.as_deref() == Some(pid.as_str());
                    let sub = match self.map_kin(pid) {
                        Some(rel) => i18n::t!("app.map.event_relation", event = sub.clone(), rel = rel),
                        None => sub.clone(),
                    };
                    body = body.child(
                        div()
                            .id(("map-row", rows))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(8.))
                            .py(px(6.))
                            .rounded(px(8.))
                            .cursor_pointer()
                            .when(is_focus, |d| d.bg(t.accent_soft))
                            .hover(move |s| s.bg(hover))
                            .child(avatar(t, Some(p), 30.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .min_w_0()
                                    .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).truncate().child(p.name()))
                                    .child(div().text_size(px(12.)).text_color(t.ink_muted).truncate().child(sub)),
                            )
                            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                                if e.click_count() >= 2 {
                                    this.open_edit(id.clone(), window, cx);
                                } else {
                                    this.set_focus(id.clone(), true, window, cx);
                                }
                            })),
                    );
                    rows += 1;
                }
            }
        }
        if hidden > 0 {
            body = body.child(div().px(px(8.)).py(px(6.)).text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.map.more", n = hidden)));
        }
        let (title, count) = if chosen.len() == 1 {
            (chosen[0].name.clone(), chosen[0].events.iter().filter(|e| passes(set, &e.people)).count())
        } else {
            (i18n::tn!("app.map.places", chosen.len()), chosen.iter().map(|g| g.events.iter().filter(|e| passes(set, &e.people)).count()).sum())
        };
        Some(self.map_panel(t, "map-popover", title, i18n::tn!("app.map.events", count), body, cx))
    }

    fn render_map_missing(&self, t: &Tokens, groups: &[PlaceGroup], missing: &[usize], cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.map.show_missing || missing.is_empty() {
            return None;
        }
        let mut body = div().flex().flex_col().gap(px(2.));
        for &gi in missing.iter().take(MISSING_ROWS) {
            let g = &groups[gi];
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(8.))
                    .py(px(5.))
                    .child(icon(IconName::Map, 14., t.ink_muted))
                    .child(div().flex_1().min_w_0().text_size(px(13.)).truncate().child(g.name.clone()))
                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(g.events.len().to_string())),
            );
        }
        if missing.len() > MISSING_ROWS {
            body = body.child(div().px(px(8.)).py(px(6.)).text_size(px(12.)).text_color(t.ink_muted).child(i18n::t!("app.map.more", n = missing.len() - MISSING_ROWS)));
        }
        Some(self.map_panel(t, "map-missing-panel", i18n::t!("app.map.not_found_title"), i18n::t!("app.map.not_found_hint"), body, cx))
    }

    /// Floating list panel at the top right of the map.
    fn map_panel(&self, t: &Tokens, id: &'static str, title: String, sub: String, body: Div, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(id)
            .occlude()
            .absolute()
            .top(px(16.))
            .right(px(16.))
            .bottom(px(40.))
            .w(px(320.))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .max_h_full()
                    .rounded(px(14.))
                    .bg(t.surface)
                    .border_1()
                    .border_color(t.border)
                    .shadow(vec![BoxShadow { color: hsla(0., 0., 0., 0.10), offset: point(px(0.), px(4.)), blur_radius: px(16.), spread_radius: px(0.), inset: false }])
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .px(px(16.))
                            .pt(px(14.))
                            .pb(px(10.))
                            .border_b_1()
                            .border_color(t.border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).line_clamp(2).child(title))
                                    .child(div().text_size(px(12.)).text_color(t.ink_muted).child(sub)),
                            )
                            .child(ui::icon_button(t, "map-panel-close", IconName::Close, true).size(px(28.)).on_click(cx.listener(|this, _, _, cx| {
                                this.map.selected = None;
                                this.map.show_missing = false;
                                cx.notify();
                            }))),
                    )
                    .child(div().id("map-panel-scroll").flex_1().min_h_0().overflow_y_scroll().p(px(8.)).child(body)),
            )
            .into_any_element()
    }
}
