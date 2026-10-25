//! A disassembler, in Sharp's mnemonics, for tracing and for the probe.

use crate::chip::Kind;
use crate::rom::{self, Addr};
use alloc::format;
use alloc::string::String;

/// The instruction at `at`: its text and its length in bytes.
pub fn instruction(kind: Kind, program: &[u8; 4096], at: Addr) -> (String, u32) {
    let op = program[at as usize];
    let arg = program[rom::next(at) as usize];
    let melodic = kind.melodic();
    let low = op & 15;
    let two = |s: &str| (format!("{s} {arg:02X}"), 2);
    let one = |s: String| (s, 1);
    match op {
        0x00 if melodic => one("ROT".into()),
        0x00 => one("SKIP".into()),
        0x01 if melodic => one("DTA".into()),
        0x01 => one("ATBP".into()),
        0x02 => one("SBM".into()),
        0x03 => one("ATPL".into()),
        0x04..=0x07 => one(format!("RM {}", op & 3)),
        0x08 => one("ADD".into()),
        0x09 => one("ADD11".into()),
        0x0A => one("COMA".into()),
        0x0B => one("EXBLA".into()),
        0x0C..=0x0F => one(format!("SM {}", op & 3)),
        0x10..=0x13 => one(format!("EXC {}", op & 3)),
        0x14..=0x17 => one(format!("EXCI {}", op & 3)),
        0x18..=0x1B => one(format!("LDA {}", op & 3)),
        0x1C..=0x1F => one(format!("EXCD {}", op & 3)),
        0x20..=0x2F => one(format!("LAX {low:X}")),
        0x3A => one("DC".into()),
        0x30..=0x3F => one(format!("ADX {low:X}")),
        0x40..=0x4F => one(format!("LB {low:X}")),
        0x50 if melodic => one("KTA".into()),
        0x51 => one("TB".into()),
        0x52 => one("TC".into()),
        0x53 => one("TAM".into()),
        0x54..=0x57 => one(format!("TMI {}", op & 3)),
        0x58 => one("TIS".into()),
        0x59 => one("ATL".into()),
        0x5A => one("TA0".into()),
        0x5B => one("TABL".into()),
        0x5C if melodic => one("ATX".into()),
        0x5D => one("CEND".into()),
        0x5E => one("TAL".into()),
        0x5F => two("LBL"),
        0x60 if melodic => match arg {
            0x30 => ("RME".into(), 2),
            0x31 => ("SME".into(), 2),
            0x32 => ("TMEL".into(), 2),
            0x33 => ("ATFC".into(), 2),
            0x34 => ("BDC".into(), 2),
            0x35 => ("ATBP".into(), 2),
            0x36 => ("CLKHI".into(), 2),
            0x37 => ("CLKLO".into(), 2),
            _ => two("?60"),
        },
        0x60 => one("ATFC".into()),
        0x61 if melodic => two("PRE"),
        0x61 => one("ATR".into()),
        0x62 => one("WR".into()),
        0x63 => one("WS".into()),
        0x64 => one("INCB".into()),
        0x65 => one("IDIV".into()),
        0x66 => one("RC".into()),
        0x67 => one("SC".into()),
        0x68..=0x6B if melodic => (format!("TML {}", target(op & 3, arg)), 2),
        0x68 => one("TF1".into()),
        0x69 => one("TF4".into()),
        0x6A => one("KTA".into()),
        0x6B => one("ROT".into()),
        0x6C => one("DECB".into()),
        0x6D if melodic => one("PTW".into()),
        0x6D => one("BDC".into()),
        0x6E => one("RTN0".into()),
        0x6F => one("RTN1".into()),
        0x70..=0x7F if melodic || op <= 0x7B => (format!("TL {}", target(low, arg)), 2),
        0x7C..=0x7F => (format!("TML {}", target(op & 3, arg)), 2),
        0x80..=0xBF => one(format!("T {:02X}", op & 0x3F)),
        0xC0..=0xFF => one(format!("TM {:02X}", op & 0x3F)),
        _ => one(format!("?{op:02X}")),
    }
}

/// A long jump's destination, `page step`: P_U from the operand's top bits, P_M from the opcode.
fn target(pm: u8, arg: u8) -> String {
    format!("{:02X} {:02X}", ((arg >> 6) << 4) | pm, arg & 0x3F)
}

/// An address as `page:step` (page = P_U:P_M), the way traces print it.
pub fn addr(at: Addr) -> String {
    format!("{:02X}:{:02X}", at >> 6, at & 0x3F)
}
