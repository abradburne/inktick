//! What each panel shows of the unit, and the LCD's optics behind it.
//!
//! | Unit | View "lcd" (default) | View "unit" | View "plain" |
//! |---|---|---|---|
//! | one screen | the screen on its background, filling the top panel | the whole unit on the top panel | the bare screen on a pale reflector |
//! | two screens | each screen on its background, one per panel | the unit opened out across both panels, cut between its screens | each bare screen on its panel |
//!
//! "lcd" and "unit" need the unit's artwork; without it both are "plain". A single-screen
//! unit's bottom panel is the unit's own buttons (`ui::controls`).

use inktick_art::layout::{Bounds, View};
use inktick_art::scene::{self, Parts, Scene};
use inktick_art::{Image, Look, Optics};
use inktick_core::Segments;
use inktick_sets::Set;

/// How the unit is shown, as the "view" option says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewChoice {
    Lcd,
    Unit,
    Plain,
}

impl ViewChoice {
    pub fn parse(s: &str) -> ViewChoice {
        match s {
            "unit" => ViewChoice::Unit,
            "plain" => ViewChoice::Plain,
            _ => ViewChoice::Lcd,
        }
    }
}

/// The artwork view for a choice, by the names the artwork packs give their views.
fn find_view(views: &[View], choice: ViewChoice) -> Option<&View> {
    let named = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| views.iter().find(|v| v.name.eq_ignore_ascii_case(n)))
    };
    match choice {
        ViewChoice::Lcd => named(&["Backgrounds Only", "Background Only", "Backdrop_Only"]),
        ViewChoice::Unit => named(&["Unit Only", "Handheld_Game_Artwork", "Unit and Backdrop"]),
        ViewChoice::Plain => None,
    }
}

/// The unit's screens on the panels.
pub struct Stage {
    /// What each panel shows: the top always, the bottom for a two-screen unit.
    pub panels: Vec<Scene>,
    addrs: Vec<Vec<[u8; 3]>>,
    pub optics: Vec<Optics>,
    look: Look,
}

impl Stage {
    /// Lays the unit out for 1024 × 768 panels.
    pub fn new(set: &Set, choice: ViewChoice, look: Look) -> Stage {
        let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
        let addrs = Scene::addresses(&screens);
        let optics = addrs.iter().map(|a| Optics::new(a.len())).collect();
        let art = set.artwork.as_ref();
        let view = art.and_then(|a| find_view(&a.layout.views, choice));
        let picture = |f: &str| art.and_then(|a| a.picture(f));
        let side_by_side = set.screens.iter().any(|(f, _)| f.contains("_left"));
        let two = screens.len() > 1;
        let (w, h) = (nibkit::screens::WIDTH, nibkit::screens::HEIGHT);
        // Each panel's part of the view: a screen alone, a half of the unit, or the whole view.
        let viewports: Vec<(Option<&View>, Option<Bounds>, Option<usize>)> = match (view, two) {
            (Some(v), true) if choice == ViewChoice::Unit => match scene::halves(v) {
                Some((a, b)) => vec![(Some(v), Some(a), None), (Some(v), Some(b), None)],
                None => vec![(Some(v), None, None)],
            },
            (Some(v), true) => (0..2)
                .map(|n| (Some(v), scene::screen_viewport(v, n), None))
                .collect(),
            (Some(v), false) => vec![(Some(v), None, None)],
            (None, true) => vec![(None, None, Some(0)), (None, None, Some(1))],
            (None, false) => vec![(None, None, None)],
        };
        let panels = viewports
            .into_iter()
            .map(|(view, viewport, only)| {
                let parts = Parts {
                    view,
                    elements: art.map(|a| &a.layout.elements),
                    screens: &screens,
                    viewport,
                    picture: &picture,
                    side_by_side,
                    only,
                };
                let mut scene = Scene::new(&parts, w, h, look);
                scene.compact();
                scene
            })
            .collect();
        Stage {
            panels,
            addrs,
            optics,
            look,
        }
    }

    /// Moves the LCD's optics on by `dt_ms` towards the unit's segments; whether anything
    /// visibly changed.
    pub fn update(&mut self, segments: &Segments, dt_ms: f32) -> bool {
        let mut changed = false;
        for (o, a) in self.optics.iter_mut().zip(&self.addrs) {
            changed |= o.update(a, segments, dt_ms, &self.look);
        }
        changed
    }

    /// Jumps the optics to the segments (after loading a state).
    pub fn snap(&mut self, segments: &Segments) {
        for (o, a) in self.optics.iter_mut().zip(&self.addrs) {
            o.snap(a, segments);
        }
    }

    /// Whether the unit takes both panels.
    pub fn two_panels(&self) -> bool {
        self.panels.len() > 1
    }

    /// Composes the panels' pictures on the CPU (the GPU draws what is shown; these are for
    /// thumbnails, the menu's overlay and screenshots).
    pub fn refresh_pictures(&mut self) {
        let optics = &self.optics;
        for p in &mut self.panels {
            p.compose(optics);
        }
    }

    /// The panels' pictures as a thumbnail: each 256 × 192, stacked (DSlot's resume picture).
    pub fn thumbnail(&self) -> Image {
        let mut out = Image::filled(256, 192 * self.panels.len(), [0, 0, 0, 255]);
        for (i, p) in self.panels.iter().enumerate() {
            let small = p.picture().resized(256, 192);
            let at = i * 192 * 256 * 4;
            out.pixels[at..at + small.pixels.len()].copy_from_slice(&small.pixels);
        }
        out
    }
}

/// The look an option set asks for.
pub fn look(ghosting: &str, shadow: &str, ink: i64) -> Look {
    let mut l = Look::default();
    match ghosting {
        "off" => {
            l.rise_ms = 0.0;
            l.fall_ms = 0.0;
        }
        "slow" => {
            l.rise_ms *= 2.5;
            l.fall_ms *= 2.5;
        }
        _ => {}
    }
    match shadow {
        "off" => l.shadow = 0.0,
        "deep" => {
            l.shadow = 0.6;
            l.shadow_offset = [0.009, 0.013];
            l.shadow_blur = 0.007;
        }
        _ => {}
    }
    l.contrast = ink as f32 / 100.0;
    l
}
