//! The Sharp SM510 family of 4-bit LCD-driver microcontrollers, as used in LCD handheld games:
//! the SM510, SM511 and SM512. A [`Chip`] is the whole microcontroller (CPU, RAM, the 15-stage
//! divider, the LCD driver, the buzzer and melody outputs, the key matrix); a [`Machine`] is a
//! chip in a unit: its ROMs, its buttons and its clock, run in real time.
//!
//! Everything here comes from Sharp's data books (the 1990 *Sharp Microcomputers Data Book*,
//! SM510 and SM511/SM512 chapters, and the 1996 *Microcomputer Data Book*'s SM3903, which shares
//! the SM510's instruction set) and from derivations recorded in `docs/provenance.md`. No other
//! emulator's source was used. `#![no_std]`, no dependencies, no `unsafe`.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod asm;
mod chip;
pub mod disasm;
mod lcd;
mod machine;
mod melody;
pub mod rom;
pub mod sha1;
pub mod state;
mod synth;
pub mod units;
mod units_table;

pub use chip::{Chip, Kind, Lines, Registers};
pub use lcd::{Lcd, Segments};
pub use machine::{Buzzer, Machine, Options, CRYSTAL_HZ};
pub use synth::{Piezo, Synth};
