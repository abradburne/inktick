//! The DS+'s controls as a unit's buttons, by what each button does (its [`Role`]).
//!
//! | Control | |
//! |---|---|
//! | D-pad (or the stick) | the unit's directions; on units with a button in each corner, Up and Down are the left pair |
//! | A, B | the action button (Jump, Hit); with only Left and Right buttons, A is Right and Y is Left too, for two thumbs |
//! | X, B | on corner-button units, the right pair (X upper, B lower) |
//! | Start | GAME A (or GAME) |
//! | Select | GAME B |
//! | L | TIME |
//! | R | ALARM |
//! | Pinball | the left flippers on Left, L and Y, the right ones (which launch the ball) on A, R and Right |
//! | Back | the menu on the bottom screen (the unit pauses); Back again to play |
//! | Menu (tapped) | save where the unit is and leave, back to DSlot's shelf |
//! | Menu + R, Menu + L | quick save, quick load (the last slot used) |
//!
//! As every program on the DS+ does it: Back opens the program's own menu, Menu goes home
//! (DSlot's docs/emulators.md). On nibOS, Menu arrives only when it is let go of unless another
//! button is pressed with it, so a tap is a press and a release together.

use inktick_core::units::{Role, Unit};
use inktick_core::Lines;

/// A control, by what is printed on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Pad {
    A,
    B,
    X,
    Y,
    Select,
    Start,
    Up,
    Down,
    Left,
    Right,
    L,
    R,
    Menu,
}

/// What a control asks of the emulator, rather than of the unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Open the menu on the bottom screen, or close it.
    PauseMenu,
    /// Save the resume state and quit.
    Leave,
    QuickSave,
    QuickLoad,
}

/// What a key asks for, by SDL's name for it: the DS+'s Back key, or Escape on a computer
/// (nibkit's Back), opens the menu.
pub fn key(name: &str) -> Option<Command> {
    let back = nibkit::input::BACK_KEYCODES.contains(&name)
        || nibkit::input::keyboard(name) == Some(nibkit::input::Button::Back);
    back.then_some(Command::PauseMenu)
}

/// A computer's keyboard as the DS+'s controls (nibkit's map: the arrows, X / Z for A / B, S / A
/// for X / Y, Q / W for L / R, Return for Start, Right Shift or Backspace for Select, Home for
/// Menu). Escape, nibkit's Back, is [`key`]'s: the menu.
pub fn keyboard_pad(name: &str) -> Option<Pad> {
    use nibkit::input::Button;
    Some(match nibkit::input::keyboard(name)? {
        Button::Up => Pad::Up,
        Button::Down => Pad::Down,
        Button::Left => Pad::Left,
        Button::Right => Pad::Right,
        Button::A => Pad::A,
        Button::B => Pad::B,
        Button::X => Pad::X,
        Button::Y => Pad::Y,
        Button::L => Pad::L,
        Button::R => Pad::R,
        Button::Start => Pad::Start,
        Button::Select => Pad::Select,
        Button::Menu => Pad::Menu,
        _ => return None,
    })
}

/// Which roles a control plays on this unit.
pub fn roles(unit: &Unit, pad: Pad) -> Vec<Role> {
    let has = |r| unit.button(r).is_some();
    let corners = has(Role::UpperLeft);
    let flippers = has(Role::LeftFlipper);
    let only_sides = !has(Role::Action) && has(Role::Left) && has(Role::Right) && !has(Role::Up);
    match pad {
        // Pinball: the left flippers on Left, L and Y, the right ones on A, R and Right (the unit's
        // TIME and ALARM are then on the menu's Play page).
        Pad::Left | Pad::L | Pad::Y if flippers => vec![Role::LeftFlipper],
        Pad::Right | Pad::R | Pad::A if flippers => vec![Role::RightFlipper],
        Pad::Up if corners => vec![Role::UpperLeft],
        Pad::Down if corners => vec![Role::LowerLeft],
        Pad::X if corners => vec![Role::UpperRight],
        Pad::B if corners => vec![Role::LowerRight],
        Pad::Up => vec![Role::Up],
        Pad::Down => vec![Role::Down],
        Pad::Left => vec![Role::Left],
        Pad::Right => vec![Role::Right],
        Pad::A if only_sides => vec![Role::Right],
        Pad::Y if only_sides => vec![Role::Left],
        Pad::A | Pad::B => vec![Role::Action],
        Pad::Start => vec![Role::GameA, Role::Game],
        Pad::Select => vec![Role::GameB],
        Pad::L => vec![Role::Time],
        Pad::R => vec![Role::Alarm],
        _ => vec![],
    }
}

/// The controls' state.
#[derive(Clone, Debug, Default)]
pub struct Controls {
    held: Vec<Pad>,
    menu_held: bool,
    /// Menu was held with another button: its release isn't a tap.
    menu_combined: bool,
    stick: (i16, i16),
    /// Unit buttons held on the touch screen (or from the menu), by role.
    touched: Vec<Role>,
}

/// Stick deflection that counts as a direction (of ±32767): half-way.
const STICK_THRESHOLD: i16 = 16_000;

impl Controls {
    /// A control went down; returns what it asks of the emulator, if anything.
    pub fn press(&mut self, pad: Pad) -> Option<Command> {
        if pad == Pad::Menu {
            self.menu_held = true;
            self.menu_combined = false;
            return None;
        }
        if !self.held.contains(&pad) {
            self.held.push(pad);
        }
        if self.menu_held {
            self.menu_combined = true;
            return match pad {
                Pad::R => Some(Command::QuickSave),
                Pad::L => Some(Command::QuickLoad),
                _ => None,
            };
        }
        None
    }

    /// A control came up.
    pub fn release(&mut self, pad: Pad) -> Option<Command> {
        if pad == Pad::Menu {
            let tap = self.menu_held && !self.menu_combined;
            self.menu_held = false;
            return tap.then_some(Command::Leave);
        }
        self.held.retain(|&p| p != pad);
        None
    }

    /// The stick's horizontal position alone (SDL reports each axis by itself).
    pub fn stick_x(&mut self, x: i16) {
        self.stick.0 = x;
    }

    /// The stick's vertical position alone.
    pub fn stick_y(&mut self, y: i16) {
        self.stick.1 = y;
    }

    /// A unit button held by touch (or let go).
    pub fn touch_role(&mut self, role: Role, down: bool) {
        self.touched.retain(|&r| r != role);
        if down {
            self.touched.push(role);
        }
    }

    /// The unit buttons held by touch.
    pub fn touched(&self) -> &[Role] {
        &self.touched
    }

    /// Lets go of everything (when the menu opens, so nothing stays held in the unit).
    pub fn clear(&mut self) {
        self.held.clear();
        self.stick = (0, 0);
        self.touched.clear();
    }

    /// The roles held now.
    pub fn held_roles(&self, unit: &Unit) -> Vec<Role> {
        if self.menu_held {
            return Vec::new();
        }
        let mut pads = self.held.clone();
        let (x, y) = self.stick;
        if x < -STICK_THRESHOLD {
            pads.push(Pad::Left);
        } else if x > STICK_THRESHOLD {
            pads.push(Pad::Right);
        }
        if y < -STICK_THRESHOLD {
            pads.push(Pad::Up);
        } else if y > STICK_THRESHOLD {
            pads.push(Pad::Down);
        }
        let mut out: Vec<Role> = pads.into_iter().flat_map(|p| roles(unit, p)).collect();
        out.extend_from_slice(&self.touched);
        out
    }

    /// The unit's input lines with what is held.
    pub fn lines(&self, unit: &Unit) -> Lines {
        let held = self.held_roles(unit);
        unit.lines(|b| held.contains(&b.role))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inktick_core::units;

    #[test]
    fn back_pauses_and_menu_leaves() {
        for back in nibkit::input::BACK_KEYCODES {
            assert_eq!(key(back), Some(Command::PauseMenu));
        }
        assert_eq!(key("Escape"), Some(Command::PauseMenu));
        let mut c = Controls::default();
        assert_eq!(c.press(Pad::Menu), None, "Menu acts on release");
        assert_eq!(c.release(Pad::Menu), Some(Command::Leave));
        c.press(Pad::Menu);
        assert_eq!(c.press(Pad::R), Some(Command::QuickSave));
        c.release(Pad::R);
        assert_eq!(c.release(Pad::Menu), None, "a combination isn't a tap");
    }

    #[test]
    fn buttons_reach_the_units_lines() {
        let dkjr = units::by_set("gnw_dkjr").unwrap();
        let mut c = Controls::default();
        c.press(Pad::A);
        c.press(Pad::Start);
        let l = c.lines(dkjr);
        let jump = dkjr.button(Role::Action).unwrap().line;
        let game_a = dkjr.button(Role::GameA).unwrap().line;
        assert_eq!(l, dkjr.lines(|b| b.line == jump || b.line == game_a));
        c.release(Pad::A);
        c.release(Pad::Start);
        assert_eq!(c.lines(dkjr), inktick_core::Lines::IDLE);
    }

    #[test]
    fn corner_buttons_and_two_thumbs() {
        let fire = units::by_set("gnw_fireatk").unwrap();
        assert_eq!(roles(fire, Pad::Up), vec![Role::UpperLeft]);
        assert_eq!(roles(fire, Pad::B), vec![Role::LowerRight]);
        let pinball = units::by_set("gnw_pinball").unwrap();
        for pad in [Pad::Left, Pad::L, Pad::Y] {
            assert_eq!(roles(pinball, pad), vec![Role::LeftFlipper]);
        }
        for pad in [Pad::A, Pad::R, Pad::Right] {
            assert_eq!(roles(pinball, pad), vec![Role::RightFlipper]);
        }
        let boat = units::by_set("gnw_lboat").unwrap();
        assert_eq!(roles(boat, Pad::A), vec![Role::Right]);
        assert_eq!(roles(boat, Pad::Y), vec![Role::Left]);
    }
}
