//! ROM addresses and the program counter's step order.
//!
//! The program counter is a 2-bit P_U, a 4-bit P_M (together the page) and a 6-bit P_L, the step
//! within the page (Sharp 1990, SM510 "Hardware Configuration (1)"). A ROM file holds the whole
//! 4,096-byte address space, `P_U:P_M:P_L` in that order, so a page is 64 bytes. P_L is not a
//! binary counter but "a polynomial counter ... shifted by 1 step according to a polynomial
//! code", with 63 steps a page; Sharp does not give the polynomial. It is the 6-bit shift
//! register below: shifted right, with bit 5 fed by the complement of bit 0 XOR bit 1. That
//! is our derivation (docs/provenance.md): it starts at 0, runs through 63 states, never reaches
//! 0x3F (so `T` takes 0x80-0xBE, as the SM3903's table says), and it is the one order of the 6-bit
//! maximal sequences in which every unit's power-on and wake-up code reads as a program.

/// A ROM address: `P_U` (bits 11-10), `P_M` (9-6), `P_L` (5-0).
pub type Addr = u16;

/// The step after `pl` in a page.
pub const fn next_step(pl: u8) -> u8 {
    let feedback = !(pl ^ (pl >> 1)) & 1;
    (pl >> 1) | (feedback << 5)
}

/// The address after `addr`: the next step, in the same page.
pub const fn next(addr: Addr) -> Addr {
    (addr & !0x3F) | next_step((addr & 0x3F) as u8) as Addr
}

/// The address of page `pu`:`pm`, step `pl`.
pub const fn addr(pu: u8, pm: u8, pl: u8) -> Addr {
    ((pu as Addr & 3) << 10) | ((pm as Addr & 15) << 6) | (pl as Addr & 0x3F)
}

/// The steps of a page in execution order, starting from step 0.
pub fn steps() -> [u8; 63] {
    let mut out = [0; 63];
    let mut pl = 0;
    for s in &mut out {
        *s = pl;
        pl = next_step(pl);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_step_counter_visits_63_states_and_never_0x3f() {
        let s = steps();
        let mut seen = [false; 64];
        for &pl in &s {
            assert!(!seen[pl as usize], "step {pl:#x} twice");
            seen[pl as usize] = true;
        }
        assert!(!seen[0x3F]);
        assert_eq!(next_step(s[62]), 0, "the sequence closes");
        assert_eq!(next_step(0x3F), 0x3F, "0x3F is the stuck state");
    }

    #[test]
    fn next_stays_in_the_page() {
        let a = addr(3, 7, 0);
        assert_eq!(a, 0xDC0);
        assert_eq!(next(a) & !0x3F, a);
        assert_eq!(next(a) & 0x3F, 0x20);
    }
}
