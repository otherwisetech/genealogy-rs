//! Web-Mercator math. "World" coordinates are pixels at zoom 0 (0..256 on both axes, y down);
//! at zoom `z` multiply by 2^z.
use std::f64::consts::PI;

pub const TILE: f64 = 256.;
pub const MIN_ZOOM: f64 = 1.;
pub const MAX_ZOOM: f64 = 18.;
/// Web-Mercator cuts the poles here
pub const MAX_LAT: f64 = 85.051_128_78;

pub fn to_world(lat: f64, lon: f64) -> (f64, f64) {
    let lat = lat.clamp(-MAX_LAT, MAX_LAT).to_radians();
    let x = (lon + 180.) / 360. * TILE;
    let y = (1. - (lat.tan() + 1. / lat.cos()).ln() / PI) / 2. * TILE;
    (x, y)
}

#[cfg(test)]
pub fn to_lat_lon(x: f64, y: f64) -> (f64, f64) {
    let lon = x / TILE * 360. - 180.;
    let n = PI - 2. * PI * y / TILE;
    (n.sinh().atan().to_degrees(), lon)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// world point at the centre of the viewport
    pub center: (f64, f64),
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { center: (TILE / 2., TILE / 2.), zoom: 2. }
    }
}

impl Camera {
    pub fn scale(&self) -> f64 {
        self.zoom.exp2()
    }

    /// world → viewport-local screen px
    pub fn to_screen(self, w: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        let s = self.scale();
        ((w.0 - self.center.0) * s + size.0 / 2., (w.1 - self.center.1) * s + size.1 / 2.)
    }

    pub fn to_world(self, p: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        let s = self.scale();
        ((p.0 - size.0 / 2.) / s + self.center.0, (p.1 - size.1 / 2.) / s + self.center.1)
    }

    pub fn pan_px(&mut self, dx: f64, dy: f64) {
        let s = self.scale();
        self.center = (self.center.0 - dx / s, self.center.1 - dy / s);
        self.clamp();
    }

    /// Zoom to `zoom` keeping the world point under `anchor` (screen px) still.
    pub fn zoom_around(&mut self, anchor: (f64, f64), size: (f64, f64), zoom: f64) {
        let before = self.to_world(anchor, size);
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let after = self.to_world(anchor, size);
        self.center = (self.center.0 + before.0 - after.0, self.center.1 + before.1 - after.1);
        self.clamp();
    }

    /// Keeps the centre on the map (x wraps visually, but we don't scroll past the date line).
    pub fn clamp(&mut self) {
        self.center = (self.center.0.clamp(0., TILE), self.center.1.clamp(0., TILE));
    }

    /// Camera showing every world point with `pad` screen px of margin, at most `max_zoom`.
    pub fn fit(points: &[(f64, f64)], size: (f64, f64), pad: f64, max_zoom: f64) -> Camera {
        if points.is_empty() || size.0 <= 0. || size.1 <= 0. {
            return Camera::default();
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in points {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        let (w, h) = ((size.0 - 2. * pad).max(1.), (size.1 - 2. * pad).max(1.));
        let zx = if x1 > x0 { (w / (x1 - x0)).log2() } else { max_zoom };
        let zy = if y1 > y0 { (h / (y1 - y0)).log2() } else { max_zoom };
        Camera { center: ((x0 + x1) / 2., (y0 + y1) / 2.), zoom: zx.min(zy).clamp(MIN_ZOOM, max_zoom) }
    }

    /// Integer tile zoom used for this camera.
    pub fn tile_zoom(&self) -> u8 {
        self.zoom.round().clamp(0., MAX_ZOOM) as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TileKey {
    pub z: u8,
    pub x: u32,
    pub y: u32,
}

/// A tile to draw: which image, and where (screen px, square side).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub key: TileKey,
    pub left: f64,
    pub top: f64,
    pub side: f64,
}

/// Tiles covering the viewport at the camera's tile zoom. x wraps around the date line; rows
/// outside the map are skipped.
pub fn visible_tiles(cam: &Camera, size: (f64, f64)) -> Vec<Placed> {
    let z = cam.tile_zoom();
    let n = 1u32 << z;
    let side = TILE * (cam.zoom - z as f64).exp2();
    let (wx0, wy0) = cam.to_world((0., 0.), size);
    let (wx1, wy1) = cam.to_world(size, size);
    let per = TILE / n as f64;
    let (tx0, tx1) = ((wx0 / per).floor() as i64, (wx1 / per).ceil() as i64 - 1);
    let (ty0, ty1) = (((wy0 / per).floor() as i64).max(0), ((wy1 / per).ceil() as i64 - 1).min(n as i64 - 1));
    let mut out = Vec::new();
    // more columns than the world has would only repeat it
    if tx1 - tx0 > n as i64 + 1 {
        return out;
    }
    for ty in ty0..=ty1 {
        for tx in tx0..=tx1 {
            let (left, top) = cam.to_screen((tx as f64 * per, ty as f64 * per), size);
            out.push(Placed { key: TileKey { z, x: tx.rem_euclid(n as i64) as u32, y: ty as u32 }, left, top, side });
        }
    }
    // centre first, so the first downloads are what the eye lands on
    let c = (size.0 / 2., size.1 / 2.);
    out.sort_by(|a, b| {
        let d = |p: &Placed| (p.left + p.side / 2. - c.0).powi(2) + (p.top + p.side / 2. - c.1).powi(2);
        d(a).total_cmp(&d(b))
    });
    out
}

/// Marker radius in px for a place with `n` events.
pub fn marker_radius(n: usize) -> f64 {
    (9. + 4. * (n.max(1) as f64).ln()).min(24.)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub x: f64,
    pub y: f64,
    pub count: usize,
    /// indices into the input
    pub members: Vec<usize>,
}

/// Greedy screen-space clustering: biggest places first; a point joins the first cluster whose
/// circle it would overlap. The cluster keeps its biggest member's position.
pub fn cluster(points: &[(f64, f64, usize)]) -> Vec<Cluster> {
    let mut order: Vec<usize> = (0..points.len()).collect();
    order.sort_by(|&a, &b| points[b].2.cmp(&points[a].2).then(a.cmp(&b)));
    let mut out: Vec<Cluster> = Vec::new();
    for i in order {
        let (x, y, n) = points[i];
        match out.iter_mut().find(|c| ((c.x - x).powi(2) + (c.y - y).powi(2)).sqrt() < marker_radius(c.count) + marker_radius(n) + 2.) {
            Some(c) => {
                c.count += n;
                c.members.push(i);
            }
            None => out.push(Cluster { x, y, count: n, members: vec![i] }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn mercator_round_trip() {
        assert_eq!(to_world(0., 0.), (128., 128.));
        let (x, y) = to_world(MAX_LAT, -180.);
        assert!(close(x, 0.) && y.abs() < 1e-4);
        for (lat, lon) in [(4.6, -74.1), (-33.9, 151.2), (60., 10.), (0., 179.9)] {
            let (x, y) = to_world(lat, lon);
            let (la, lo) = to_lat_lon(x, y);
            assert!(close(la, lat) && close(lo, lon), "{lat},{lon} → {la},{lo}");
        }
        assert!(to_world(45., 0.).1 < 128., "north is up");
    }

    #[test]
    fn camera_screen_world() {
        let size = (800., 600.);
        let mut c = Camera { center: (100., 50.), zoom: 3. };
        assert_eq!(c.to_screen((100., 50.), size), (400., 300.));
        assert_eq!(c.to_screen((101., 50.), size), (408., 300.));
        let p = c.to_world((10., 20.), size);
        let s = c.to_screen(p, size);
        assert!(close(s.0, 10.) && close(s.1, 20.));
        let anchor = (700., 100.);
        let w = c.to_world(anchor, size);
        c.zoom_around(anchor, size, 5.5);
        let s = c.to_screen(w, size);
        assert!(close(s.0, anchor.0) && close(s.1, anchor.1), "anchor stays put");
        c.zoom_around(anchor, size, 99.);
        assert_eq!(c.zoom, MAX_ZOOM);
        c.pan_px(1e9, 0.);
        assert_eq!(c.center.0, 0.);
    }

    #[test]
    fn fit_contains_points() {
        let pts = [to_world(10., -20.), to_world(-5., 30.), to_world(40., 0.)];
        let size = (900., 600.);
        let c = Camera::fit(&pts, size, 40., 12.);
        for p in pts {
            let (x, y) = c.to_screen(p, size);
            assert!((39.9..=860.1).contains(&x) && (39.9..=560.1).contains(&y), "{x},{y}");
        }
        let one = Camera::fit(&pts[..1], size, 40., 12.);
        assert_eq!((one.center, one.zoom), (pts[0], 12.));
        assert_eq!(Camera::fit(&[], size, 40., 12.), Camera::default());
    }

    #[test]
    fn tiles_cover_viewport() {
        let size = (512., 256.);
        let c = Camera { center: (128., 128.), zoom: 1. };
        let t = visible_tiles(&c, size);
        assert_eq!(t.len(), 4, "the whole z1 world is 2×2");
        assert!(t.iter().all(|p| p.side == 256. && p.key.z == 1));
        let mut keys: Vec<_> = t.iter().map(|p| (p.key.x, p.key.y)).collect();
        keys.sort();
        assert_eq!(keys, vec![(0, 0), (0, 1), (1, 0), (1, 1)]);
        let tl = t.iter().find(|p| p.key.x == 0 && p.key.y == 0).unwrap();
        assert_eq!((tl.left, tl.top), (0., -128.));
        // fractional zoom: tiles of the rounded level, scaled
        let c = Camera { center: (128., 128.), zoom: 4.4 };
        let t = visible_tiles(&c, (300., 200.));
        assert!(t.iter().all(|p| p.key.z == 4 && close(p.side, 256. * 0.4f64.exp2())));
        // wraps across the date line, skips rows off the map
        let c = Camera { center: (0., 0.), zoom: 2. };
        let t = visible_tiles(&c, (512., 512.));
        assert!(t.iter().all(|p| p.key.x < 4 && p.key.y < 4));
        assert!(t.iter().any(|p| p.key.x == 3), "west of x=0 is the last column");
        assert_eq!(t.len(), 2, "one row (y < 0 is off the map) × two columns");
    }

    #[test]
    fn clustering() {
        let pts = [(100., 100., 1), (105., 100., 10), (300., 300., 2), (400., 100., 1)];
        let c = cluster(&pts);
        assert_eq!(c.len(), 3);
        assert_eq!((c[0].x, c[0].count, c[0].members.clone()), (105., 11, vec![1, 0]));
        assert!(c.iter().all(|c| c.members.len() == 1 || c.count == 11));
        assert!(marker_radius(1) < marker_radius(50) && marker_radius(10_000) == 24.);
    }
}
