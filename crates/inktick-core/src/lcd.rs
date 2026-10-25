//! The LCD driver: which segments the display RAM and the bs outputs turn on, frame by frame.
//!
//! Sharp 1990: the LCD is driven at 1/4 duty, 1/3 bias, with a 64 Hz frame (Fig. 10 of the
//! SM510 chapter, Fig. 9 of the SM511/512's), so each of the commons H1-H4 has 1/256 s. The
//! display RAM's file R (B_M = 6) drives the segment outputs a1-a16 and file S (B_M = 7)
//! b1-b16, one RAM word per output, bit M_i during common H_i (SM510 Fig. 8); the SM512's file Q
//! (B_M = 5) drives c1-c16 (SM511/512 Fig. 7). The display shows while BP = 1 and BC = 0.
//!
//! Segments are addressed as the screen artwork names them, `x.y.z`: output row x (0 = a,
//! 1 = b, 2 = bs, 3 = c), column y (the output's number less one; bs1 and bs2 are columns 0 and
//! 1) and common z (H1-H4 = 0-3). That reading is derived (docs/provenance.md).

use crate::chip::Chip;
use crate::state::{Visit, Visitor};

/// Crystal cycles per common: 32,768 Hz / 256.
pub(crate) const COMMON_TICKS: u64 = 128;

/// One LCD frame: for each row and column, the commons (bits 0-3) whose segment is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
pub struct Segments(pub [[u8; 16]; 4]);

impl Segments {
    /// Whether segment `x.y.z` is on.
    pub fn on(&self, x: usize, y: usize, z: usize) -> bool {
        x < 4 && y < 16 && z < 4 && self.0[x][y] & (1 << z) != 0
    }

    /// How many segments are on.
    pub fn count(&self) -> u32 {
        self.0.iter().flatten().map(|b| (b & 15).count_ones()).sum()
    }
}

/// The driver's sampling: each common's quarter of the frame latches the RAM it shows.
#[derive(Clone, Default)]
pub struct Lcd {
    building: Segments,
    shown: Segments,
    frames: u64,
}

impl Lcd {
    /// The last complete frame.
    pub fn segments(&self) -> &Segments {
        &self.shown
    }

    /// Frames completed since power-on: a frontend redraws when it changes and the segments do.
    pub fn frames(&self) -> u64 {
        self.frames
    }

    /// Latches common `z` from the chip's RAM and outputs; after H4 the frame is complete.
    pub(crate) fn sample(&mut self, chip: &Chip, z: usize) {
        let bit = 1u8 << z;
        let visible = chip.bp && !chip.bc;
        let rows: [Option<usize>; 4] = [
            Some(6 * 16),
            Some(7 * 16),
            None,
            (chip.kind() == crate::chip::Kind::Sm512).then_some(5 * 16),
        ];
        for (x, file) in rows.iter().enumerate() {
            let Some(file) = file else { continue };
            for y in 0..16 {
                let on = visible && chip.ram[file + y] & bit != 0;
                set(&mut self.building.0[x][y], bit, on);
            }
        }
        // bs (bs1): the L register, bit i during H_i, flashed by Y; bs2 on the SM511/512: the
        // X register. Derived: "The bs is used to output the contents of the L or Y register
        // ... to flash the display such as a colon under the control of Y" (SM510), "on for
        // 0.5 sec and off for 0.5 sec" (SM511/512): a Y bit blanks its bs segment while the
        // 1 Hz signal is low.
        let flash = !chip.tap(0);
        let bs1 = chip.l & bit != 0 && !(flash && chip.y & bit != 0);
        set(&mut self.building.0[2][0], bit, visible && bs1);
        let bs2 = chip.kind().melodic() && chip.x & bit != 0 && !(flash && chip.y & bit != 0);
        set(&mut self.building.0[2][1], bit, visible && bs2);
        if z == 3 {
            self.shown = self.building;
            self.frames += 1;
        }
    }

    /// The clock stop leaves RAM and the outputs as they are: a whole frame is latched at once.
    pub(crate) fn sample_all(&mut self, chip: &Chip) {
        for z in 0..4 {
            self.sample(chip, z);
        }
    }
}

fn set(b: &mut u8, bit: u8, on: bool) {
    if on {
        *b |= bit;
    } else {
        *b &= !bit;
    }
}

impl Visit for Lcd {
    fn visit(&mut self, v: &mut impl Visitor) {
        for row in self.building.0.iter_mut().chain(self.shown.0.iter_mut()) {
            v.bytes(row);
        }
        v.u64(&mut self.frames);
    }
}
