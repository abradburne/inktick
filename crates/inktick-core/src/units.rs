//! The units inktick knows: what each is, how its buttons are wired, and where its clock is.
//!
//! Titles, model numbers, years and series are from public catalogues of the units. A button's
//! line was found by our probe (`inktick-test probe`): pressing it on our core does what the
//! unit's button does. The button names and their places on the unit come from the units
//! themselves (the artwork's pictures of them, where there is artwork). Nothing here is from
//! another emulator. The rows are in `units_table.rs`.

use crate::chip::{Kind, Lines};
use crate::machine::Machine;
use crate::CRYSTAL_HZ;

/// The unit's series: how its case and screens are laid out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Series {
    /// Silver and Gold: one small screen.
    Silver,
    /// Gold.
    Gold,
    /// Wide Screen.
    Wide,
    /// Multi Screen: two screens, one above the other or side by side.
    MultiScreen,
    /// New Wide Screen.
    NewWide,
    /// Table Top: a colour screen lit from behind.
    TableTop,
    /// Panorama: a colour screen seen in a mirror.
    Panorama,
    /// Super Color.
    SuperColor,
    /// Micro Vs. System: two players, two controllers.
    MicroVs,
    /// Crystal Screen: a transparent screen.
    Crystal,
}

impl Series {
    /// Its name, as Nintendo's catalogues give it.
    pub fn name(self) -> &'static str {
        match self {
            Series::Silver => "Silver",
            Series::Gold => "Gold",
            Series::Wide => "Wide Screen",
            Series::MultiScreen => "Multi Screen",
            Series::NewWide => "New Wide Screen",
            Series::TableTop => "Table Top",
            Series::Panorama => "Panorama",
            Series::SuperColor => "Super Color",
            Series::MicroVs => "Micro Vs. System",
            Series::Crystal => "Crystal Screen",
        }
    }
}

/// What a button does, so each platform can put it where it belongs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// Starts game A.
    GameA,
    /// Starts game B.
    GameB,
    /// The single game button of units that have one.
    Game,
    /// Shows and sets the time.
    Time,
    /// Shows and sets the alarm.
    Alarm,
    /// Movement.
    Left,
    /// Movement.
    Right,
    /// Movement.
    Up,
    /// Movement.
    Down,
    /// The action button (jump, hit, push).
    Action,
    /// The four corner buttons of units without a pad.
    UpperLeft,
    /// Corner button.
    LowerLeft,
    /// Corner button.
    UpperRight,
    /// Corner button.
    LowerRight,
    /// Pinball's left flippers.
    LeftFlipper,
    /// Pinball's right flippers (which launch the ball, too).
    RightFlipper,
    /// Anything else (Black Jack's cards), by its label.
    Other,
}

impl Role {
    /// The unit's own buttons, as opposed to the game's controls.
    pub fn is_unit_button(self) -> bool {
        matches!(
            self,
            Role::GameA | Role::GameB | Role::Game | Role::Time | Role::Alarm
        )
    }
}

/// Where a button is wired.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Line {
    /// Between strobe S(s+1) and input K(k+1).
    Matrix(u8, u8),
    /// The BA input (pulled low when pressed).
    Ba,
    /// The β input (pulled low when pressed).
    Beta,
}

/// One of a unit's buttons.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Button {
    /// What it does.
    pub role: Role,
    /// Its label, as printed on the unit or as the instructions call it.
    pub label: &'static str,
    /// Its wiring.
    pub line: Line,
    /// Where it is on the unit, x and y as fractions of the unit's face (0 if unknown).
    pub at: (f32, f32),
}

/// Where a unit keeps the time in RAM, so it can be set from the system clock: six decimal
/// digits, one a nibble, at RAM addresses (B_M × 16 + B_L), found by running each unit a day
/// untouched (`inktick-test clockfind`, `docs/accuracy.md`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Clock {
    /// The hour's tens and ones.
    pub hour: (u8, u8),
    /// The minute's tens and ones.
    pub minute: (u8, u8),
    /// The second's tens and ones.
    pub second: (u8, u8),
    /// A 12-hour clock (1 to 12) keeps PM in this bit of the hour's tens; without, the hours
    /// run 0 to 23.
    pub pm_bit: Option<u8>,
}

impl Clock {
    /// The nibbles that put the clock at `hour` (0-23), `minute` and `second`: (address, value).
    pub fn digits(&self, hour: u8, minute: u8, second: u8) -> [(u8, u8); 6] {
        let (shown, pm) = match self.pm_bit {
            Some(bit) => ((hour + 11) % 12 + 1, u8::from(hour >= 12) << bit),
            None => (hour % 24, 0),
        };
        [
            (self.hour.0, (shown / 10) | pm),
            (self.hour.1, shown % 10),
            (self.minute.0, minute / 10),
            (self.minute.1, minute % 10),
            (self.second.0, second / 10),
            (self.second.1, second % 10),
        ]
    }

    /// The time the nibbles in `ram` hold, as (hour 0-23, minute, second); `None` if they
    /// don't hold one.
    pub fn read(&self, ram: &[u8; 128]) -> Option<(u8, u8, u8)> {
        let at = |a: u8| ram[a as usize & 127] & 15;
        let tens = at(self.hour.0);
        let (pm, tens) = match self.pm_bit {
            Some(bit) => (tens >> bit & 1 == 1, tens & !(1 << bit)),
            None => (false, tens),
        };
        let shown = tens * 10 + at(self.hour.1);
        let hour = match self.pm_bit {
            Some(_) if (1..=12).contains(&shown) => shown % 12 + if pm { 12 } else { 0 },
            None if shown < 24 => shown,
            _ => return None,
        };
        let minute = at(self.minute.0) * 10 + at(self.minute.1);
        let second = at(self.second.0) * 10 + at(self.second.1);
        (minute < 60 && second < 60).then_some((hour, minute, second))
    }
}

/// A unit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Unit {
    /// The set's name: `gnw_dkong`.
    pub set: &'static str,
    /// The program ROM's SHA-1, lowercase hex.
    pub sha1: &'static str,
    /// Its name.
    pub title: &'static str,
    /// Its model number.
    pub model: &'static str,
    /// The year it came out.
    pub year: u16,
    /// Its series.
    pub series: Series,
    /// Its chip.
    pub kind: Kind,
    /// The set holding its ROM, for clones whose archive holds only their screens.
    pub parent: Option<&'static str>,
    /// Its buttons.
    pub buttons: &'static [Button],
    /// Where its clock is, once found.
    pub clock: Option<Clock>,
}

impl Unit {
    /// The input lines with the buttons in `pressed` held.
    pub fn lines(&self, pressed: impl Fn(&Button) -> bool) -> Lines {
        let mut l = Lines::IDLE;
        for b in self.buttons.iter().filter(|b| pressed(b)) {
            match b.line {
                Line::Matrix(s, k) => l.matrix[s as usize & 7] |= 1 << (k & 3),
                Line::Ba => l.ba = false,
                Line::Beta => l.beta = false,
            }
        }
        l
    }

    /// Its button with `role`, if it has one.
    pub fn button(&self, role: Role) -> Option<&Button> {
        self.buttons.iter().find(|b| b.role == role)
    }

    /// Takes a unit just started (or after ACL) from the test showing every segment to its
    /// clock, as a person does: TIME pressed a fifth of a second, then the clock left to settle
    /// (some units ignore a button pressed straight after TIME). About two seconds of the unit's
    /// time.
    pub fn show_clock(&self, m: &mut Machine) {
        let second = CRYSTAL_HZ as u64;
        m.run_for(second / 2);
        m.set_lines(self.lines(|b| b.role == Role::Time));
        m.run_for(second / 5);
        m.set_lines(Lines::IDLE);
        m.run_for(3 * second / 2);
    }

    /// Sets its clock to `hour` (0-23), `minute` and `second`; false if where it keeps the time
    /// isn't known.
    pub fn set_time(&self, m: &mut Machine, hour: u8, minute: u8, second: u8) -> bool {
        let Some(clock) = self.clock else {
            return false;
        };
        for (at, value) in clock.digits(hour, minute, second) {
            m.chip_mut().set_ram(at as usize, value);
        }
        true
    }

    /// The time its clock shows, as (hour 0-23, minute, second).
    pub fn time(&self, m: &Machine) -> Option<(u8, u8, u8)> {
        self.clock?.read(m.chip().ram())
    }
}

/// Every unit known.
pub fn all() -> &'static [Unit] {
    crate::units_table::UNITS
}

/// The unit whose set is named `set`.
pub fn by_set(set: &str) -> Option<&'static Unit> {
    all().iter().find(|u| u.set == set)
}

/// The units whose program ROM has this SHA-1 (a parent and its clones share one).
pub fn by_sha1(sha1: &str) -> impl Iterator<Item = &'static Unit> + '_ {
    all()
        .iter()
        .filter(move |u| u.sha1.eq_ignore_ascii_case(sha1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_time_of_day_reads_back_as_written() {
        let twelve = Clock {
            hour: (0x14, 0x15),
            minute: (0x16, 0x17),
            second: (0x18, 0x19),
            pm_bit: Some(3),
        };
        let day = Clock {
            hour: (0x05, 0x04),
            minute: (0x03, 0x02),
            second: (0x01, 0x00),
            pm_bit: None,
        };
        for clock in [twelve, day] {
            for t in 0..86400u32 {
                let (h, m, s) = ((t / 3600) as u8, (t / 60 % 60) as u8, (t % 60) as u8);
                let mut ram = [0u8; 128];
                for (at, v) in clock.digits(h, m, s) {
                    assert!(v < 16);
                    ram[at as usize] = v;
                }
                assert_eq!(clock.read(&ram), Some((h, m, s)), "{clock:?} {h}:{m}:{s}");
            }
        }
        // 12:30 AM and PM on a 12-hour clock: the hour shows as 12.
        let d = twelve.digits(0, 30, 0);
        assert_eq!((d[0].1, d[1].1), (1, 2));
        assert_eq!(twelve.digits(12, 30, 0)[0].1, 1 | 8);
    }

    #[test]
    fn every_clock_has_six_places() {
        for u in all() {
            if let Some(c) = u.clock {
                let mut at = [
                    c.hour.0, c.hour.1, c.minute.0, c.minute.1, c.second.0, c.second.1,
                ];
                at.sort();
                assert!(at.windows(2).all(|w| w[0] != w[1]), "{}", u.set);
                assert!(at[5] < 128 && c.pm_bit.is_none_or(|b| b < 4), "{}", u.set);
            }
        }
    }
}
