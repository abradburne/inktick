//! What the glass does: a segment darkens and fades over tens of milliseconds, and casts a
//! shadow on the reflector behind it.
//!
//! A twisted-nematic segment's optical response to its drive is close to a first-order lag,
//! slower to relax than to switch: switching takes some 20-40 ms, relaxing 50-100 ms at room
//! temperature (typical TN figures in LCD texts). Each segment's darkness follows its drive
//! with those two time constants, which gives the faint trail a moving figure leaves on a
//! real unit.

use inktick_core::Segments;

/// How the LCD looks: all tunable, with defaults like a unit in good light.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Look {
    /// Darkening time constant, ms (0: instant).
    pub rise_ms: f32,
    /// Fading time constant, ms (0: instant).
    pub fall_ms: f32,
    /// Ink strength, relative to the screen file's own (1.0).
    pub contrast: f32,
    /// Shadow strength, 0 (none) to 1.
    pub shadow: f32,
    /// Shadow offset right and down, and blur radius, as fractions of the screen's height.
    pub shadow_offset: [f32; 2],
    /// Shadow blur radius, as a fraction of the screen's height.
    pub shadow_blur: f32,
}

impl Default for Look {
    fn default() -> Look {
        Look {
            rise_ms: 25.0,
            fall_ms: 70.0,
            contrast: 1.0,
            shadow: 0.35,
            shadow_offset: [0.006, 0.009],
            shadow_blur: 0.005,
        }
    }
}

impl Look {
    /// No fading and no shadow: the raw segments (tests, thumbnails of exact states).
    pub const SHARP: Look = Look {
        rise_ms: 0.0,
        fall_ms: 0.0,
        contrast: 1.0,
        shadow: 0.0,
        shadow_offset: [0.0, 0.0],
        shadow_blur: 0.0,
    };
}

/// Each segment's darkness, 0 to 1, for one screen's segments in their file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Optics {
    /// Darkness per segment.
    pub level: Vec<f32>,
}

impl Optics {
    /// All segments clear.
    pub fn new(segments: usize) -> Optics {
        Optics {
            level: vec![0.0; segments],
        }
    }

    /// Moves every segment `dt_ms` towards its drive; returns whether anything visibly changed.
    pub fn update(&mut self, addrs: &[[u8; 3]], drive: &Segments, dt_ms: f32, look: &Look) -> bool {
        let k = |tau: f32| {
            if tau <= 0.0 {
                1.0
            } else {
                1.0 - (-dt_ms / tau).exp()
            }
        };
        let (up, down) = (k(look.rise_ms), k(look.fall_ms));
        let mut changed = false;
        for (lvl, a) in self.level.iter_mut().zip(addrs) {
            let target = drive.on(a[0] as usize, a[1] as usize, a[2] as usize) as u8 as f32;
            if *lvl == target {
                continue;
            }
            let step = if target > *lvl { up } else { down };
            let mut next = *lvl + (target - *lvl) * step;
            // Within a 256th it is there: stop, so a still screen stops redrawing.
            if (next - target).abs() < 1.0 / 256.0 {
                next = target;
            }
            changed |= (next - *lvl).abs() > 0.0;
            *lvl = next;
        }
        changed
    }

    /// Whether every segment has reached its drive.
    pub fn settled(&self, addrs: &[[u8; 3]], drive: &Segments) -> bool {
        self.level
            .iter()
            .zip(addrs)
            .all(|(l, a)| *l == drive.on(a[0] as usize, a[1] as usize, a[2] as usize) as u8 as f32)
    }

    /// Jumps every segment to its drive (after loading a state).
    pub fn snap(&mut self, addrs: &[[u8; 3]], drive: &Segments) {
        for (l, a) in self.level.iter_mut().zip(addrs) {
            *l = drive.on(a[0] as usize, a[1] as usize, a[2] as usize) as u8 as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn darkens_faster_than_it_fades_and_settles() {
        let addrs = [[0u8, 0, 0]];
        let mut on = Segments::default();
        on.0[0][0] = 1;
        let look = Look::default();
        let mut o = Optics::new(1);
        o.update(&addrs, &on, 25.0, &look);
        let after_rise = o.level[0];
        assert!((after_rise - 0.632).abs() < 0.01);
        let mut t = 0;
        while !o.settled(&addrs, &on) {
            o.update(&addrs, &on, 16.7, &look);
            t += 1;
            assert!(t < 100);
        }
        o.update(&addrs, &Segments::default(), 25.0, &look);
        assert!(o.level[0] > 1.0 - after_rise, "fading is slower");
    }
}
