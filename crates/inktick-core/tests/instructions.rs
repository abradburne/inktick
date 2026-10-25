//! Each instruction, as Sharp's tables describe it (1990 data book: SM510 pp. 156-157, SM511/512
//! p. 170), or as derived where they are silent (docs/provenance.md). Programs start at page
//! 0x37, step 0, the power-on address.

use inktick_core::asm::assemble;
use inktick_core::{rom, Chip, Kind, Lines, Machine, Options, CRYSTAL_HZ};

fn chip(kind: Kind, src: &str) -> Chip {
    let p = assemble(kind, src).unwrap_or_else(|e| panic!("{e}"));
    Chip::new(kind, &p, kind.melodic().then_some(&[0; 256]))
}

fn run(kind: Kind, src: &str, steps: usize) -> Chip {
    let mut c = chip(kind, src);
    for _ in 0..steps {
        c.step();
    }
    c
}

fn both(src: &str, steps: usize, check: impl Fn(&Chip)) {
    for kind in [Kind::Sm510, Kind::Sm511] {
        check(&run(kind, src, steps));
    }
}

#[test]
fn lax_loads_and_a_run_loads_its_first() {
    both("LAX 5", 1, |c| assert_eq!(c.registers().acc, 5));
    both("LAX 5\nLAX 6\nLAX 7", 3, |c| {
        assert_eq!(c.registers().acc, 5)
    });
    // A LAX skipped by a test breaks the run: ADX's carry skips LAX 0, LAX 3 runs.
    both("LAX 8\nADX 8\nLAX 0\nLAX 3", 4, |c| {
        assert_eq!(c.registers().acc, 3)
    });
}

#[test]
fn lb_sets_bm_and_bl() {
    // B_L1,0 ← I3,I2; B_M1,0 ← I1,I0; B_L3,2 ← I3 ∨ I2.
    for (x, bm, bl) in [
        (0x0, 0, 0x0),
        (0x5, 1, 0xD),
        (0xA, 2, 0xE),
        (0xF, 3, 0xF),
        (0xC, 0, 0xF),
    ] {
        both(&format!("LB {x:X}"), 1, |c| {
            let r = c.registers();
            assert_eq!((r.bm, r.bl), (bm, bl), "LB {x:X}");
        });
    }
}

#[test]
fn lbl_sets_both() {
    both("LBL 6A", 1, |c| {
        assert_eq!((c.registers().bm, c.registers().bl), (6, 0xA))
    });
}

#[test]
fn exchanges_and_loads_flip_bm() {
    // EXC x: Acc ↔ M, B_M1,0 ← B_M1,0 ⊕ x.
    both("LBL 12\nLAX 9\nEXC 3", 3, |c| {
        assert_eq!(c.ram()[0x12], 9);
        assert_eq!(c.registers().bm, 1 ^ 3);
        assert_eq!(c.registers().acc, 0);
    });
    // LDA x: Acc ← M.
    both("LBL 12\nLAX 9\nEXC 0\nLAX 0\nLDA 1", 5, |c| {
        assert_eq!(c.registers().acc, 9);
        assert_eq!(c.registers().bm, 0);
    });
}

#[test]
fn exci_and_excd_step_bl_and_skip_at_the_ends() {
    // EXCI: skip if B_L = F, then B_L + 1.
    both("LBL 0F\nEXCI 0\nLAX 1\nLAX 2", 4, |c| {
        assert_eq!(c.registers().bl, 0);
        assert_eq!(c.registers().acc, 2, "LAX 1 skipped");
    });
    // EXCD: skip if B_L = 0, then B_L − 1.
    both("LBL 00\nEXCD 0\nLAX 1\nLAX 2", 4, |c| {
        assert_eq!(c.registers().bl, 0xF);
        assert_eq!(c.registers().acc, 2);
    });
    both("LBL 05\nEXCD 0\nLAX 1", 3, |c| {
        assert_eq!(c.registers().acc, 1)
    });
}

#[test]
fn incb_decb() {
    both("LBL 0F\nINCB\nLAX 1\nLAX 2", 4, |c| {
        assert_eq!((c.registers().bl, c.registers().acc), (0, 2))
    });
    both("LBL 03\nDECB\nLAX 1", 3, |c| {
        assert_eq!((c.registers().bl, c.registers().acc), (2, 1))
    });
}

#[test]
fn exbla() {
    both("LBL 07\nLAX 3\nEXBLA", 3, |c| {
        assert_eq!((c.registers().bl, c.registers().acc), (3, 7))
    });
}

#[test]
fn arithmetic() {
    // ADD: no carry, no skip.
    both("LBL 00\nLAX 9\nEXC 0\nLAX 9\nADD", 5, |c| {
        assert_eq!(c.registers().acc, 2);
        assert!(!c.registers().carry);
    });
    // ADD11: with carry in, carry out, skip on carry.
    both(
        "LBL 00\nLAX 9\nEXC 0\nSC\nLAX 6\nADD11\nLAX 1\nLAX 2",
        8,
        |c| {
            assert_eq!(c.registers().acc, 2, "skipped LAX 1");
            assert!(c.registers().carry);
        },
    );
    // ADX: skip on carry, C unchanged.
    both("RC\nLAX F\nADX 2\nLAX 7", 4, |c| {
        assert_eq!(c.registers().acc, 1);
        assert!(!c.registers().carry);
    });
    // DC adds 10 and never skips.
    both("LAX 9\nDC\nLAX 0", 3, |c| assert_eq!(c.registers().acc, 0));
    both("LAX 5\nDC", 2, |c| assert_eq!(c.registers().acc, 0xF));
    both("LAX 5\nCOMA", 2, |c| assert_eq!(c.registers().acc, 0xA));
}

#[test]
fn rot_turns_right_through_carry() {
    both("SC\nLAX 6\nROT", 3, |c| {
        assert_eq!(c.registers().acc, 0xB);
        assert!(!c.registers().carry);
    });
    both("RC\nLAX 1\nROT", 3, |c| {
        assert_eq!(c.registers().acc, 0);
        assert!(c.registers().carry);
    });
}

#[test]
fn tests_skip() {
    both("LBL 00\nLAX 4\nEXC 0\nLAX 4\nTAM\nLAX 1\nLAX 2", 7, |c| {
        assert_eq!(c.registers().acc, 2)
    });
    both("LBL 00\nLAX 4\nEXC 0\nTMI 2\nLAX 1\nLAX 2", 6, |c| {
        assert_eq!(c.registers().acc, 2)
    });
    both("LBL 00\nLAX 4\nEXC 0\nTMI 1\nLAX 1", 5, |c| {
        assert_eq!(c.registers().acc, 1)
    });
    both("LAX 0\nTA0\nLAX 1\nLAX 2", 4, |c| {
        assert_eq!(c.registers().acc, 2)
    });
    both("LBL 07\nLAX 7\nTABL\nLAX 1\nLAX 2", 5, |c| {
        assert_eq!(c.registers().acc, 2)
    });
    both("RC\nTC\nLAX 1\nLAX 2", 4, |c| {
        assert_eq!(c.registers().acc, 2)
    });
    both("SC\nTC\nLAX 1", 3, |c| assert_eq!(c.registers().acc, 1));
}

#[test]
fn tb_and_tal_test_beta_and_ba() {
    for kind in [Kind::Sm510, Kind::Sm511] {
        for (level, want) in [(true, 2), (false, 1)] {
            let mut c = chip(kind, "TB\nLAX 1\nLAX 2\nTAL\nLAX 1\nLAX 2");
            c.set_lines(Lines {
                beta: level,
                ba: level,
                ..Lines::IDLE
            });
            for _ in 0..3 {
                c.step();
            }
            assert_eq!(c.registers().acc, want);
            for _ in 0..3 {
                c.step();
            }
            assert_eq!(c.registers().acc, want);
        }
    }
}

#[test]
fn bit_set_and_reset() {
    both("LBL 00\nSM 3\nSM 0\nRM 3\nSM 1", 5, |c| {
        assert_eq!(c.ram()[0], 0b0011)
    });
}

#[test]
fn sbm_lasts_one_instruction() {
    // SBM: B_M2 ← 1 for the next step only.
    both("LBL 12\nLAX 5\nSBM\nEXC 0\nLAX 6\nEXC 0", 6, |c| {
        assert_eq!(c.ram()[0x52], 5);
        assert_eq!(c.ram()[0x12], 6);
    });
}

#[test]
fn t_jumps_within_the_page() {
    both("T skip\nLAX 1\nskip: LAX 2", 2, |c| {
        assert_eq!(c.registers().acc, 2)
    });
}

#[test]
fn calls_return_and_rtn1_skips() {
    // Two levels: R ← S ← PC + 1 on each call.
    let src = "TML sub\nLAX 1\nLAX 2\n.page 02\nsub: TML sub2\nRTN1\n.page 03\nsub2: RTN0";
    for kind in [Kind::Sm510, Kind::Sm511] {
        let mut c = chip(kind, src);
        c.step();
        assert_eq!(c.pc(), rom::addr(0, 2, 0));
        c.step();
        assert_eq!(c.pc(), rom::addr(0, 3, 0));
        let r = c.registers();
        assert_eq!(r.stack[1] >> 6, 0x37, "the first return address moved to R");
        c.step(); // RTN0 back into sub
        c.step(); // RTN1 back to the main line, skipping LAX 1
        c.step();
        c.step();
        assert_eq!(c.registers().acc, 2, "{kind:?}");
    }
}

#[test]
fn tl_reaches_any_page() {
    both("TL far\n.page 2A\nfar: LAX 4", 2, |c| {
        assert_eq!(c.registers().acc, 4)
    });
}

#[test]
fn tm_calls_through_the_idx_table() {
    // TM x: R ← S ← PC + 1; the IDX byte at page 0 step x gives P_U (I7, I6) and P_L (I5-I0),
    // P_M ← 4.
    let src = ".page 00\n.byte 00\n.page 37\nTM 00\nLAX 1\n.page 04\nLAX 6\nRTN0";
    for kind in [Kind::Sm510, Kind::Sm511] {
        let mut c = chip(kind, src);
        c.step();
        assert_eq!(c.pc(), rom::addr(0, 4, 0));
        c.step(); // LAX 6
        c.step(); // RTN0
        c.step(); // LAX 1, after the return
        assert_eq!(c.registers().acc, 1);
    }
    let mut p = assemble(Kind::Sm510, "TM 05").unwrap();
    p[5] = 0b1001_0101; // P_U 2, P_L 0x15
    let mut c = Chip::new(Kind::Sm510, &p, None);
    c.step();
    assert_eq!(c.pc(), rom::addr(2, 4, 0x15));
}

#[test]
fn atpl_replaces_the_low_step_bits() {
    both("LAX 6\nATPL", 2, |c| {
        let at = rom::next(rom::addr(3, 7, 0));
        assert_eq!(c.pc(), (at & !0x0F) | 6);
    });
}

#[test]
fn strobes_shift_and_keys_read_through_them() {
    let mut c = run(Kind::Sm510, "WS\nWR\nWS", 3);
    assert_eq!(c.strobes(), 0b101);
    c.set_lines(Lines {
        matrix: [0, 0, 0b0100, 0, 0, 0, 0, 0],
        ..Lines::IDLE
    });
    assert_eq!(c.k(), 0b0100, "S3 is driven");
    // SM511/512: WR/WS shift W'; PTW copies it to the strobes.
    let c = run(Kind::Sm511, "WS\nWS", 2);
    assert_eq!(c.strobes(), 0);
    let c = run(Kind::Sm511, "WS\nWS\nPTW", 3);
    assert_eq!(c.strobes(), 0b11);
    for (kind, src) in [
        (Kind::Sm510, "WS\nLAX 0\nKTA"),
        (Kind::Sm511, "WS\nPTW\nKTA"),
    ] {
        let mut c = chip(kind, src);
        c.set_lines(Lines {
            matrix: [0b1001, 0, 0, 0, 0, 0, 0, 0],
            ..Lines::IDLE
        });
        for _ in 0..3 {
            c.step();
        }
        assert_eq!(c.registers().acc, 0b1001, "{kind:?}");
    }
}

#[test]
fn output_registers() {
    both("LAX 5\nATL\nLAX 9\nATFC\nLAX 0\nATBP", 6, |c| {
        let r = c.registers();
        assert_eq!((r.l, r.y, r.bp), (5, 9, false));
    });
    both("SC\nBDC", 2, |c| assert!(c.registers().bc));
    let c = run(Kind::Sm510, "LAX 3\nATR", 2);
    assert_eq!(c.registers().r, 3);
    let c = run(Kind::Sm511, "LAX 6\nATX", 2);
    assert_eq!(c.registers().x, 6);
}

fn machine(kind: Kind, src: &str) -> Machine {
    let p = assemble(kind, src).unwrap();
    Machine::new(
        kind,
        &p,
        kind.melodic().then_some(&[0; 256]),
        Options::default(),
    )
}

#[test]
fn the_one_second_flag_sets_on_the_dividers_edge_and_tis_clears_it() {
    // A loop that counts TIS hits into RAM 0: TIS skips while γ = 0.
    let src = "loop: TIS\nT hit\nT loop\nhit: LBL 00\nLDA 0\nADX 1\nT st\nst: EXC 0\nT loop";
    for kind in [Kind::Sm510, Kind::Sm511] {
        let mut m = machine(kind, src);
        m.run_until(5 * CRYSTAL_HZ as u64 + 100);
        // The divider wraps at 1 s, 2 s, … 5 s.
        assert_eq!(m.chip().ram()[0], 5, "{kind:?}");
    }
}

#[test]
fn idiv_resets_the_divider() {
    let mut m = machine(Kind::Sm510, "LAX 0\nLAX 0\nIDIV\nloop: T loop");
    m.run_for(1000);
    assert!(m.chip().registers().div < 1000);
}

#[test]
fn cend_stops_until_the_second_then_starts_at_page_10() {
    for kind in [Kind::Sm510, Kind::Sm511] {
        let mut m = machine(kind, "CEND\n.page 10\nLAX 7\nloop: T loop");
        m.run_for(100);
        assert!(m.chip().registers().halted);
        // γ comes as the divider wraps, a second in.
        m.run_until(CRYSTAL_HZ as u64 - 50);
        assert!(m.chip().registers().halted);
        m.run_until(CRYSTAL_HZ as u64 + 50);
        let r = m.chip().registers();
        assert!(!r.halted);
        assert_eq!(r.acc, 7);
    }
}

#[test]
fn a_key_wakes_the_clock_stop() {
    let mut m = machine(Kind::Sm510, "WS\nCEND\n.page 10\nLAX 7\nloop: T loop");
    m.run_for(100);
    assert!(m.chip().registers().halted);
    m.set_lines(Lines {
        matrix: [1, 0, 0, 0, 0, 0, 0, 0],
        ..Lines::IDLE
    });
    m.run_for(100);
    assert_eq!(m.chip().registers().acc, 7);
}

#[test]
fn the_sm511_clock_is_slow_after_reset_and_selectable() {
    // 1,000 single-cycle instructions take 4,000 crystal cycles at 8 kHz and 2,000 at 16 kHz.
    let mut slow = machine(Kind::Sm511, "loop: T loop");
    slow.run_for(4000);
    let mut fast = machine(Kind::Sm511, "CLKHI\nloop: T loop");
    fast.run_for(4000);
    assert!(!slow.chip().registers().fast);
    assert!(fast.chip().registers().fast);
    assert_eq!(slow.chip().ticks_per_cycle(), 4);
    assert_eq!(fast.chip().ticks_per_cycle(), 2);
}

#[test]
fn the_sm510_buzzer_is_4096_hz_while_r_is_set() {
    let mut m = machine(Kind::Sm510, "LAX 3\nATR\nloop: T loop");
    m.run_for(CRYSTAL_HZ as u64);
    let edges = m.take_edges();
    // 4,096 periods a second: 8,192 edges, give or take the start.
    assert!((8180..=8200).contains(&edges.len()), "{}", edges.len());
}

#[test]
fn the_display_shows_ram_through_the_commons() {
    // a1 at common H2: file 6 word 0 bit 1; b16 at H4: file 7 word 15 bit 3.
    let mut m = machine(Kind::Sm510, "LBL 60\nSM 1\nLBL 7F\nSM 3\nloop: T loop");
    m.run_for(2000);
    let s = m.segments();
    assert!(s.on(0, 0, 1));
    assert!(s.on(1, 15, 3));
    assert_eq!(s.count(), 2);
    // BC blanks it.
    let mut m = machine(Kind::Sm510, "LBL 60\nSM 1\nSC\nBDC\nloop: T loop");
    m.run_for(2000);
    assert_eq!(m.segments().count(), 0);
}

#[test]
fn save_states_round_trip() {
    let mut m = machine(Kind::Sm511, "LAX 5\nloop: ADX 1\nT st\nst: T loop");
    m.run_for(12345);
    let saved = m.save();
    let mut other = machine(Kind::Sm511, "LAX 5\nloop: ADX 1\nT st\nst: T loop");
    assert!(other.load(&saved));
    m.run_for(5000);
    other.run_for(5000);
    assert_eq!(m.chip().registers(), other.chip().registers());
    assert!(!other.load(&saved[..saved.len() - 1]));
}

#[test]
fn stretches_run_as_cycle_by_cycle() {
    // A busy unit (a counting loop shown on the LCD, the buzzer on, toggled each second) gives
    // the same state, frames and sound whether the machine advances by stretches (as it does)
    // or a crystal cycle at a time.
    let src = "LAX 3\nATR\nloop: LBL 60\nLDA 0\nADX 1\nT st\nst: EXC 0\nTIS\nT loop\nLAX 0\nATR\nLAX 3\nATR\nT loop";
    let mut a = machine(Kind::Sm510, src);
    a.run_for(3 * CRYSTAL_HZ as u64);
    let edges = a.take_edges();
    assert!(edges.len() > 1000, "the buzzer sounded");
    let mut b = machine(Kind::Sm510, src);
    b.set_cycle_by_cycle(true);
    b.run_for(3 * CRYSTAL_HZ as u64);
    assert_eq!(a.chip().registers(), b.chip().registers());
    assert_eq!(a.segments(), b.segments());
    assert_eq!(edges, b.take_edges());
}
