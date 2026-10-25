//! A chip in a unit, run in crystal time.

use crate::chip::{Chip, Kind, Lines};
use crate::lcd::{Lcd, Segments, COMMON_TICKS};
use crate::state::{Loader, Saver, Visit, Visitor};
use crate::synth::Synth;
use alloc::vec::Vec;

/// The crystal every unit runs from, Hz.
pub const CRYSTAL_HZ: u32 = 32_768;

/// How the SM510's R1/R2 pins reach the piezo. Sharp: they "directly drive a piezo electric
/// buzzer at a frequency of 4,096kHz" [sic], and "the R port can generate the contents of the R
/// register with a mask change" (SM510, "Pin Functions (6)"): a mask option per unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Buzzer {
    /// R1 and R2 gate the divider's 4,096 Hz (f12) in opposite phases.
    Gated,
    /// R1 and R2 are the R register's bits, as the program writes them.
    Direct,
}

/// How a unit is wired.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Options {
    /// The SM510's buzzer mask option.
    pub buzzer: Buzzer,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            buzzer: Buzzer::Gated,
        }
    }
}

/// The peak level of the buzzer output, before the frontend's volume.
const LOUD: f32 = 0.35;

/// A unit's chip, its LCD and its buzzer, run by crystal cycles.
#[derive(Clone)]
pub struct Machine {
    chip: Chip,
    lcd: Lcd,
    ticks: u64,
    options: Options,
    acl: bool,
    level: f32,
    edges: Vec<(u64, f32)>,
    cycle_by_cycle: bool,
}

impl Machine {
    /// A unit with `kind` of chip and its ROMs, just powered on.
    pub fn new(
        kind: Kind,
        program: &[u8; 4096],
        melody: Option<&[u8; 256]>,
        options: Options,
    ) -> Machine {
        Machine {
            chip: Chip::new(kind, program, melody),
            lcd: Lcd::default(),
            ticks: 0,
            options,
            acl: false,
            level: 0.0,
            edges: Vec::new(),
            cycle_by_cycle: false,
        }
    }

    /// The chip.
    pub fn chip(&self) -> &Chip {
        &self.chip
    }

    /// The chip, to change (tests, the clock setter, the probe).
    pub fn chip_mut(&mut self) -> &mut Chip {
        &mut self.chip
    }

    /// Crystal cycles since power-on.
    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// The input pins.
    pub fn set_lines(&mut self, lines: Lines) {
        self.chip.set_lines(lines);
    }

    /// The ACL pin (the units' ACL button): while high the chip is held in reset; released, the
    /// program starts again at P_U = 3, P_M = 7.
    pub fn set_acl(&mut self, high: bool) {
        self.acl = high;
        if high {
            self.chip.reset();
        }
    }

    /// The last complete LCD frame.
    pub fn segments(&self) -> &Segments {
        self.lcd.segments()
    }

    /// LCD frames completed so far.
    pub fn frames(&self) -> u64 {
        self.lcd.frames()
    }

    /// Runs until crystal cycle `until`.
    pub fn run_until(&mut self, until: u64) {
        while self.ticks < until {
            self.advance(until);
        }
    }

    /// Runs one instruction, or one stretch of the clock stop: what the probe and tracer step by.
    pub fn step(&mut self) {
        self.advance(u64::MAX);
    }

    fn advance(&mut self, until: u64) {
        if self.acl {
            self.chip.reset();
            self.tick();
            return;
        }
        if self.chip.halted {
            if self.chip.wakes() {
                self.chip.wake();
            } else if self.quiet() && until != u64::MAX {
                self.skip_halt(until);
            } else {
                self.tick();
            }
            return;
        }
        let cycles = self.chip.step() * self.chip.ticks_per_cycle();
        self.ticks_for(cycles as u64);
    }

    /// Advances `n` crystal cycles a stretch at a time: to the next thing that happens (a
    /// common of the LCD driver, a change of the 1-second signal, a buzzer edge), with what
    /// lies between done at once. The same as ticking each cycle, for a fraction of the work.
    fn ticks_for(&mut self, mut n: u64) {
        // The melody generator counts every cycle: tick by tick while it plays.
        if self.chip.melody.active() || self.cycle_by_cycle {
            for _ in 0..n {
                self.tick();
            }
            return;
        }
        // The first cycle by itself: what the instruction just changed (R) shows there.
        if n > 0 {
            self.tick();
            n -= 1;
        }
        while n > 0 {
            let div = self.chip.div as u64;
            let to_common = COMMON_TICKS - self.ticks % COMMON_TICKS;
            let to_second = 0x4000 - (div & 0x3FFF);
            // The gated buzzer follows f12, the divider's bit 2.
            let to_buzz = if self.chip.r != 0 {
                4 - (div & 3)
            } else {
                u64::MAX
            };
            let k = n.min(to_common).min(to_second).min(to_buzz);
            if k > 1 {
                // The cycles before the stretch's last: nothing but the divider moves.
                self.chip.div = ((div + k - 1) & 0x7FFF) as u16;
                self.ticks += k - 1;
            }
            self.tick();
            n -= k;
        }
    }

    /// Advances a crystal cycle at a time rather than by stretches (for testing that the two
    /// agree).
    #[doc(hidden)]
    pub fn set_cycle_by_cycle(&mut self, on: bool) {
        self.cycle_by_cycle = on;
    }

    /// Runs for `ticks` crystal cycles.
    pub fn run_for(&mut self, ticks: u64) {
        self.run_until(self.ticks + ticks);
    }

    /// Nothing changes while halted but the divider: no sound being made.
    fn quiet(&self) -> bool {
        self.level == 0.0 && !self.chip.melody.active() && self.chip.r == 0
    }

    /// Jumps a halted, quiet chip to the next change of the 1-second signal (which sets γ or
    /// moves the bs flash) or to `until`, latching the unchanged LCD on the way.
    fn skip_halt(&mut self, until: u64) {
        let div = self.chip.div as u64;
        let to_change = 0x4000 - (div & 0x3FFF);
        let n = to_change.min(until - self.ticks).max(1);
        if n <= 2 * COMMON_TICKS * 4 {
            for _ in 0..n {
                self.tick();
            }
            return;
        }
        // Whole frames in between latch the same segments.
        let before = self.ticks / (COMMON_TICKS * 4);
        self.chip.div = ((div + n - 1) & 0x7FFF) as u16;
        self.ticks += n - 1;
        let after = self.ticks / (COMMON_TICKS * 4);
        if after > before {
            self.lcd.sample_all(&self.chip);
        }
        self.tick();
    }

    fn tick(&mut self) {
        self.chip.tick_divider();
        self.ticks += 1;
        if self.ticks.is_multiple_of(COMMON_TICKS) {
            let z = ((self.ticks / COMMON_TICKS) % 4) as usize;
            self.lcd.sample(&self.chip, z);
        }
        let level = self.output();
        if level != self.level {
            self.level = level;
            self.edges.push((self.ticks, level));
        }
    }

    /// The buzzer pin's level this crystal cycle.
    fn output(&mut self) -> f32 {
        if self.chip.kind().melodic() {
            if !self.chip.melody.active() {
                return 0.0;
            }
            return if self.chip.melody.tick() { LOUD } else { -LOUD };
        }
        let r = self.chip.r;
        match self.options.buzzer {
            Buzzer::Gated => {
                if r == 0 {
                    return 0.0;
                }
                // R1 and R2 in opposite phases across the disc: both enabled is twice one.
                let amp = if r == 3 { LOUD } else { LOUD / 2.0 };
                if self.chip.tap(12) {
                    amp
                } else {
                    -amp
                }
            }
            Buzzer::Direct => {
                let r1 = r & 1 != 0;
                let r2 = r & 2 != 0;
                (r1 as u8 as f32 - r2 as u8 as f32) * LOUD
            }
        }
    }

    /// Hands the buzzer's level changes so far to `synth`.
    pub fn sound_into(&mut self, synth: &mut Synth) {
        for (tick, level) in self.edges.drain(..) {
            synth.edge(tick, level);
        }
    }

    /// The buzzer's level changes so far, taken (for tests and hashes).
    pub fn take_edges(&mut self) -> Vec<(u64, f32)> {
        core::mem::take(&mut self.edges)
    }

    /// The machine's state, for a save state.
    pub fn save(&mut self) -> Vec<u8> {
        let mut s = Saver::default();
        self.visit(&mut s);
        s.0
    }

    /// Loads a state saved by [`Machine::save`] for the same ROMs; false if it doesn't fit.
    pub fn load(&mut self, data: &[u8]) -> bool {
        let mut copy = self.clone();
        let mut l = Loader::new(data);
        copy.visit(&mut l);
        if !l.finished() {
            return false;
        }
        copy.edges.clear();
        *self = copy;
        true
    }
}

impl Visit for Machine {
    fn visit(&mut self, v: &mut impl Visitor) {
        let mut version = 1u8;
        v.u8(&mut version);
        self.chip.visit(v);
        self.lcd.visit(v);
        v.u64(&mut self.ticks);
        v.bool(&mut self.acl);
        v.f32(&mut self.level);
    }
}
