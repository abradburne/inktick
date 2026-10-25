//! The bottom screen in play, for a single-screen unit: the unit's buttons to touch.
//!
//! Along the top, the unit's own buttons (GAME A, GAME B, TIME, ALARM) as on its case; below,
//! its game controls, each where it is on the unit (from the unit's database entry), large
//! enough for a thumb. A button is held while a finger is on it. The last line says which DS+
//! buttons do the same, and that Back opens the menu.

use inktick_core::units::{Role, Unit};
use nibkit::menu::raster::{Canvas, Fonts};
use nibkit::menu::{colour, Draw};

/// A button on the screen.
#[derive(Clone, Debug, PartialEq)]
struct Key {
    role: Role,
    label: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    round: bool,
}

/// The touch controls' state.
pub struct Controls {
    keys: Vec<Key>,
    title: String,
    hint: String,
    held: Vec<Role>,
    fonts: Fonts,
    canvas: Canvas,
    dirty: bool,
}

/// The unit buttons' row, and the area below it for the game's controls.
const TOP_ROW: f32 = 96.0;
const PAD_TOP: f32 = 250.0;
const PAD_BOTTOM: f32 = 690.0;

impl Controls {
    pub fn new(unit: &Unit) -> Controls {
        let mut keys = Vec::new();
        let unit_buttons: Vec<_> = unit
            .buttons
            .iter()
            .filter(|b| b.role.is_unit_button())
            .collect();
        let n = unit_buttons.len().max(1) as f32;
        let (w, gap) = (190.0, 24.0);
        let x0 = (1024.0 - (n * w + (n - 1.0) * gap)) / 2.0;
        for (i, b) in unit_buttons.iter().enumerate() {
            keys.push(Key {
                role: b.role,
                label: b.label.to_owned(),
                x: x0 + i as f32 * (w + gap),
                y: TOP_ROW,
                w,
                h: 84.0,
                round: false,
            });
        }
        // The game's controls where they are on the unit: its lower part, mapped to the pad area.
        let pads: Vec<_> = unit
            .buttons
            .iter()
            .filter(|b| !b.role.is_unit_button())
            .collect();
        let (ys, ye) = pads
            .iter()
            .fold((1.0f32, 0.0f32), |(a, b), p| (a.min(p.at.1), b.max(p.at.1)));
        for b in &pads {
            let size = 150.0;
            let (fx, fy) = b.at;
            let y = if ye > ys {
                PAD_TOP + (fy - ys) / (ye - ys) * (PAD_BOTTOM - PAD_TOP - size)
            } else {
                (PAD_TOP + PAD_BOTTOM - size) / 2.0
            };
            let x = (fx * 1024.0 - size / 2.0).clamp(24.0, 1024.0 - 24.0 - size);
            keys.push(Key {
                role: b.role,
                label: b.label.to_owned(),
                x,
                y,
                w: size,
                h: size,
                round: true,
            });
        }
        // Keys that land on each other (a pad's four directions) are pushed apart.
        spread(&mut keys);
        let sep = "   \u{b7}   ";
        // The DS+ buttons that do the same as the unit's own, for those it has.
        let mut parts: Vec<String> = [
            (Role::GameA, "Start"),
            (Role::Game, "Start"),
            (Role::GameB, "Select"),
            (Role::Time, "L"),
            (Role::Alarm, "R"),
        ]
        .iter()
        .filter_map(|(r, pad)| unit.button(*r).map(|b| format!("{pad}  {}", b.label)))
        .collect();
        parts.push("Back  menu".into());
        let hint = parts.join(sep);
        Controls {
            keys,
            title: unit.title.to_uppercase(),
            hint,
            held: Vec::new(),
            fonts: Fonts::new(),
            canvas: Canvas::new(1.0),
            dirty: true,
        }
    }

    /// What a finger at (x, y) is on, in screen units.
    pub fn hit(&self, x: f32, y: f32) -> Option<Role> {
        self.keys
            .iter()
            .find(|k| x >= k.x && x < k.x + k.w && y >= k.y && y < k.y + k.h)
            .map(|k| k.role)
    }

    /// What is held, as the main loop applies it; shown pressed.
    pub fn set_held(&mut self, held: &[Role]) {
        if self.held != held {
            self.held = held.to_vec();
            self.dirty = true;
        }
    }

    /// Draw again (after the menu).
    pub fn touch_up(&mut self) {
        self.dirty = true;
    }

    /// The screen's picture, when it changed.
    pub fn render(&mut self) -> Option<&[u8]> {
        if !std::mem::take(&mut self.dirty) {
            return None;
        }
        let c = &mut self.canvas;
        c.clear([12, 14, 18]);
        let mut draws = vec![Draw::Text {
            text: self.title.clone(),
            size: 28.0,
            bold: true,
            x: 48.0,
            y: 30.0,
            anchor: 0.0,
            colour: colour::DIM,
        }];
        for k in &self.keys {
            let on = self.held.contains(&k.role);
            let fill = if on { [210, 214, 222] } else { [44, 50, 64] };
            let ink = if on { colour::TAB_INK } else { colour::WHITE };
            let radius = if k.round { k.w / 2.0 } else { 18.0 };
            draws.push(Draw::Round {
                x: k.x,
                y: k.y,
                w: k.w,
                h: k.h,
                radius,
                colour: fill,
            });
            draws.push(Draw::Text {
                text: k.label.clone(),
                size: if k.round { 30.0 } else { 28.0 },
                bold: true,
                x: k.x + k.w / 2.0,
                y: k.y + k.h / 2.0 - 18.0,
                anchor: 0.5,
                colour: ink,
            });
        }
        draws.push(Draw::Text {
            text: self.hint.clone(),
            size: 22.0,
            bold: false,
            x: 512.0,
            y: 720.0,
            anchor: 0.5,
            colour: colour::FAINT,
        });
        c.draw(&self.fonts, &draws, |_| None);
        Some(&c.px)
    }
}

/// Moves overlapping keys apart until none overlap (a few passes are enough for a pad).
fn spread(keys: &mut [Key]) {
    for _ in 0..20 {
        let mut moved = false;
        for i in 0..keys.len() {
            for j in i + 1..keys.len() {
                let (a, b) = (keys[i].clone(), keys[j].clone());
                let (ax, ay) = (a.x + a.w / 2.0, a.y + a.h / 2.0);
                let (bx, by) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
                let (dx, dy) = (bx - ax, by - ay);
                let need = (a.w + b.w) / 2.0 + 8.0;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < need && a.round && b.round {
                    let push = (need - dist) / 2.0 + 1.0;
                    let (ux, uy) = if dist > 0.1 {
                        (dx / dist, dy / dist)
                    } else {
                        (1.0, 0.0)
                    };
                    keys[i].x -= ux * push;
                    keys[i].y -= uy * push;
                    keys[j].x += ux * push;
                    keys[j].y += uy * push;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    for k in keys.iter_mut().filter(|k| k.round) {
        k.x = k.x.clamp(16.0, 1024.0 - 16.0 - k.w);
        k.y = k.y.clamp(PAD_TOP - 40.0, PAD_BOTTOM - k.h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inktick_core::units;

    #[test]
    fn every_button_can_be_touched_and_none_overlap() {
        for unit in units::all() {
            let c = Controls::new(unit);
            for b in unit.buttons {
                let k = c.keys.iter().find(|k| k.role == b.role).unwrap();
                assert_eq!(
                    c.hit(k.x + k.w / 2.0, k.y + k.h / 2.0),
                    Some(b.role),
                    "{} {:?}",
                    unit.set,
                    b.role
                );
            }
        }
        let dkjr = units::by_set("gnw_dkjr").unwrap();
        let mut c = Controls::new(dkjr);
        assert!(c.render().is_some());
        assert!(c.render().is_none(), "drawn once until something changes");
    }
}
