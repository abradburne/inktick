//! A small assembler in Sharp's mnemonics, for tests: there are no public test ROMs for these
//! chips, so the tests are programs.
//!
//! ```text
//! .page 37          ; following instructions go to page 0x37 from step 0, in execution order
//! start: LAX 5      ; a label
//!        EXC 0
//!        T start    ; T, TL and TML take a label or a number
//!        .byte 2A   ; a raw byte
//! ```
//!
//! Operands are hex. `TL page step`, `TML page step` and `LBL xy` as the disassembler prints
//! them, or `TL label`. Labels may be used before they are defined.

use crate::chip::Kind;
use crate::rom::{self, Addr};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Assembles `src` for `kind` into a 4,096-byte program (unused bytes 0).
pub fn assemble(kind: Kind, src: &str) -> Result<[u8; 4096], String> {
    let steps = rom::steps();
    // Two passes: the first places every instruction and label, the second encodes.
    let mut labels: BTreeMap<String, Addr> = BTreeMap::new();
    let mut placed: Vec<(Addr, String, usize)> = Vec::new();
    let (mut page, mut index) = (0x37u16, 0usize);
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        let mut line = line;
        if let Some((label, rest)) = line.split_once(':') {
            if !label.contains(' ') && !label.is_empty() {
                let at = (page << 6) | *steps.get(index).ok_or("page full")? as Addr;
                labels.insert(label.trim().to_string(), at);
                line = rest.trim();
            }
        }
        if line.is_empty() {
            continue;
        }
        if let Some(p) = line.strip_prefix(".page") {
            page =
                u16::from_str_radix(p.trim(), 16).map_err(|_| format!("line {}: page", n + 1))?;
            index = 0;
            continue;
        }
        let at = (page << 6)
            | *steps
                .get(index)
                .ok_or(format!("line {}: page full", n + 1))? as Addr;
        let len = length(kind, line);
        placed.push((at, line.to_string(), n + 1));
        index += len;
    }
    let mut out = [0u8; 4096];
    for (at, line, n) in placed {
        let bytes =
            encode(kind, &line, at, &labels).map_err(|e| format!("line {n}: {e}: {line}"))?;
        let mut a = at;
        for b in bytes {
            out[a as usize] = b;
            a = rom::next(a);
        }
    }
    Ok(out)
}

fn length(kind: Kind, line: &str) -> usize {
    let m = line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_uppercase();
    let melodic_two = ["RME", "SME", "TMEL", "CLKHI", "CLKLO", "PRE"];
    match m.as_str() {
        "LBL" | "TL" | "TML" => 2,
        "ATFC" | "BDC" | "ATBP" if kind.melodic() => 2,
        _ if kind.melodic() && melodic_two.contains(&m.as_str()) => 2,
        _ => 1,
    }
}

fn encode(
    kind: Kind,
    line: &str,
    at: Addr,
    labels: &BTreeMap<String, Addr>,
) -> Result<Vec<u8>, String> {
    let mut parts = line.split_whitespace();
    let m = parts.next().unwrap_or("").to_ascii_uppercase();
    let args: Vec<&str> = parts.collect();
    let hex = |i: usize| -> Result<u8, String> {
        let a = args.get(i).ok_or("missing operand")?;
        u8::from_str_radix(a, 16).map_err(|_| "bad operand".to_string())
    };
    let target = |i: usize| -> Result<Addr, String> {
        let a = args.get(i).ok_or("missing operand")?;
        if let Some(&l) = labels.get(*a) {
            return Ok(l);
        }
        // `page step` as two hex numbers.
        let page = u16::from_str_radix(a, 16).map_err(|_| format!("no label {a}"))?;
        let step =
            u16::from_str_radix(args.get(i + 1).ok_or("missing step")?, 16).map_err(|_| "step")?;
        Ok((page << 6) | step)
    };
    let melodic = kind.melodic();
    let low = |base: u8, max: u8| -> Result<Vec<u8>, String> {
        let x = hex(0)?;
        if x > max {
            return Err("operand too big".into());
        }
        Ok(alloc::vec![base + x])
    };
    let one = |b: u8| Ok(alloc::vec![b]);
    let prefixed = |b: u8| Ok(alloc::vec![0x60, b]);
    match m.as_str() {
        "SKIP" if !melodic => one(0x00),
        "ROT" if melodic => one(0x00),
        "ROT" => one(0x6B),
        "DTA" if melodic => one(0x01),
        "ATBP" if melodic => prefixed(0x35),
        "ATBP" => one(0x01),
        "SBM" => one(0x02),
        "ATPL" => one(0x03),
        "RM" => low(0x04, 3),
        "ADD" => one(0x08),
        "ADD11" => one(0x09),
        "COMA" => one(0x0A),
        "EXBLA" => one(0x0B),
        "SM" => low(0x0C, 3),
        "EXC" => low(0x10, 3),
        "EXCI" => low(0x14, 3),
        "LDA" => low(0x18, 3),
        "EXCD" => low(0x1C, 3),
        "LAX" => low(0x20, 15),
        "DC" => one(0x3A),
        "ADX" => low(0x30, 15),
        "LB" => low(0x40, 15),
        "KTA" if melodic => one(0x50),
        "KTA" => one(0x6A),
        "TB" => one(0x51),
        "TC" => one(0x52),
        "TAM" => one(0x53),
        "TMI" => low(0x54, 3),
        "TIS" => one(0x58),
        "ATL" => one(0x59),
        "TA0" => one(0x5A),
        "TABL" => one(0x5B),
        "ATX" if melodic => one(0x5C),
        "CEND" => one(0x5D),
        "TAL" => one(0x5E),
        "LBL" => Ok(alloc::vec![0x5F, hex(0)?]),
        "RME" if melodic => prefixed(0x30),
        "SME" if melodic => prefixed(0x31),
        "TMEL" if melodic => prefixed(0x32),
        "ATFC" if melodic => prefixed(0x33),
        "BDC" if melodic => prefixed(0x34),
        "CLKHI" if melodic => prefixed(0x36),
        "CLKLO" if melodic => prefixed(0x37),
        "ATFC" => one(0x60),
        "PRE" if melodic => Ok(alloc::vec![0x61, hex(0)?]),
        "ATR" if !melodic => one(0x61),
        "WR" => one(0x62),
        "WS" => one(0x63),
        "INCB" => one(0x64),
        "IDIV" => one(0x65),
        "RC" => one(0x66),
        "SC" => one(0x67),
        "TF1" if !melodic => one(0x68),
        "TF4" if !melodic => one(0x69),
        "DECB" => one(0x6C),
        "PTW" if melodic => one(0x6D),
        "BDC" => one(0x6D),
        "RTN0" => one(0x6E),
        "RTN1" => one(0x6F),
        "T" => {
            let t = target(0).or_else(|_| hex(0).map(|s| (at & !0x3F) | s as Addr))?;
            if t >> 6 != at >> 6 {
                return Err("T stays in its page".into());
            }
            Ok(alloc::vec![0x80 | (t & 0x3F) as u8])
        }
        "TM" => low(0xC0, 0x3F),
        "TL" => {
            let t = target(0)?;
            let (pu, pm, pl) = ((t >> 10) as u8, ((t >> 6) & 15) as u8, (t & 0x3F) as u8);
            if !melodic && pm > 10 {
                return Err("SM510 pages have P_M 0-A".into());
            }
            Ok(alloc::vec![0x70 | pm, (pu << 6) | pl])
        }
        "TML" => {
            let t = target(0)?;
            let (pu, pm, pl) = ((t >> 10) as u8, ((t >> 6) & 15) as u8, (t & 0x3F) as u8);
            if pm > 3 {
                return Err("TML reaches P_M 0-3".into());
            }
            Ok(alloc::vec![
                if melodic { 0x68 } else { 0x7C } | pm,
                (pu << 6) | pl
            ])
        }
        ".BYTE" => Ok(alloc::vec![hex(0)?]),
        _ => Err("unknown instruction".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disasm;

    #[test]
    fn round_trips_through_the_disassembler() {
        for kind in [Kind::Sm510, Kind::Sm511] {
            let src = "LAX 5\nEXC 1\nLBL 6F\nTMI 2\nTL 10 05\nTM 3F\nLB C\nRTN1";
            let p = assemble(kind, src).unwrap();
            let mut at = rom::addr(3, 7, 0);
            for line in src.lines() {
                let (text, len) = disasm::instruction(kind, &p, at);
                assert_eq!(text, line, "{kind:?}");
                for _ in 0..len {
                    at = rom::next(at);
                }
            }
        }
    }
}
