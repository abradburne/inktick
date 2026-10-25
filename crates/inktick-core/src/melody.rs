//! The SM511/SM512 melody generator.
//!
//! Sharp 1990, SM511/SM512 "(8) Melody ROM": "A melody ROM is organized as 256 steps × 6 bits
//! which executes musical notes, pause and stop instructions", "12 musical scales (555 to
//! 2097Hz) in two octaves", "tone length either 125ms or 62.5ms". Fig. 5 gives each note's code
//! (m3-m0 = 0010 for do down to 1101 for do#) and its waveform as four half-periods counted in
//! crystal cycles, two periods that average to the stated frequency.
//!
//! Derived from the units' melody ROMs (docs/provenance.md): the low nibble is the note, 0 a rest
//! and 1 stop; bit 4 selects the octave and bit 5 the tone length, both used with every note.
//! Which value of each is which was fitted to the black box (Snoopy's tune, its notes' timing
//! and pitch): bit 5 set is the long tone, bit 4 set the upper octave.

use crate::state::{Visit, Visitor};

/// Fig. 5: four half-periods, in crystal cycles, for codes 2 (do, 2114.1 Hz) to 13 (do#, 1110.8 Hz).
const WAVES: [[u8; 4]; 12] = [
    [7, 8, 8, 8],
    [8, 8, 8, 9],
    [8, 9, 9, 9],
    [9, 9, 9, 10],
    [9, 10, 10, 10],
    [10, 11, 10, 11],
    [11, 11, 11, 11],
    [11, 12, 12, 12],
    [12, 13, 12, 13],
    [13, 13, 13, 14],
    [14, 14, 14, 14],
    [14, 15, 15, 15],
];

/// Bit 4 of a melody step: the upper octave (else the half-periods are doubled). See the module.
const UPPER_OCTAVE: u8 = 0x10;
/// Bit 5 of a melody step: the long tone, 125 ms (else 62.5 ms). See the module.
const LONG: u8 = 0x20;
/// 125 ms in crystal cycles.
const LONG_TICKS: u32 = 4096;

/// The melody generator's state.
#[derive(Clone)]
pub struct Melody {
    rom: [u8; 256],
    present: bool,
    /// ME: enabled by SME, disabled by RME.
    enabled: bool,
    /// MES: the melody reached a stop step; tested and cleared by TMEL.
    ended: bool,
    pointer: u8,
    /// Crystal cycles left in the current step.
    left: u32,
    /// Where in the four half-periods the waveform is, and cycles left in that half-period.
    half: u8,
    half_left: u8,
    level: bool,
    /// The step being played.
    current: u8,
}

impl Melody {
    pub(crate) fn new(rom: Option<&[u8; 256]>) -> Melody {
        Melody {
            rom: rom.copied().unwrap_or([0; 256]),
            present: rom.is_some(),
            enabled: false,
            ended: false,
            pointer: 0,
            left: 0,
            half: 0,
            half_left: 0,
            level: false,
            current: 0,
        }
    }

    pub(crate) fn stop(&mut self) {
        self.enabled = false;
        self.level = false;
    }

    pub(crate) fn enable(&mut self, on: bool) {
        if on && !self.enabled {
            self.left = 0;
        }
        self.enabled = on && self.present;
        if !self.enabled {
            self.level = false;
        }
    }

    pub(crate) fn preset(&mut self, at: u8) {
        self.pointer = at;
        self.left = 0;
    }

    pub(crate) fn take_end(&mut self) -> bool {
        core::mem::take(&mut self.ended)
    }

    /// Whether it is making sound or about to.
    pub(crate) fn active(&self) -> bool {
        self.enabled
    }

    /// One crystal cycle; returns the output level.
    pub(crate) fn tick(&mut self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.left == 0 {
            self.current = self.rom[self.pointer as usize] & 0x3F;
            if self.current & 15 == 1 {
                // A stop step: the melody ends where it is.
                self.ended = true;
                self.enabled = false;
                self.level = false;
                return false;
            }
            self.pointer = self.pointer.wrapping_add(1);
            self.left = if self.current & LONG != 0 {
                LONG_TICKS
            } else {
                LONG_TICKS / 2
            };
            self.half = 0;
            self.half_left = 0;
        }
        self.left -= 1;
        let note = self.current & 15;
        if !(2..=13).contains(&note) {
            self.level = false;
            return false;
        }
        if self.half_left == 0 {
            let len = WAVES[note as usize - 2][self.half as usize];
            self.half_left = if self.current & UPPER_OCTAVE != 0 {
                len
            } else {
                len * 2
            };
            self.half = (self.half + 1) & 3;
            self.level = !self.level;
        }
        self.half_left -= 1;
        self.level
    }
}

impl Visit for Melody {
    fn visit(&mut self, v: &mut impl Visitor) {
        v.bool(&mut self.enabled);
        v.bool(&mut self.ended);
        v.u8(&mut self.pointer);
        v.u32(&mut self.left);
        v.u8(&mut self.half);
        v.u8(&mut self.half_left);
        v.bool(&mut self.level);
        v.u8(&mut self.current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_matches_sharps_frequencies() {
        // Fig. 5's "clock cycle" column: half the four half-periods' sum.
        let cycles = [
            15.5, 16.5, 17.5, 18.5, 19.5, 21.0, 22.0, 23.5, 25.0, 26.5, 28.0, 29.5,
        ];
        for (w, c) in WAVES.iter().zip(cycles) {
            let sum: u32 = w.iter().map(|&x| x as u32).sum();
            assert_eq!(sum as f32 / 2.0, c);
        }
    }

    #[test]
    fn a_note_plays_for_its_length_then_the_stop_ends_it() {
        let mut rom = [0u8; 256];
        rom[0] = 0x32; // do, long, upper octave
        rom[1] = 0x01; // stop
        let mut m = Melody::new(Some(&rom));
        m.enable(true);
        let mut edges = 0;
        let mut last = false;
        for _ in 0..LONG_TICKS {
            let l = m.tick();
            edges += (l != last) as u32;
            last = l;
        }
        // 125 ms of 2114 Hz: about 264 periods, 528 edges.
        assert!((520..=536).contains(&edges), "{edges}");
        m.tick();
        assert!(m.take_end());
        assert!(!m.active());
    }
}
