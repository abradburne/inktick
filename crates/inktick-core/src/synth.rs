//! Sound: the buzzer pin's edges, made into samples without aliasing.
//!
//! Each change of the output level is placed at its exact time as a band-limited step: the
//! integral of a Blackman-windowed sinc, tabulated for 64 sub-sample phases over 16 samples
//! (the BLEP method, Stilson and Smith, "Alias-Free Digital Synthesis of Classic Analog
//! Waveforms", ICMC 1996). Any output rate works with no resampling stage. After it, a DC-blocking
//! high-pass and an optional piezo model: a resonant band-pass (the RBJ audio-EQ cookbook's)
//! mixed with the dry signal, for the disc's ringing peak.

use alloc::vec;
use alloc::vec::Vec;

const HALF: usize = 8;
const TAPS: usize = 2 * HALF;
const PHASES: usize = 64;

/// The piezo model's settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Piezo {
    /// How much of the resonant band-pass is heard: 0 (the clean pin) to 1.
    pub amount: f32,
    /// The disc's resonance, Hz.
    pub resonance: f32,
    /// Its sharpness.
    pub q: f32,
}

impl Piezo {
    /// A small disc of the kind the units use: a broad peak near 3.5 kHz, half mixed in.
    pub const DEFAULT: Piezo = Piezo {
        amount: 0.5,
        resonance: 3500.0,
        q: 1.5,
    };
}

/// Below this a filter's state is silence (about -140 dB).
const QUIET: f32 = 1e-7;

/// Turns level changes at crystal-cycle times into samples at an output rate.
pub struct Synth {
    rate: u32,
    table: Vec<[f32; TAPS]>,
    /// The sample index of `ramp[0]` and `diff[0]`.
    base: u64,
    ramp: Vec<f32>,
    diff: Vec<f32>,
    running: f32,
    level: f32,
    hp_in: f32,
    hp_out: f32,
    hp_r: f32,
    bp: Biquad,
    piezo: Piezo,
}

impl Synth {
    /// A synthesiser for `rate` Hz output.
    pub fn new(rate: u32, piezo: Piezo) -> Synth {
        let mut s = Synth {
            rate,
            table: step_table(),
            base: 0,
            ramp: Vec::new(),
            diff: Vec::new(),
            running: 0.0,
            level: 0.0,
            hp_in: 0.0,
            hp_out: 0.0,
            // A 20 Hz one-pole high-pass.
            hp_r: 1.0 - (2.0 * core::f32::consts::PI * 20.0 / rate as f32),
            bp: Biquad::default(),
            piezo,
        };
        s.set_piezo(piezo);
        s
    }

    /// The output rate.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Changes the piezo model.
    pub fn set_piezo(&mut self, piezo: Piezo) {
        self.piezo = piezo;
        self.bp = Biquad::band_pass(piezo.resonance as f64, piezo.q as f64, self.rate as f64);
    }

    /// The sample position of crystal cycle `tick`, delayed by half the step's width.
    fn position(&self, tick: u64) -> f64 {
        tick as f64 * self.rate as f64 / crate::CRYSTAL_HZ as f64 + HALF as f64
    }

    /// The output level becomes `level` at crystal cycle `tick` (ticks never go backwards).
    pub fn edge(&mut self, tick: u64, level: f32) {
        let delta = level - self.level;
        if delta == 0.0 {
            return;
        }
        self.level = level;
        let s = self.position(tick);
        let whole = libm_floor(s);
        let frac = s - whole;
        let phase = ((frac * PHASES as f64) as usize).min(PHASES - 1);
        let first = (whole as i64 - HALF as i64 + 1).max(self.base as i64) as u64;
        let start = first - self.base;
        let need = start as usize + TAPS + 1;
        if self.ramp.len() < need {
            self.ramp.resize(need, 0.0);
            self.diff.resize(need, 0.0);
        }
        let row = &self.table[phase];
        for (k, v) in row.iter().enumerate() {
            self.ramp[start as usize + k] += delta * v;
        }
        self.diff[start as usize + TAPS] += delta;
    }

    /// Starts the output afresh at crystal cycle `tick`, silent: after a state is loaded or the
    /// machine's time jumps, nothing before it is made (it would only queue up as a delay).
    pub fn restart(&mut self, tick: u64) {
        self.base = libm_floor(tick as f64 * self.rate as f64 / crate::CRYSTAL_HZ as f64) as u64;
        self.ramp.clear();
        self.diff.clear();
        self.running = self.level;
        self.hp_in = self.level;
        self.hp_out = 0.0;
        self.bp.clear();
    }

    /// Samples whose every edge is known by crystal cycle `tick`.
    pub fn ready(&self, tick: u64) -> usize {
        let last = libm_floor(tick as f64 * self.rate as f64 / crate::CRYSTAL_HZ as f64) as u64;
        last.saturating_sub(self.base) as usize
    }

    /// Takes `count` finished samples into `out`.
    pub fn take(&mut self, count: usize, out: &mut Vec<f32>) {
        // Silence in and the filters settled: silence out, without running them.
        let pending = |v: &[f32]| v.iter().take(count).any(|x| *x != 0.0);
        if self.running == self.hp_in
            && self.hp_out.abs() < QUIET
            && self.bp.quiet()
            && !pending(&self.ramp)
            && !pending(&self.diff)
        {
            out.extend(core::iter::repeat_n(0.0, count));
            self.hp_out = 0.0;
            self.bp.clear();
            let n = count.min(self.ramp.len());
            self.ramp.drain(..n);
            self.diff.drain(..n);
            self.base += count as u64;
            return;
        }
        if self.ramp.len() < count {
            self.ramp.resize(count, 0.0);
            self.diff.resize(count, 0.0);
        }
        let mix = self.piezo.amount.clamp(0.0, 1.0);
        for i in 0..count {
            self.running += self.diff[i];
            let x = self.running + self.ramp[i];
            // DC-blocking high-pass.
            let y = x - self.hp_in + self.hp_r * self.hp_out;
            self.hp_in = x;
            self.hp_out = y;
            let resonant = self.bp.run(y);
            out.push((1.0 - mix) * y + mix * 2.0 * resonant);
        }
        self.ramp.drain(..count);
        self.diff.drain(..count);
        self.base += count as u64;
    }
}

#[derive(Default, Clone, Copy)]
struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn quiet(&self) -> bool {
        [self.x1, self.x2, self.y1, self.y2]
            .iter()
            .all(|v| v.abs() < QUIET)
    }

    fn clear(&mut self) {
        (self.x1, self.x2, self.y1, self.y2) = (0.0, 0.0, 0.0, 0.0);
    }

    /// RBJ cookbook band-pass, 0 dB peak gain.
    fn band_pass(f0: f64, q: f64, rate: f64) -> Biquad {
        let w0 = 2.0 * core::f64::consts::PI * (f0 / rate).min(0.49);
        let alpha = sin(w0) / (2.0 * q.max(0.1));
        let a0 = 1.0 + alpha;
        Biquad {
            b0: (alpha / a0) as f32,
            b2: (-alpha / a0) as f32,
            a1: (-2.0 * cos(w0) / a0) as f32,
            a2: ((1.0 - alpha) / a0) as f32,
            ..Biquad::default()
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// The band-limited step for each phase: `table[p][k]` is the step's value `k` samples after
/// the first it touches, for an edge `p / PHASES` of a sample after a sample boundary.
fn step_table() -> Vec<[f32; TAPS]> {
    // The windowed sinc on a grid of 1/PHASES of a sample, from −HALF to +HALF, integrated.
    let n = TAPS * PHASES;
    let mut cum = vec![0.0f64; n + 1];
    let mut sum = 0.0;
    for (i, c) in cum.iter_mut().enumerate().skip(1) {
        let t = (i as f64 - 0.5) / PHASES as f64 - HALF as f64;
        let x = core::f64::consts::PI * t;
        let sinc = if x.abs() < 1e-12 { 1.0 } else { sin(x) / x };
        let u = (t + HALF as f64) / TAPS as f64;
        let w = 0.42 - 0.5 * cos(2.0 * core::f64::consts::PI * u)
            + 0.08 * cos(4.0 * core::f64::consts::PI * u);
        sum += sinc * w;
        *c = sum;
    }
    let total = sum;
    (0..PHASES)
        .map(|p| {
            let mut row = [0.0; TAPS];
            for (k, v) in row.iter_mut().enumerate() {
                // The sample k after the first sits (k + 1 − HALF) − p/PHASES from the edge.
                let at = (k + 1) * PHASES - p;
                *v = (cum[at.min(n)] / total) as f32;
            }
            row
        })
        .collect()
}

fn libm_floor(x: f64) -> f64 {
    let t = x as i64 as f64;
    if t > x {
        t - 1.0
    } else {
        t
    }
}

/// sin(x) for any x: reduced to [−π, π], then its Taylor series to x^19 (error below 1e-12).
fn sin(x: f64) -> f64 {
    let tau = 2.0 * core::f64::consts::PI;
    let mut r = x - tau * libm_floor(x / tau + 0.5);
    if r > core::f64::consts::FRAC_PI_2 {
        r = core::f64::consts::PI - r;
    } else if r < -core::f64::consts::FRAC_PI_2 {
        r = -core::f64::consts::PI - r;
    }
    let r2 = r * r;
    let mut term = r;
    let mut sum = r;
    for i in 1..10 {
        term *= -r2 / ((2 * i) as f64 * (2 * i + 1) as f64);
        sum += term;
    }
    sum
}

fn cos(x: f64) -> f64 {
    sin(x + core::f64::consts::FRAC_PI_2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sin_is_accurate() {
        for i in -100..100 {
            let x = i as f64 * 0.37;
            let s = sin(x);
            let c = cos(x);
            assert!((s * s + c * c - 1.0).abs() < 1e-9, "{x}");
        }
        assert!((sin(core::f64::consts::FRAC_PI_6) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn the_step_rises_from_0_to_1() {
        let t = step_table();
        for row in &t {
            assert!(row[0].abs() < 0.01);
            assert!((row[TAPS - 1] - 1.0).abs() < 0.01);
        }
    }

    #[test]
    fn a_square_wave_comes_out_at_its_level() {
        let mut s = Synth::new(
            48_000,
            Piezo {
                amount: 0.0,
                ..Piezo::DEFAULT
            },
        );
        // 1 kHz square: an edge every 16.384 crystal cycles.
        for i in 0..2000u64 {
            s.edge(i * 16 + i / 3, if i % 2 == 0 { 0.5 } else { -0.5 });
        }
        let mut out = Vec::new();
        let n = s.ready(2000 * 16);
        s.take(n, &mut out);
        let peak = out[1000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak > 0.4 && peak < 0.7, "{peak}");
    }

    #[test]
    fn a_restart_far_on_makes_nothing_for_the_time_before() {
        let mut s = Synth::new(48_000, Piezo::DEFAULT);
        let later = 600 * crate::CRYSTAL_HZ as u64;
        s.restart(later);
        assert_eq!(s.ready(later), 0, "no backlog after a jump in time");
        s.edge(later + 100, 1.0);
        let n = s.ready(later + crate::CRYSTAL_HZ as u64 / 10);
        assert!(n > 4000 && n < 5000, "{n}");
        let mut out = Vec::new();
        s.take(n, &mut out);
        assert!(
            out.iter().any(|v| v.abs() > 0.1),
            "an edge after the restart sounds"
        );
    }

    #[test]
    fn a_tone_dies_away_into_exact_silence() {
        let mut s = Synth::new(48_000, Piezo::DEFAULT);
        for i in 0..400u64 {
            s.edge(i * 8, if i % 2 == 0 { 1.0 } else { 0.0 });
        }
        s.edge(400 * 8, 0.0);
        let mut out = Vec::new();
        // In 10 ms pieces, as the frontend takes them, for 2 s.
        for t in 1..=200u64 {
            let n = s.ready(t * crate::CRYSTAL_HZ as u64 / 100);
            s.take(n, &mut out);
        }
        assert!(
            out[..2000].iter().any(|v| v.abs() > 0.1),
            "the tone sounded"
        );
        let tail = &out[out.len() - 48_000..];
        assert!(tail.iter().all(|v| *v == 0.0), "silent once settled");
        // Settling cut nothing audible: the last sample before the silence was already tiny.
        let last = out.iter().rposition(|v| *v != 0.0).unwrap();
        assert!(out[last].abs() < 1e-5, "{}", out[last]);
        // A new edge after the silence sounds again.
        s.edge(3 * crate::CRYSTAL_HZ as u64, 1.0);
        let mut more = Vec::new();
        s.take(s.ready(3 * crate::CRYSTAL_HZ as u64 + 3000), &mut more);
        assert!(more.iter().any(|v| v.abs() > 0.1));
    }
}
