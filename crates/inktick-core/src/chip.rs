//! The microcontroller: registers, RAM, the 15-stage divider and the instruction set.
//!
//! Sources, by section of Sharp's *Microcomputers Data Book* (1990): SM510 pp. 149-158 and
//! SM511/SM512 pp. 159-171 ("Hardware Configuration", "Pin Functions", "Instruction Set"); the
//! SM510's instruction table is confirmed by the SM3903's in the 1996 *Microcomputer Data
//! Book* (p. 325). Where they are silent, the behaviour chosen is marked "Derived" and recorded
//! in docs/provenance.md.

use crate::rom::{self, Addr};
use crate::state::{Visit, Visitor};
use alloc::boxed::Box;

/// Which chip of the family.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// 2,772-step ROM (44 pages), 96 + 32 nibbles of RAM, 4.096 kHz buzzer output.
    Sm510,
    /// 4,032-step ROM (64 pages), 96 + 32 nibbles, a melody generator.
    Sm511,
    /// As the SM511, with 80 + 48 nibbles: a third display file drives the c segments.
    Sm512,
}

impl Kind {
    /// The SM511 and SM512: a melody ROM, the 0x60 prefix and the selectable system clock.
    pub fn melodic(self) -> bool {
        self != Kind::Sm510
    }

    /// Its name, as Sharp writes it.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Sm510 => "SM510",
            Kind::Sm511 => "SM511",
            Kind::Sm512 => "SM512",
        }
    }
}

/// The input pins' levels, as the unit's buttons and wiring set them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Lines {
    /// For each strobe output S1-S8 (bit i of W), the K1-K4 lines (bits 0-3) its pressed keys
    /// pull high when it is driven high.
    pub matrix: [u8; 8],
    /// K lines held high whatever the strobes say (keys wired to the supply).
    pub k: u8,
    /// The BA input's level (pulled up, so true when its key is not pressed).
    pub ba: bool,
    /// The β input's level (pulled up).
    pub beta: bool,
}

impl Lines {
    /// Nothing pressed: K low, BA and β high.
    pub const IDLE: Lines = Lines {
        matrix: [0; 8],
        k: 0,
        ba: true,
        beta: true,
    };
}

/// A snapshot of the registers, for tests, the probe and debugging.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Registers {
    /// The program counter.
    pub pc: Addr,
    /// The two stack levels, S then R.
    pub stack: [Addr; 2],
    /// Acc.
    pub acc: u8,
    /// C.
    pub carry: bool,
    /// B_M.
    pub bm: u8,
    /// B_L.
    pub bl: u8,
    /// W, the strobes.
    pub w: u8,
    /// L, Y and X.
    pub l: u8,
    /// Y.
    pub y: u8,
    /// X (SM511/512).
    pub x: u8,
    /// BP and BC.
    pub bp: bool,
    /// BC.
    pub bc: bool,
    /// R (SM510).
    pub r: u8,
    /// The divider.
    pub div: u16,
    /// γ.
    pub gamma: bool,
    /// Stopped by CEND.
    pub halted: bool,
    /// The SM511/512's fast clock.
    pub fast: bool,
}

/// The program counter after the clock stop wakes: P_U = 1, P_M = 0, P_L = 0.
const WAKE: Addr = rom::addr(1, 0, 0);
/// The program counter after power-on or ACL: P_U = 3, P_M = 7, P_L = 0.
const RESET: Addr = rom::addr(3, 7, 0);

/// A Sharp SM510, SM511 or SM512.
#[derive(Clone)]
pub struct Chip {
    kind: Kind,
    program: Box<[u8; 4096]>,

    pub(crate) pc: Addr,
    /// The address of the instruction being executed.
    at: Addr,
    stack: [Addr; 2],
    pub(crate) acc: u8,
    pub(crate) carry: bool,
    pub(crate) bm: u8,
    pub(crate) bl: u8,
    /// SBM: B_M's top bit forced to 1 for the next instruction only.
    sbm: bool,
    pub(crate) ram: [u8; 128],

    /// The strobe outputs S1-S8 (SM510: shifted by WR/WS; SM511/512: loaded from W' by PTW).
    pub(crate) w: u8,
    /// The SM511/512's shift register behind W.
    w_shift: u8,
    pub(crate) l: u8,
    pub(crate) y: u8,
    /// The SM511/512's X register (ATX), shown on bs2.
    pub(crate) x: u8,
    /// BP: the display is shown while BP = 1 and BC = 0.
    pub(crate) bp: bool,
    pub(crate) bc: bool,
    /// The SM510's R register (ATR): R1 and R2 enabled, bits 0 and 1.
    pub(crate) r: u8,

    /// The 15-stage divider, counting 32.768 kHz crystal cycles: bit 14 is the 1-second signal.
    pub(crate) div: u16,
    /// γ, set on the rising edge of the 1-second signal, tested and cleared by TIS.
    gamma: bool,
    /// Halted by CEND.
    pub(crate) halted: bool,
    /// The SM511/512's system clock: 16.384 kHz (true) or 8.192 kHz.
    pub(crate) fast: bool,
    /// The next instruction is skipped.
    skip: bool,
    /// The last instruction executed was LAX (a following LAX is skipped).
    after_lax: bool,

    pub(crate) melody: crate::melody::Melody,
    lines: Lines,
}

impl Chip {
    /// A chip of `kind` with its 4,096-byte program ROM (the whole address space, as dumped),
    /// powered on: the program starts at P_U = 3, P_M = 7, P_L = 0.
    pub fn new(kind: Kind, program: &[u8; 4096], melody: Option<&[u8; 256]>) -> Chip {
        let mut chip = Chip {
            kind,
            program: Box::new(*program),
            pc: RESET,
            at: RESET,
            stack: [0; 2],
            acc: 0,
            carry: false,
            bm: 0,
            bl: 0,
            sbm: false,
            ram: [0; 128],
            w: 0,
            w_shift: 0,
            l: 0,
            y: 0,
            x: 0,
            bp: false,
            bc: false,
            r: 0,
            div: 0,
            gamma: false,
            halted: false,
            fast: false,
            skip: false,
            after_lax: false,
            melody: crate::melody::Melody::new(melody),
            lines: Lines::IDLE,
        };
        chip.reset();
        chip
    }

    /// ACL: the program restarts at P_U = 3, P_M = 7, P_L = 0. "The system clock immediately
    /// after executing ACL is set to 8.192kHz" (SM511/512, "Selection of system clock"). Derived:
    /// BP is set (several units never execute ATBP yet show their LCD), the other outputs are
    /// cleared (W, BC, R, the melody), RAM and the divider are kept.
    pub fn reset(&mut self) {
        self.pc = RESET;
        self.halted = false;
        self.skip = false;
        self.after_lax = false;
        self.sbm = false;
        self.fast = false;
        self.w = 0;
        self.w_shift = 0;
        self.bp = true;
        self.bc = false;
        self.r = 0;
        self.melody.stop();
    }

    /// Which chip this is.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The program ROM.
    pub fn program(&self) -> &[u8; 4096] {
        &self.program
    }

    /// The program counter.
    pub fn pc(&self) -> Addr {
        self.pc
    }

    /// The registers.
    pub fn registers(&self) -> Registers {
        Registers {
            pc: self.pc,
            stack: self.stack,
            acc: self.acc,
            carry: self.carry,
            bm: self.bm,
            bl: self.bl,
            w: self.w,
            l: self.l,
            y: self.y,
            x: self.x,
            bp: self.bp,
            bc: self.bc,
            r: self.r,
            div: self.div,
            gamma: self.gamma,
            halted: self.halted,
            fast: self.fast,
        }
    }

    /// RAM, 8 files of 16 nibbles: `ram()[bm * 16 + bl]`.
    pub fn ram(&self) -> &[u8; 128] {
        &self.ram
    }

    /// Writes a RAM nibble (for setting a unit's clock from outside, by its database entry).
    pub fn set_ram(&mut self, at: usize, value: u8) {
        self.ram[at & 127] = value & 15;
    }

    /// The strobe outputs S1-S8, bit i for S(i+1).
    pub fn strobes(&self) -> u8 {
        self.w
    }

    /// The input pins.
    pub fn set_lines(&mut self, lines: Lines) {
        self.lines = lines;
    }

    /// The input pins as last set.
    pub fn lines(&self) -> Lines {
        self.lines
    }

    /// K1-K4 as the chip reads them now: the matrix through the strobes, and keys held high.
    pub fn k(&self) -> u8 {
        let mut k = self.lines.k;
        for (i, keys) in self.lines.matrix.iter().enumerate() {
            if self.w & (1 << i) != 0 {
                k |= keys;
            }
        }
        k & 15
    }

    /// Crystal cycles per instruction cycle: the system clock is the crystal halved (16.384 kHz)
    /// or, on the SM511/512 when selected, quartered (8.192 kHz).
    pub fn ticks_per_cycle(&self) -> u32 {
        if self.kind.melodic() && !self.fast {
            4
        } else {
            2
        }
    }

    /// Whether the clock stop would end now: γ set, or a K line high.
    pub(crate) fn wakes(&self) -> bool {
        self.gamma || self.k() != 0
    }

    /// Ends the clock stop: the program starts at P_U = 1, P_M = 0, P_L = 0.
    pub(crate) fn wake(&mut self) {
        self.halted = false;
        self.pc = WAKE;
    }

    /// Advances the divider by one crystal cycle; γ is set as the count wraps (the 1-second
    /// signal's falling edge), a second after IDIV: fitted to the black box (Fire Attack's,
    /// Tennis's and Snoopy's sounds at a game's start fall on it, not on the rising edge).
    pub(crate) fn tick_divider(&mut self) {
        self.div = (self.div + 1) & 0x7FFF;
        if self.div == 0 {
            self.gamma = true;
        }
    }

    /// Divider tap f_n, the 2^n Hz square wave (Sharp's naming in the SM500 and SM5L chapters,
    /// where R1 is gated "with the f1, f4 or f12 of a divider", and the SM5L's divider runs
    /// f15 to f0 with f0 at 1 Hz): bit 14 − n of the 15-stage count.
    pub(crate) fn tap(&self, n: u32) -> bool {
        self.div & (1 << (14 - n)) != 0
    }

    /// The RAM address: B_M (with SBM's top bit for this instruction) and B_L.
    fn m_addr(&self) -> usize {
        let bm = if self.sbm { self.bm | 4 } else { self.bm };
        ((bm as usize & 7) << 4) | self.bl as usize
    }

    fn m(&self) -> u8 {
        self.ram[self.m_addr()]
    }

    fn set_m(&mut self, v: u8) {
        let at = self.m_addr();
        self.ram[at] = v & 15;
    }

    fn fetch(&mut self) -> u8 {
        let byte = self.program[self.pc as usize];
        self.pc = rom::next(self.pc);
        byte
    }

    fn call(&mut self, to: Addr) {
        // "R ← S ← PC + 1": the return address is the instruction after the call.
        self.stack[1] = self.stack[0];
        self.stack[0] = self.pc;
        self.pc = to;
    }

    fn ret(&mut self) {
        self.pc = self.stack[0];
        self.stack[0] = self.stack[1];
    }

    /// Exchanges or loads M and XORs B_M's low bits with the opcode's (EXC, EXCI, EXCD, LDA).
    fn flip_bm(&mut self, op: u8) {
        self.bm ^= op & 3;
    }

    /// Runs one instruction (or one skipped instruction) and returns its instruction cycles:
    /// 1, or 2 for a two-byte instruction or TM.
    pub fn step(&mut self) -> u32 {
        self.at = self.pc;
        let op = self.fetch();
        let two = self.two_byte(op);
        let arg = if two { self.fetch() } else { 0 };
        let cycles = if two || (0xC0..=0xFF).contains(&op) {
            2
        } else {
            1
        };

        let lax = (0x20..=0x2F).contains(&op);
        let skipped_by_test = self.skip;
        let skipped = skipped_by_test || (self.after_lax && lax);
        self.skip = false;
        // Derived: "skip when in succession" makes a run of LAX a table that is entered part-way
        // and loads only its first value, so a LAX skipped for following one still counts;
        // a LAX skipped by a test does not (Super Mario Bros.' clock skips one by ADX's carry
        // and needs the LAX after it to run).
        self.after_lax = lax && !skipped_by_test;
        let sbm_now = self.sbm;
        if !skipped {
            match self.kind {
                Kind::Sm510 => self.exec_510(op, arg),
                Kind::Sm511 | Kind::Sm512 => self.exec_511(op, arg),
            }
        }
        // SBM lasts for "only next step": the instruction after it.
        if sbm_now {
            self.sbm = false;
        }
        cycles
    }

    fn two_byte(&self, op: u8) -> bool {
        match self.kind {
            Kind::Sm510 => op == 0x5F || (0x70..=0x7F).contains(&op),
            _ => matches!(op, 0x5F | 0x60 | 0x61 | 0x68..=0x6B | 0x70..=0x7F),
        }
    }

    /// The instructions the family shares, by the SM510's codes where the SM511 differs not.
    /// Returns false if `op` is not one of them.
    fn exec_common(&mut self, op: u8, arg: u8) -> bool {
        match op {
            // SBM: B_M2 ← 1 (only next step).
            0x02 => self.sbm = true,
            // ATPL: P_L3-P_L0 ← Acc. Derived: P_L5,4 are the ATPL's own.
            0x03 => self.pc = (self.at & !0x0F) | self.acc as Addr,
            // RM x / SM x: M_i ← 0 / 1.
            0x04..=0x07 => self.set_m(self.m() & !(1 << (op & 3))),
            0x0C..=0x0F => self.set_m(self.m() | (1 << (op & 3))),
            // ADD: Acc ← Acc + M.
            0x08 => self.acc = (self.acc + self.m()) & 15,
            // ADD11: Acc ← Acc + M + C, C ← CY, skip if CY = 1.
            0x09 => {
                let sum = self.acc + self.m() + self.carry as u8;
                self.acc = sum & 15;
                self.carry = sum > 15;
                self.skip = self.carry;
            }
            // COMA: Acc ← ¬Acc.
            0x0A => self.acc ^= 15,
            // EXBLA: Acc ↔ B_L.
            0x0B => core::mem::swap(&mut self.acc, &mut self.bl),
            // EXC x: Acc ↔ M, B_M1,0 ← B_M1,0 ⊕ x.
            0x10..=0x13 => {
                let m = self.m();
                self.set_m(self.acc);
                self.acc = m;
                self.flip_bm(op);
            }
            // EXCI x: as EXC, then skip if B_L = F, B_L ← B_L + 1.
            0x14..=0x17 => {
                let m = self.m();
                self.set_m(self.acc);
                self.acc = m;
                self.flip_bm(op);
                self.skip = self.bl == 15;
                self.bl = (self.bl + 1) & 15;
            }
            // LDA x: Acc ← M, B_M1,0 ← B_M1,0 ⊕ x.
            0x18..=0x1B => {
                self.acc = self.m();
                self.flip_bm(op);
            }
            // EXCD x: as EXC, then skip if B_L = 0, B_L ← B_L − 1.
            0x1C..=0x1F => {
                let m = self.m();
                self.set_m(self.acc);
                self.acc = m;
                self.flip_bm(op);
                self.skip = self.bl == 0;
                self.bl = self.bl.wrapping_sub(1) & 15;
            }
            // LAX x: Acc ← x (a LAX right after a LAX is skipped: Chip::step).
            0x20..=0x2F => self.acc = op & 15,
            // DC: Acc ← Acc + 10 (no skip: the SM3903's table).
            0x3A => self.acc = (self.acc + 10) & 15,
            // ADX x: Acc ← Acc + x, skip if CY = 1 (C unchanged).
            0x30..=0x3F => {
                let sum = self.acc + (op & 15);
                self.acc = sum & 15;
                self.skip = sum > 15;
            }
            // LB x: B_L1,0 ← x(I3, I2), B_M1,0 ← x(I1, I0), B_L3,2 ← I3 ∨ I2 (see lb_high).
            0x40..=0x4F => {
                let (i3, i2) = ((op >> 3) & 1, (op >> 2) & 1);
                self.bm = (self.bm & 4) | (op & 3);
                self.bl = (lb_high(i3, i2) << 2) | ((op >> 2) & 3);
            }
            // TB: skip if β = 1.
            0x51 => self.skip = self.lines.beta,
            // TC: skip if C = 0.
            0x52 => self.skip = !self.carry,
            // TAM: skip if Acc = M.
            0x53 => self.skip = self.acc == self.m(),
            // TMI x: skip if M_i = 1.
            0x54..=0x57 => self.skip = self.m() & (1 << (op & 3)) != 0,
            // TIS: skip if 1S = 0, γ ← 0. Derived: "1S" here is γ, the flag the 1-second
            // signal's edge sets ("Y F/F can be tested by an instruction, and reset by the test").
            0x58 => {
                self.skip = !self.gamma;
                self.gamma = false;
            }
            // ATL: L ← Acc.
            0x59 => self.l = self.acc,
            // TA0: skip if Acc = 0.
            0x5A => self.skip = self.acc == 0,
            // TABL: skip if Acc = B_L.
            0x5B => self.skip = self.acc == self.bl,
            // CEND: the system clock stops.
            0x5D => self.halted = true,
            // TAL: skip if BA = 1.
            0x5E => self.skip = self.lines.ba,
            // LBL xy: B_M ← x(I6-I4), B_L ← y(I3-I0).
            0x5F => {
                self.bm = (arg >> 4) & 7;
                self.bl = arg & 15;
            }
            // WR / WS: shift a 0 or 1 into the strobe register (SM511/512: into W').
            0x62 | 0x63 => {
                let bit = op & 1;
                if self.kind.melodic() {
                    self.w_shift = (self.w_shift << 1) | bit;
                } else {
                    self.w = (self.w << 1) | bit;
                }
            }
            // INCB: skip if B_L = F, B_L ← B_L + 1.
            0x64 => {
                self.skip = self.bl == 15;
                self.bl = (self.bl + 1) & 15;
            }
            // IDIV: DIV ← 0.
            0x65 => self.div = 0,
            // RC / SC.
            0x66 => self.carry = false,
            0x67 => self.carry = true,
            // DECB: skip if B_L = 0, B_L ← B_L − 1.
            0x6C => {
                self.skip = self.bl == 0;
                self.bl = self.bl.wrapping_sub(1) & 15;
            }
            // RTN0 / RTN1 (RTN1 skips the next step).
            0x6E => self.ret(),
            0x6F => {
                self.ret();
                self.skip = true;
            }
            // T xy: P_L ← x(I5-I0).
            0x80..=0xBF => self.pc = (self.at & !0x3F) | (op & 0x3F) as Addr,
            // TM x: R ← S ← PC + 1, then the IDX byte at page 0 step x: P_U ← y(I7, I6),
            // P_M ← 4, P_L ← z(I5-I0).
            0xC0..=0xFF => {
                let idx = self.program[(op & 0x3F) as usize];
                self.call(rom::addr(idx >> 6, 4, idx & 0x3F));
            }
            _ => return false,
        }
        true
    }

    fn exec_510(&mut self, op: u8, arg: u8) {
        if self.exec_common(op, arg) {
            return;
        }
        match op {
            // SKIP: no operation.
            0x00 => {}
            // ATBP: BP ← Acc (bit 0).
            0x01 => self.bp = self.acc & 1 != 0,
            // ATFC: Y ← Acc.
            0x60 => self.y = self.acc,
            // ATR: R_i ← Acc (i = 1, 2).
            0x61 => self.r = self.acc & 3,
            // TF1 / TF4: skip if f1 / f4 = 1.
            0x68 => self.skip = self.tap(1),
            0x69 => self.skip = self.tap(4),
            // KTA: Acc ← K.
            0x6A => self.acc = self.k(),
            // ROT: C ← Acc0, Acc_i ← Acc_i+1, Acc3 ← C.
            0x6B => self.rot(),
            // BDC: BC ← C (display off while C = 1).
            0x6D => self.bc = self.carry,
            // TL xyz: P_M ← x(I3-I0), P_U ← y(I7, I6), P_L ← z(I5-I0).
            0x70..=0x7B => self.pc = rom::addr(arg >> 6, op & 15, arg & 0x3F),
            // TML xyz: R ← S ← PC + 1, P_M ← (0, 0, I1, I0), P_U ← y(I7, I6), P_L ← z.
            0x7C..=0x7F => self.call(rom::addr(arg >> 6, op & 3, arg & 0x3F)),
            // 0x50, 0x5C: not in Sharp's table; found only in page 0's IDX data, never run.
            _ => {}
        }
    }

    fn exec_511(&mut self, op: u8, arg: u8) {
        if self.exec_common(op, arg) {
            return;
        }
        match op {
            // ROT: C ← Acc0, Acc_i ← Acc_i+1, Acc3 ← C.
            0x00 => self.rot(),
            // Derived (not in Sharp's SM511 table, whose count of 55 it completes): DTA,
            // Acc ← the divider's top four bits, f0 (1 Hz) in bit 3; the order fitted to the
            // black box (Snoopy's game starts when it does only this way round).
            0x01 => self.acc = ((self.div >> 11) & 15) as u8,
            // KTA: Acc ← K.
            0x50 => self.acc = self.k(),
            // ATX: X ← Acc.
            0x5C => self.x = self.acc,
            0x60 => match arg {
                // RME / SME: ME ← 0 / 1.
                0x30 => self.melody.enable(false),
                0x31 => self.melody.enable(true),
                // TMEL: skip if MES = 1, MES ← 0.
                0x32 => self.skip = self.melody.take_end(),
                // ATFC: Y ← Acc.
                0x33 => self.y = self.acc,
                // BDC: BC ← C.
                0x34 => self.bc = self.carry,
                // ATBP: BP ← Acc.
                0x35 => self.bp = self.acc & 1 != 0,
                // Derived (not in Sharp's table): select the 16.384 kHz system clock. Every
                // unit issues it at power-on and on waking from CEND, then idles a few steps.
                0x36 => self.fast = true,
                // Derived: select 8.192 kHz. Every unit issues it, then idles, before CEND.
                0x37 => self.fast = false,
                _ => {}
            },
            // PRE x: melody ROM pointer preset.
            0x61 => self.melody.preset(arg),
            // TML xyz: as the SM510's, at 0x68-0x6B.
            0x68..=0x6B => self.call(rom::addr(arg >> 6, op & 3, arg & 0x3F)),
            // PTW: W_i ← W'_i.
            0x6D => self.w = self.w_shift,
            // TL xyz: P_M ← x(I3-I0), all 16 pages of each P_U.
            0x70..=0x7F => self.pc = rom::addr(arg >> 6, op & 15, arg & 0x3F),
            _ => {}
        }
    }

    fn rot(&mut self) {
        let out = self.acc & 1 != 0;
        self.acc = (self.acc >> 1) | ((self.carry as u8) << 3);
        self.carry = out;
    }
}

/// LB's B_L3 and B_L2: Sharp writes "B_L3, B_L2 ← x(I3) ⊕ x(I2)" (SM510, SM3903), where ⊕
/// could be XOR or OR. Derived: OR. LB then reaches B_L 0, D, E and F, so a program can start a
/// count-down loop at the top of a file; with XOR (0, D, E, 3) Donkey Kong's power-on loop
/// leaves most of the display RAM clear, and with OR the unit shows every segment after ACL,
/// as the units do. The SM511/512 table omits the term; it is taken to be the same.
fn lb_high(i3: u8, i2: u8) -> u8 {
    let h = i3 | i2;
    (h << 1) | h
}

impl Visit for Chip {
    fn visit(&mut self, v: &mut impl Visitor) {
        v.u16(&mut self.pc);
        v.u16(&mut self.at);
        v.u16(&mut self.stack[0]);
        v.u16(&mut self.stack[1]);
        v.u8(&mut self.acc);
        v.bool(&mut self.carry);
        v.u8(&mut self.bm);
        v.u8(&mut self.bl);
        v.bool(&mut self.sbm);
        v.bytes(&mut self.ram);
        v.u8(&mut self.w);
        v.u8(&mut self.w_shift);
        v.u8(&mut self.l);
        v.u8(&mut self.y);
        v.u8(&mut self.x);
        v.bool(&mut self.bp);
        v.bool(&mut self.bc);
        v.u8(&mut self.r);
        v.u16(&mut self.div);
        v.bool(&mut self.gamma);
        v.bool(&mut self.halted);
        v.bool(&mut self.fast);
        v.bool(&mut self.skip);
        v.bool(&mut self.after_lax);
        self.melody.visit(v);
    }
}
