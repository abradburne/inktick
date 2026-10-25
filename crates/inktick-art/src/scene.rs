//! A view laid out at an output size, and composed frame by frame.
//!
//! Building a scene does the slow work once: every picture is resampled to the pixels it
//! covers, every segment rasterised to a coverage mask at its final size (so the LCD is crisp
//! at any size: it is vector art drawn for those pixels), and every layer behind the first
//! screen composed into a base picture. A frame then recomposes only the screens' rectangles:
//! the base, the segments at their darkness, their shadows, and the layers in front.

use crate::image::Image;
use crate::layout::{Blend, Bounds, Orientation, View, What};
use crate::optics::{Look, Optics};
use crate::svg::Screen;
use tiny_skia::{FillRule, Mask, Transform};

/// A rectangle of output pixels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Rect {
    fn clip(self, w: i32, h: i32) -> Rect {
        let x0 = self.x.max(0);
        let y0 = self.y.max(0);
        let x1 = (self.x + self.w).min(w);
        let y1 = (self.y + self.h).min(h);
        Rect {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }

    fn union(self, o: Rect) -> Rect {
        if self.w == 0 || self.h == 0 {
            return o;
        }
        if o.w == 0 || o.h == 0 {
            return self;
        }
        let x0 = self.x.min(o.x);
        let y0 = self.y.min(o.y);
        let x1 = (self.x + self.w).max(o.x + o.w);
        let y1 = (self.y + self.h).max(o.y + o.h);
        Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    fn intersect(self, o: Rect) -> Rect {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        Rect {
            x: x0,
            y: y0,
            w: (x1 - x0).max(0),
            h: (y1 - y0).max(0),
        }
    }

    fn grow(self, by: i32) -> Rect {
        Rect {
            x: self.x - by,
            y: self.y - by,
            w: self.w + 2 * by,
            h: self.h + 2 * by,
        }
    }
}

/// A segment's coverage, in output pixels.
#[derive(Clone)]
struct Coverage {
    rect: Rect,
    alpha: Vec<u8>,
}

/// A screen item's segments at their final size.
struct Raster {
    /// Which screen file (the unit's screen n).
    screen: usize,
    segments: Vec<(Coverage, [f32; 3], f32)>,
    shadows: Vec<Coverage>,
}

enum Kind {
    /// A picture already at its pixels, clipped to the output; its colour applied.
    Picture(Image),
    /// A screen's segments.
    Segments(Raster),
    /// The shadows of a screen's segments, multiplied in.
    Shadow(usize),
}

struct Layer {
    rect: Rect,
    blend: Blend,
    kind: Kind,
}

/// What a scene is made from.
pub struct Parts<'a> {
    /// The view to draw, or none for the plain look (each screen on a light LCD background).
    pub view: Option<&'a View>,
    /// The layout's elements, for the view's items.
    pub elements: Option<&'a std::collections::HashMap<String, crate::layout::Element>>,
    /// The unit's screens, by number.
    pub screens: &'a [Screen],
    /// The part of the view to show (view coordinates); none for the whole view.
    pub viewport: Option<Bounds>,
    /// Loads a picture of the artwork by file name.
    pub picture: &'a dyn Fn(&str) -> Option<Image>,
    /// For the plain look: two screens side by side rather than one above the other.
    pub side_by_side: bool,
    /// For the plain look: only this screen.
    pub only: Option<usize>,
}

/// Where screen `n` shows alone in a view: the screen and what is drawn about it (its
/// background, its frame, a gradient over it), not the rest of the case.
pub fn screen_viewport(view: &View, n: usize) -> Option<Bounds> {
    let mut vp = around(view, n)?;
    let screen = view.screen_bounds(n)?;
    // Nothing of another screen shows: towards each other screen the viewport stops at the
    // first item that begins past this screen's edge (the other screen's frame), or that is the
    // other screen's (centred on it: its backdrop, which may overlap this screen's), whichever
    // comes first; never short of this screen's middle.
    for m in (0..)
        .take_while(|&m| view.screen_bounds(m).is_some())
        .filter(|&m| m != n)
    {
        let other = view.screen_bounds(m)?;
        let inside = |b: &Bounds, s: &Bounds| {
            let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
            cx > s.x && cx < s.x + s.w && cy > s.y && cy < s.y + s.h
        };
        let theirs: Vec<Bounds> = view
            .items
            .iter()
            .map(|i| i.bounds)
            .filter(|b| inside(b, &other) && !inside(b, &screen))
            .collect();
        let items = view.items.iter().map(|i| i.bounds);
        if other.y >= screen.y + screen.h / 2.0 {
            let end = screen.y + screen.h;
            let edge = items
                .filter(|b| b.y >= end)
                .map(|b| b.y)
                .fold(other.y, f32::min);
            let edge = theirs.iter().map(|b| b.y).fold(edge, f32::min);
            let edge = edge.max(screen.y + screen.h / 2.0);
            vp.h = vp.h.min(edge - vp.y);
        } else if other.y + other.h <= screen.y + screen.h / 2.0 {
            let edge = items
                .map(|b| b.y + b.h)
                .filter(|&e| e <= screen.y)
                .fold(other.y + other.h, f32::max);
            let edge = theirs.iter().map(|b| b.y + b.h).fold(edge, f32::max);
            let edge = edge.min(screen.y + screen.h / 2.0);
            let bottom = vp.y + vp.h;
            vp.y = vp.y.max(edge);
            vp.h = bottom - vp.y;
        } else if other.x >= screen.x + screen.w / 2.0 {
            let end = screen.x + screen.w;
            let edge = items
                .filter(|b| b.x >= end)
                .map(|b| b.x)
                .fold(other.x, f32::min);
            let edge = theirs.iter().map(|b| b.x).fold(edge, f32::min);
            let edge = edge.max(screen.x + screen.w / 2.0);
            vp.w = vp.w.min(edge - vp.x);
        } else {
            let edge = items
                .map(|b| b.x + b.w)
                .filter(|&e| e <= screen.x)
                .fold(other.x + other.w, f32::max);
            let edge = theirs.iter().map(|b| b.x + b.w).fold(edge, f32::max);
            let edge = edge.min(screen.x + screen.w / 2.0);
            let right = vp.x + vp.w;
            vp.x = vp.x.max(edge);
            vp.w = right - vp.x;
        }
    }
    Some(vp)
}

/// A screen and the items centred on it (its backdrop, its frame).
fn around(view: &View, n: usize) -> Option<Bounds> {
    let screen = view.screen_bounds(n)?;
    let mut vp = screen;
    for item in &view.items {
        let b = item.bounds;
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        let centred =
            cx > screen.x && cx < screen.x + screen.w && cy > screen.y && cy < screen.y + screen.h;
        if centred && b.area() <= screen.area() * 3.0 {
            vp = vp.union(b);
        }
    }
    Some(vp)
}

/// A two-screen view cut in two between its screens, for showing the whole case across two
/// panels: the half with screen 0, the half with screen 1. Screens one above the other are cut
/// across, side by side down the middle.
pub fn halves(view: &View) -> Option<(Bounds, Bounds)> {
    let (a, b) = (view.screen_bounds(0)?, view.screen_bounds(1)?);
    let v = view.bounds;
    if b.y >= a.y + a.h / 2.0 {
        let cut = (a.y + a.h + b.y) / 2.0;
        Some((
            Bounds {
                x: v.x,
                y: v.y,
                w: v.w,
                h: cut - v.y,
            },
            Bounds {
                x: v.x,
                y: cut,
                w: v.w,
                h: v.y + v.h - cut,
            },
        ))
    } else {
        let cut = (a.x + a.w + b.x) / 2.0;
        Some((
            Bounds {
                x: v.x,
                y: v.y,
                w: cut - v.x,
                h: v.h,
            },
            Bounds {
                x: cut,
                y: v.y,
                w: v.x + v.w - cut,
                h: v.h,
            },
        ))
    }
}

/// The plain look's background behind a screen: a pale grey-green reflector.
const PLAIN_LCD: [u8; 4] = [204, 210, 196, 255];

/// A view at an output size.
pub struct Scene {
    width: i32,
    height: i32,
    layers: Vec<Layer>,
    /// The first layer that changes from frame to frame.
    first_live: usize,
    base: Vec<f32>,
    out: Vec<f32>,
    picture: Image,
    dirty: Rect,
    /// The segments' darkness as last drawn, by screen; empty before the first frame.
    seen: Vec<Vec<f32>>,
    /// The viewport's pixels: nothing is drawn outside it.
    clip: Rect,
    look: Look,
    /// The pixel rectangle of each screen item, by screen number (the first item of each).
    screen_rects: Vec<(usize, Rect)>,
    /// Every layer composed with all segments off: while building, and as 8-bit colour.
    off: Vec<f32>,
    off8: Vec<u8>,
    /// A frame's sums for one rectangle.
    acc: Vec<i32>,
    /// What each segment adds when fully on: see [`Delta`].
    deltas: Vec<Delta>,
}

/// What one segment adds to the picture at any darkness, over its place.
///
/// Every blend a view uses (alpha, multiply, add) is affine in what is under it and in what is
/// drawn, so a pixel's colour is a polynomial in the darkness L of the one segment there, of
/// the degree of the number of the segment's layers over it: its shadow and the screen drawn
/// once (degree 2), or twice as Panorama views draw it (3). The picture at L is the all-off
/// picture plus that cubic, kept as Newton's forward differences of the picture at L = 1/3,
/// 2/3 and 1: with t = 3L, `t·d1 + t(t−1)/2·d2 + t(t−1)(t−2)/6·d3`, exact for every view used.
/// A frame is then a sum over the segments that changed, whatever the layers above and below.
/// Where two segments' shadows fall on the same pixel their terms are added rather than
/// multiplied, an error below a level of 8-bit colour for the soft shadows used.
struct Delta {
    screen: usize,
    segment: usize,
    rect: Rect,
    /// The three differences, interleaved per channel: d1, d2, d3 for R, then for G and B; in
    /// [`FIXED`] units of 8-bit colour.
    rgb: Vec<i16>,
}

/// A scene's frames as data for a GPU ([`Scene::lcd`]).
pub struct Lcd<'a> {
    /// The picture's size.
    pub width: u32,
    /// Its height.
    pub height: u32,
    /// The all-off picture, 8-bit RGB, rows top first.
    pub off: &'a [u8],
    /// Each segment's differences.
    pub segments: Vec<LcdSegment<'a>>,
}

/// One segment's share of a frame: at darkness L, with t = 3L, the picture gains
/// `t·d1 + t(t−1)/2·d2 + t(t−1)(t−2)/6·d3` over `rect`, the d's in [`LCD_FIXED`] units of colour.
pub struct LcdSegment<'a> {
    /// Its screen.
    pub screen: usize,
    /// Its place in the screen's optics.
    pub segment: usize,
    /// Where it is drawn, in output pixels.
    pub rect: Rect,
    /// d1, d2, d3 for red, then for green and blue, per pixel, rows top first.
    pub rgb: &'a [i16],
}

/// The differences' scale: this much is a full step of colour (0 to 1).
pub const LCD_FIXED: f32 = FIXED;

/// The fixed-point scale of a frame's sums: 1/32 of a level of 8-bit colour in the
/// differences, times 1/256 in the weights. Integer arithmetic on 8- and 16-bit data keeps a
/// frame's memory traffic small, which is what costs on the handheld's cores.
const FIXED: f32 = 255.0 * 32.0;

impl Scene {
    /// Lays `parts` out in `width` × `height` pixels, centred and as large as fits.
    pub fn new(parts: &Parts, width: u32, height: u32, look: Look) -> Scene {
        let (w, h) = (width as i32, height as i32);
        let plain_items;
        let (items, view_bounds): (&[crate::layout::Item], Bounds) = match parts.view {
            Some(v) => (&v.items, v.bounds),
            None => {
                plain_items = plain(parts.screens, parts.side_by_side, parts.only);
                let b = plain_items
                    .iter()
                    .map(|i| i.bounds)
                    .reduce(Bounds::union)
                    .unwrap_or(Bounds::UNIT);
                (&plain_items, b)
            }
        };
        let vp = parts.viewport.unwrap_or(view_bounds);
        let s = (w as f32 / vp.w).min(h as f32 / vp.h);
        let ox = (w as f32 - vp.w * s) / 2.0 - vp.x * s;
        let oy = (h as f32 - vp.h * s) / 2.0 - vp.y * s;
        let to_px = |b: Bounds| {
            let x0 = (ox + b.x * s).round() as i32;
            let y0 = (oy + b.y * s).round() as i32;
            let x1 = (ox + (b.x + b.w) * s).round() as i32;
            let y1 = (oy + (b.y + b.h) * s).round() as i32;
            Rect {
                x: x0,
                y: y0,
                w: (x1 - x0).max(1),
                h: (y1 - y0).max(1),
            }
        };

        let mut layers = Vec::new();
        let mut screen_rects = Vec::new();
        for item in items {
            let rect = to_px(item.bounds);
            if rect.clip(w, h).w == 0 || rect.clip(w, h).h == 0 {
                continue;
            }
            match &item.what {
                What::Screen(n) => {
                    let Some(screen) = parts.screens.get(*n) else {
                        continue;
                    };
                    let raster = rasterise(screen, *n, rect, item.orientation, item.color.0, &look);
                    if !screen_rects.iter().any(|(k, _)| k == n) {
                        screen_rects.push((*n, rect));
                    }
                    let dark = raster
                        .segments
                        .iter()
                        .any(|(_, c, _)| c[0] + c[1] + c[2] < 1.5);
                    if look.shadow > 0.0
                        && dark
                        && matches!(item.blend, Blend::Multiply | Blend::Alpha)
                    {
                        let sh = shadow_rect(&raster);
                        layers.push(Layer {
                            rect: sh,
                            blend: Blend::Multiply,
                            kind: Kind::Shadow(layers.len() + 1),
                        });
                    }
                    layers.push(Layer {
                        rect,
                        blend: item.blend,
                        kind: Kind::Segments(raster),
                    });
                }
                What::Element { name, state } => {
                    let element = match (parts.elements.and_then(|e| e.get(name)), name.as_str()) {
                        (Some(e), _) => e.clone(),
                        (None, "inktick-plain") => plain_element(),
                        _ => continue,
                    };
                    let Some(img) = draw_element(
                        &element,
                        *state,
                        rect,
                        item.orientation,
                        item.color.0,
                        parts.picture,
                    ) else {
                        continue;
                    };
                    let clipped = rect.clip(w, h);
                    let img = crop(
                        &img,
                        Rect {
                            x: clipped.x - rect.x,
                            y: clipped.y - rect.y,
                            w: clipped.w,
                            h: clipped.h,
                        },
                    );
                    layers.push(Layer {
                        rect: clipped,
                        blend: item.blend,
                        kind: Kind::Picture(img),
                    });
                }
            }
        }
        let first_live = layers
            .iter()
            .position(|l| !matches!(l.kind, Kind::Picture(_)))
            .unwrap_or(layers.len());
        let n = (w * h * 3) as usize;
        let mut scene = Scene {
            width: w,
            height: h,
            layers,
            first_live,
            base: vec![0.0; n],
            out: vec![0.0; n],
            picture: Image::filled(width as usize, height as usize, [0, 0, 0, 255]),
            dirty: Rect::default(),
            seen: Vec::new(),
            off: Vec::new(),
            off8: Vec::new(),
            acc: Vec::new(),
            deltas: Vec::new(),
            clip: to_px(vp).clip(w, h),
            look,
            screen_rects,
        };
        let all = Rect { x: 0, y: 0, w, h };
        // Only the viewport shows: the rest of the view stays out of the letterbox.
        let clip = scene.clip;
        for layer in &scene.layers[..scene.first_live] {
            apply(&mut scene.base, w, layer, clip, &[]);
        }
        scene.out.copy_from_slice(&scene.base);
        // Everything behind the screens is drawn; later frames redraw only what can change.
        scene.dirty = scene.layers[scene.first_live..]
            .iter()
            .fold(Rect::default(), |r, l| r.union(l.rect))
            .clip(w, h)
            .intersect(clip);
        scene.prepare_deltas();
        scene.finish_picture(all);
        scene
    }

    /// The all-off picture and each segment's difference from it (see [`Delta`]).
    fn prepare_deltas(&mut self) {
        let counts: Vec<usize> = {
            let mut c = Vec::new();
            for l in &self.layers {
                if let Kind::Segments(r) = &l.kind {
                    if c.len() <= r.screen {
                        c.resize(r.screen + 1, 0);
                    }
                    c[r.screen] = c[r.screen].max(r.segments.len());
                }
            }
            c
        };
        let zero: Vec<Optics> = counts.iter().map(|&n| Optics::new(n)).collect();
        self.compose_rect(self.dirty, &zero);
        self.off = self.out.clone();
        let mut deltas = Vec::new();
        for (screen, &n) in counts.iter().enumerate() {
            for segment in 0..n {
                // Its place in every layer showing its screen: coverage and shadow.
                let mut rect = Rect::default();
                for l in &self.layers {
                    if let Kind::Segments(r) = &l.kind {
                        if r.screen == screen && segment < r.segments.len() {
                            rect = rect
                                .union(r.segments[segment].0.rect.union(r.shadows[segment].rect));
                        }
                    }
                }
                let rect = rect
                    .clip(self.width, self.height)
                    .intersect(self.clip)
                    .intersect(self.dirty);
                if rect.w == 0 || rect.h == 0 {
                    continue;
                }
                // The picture at a third, two thirds and all of the way on, less the off one.
                let mut samples = Vec::with_capacity(3);
                for level in [1.0 / 3.0, 2.0 / 3.0, 1.0] {
                    let mut one = zero.clone();
                    one[screen].level[segment] = level;
                    self.compose_rect(rect, &one);
                    let mut v = Vec::with_capacity((rect.w * rect.h * 3) as usize);
                    for y in rect.y..rect.y + rect.h {
                        let a = ((y * self.width + rect.x) * 3) as usize;
                        let b = a + (rect.w * 3) as usize;
                        v.extend(
                            self.out[a..b]
                                .iter()
                                .zip(&self.off[a..b])
                                .map(|(o, f)| o - f),
                        );
                        self.out[a..b].copy_from_slice(&self.off[a..b]);
                    }
                    samples.push(v);
                }
                let fixed = |v: f32| (v * FIXED).round().clamp(-32767.0, 32767.0) as i16;
                let mut rgb = Vec::with_capacity(samples[0].len() * 3);
                for ((a, b), c) in samples[0].iter().zip(&samples[1]).zip(&samples[2]) {
                    let (a, b, c) = (*a, *b, *c);
                    rgb.extend_from_slice(&[
                        fixed(a),
                        fixed(b - 2.0 * a),
                        fixed(c - 3.0 * b + 3.0 * a),
                    ]);
                }
                deltas.push(Delta {
                    screen,
                    segment,
                    rect,
                    rgb,
                });
            }
        }
        self.deltas = deltas;
        self.off8 = self
            .off
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect();
    }

    /// What a GPU needs to draw frames itself: the all-off picture (8-bit RGB) and each
    /// segment's cubic differences over its place (see [`Delta`]), in [`LCD_FIXED`] units.
    pub fn lcd(&self) -> Lcd<'_> {
        Lcd {
            width: self.width as u32,
            height: self.height as u32,
            off: &self.off8,
            segments: self
                .deltas
                .iter()
                .map(|d| LcdSegment {
                    screen: d.screen,
                    segment: d.segment,
                    rect: d.rect,
                    rgb: &d.rgb,
                })
                .collect(),
        }
    }

    /// Frees what only building needed (the layers' pictures and the composed background):
    /// frames come from the all-off picture and the segments' differences alone.
    pub fn compact(&mut self) {
        self.base = Vec::new();
        self.out = Vec::new();
        self.off = Vec::new();
        for l in &mut self.layers {
            if let Kind::Picture(img) = &mut l.kind {
                *img = Image::filled(0, 0, [0; 4]);
            }
        }
    }

    /// The output size.
    pub fn size(&self) -> (u32, u32) {
        (self.width as u32, self.height as u32)
    }

    /// Where screen `n` is drawn, if it is.
    pub fn screen_rect(&self, n: usize) -> Option<Rect> {
        self.screen_rects
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, r)| *r)
    }

    /// Each screen's segment addresses in the order [`Scene::compose`] takes their darkness.
    pub fn addresses(screens: &[Screen]) -> Vec<Vec<[u8; 3]>> {
        screens
            .iter()
            .map(|s| s.segments.iter().map(|g| g.addr).collect())
            .collect()
    }

    /// Composes a frame: `optics[n]` is screen n's segment darkness. Returns the picture and
    /// the rectangle that changed.
    pub fn compose(&mut self, optics: &[Optics]) -> (&Image, Vec<Rect>) {
        let rects = self.changes(optics);
        for &r in &rects {
            self.sum_rect(r, optics);
        }
        self.seen = optics.iter().map(|o| o.level.clone()).collect();
        (&self.picture, rects)
    }

    /// The all-off picture plus each segment's share at its darkness, over `r`.
    fn sum_rect(&mut self, r: Rect, optics: &[Optics]) {
        let r = r.intersect(self.clip);
        if r.w == 0 || r.h == 0 {
            return;
        }
        let w = self.width;
        let n = (r.w * r.h * 3) as usize;
        self.acc.clear();
        self.acc.reserve(n);
        for y in r.y..r.y + r.h {
            let a = ((y * w + r.x) * 3) as usize;
            let b = a + (r.w * 3) as usize;
            self.acc
                .extend(self.off8[a..b].iter().map(|&v| (v as i32) << 13));
        }
        for d in &self.deltas {
            let level = optics
                .get(d.screen)
                .and_then(|o| o.level.get(d.segment))
                .copied()
                .unwrap_or(0.0);
            if level <= 0.0 {
                continue;
            }
            let i = d.rect.intersect(r);
            if i.w == 0 || i.h == 0 {
                continue;
            }
            let t = 3.0 * level;
            let weight = |v: f32| (v * 256.0).round() as i32;
            let (w1, w2, w3) = (
                weight(t),
                weight(t * (t - 1.0) / 2.0),
                weight(t * (t - 1.0) * (t - 2.0) / 6.0),
            );
            for y in i.y..i.y + i.h {
                let src = (((y - d.rect.y) * d.rect.w + (i.x - d.rect.x)) * 9) as usize;
                let dst = (((y - r.y) * r.w + (i.x - r.x)) * 3) as usize;
                let len = (i.w * 3) as usize;
                for (o, s) in self.acc[dst..dst + len]
                    .iter_mut()
                    .zip(d.rgb[src..src + len * 3].chunks_exact(3))
                {
                    *o += w1 * s[0] as i32 + w2 * s[1] as i32 + w3 * s[2] as i32;
                }
            }
        }
        for y in 0..r.h {
            let src = (y * r.w * 3) as usize;
            let dst = (((y + r.y) * w + r.x) * 4) as usize;
            for x in 0..r.w as usize {
                let px = &mut self.picture.pixels[dst + x * 4..dst + x * 4 + 4];
                for (p, v) in px.iter_mut().zip(&self.acc[src + x * 3..src + x * 3 + 3]) {
                    *p = ((v + 4096) >> 13).clamp(0, 255) as u8;
                }
                px[3] = 255;
            }
        }
    }

    /// What must be drawn again: everything the first time, then the places of the segments
    /// whose darkness changed (their coverage and shadow), merged where they touch.
    fn changes(&self, optics: &[Optics]) -> Vec<Rect> {
        if self.seen.is_empty() {
            return vec![self.dirty];
        }
        let mut rects: Vec<Rect> = Vec::new();
        for layer in &self.layers[self.first_live..] {
            let Kind::Segments(r) = &layer.kind else {
                continue;
            };
            let new = optics.get(r.screen).map_or(&[][..], |o| &o.level[..]);
            let old = self.seen.get(r.screen).map_or(&[][..], |l| &l[..]);
            for (i, (cov, _, _)) in r.segments.iter().enumerate() {
                let (a, b) = (
                    new.get(i).copied().unwrap_or(0.0),
                    old.get(i).copied().unwrap_or(0.0),
                );
                if (a - b).abs() * 255.0 < 0.5 {
                    continue;
                }
                let area = cov
                    .rect
                    .union(r.shadows[i].rect)
                    .clip(self.width, self.height);
                if area.w > 0 && area.h > 0 {
                    rects.push(area);
                }
            }
        }
        merge(rects)
    }

    fn compose_rect(&mut self, dirty: Rect, optics: &[Optics]) {
        let dirty = dirty.intersect(self.clip);
        if dirty.w == 0 || dirty.h == 0 {
            return;
        }
        let w = self.width;
        for y in dirty.y..dirty.y + dirty.h {
            let a = ((y * w + dirty.x) * 3) as usize;
            let b = a + (dirty.w * 3) as usize;
            self.out[a..b].copy_from_slice(&self.base[a..b]);
        }
        let Scene {
            layers,
            out,
            look,
            first_live,
            ..
        } = self;
        let levels_of = |r: &Raster| optics.get(r.screen).map_or(&[][..], |o| &o.level[..]);
        for layer in &layers[*first_live..] {
            match &layer.kind {
                Kind::Shadow(of) => {
                    if let Kind::Segments(r) = &layers[*of].kind {
                        shadow(out, w, r, levels_of(r), dirty, look.shadow);
                    }
                }
                Kind::Segments(r) => apply(out, w, layer, dirty, levels_of(r)),
                Kind::Picture(_) => apply(out, w, layer, dirty, &[]),
            }
        }
        self.finish_picture(dirty);
    }

    /// The last composed picture.
    pub fn picture(&self) -> &Image {
        &self.picture
    }

    fn finish_picture(&mut self, r: Rect) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                let i = ((y * self.width + x) * 3) as usize;
                let o = ((y * self.width + x) * 4) as usize;
                for c in 0..3 {
                    self.picture.pixels[o + c] =
                        (self.out[i + c].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
                self.picture.pixels[o + 3] = 255;
            }
        }
    }
}

/// Rectangles merged wherever they overlap or nearly touch, so neighbouring segments are drawn
/// in one pass and nothing is drawn twice.
fn merge(mut rects: Vec<Rect>) -> Vec<Rect> {
    let near = |a: Rect, b: Rect| {
        let g = 8;
        a.x - g < b.x + b.w && b.x - g < a.x + a.w && a.y - g < b.y + b.h && b.y - g < a.y + a.h
    };
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for i in 0..rects.len() {
            for j in i + 1..rects.len() {
                if near(rects[i], rects[j]) {
                    let u = rects[i].union(rects[j]);
                    rects[i] = u;
                    rects.swap_remove(j);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    rects
}

/// The plain look: each screen on a pale reflector; two screens one above the other, or side
/// by side for units that hold them so.
fn plain(screens: &[Screen], side_by_side: bool, only: Option<usize>) -> Vec<crate::layout::Item> {
    use crate::layout::{Color, Item};
    let mut out = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    for (n, s) in screens.iter().enumerate() {
        if only.is_some_and(|o| o != n) {
            continue;
        }
        let h = 1000.0;
        let w = h * s.aspect();
        let b = Bounds { x, y, w, h };
        out.push(Item {
            input: None,
            what: What::Element {
                name: "inktick-plain".into(),
                state: 0,
            },
            bounds: b,
            color: Color::WHITE,
            blend: Blend::Alpha,
            orientation: Orientation::default(),
        });
        out.push(Item {
            input: None,
            what: What::Screen(n),
            bounds: b,
            color: Color::WHITE,
            blend: Blend::Alpha,
            orientation: Orientation::default(),
        });
        if side_by_side {
            x += w + 40.0;
        } else {
            y += h + 40.0;
        }
    }
    out
}

fn plain_element() -> crate::layout::Element {
    use crate::layout::{Color, Component, Element, Part};
    let c = PLAIN_LCD.map(|v| v as f32 / 255.0);
    Element {
        components: vec![Component {
            part: Part::Rect,
            bounds: Bounds::UNIT,
            color: Color(c),
            state: None,
        }],
        default_state: 0,
    }
}

fn oriented(img: Image, o: Orientation) -> Image {
    let mut img = img;
    for _ in 0..o.quarter_turns {
        img = img.rotated_90();
    }
    if o.swap_xy {
        img = img.rotated_90().flipped_x();
    }
    if o.flip_x {
        img = img.flipped_x();
    }
    if o.flip_y {
        img = img.flipped_y();
    }
    img
}

/// Draws an element's components for `state` into a picture of `rect`'s size, its colour
/// applied.
fn draw_element(
    e: &crate::layout::Element,
    state: i32,
    rect: Rect,
    o: Orientation,
    tint: [f32; 4],
    picture: &dyn Fn(&str) -> Option<Image>,
) -> Option<Image> {
    let own = e.bounds();
    let (w, h) = (rect.w as usize, rect.h as usize);
    let mut acc = vec![0f32; w * h * 4];
    let mut drew = false;
    for c in &e.components {
        if c.state.is_some_and(|s| s != state) {
            continue;
        }
        let sx = w as f32 / own.w;
        let sy = h as f32 / own.h;
        let px = Rect {
            x: ((c.bounds.x - own.x) * sx).round() as i32,
            y: ((c.bounds.y - own.y) * sy).round() as i32,
            w: (c.bounds.w * sx).round().max(1.0) as i32,
            h: (c.bounds.h * sy).round().max(1.0) as i32,
        };
        let color = c.color.0;
        let src = match &c.part {
            crate::layout::Part::Image { file, alpha_file } => {
                let Some(mut img) = picture(file) else {
                    continue;
                };
                if let Some(af) = alpha_file.as_deref().and_then(picture) {
                    let af = af.resized(img.width, img.height);
                    for (p, a) in img
                        .pixels
                        .chunks_exact_mut(4)
                        .zip(af.pixels.chunks_exact(4))
                    {
                        p[3] = ((a[0] as u32 + a[1] as u32 + a[2] as u32) / 3) as u8;
                    }
                }
                img.resized(px.w as usize, px.h as usize)
            }
            crate::layout::Part::Rect => Image::filled(px.w as usize, px.h as usize, [255; 4]),
        };
        drew = true;
        for y in 0..px.h {
            let ty = px.y + y;
            if ty < 0 || ty >= h as i32 {
                continue;
            }
            for x in 0..px.w {
                let tx = px.x + x;
                if tx < 0 || tx >= w as i32 {
                    continue;
                }
                let p = src.at(x as usize, y as usize);
                let a = p[3] as f32 / 255.0 * color[3];
                let d = &mut acc[(ty as usize * w + tx as usize) * 4..][..4];
                for k in 0..3 {
                    let s = p[k] as f32 / 255.0 * color[k];
                    d[k] = s * a + d[k] * (1.0 - a);
                }
                d[3] = a + d[3] * (1.0 - a);
            }
        }
    }
    if !drew {
        return None;
    }
    let mut img = Image::filled(w, h, [0; 4]);
    for (p, d) in img.pixels.chunks_exact_mut(4).zip(acc.chunks_exact(4)) {
        let a = d[3];
        for k in 0..3 {
            let straight = if a > 0.0 { d[k] / a } else { 0.0 };
            p[k] = ((straight * tint[k]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
        p[3] = ((a * tint[3]).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    }
    Some(oriented(img, o))
}

fn crop(img: &Image, r: Rect) -> Image {
    let mut out = Image::filled(r.w as usize, r.h as usize, [0; 4]);
    for y in 0..r.h as usize {
        let s = ((r.y as usize + y) * img.width + r.x as usize) * 4;
        let d = y * r.w as usize * 4;
        out.pixels[d..d + r.w as usize * 4].copy_from_slice(&img.pixels[s..s + r.w as usize * 4]);
    }
    out
}

/// Each segment's coverage at `rect`, the screen's viewBox stretched to fill it.
fn rasterise(
    screen: &Screen,
    n: usize,
    rect: Rect,
    o: Orientation,
    color: [f32; 4],
    look: &Look,
) -> Raster {
    let [vx, vy, vw, vh] = screen.view;
    let (w, h) = (rect.w as f32, rect.h as f32);
    // Orientation about the screen's own box, then the box into the rectangle.
    let mut t = Transform::from_translate(-vx, -vy).post_scale(1.0 / vw, 1.0 / vh);
    let unit_flip = |t: Transform, fx: bool, fy: bool| {
        t.post_concat(Transform::from_row(
            if fx { -1.0 } else { 1.0 },
            0.0,
            0.0,
            if fy { -1.0 } else { 1.0 },
            if fx { 1.0 } else { 0.0 },
            if fy { 1.0 } else { 0.0 },
        ))
    };
    for _ in 0..o.quarter_turns {
        // A quarter turn clockwise in the unit square: (x, y) → (1 − y, x).
        t = t.post_concat(Transform::from_row(0.0, 1.0, -1.0, 0.0, 1.0, 0.0));
    }
    if o.swap_xy {
        t = t.post_concat(Transform::from_row(0.0, 1.0, 1.0, 0.0, 0.0, 0.0));
    }
    t = unit_flip(t, o.flip_x, o.flip_y);
    let t = t
        .post_scale(w, h)
        .post_translate(rect.x as f32, rect.y as f32);
    let blur = (look.shadow_blur * h).round().max(0.0) as i32;
    let off = [
        (look.shadow_offset[0] * h).round() as i32,
        (look.shadow_offset[1] * h).round() as i32,
    ];
    let mut segments = Vec::new();
    let mut shadows = Vec::new();
    for seg in &screen.segments {
        let Some(p) = seg.path.clone().transform(t) else {
            continue;
        };
        let b = p.bounds();
        let r = Rect {
            x: b.left().floor() as i32,
            y: b.top().floor() as i32,
            w: (b.right().ceil() - b.left().floor()) as i32 + 1,
            h: (b.bottom().ceil() - b.top().floor()) as i32 + 1,
        };
        let cov = coverage(
            &p,
            r,
            if seg.even_odd {
                FillRule::EvenOdd
            } else {
                FillRule::Winding
            },
        );
        let c = seg.color.map(|v| v as f32 / 255.0);
        let sh = blurred(&cov, blur, off);
        segments.push((
            cov,
            [c[0] * color[0], c[1] * color[1], c[2] * color[2]],
            (seg.opacity * look.contrast).min(1.0) * color[3],
        ));
        shadows.push(sh);
    }
    Raster {
        screen: n,
        segments,
        shadows,
    }
}

fn coverage(p: &tiny_skia::Path, r: Rect, rule: FillRule) -> Coverage {
    let Some(mut mask) = Mask::new(r.w.max(1) as u32, r.h.max(1) as u32) else {
        return Coverage {
            rect: r,
            alpha: vec![0; (r.w * r.h).max(0) as usize],
        };
    };
    mask.fill_path(
        p,
        rule,
        true,
        Transform::from_translate(-r.x as f32, -r.y as f32),
    );
    Coverage {
        rect: r,
        alpha: mask.data().to_vec(),
    }
}

/// A coverage grown by `radius`, box-blurred twice (close to a Gaussian), moved by `off`.
fn blurred(c: &Coverage, radius: i32, off: [i32; 2]) -> Coverage {
    let r = Rect {
        x: c.rect.x + off[0],
        y: c.rect.y + off[1],
        ..c.rect
    }
    .grow(2 * radius);
    let (w, h) = (r.w as usize, r.h as usize);
    let mut a = vec![0f32; w * h];
    let pad = (2 * radius) as usize;
    for y in 0..c.rect.h as usize {
        for x in 0..c.rect.w as usize {
            a[(y + pad) * w + x + pad] = c.alpha[y * c.rect.w as usize + x] as f32;
        }
    }
    for _ in 0..2 {
        if radius > 0 {
            box_blur(&mut a, w, h, radius as usize);
        }
    }
    Coverage {
        rect: r,
        alpha: a
            .iter()
            .map(|v| v.round().clamp(0.0, 255.0) as u8)
            .collect(),
    }
}

fn box_blur(a: &mut [f32], w: usize, h: usize, r: usize) {
    let n = (2 * r + 1) as f32;
    let mut row = vec![0f32; w.max(h)];
    for y in 0..h {
        let line = &mut a[y * w..(y + 1) * w];
        let mut sum = 0.0;
        for x in 0..w + r {
            if x < w {
                sum += line[x];
            }
            if x > 2 * r {
                sum -= line[x - 2 * r - 1];
            }
            if x >= r {
                row[x - r] = sum / n;
            }
        }
        line.copy_from_slice(&row[..w]);
    }
    for x in 0..w {
        let mut sum = 0.0;
        for y in 0..h + r {
            if y < h {
                sum += a[y * w + x];
            }
            if y > 2 * r {
                sum -= a[(y - 2 * r - 1) * w + x];
            }
            if y >= r {
                row[y - r] = sum / n;
            }
        }
        for y in 0..h {
            a[y * w + x] = row[y];
        }
    }
}

fn shadow_rect(r: &Raster) -> Rect {
    r.shadows
        .iter()
        .fold(Rect::default(), |acc, s| acc.union(s.rect))
}

/// Multiplies the shadows of a screen's segments into `out`.
fn shadow(out: &mut [f32], w: i32, r: &Raster, levels: &[f32], clip: Rect, strength: f32) {
    let h = out.len() as i32 / 3 / w;
    for (i, sh) in r.shadows.iter().enumerate() {
        let lvl = levels.get(i).copied().unwrap_or(0.0) * strength * r.segments[i].2;
        if lvl <= 0.0 {
            continue;
        }
        let area = sh.rect.clip(w, h);
        for y in area.y.max(clip.y)..(area.y + area.h).min(clip.y + clip.h) {
            for x in area.x.max(clip.x)..(area.x + area.w).min(clip.x + clip.w) {
                let a = sh.alpha[((y - sh.rect.y) * sh.rect.w + (x - sh.rect.x)) as usize] as f32
                    / 255.0
                    * lvl;
                let o = ((y * w + x) * 3) as usize;
                for c in 0..3 {
                    out[o + c] *= 1.0 - a;
                }
            }
        }
    }
}

/// Blends one layer into `out` within `clip`.
fn apply(out: &mut [f32], w: i32, layer: &Layer, clip: Rect, levels: &[f32]) {
    let h = out.len() as i32 / 3 / w;
    let area = layer.rect.clip(w, h);
    let x0 = area.x.max(clip.x);
    let y0 = area.y.max(clip.y);
    let x1 = (area.x + area.w).min(clip.x + clip.w);
    let y1 = (area.y + area.h).min(clip.y + clip.h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    match &layer.kind {
        Kind::Picture(img) => {
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = img.at((x - layer.rect.x) as usize, (y - layer.rect.y) as usize);
                    let a = p[3] as f32 / 255.0;
                    let s = [
                        p[0] as f32 / 255.0,
                        p[1] as f32 / 255.0,
                        p[2] as f32 / 255.0,
                    ];
                    blend(
                        &mut out[((y * w + x) * 3) as usize..][..3],
                        s,
                        a,
                        layer.blend,
                    );
                }
            }
        }
        Kind::Segments(r) => {
            // The screen's own picture in its rectangle: segments over transparency.
            let (rw, rh) = (x1 - x0, y1 - y0);
            let mut px = vec![0f32; (rw * rh * 4) as usize];
            for (i, (cov, color, opacity)) in r.segments.iter().enumerate() {
                let lvl = levels.get(i).copied().unwrap_or(0.0) * opacity;
                if lvl <= 0.0 {
                    continue;
                }
                let c = cov.rect;
                for y in c.y.max(y0)..(c.y + c.h).min(y1) {
                    for x in c.x.max(x0)..(c.x + c.w).min(x1) {
                        let a =
                            cov.alpha[((y - c.y) * c.w + (x - c.x)) as usize] as f32 / 255.0 * lvl;
                        if a <= 0.0 {
                            continue;
                        }
                        let d = &mut px[(((y - y0) * rw + (x - x0)) * 4) as usize..][..4];
                        for k in 0..3 {
                            d[k] = color[k] * a + d[k] * (1.0 - a);
                        }
                        d[3] = a + d[3] * (1.0 - a);
                    }
                }
            }
            for y in 0..rh {
                for x in 0..rw {
                    let d = &px[((y * rw + x) * 4) as usize..][..4];
                    if d[3] <= 0.0 {
                        continue;
                    }
                    let s = [d[0] / d[3], d[1] / d[3], d[2] / d[3]];
                    blend(
                        &mut out[(((y + y0) * w + x + x0) * 3) as usize..][..3],
                        s,
                        d[3],
                        layer.blend,
                    );
                }
            }
        }
        Kind::Shadow(_) => {}
    }
}

fn blend(d: &mut [f32], s: [f32; 3], a: f32, mode: Blend) {
    match mode {
        Blend::Alpha => {
            for k in 0..3 {
                d[k] = s[k] * a + d[k] * (1.0 - a);
            }
        }
        Blend::None => d.copy_from_slice(&s),
        Blend::Multiply => {
            for k in 0..3 {
                d[k] *= s[k] * a + (1.0 - a);
            }
        }
        Blend::Add => {
            for k in 0..3 {
                d[k] = (d[k] + s[k] * a).min(1.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screens_panel_shows_nothing_of_the_other_screen() {
        // Black Jack's layout: each screen's backdrop overlaps the other's, and the top screen
        // reaches past where the bottom one's backdrop begins.
        let lay = r#"<mamelayout version="2">
          <element name="Top"><rect><color red="1" green="0" blue="0"/></rect></element>
          <element name="Bottom"><rect><color red="0" green="0" blue="1"/></rect></element>
          <view name="Backgrounds Only">
            <element ref="Top"><bounds x="0" y="0" width="1290" height="854"/></element>
            <screen index="0"><bounds x="6" y="3" width="1289" height="887"/></screen>
            <element ref="Bottom"><bounds x="0" y="819" width="1290" height="854"/></element>
            <screen index="1"><bounds x="21" y="858" width="1273" height="854"/></screen>
          </view></mamelayout>"#;
        let layout = crate::layout::Layout::parse(lay).unwrap();
        let view = layout.view("Backgrounds Only").unwrap();
        let top = screen_viewport(view, 0).unwrap();
        let bottom = screen_viewport(view, 1).unwrap();
        assert!(top.y + top.h <= 819.0, "{top:?}");
        assert!(bottom.y >= 854.0, "{bottom:?}");
        assert!(top.y + top.h <= bottom.y, "{top:?} {bottom:?}");
    }

    #[test]
    fn the_sum_of_segments_is_the_layers_composed() {
        // Two segments side by side with soft shadows, at in-between darkness: the sum of
        // their sums must match composing every layer, within two levels of 8-bit colour.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
          <g style="fill:#000000;fill-opacity:0.85"><title>0.0.0</title><rect x="20" y="40" width="20" height="20"/></g>
          <g style="fill:#203040;fill-opacity:0.85"><title>0.1.0</title><rect x="44" y="40" width="20" height="20"/></g></svg>"##;
        let screens = [crate::svg::parse(svg).unwrap()];
        let none = |_: &str| None;
        let parts = Parts {
            view: None,
            elements: None,
            screens: &screens,
            viewport: None,
            picture: &none,
            side_by_side: false,
            only: None,
        };
        let look = Look {
            shadow: 0.5,
            shadow_blur: 0.02,
            shadow_offset: [0.02, 0.03],
            ..Look::default()
        };
        let mut scene = Scene::new(&parts, 200, 200, look);
        let mut o = Optics::new(2);
        o.level = vec![0.6, 0.3];
        let optics = [o];
        let summed = scene.compose(&optics).0.clone();
        let all = Rect {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        };
        scene.compose_rect(all, &optics);
        let composed = scene.picture().clone();
        let worst = summed
            .pixels
            .iter()
            .zip(&composed.pixels)
            .map(|(a, b)| (*a as i32 - *b as i32).abs())
            .max()
            .unwrap();
        // Each segment's shadow falls on the other: their terms add rather than multiply.
        assert!(worst <= 2, "{worst}");
    }

    #[test]
    fn a_plain_scene_draws_a_lit_segment() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
          <g style="fill:#000000"><title>0.0.0</title><rect x="40" y="40" width="20" height="20"/></g></svg>"##;
        let screen = crate::svg::parse(svg).unwrap();
        let screens = [screen];
        let none = |_: &str| None;
        let parts = Parts {
            view: None,
            elements: None,
            screens: &screens,
            viewport: None,
            picture: &none,
            side_by_side: false,
            only: None,
        };
        let mut scene = Scene::new(&parts, 100, 100, Look::SHARP);
        let mut o = Optics::new(1);
        let (img, _) = scene.compose(std::slice::from_ref(&o));
        assert_eq!(img.at(50, 50), PLAIN_LCD);
        o.level[0] = 1.0;
        let (img, dirty) = scene.compose(std::slice::from_ref(&o));
        assert_eq!(img.at(50, 50), [0, 0, 0, 255]);
        assert_eq!(img.at(10, 10), PLAIN_LCD);
        assert_eq!(dirty.len(), 1, "only the segment's place");
        assert!(dirty[0].w < 30 && dirty[0].h < 30);
        let (_, dirty) = scene.compose(std::slice::from_ref(&o));
        assert!(dirty.is_empty(), "nothing changed");
    }
}
