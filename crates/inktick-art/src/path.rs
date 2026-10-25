//! SVG path data (`d`), transforms and paint, as the SVG 1.1 specification defines them:
//! "Paths" (8.3, the grammar and every command), "Coordinate Systems" (7.6, the transform
//! list) and the elliptical arc implementation notes (F.6).

use tiny_skia::{PathBuilder, Transform};

/// Reads numbers and command letters out of path data or a list of numbers.
struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Lexer<'a> {
        Lexer {
            s: s.as_bytes(),
            i: 0,
        }
    }

    fn skip_separators(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\n' | b'\r' | b',')
        {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip_separators();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        matches!(self.s.get(self.i), Some(c) if c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    /// A number: sign, digits, one decimal point, exponent. "1.5.5" is 1.5 then .5.
    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let start = self.i;
        let s = self.s;
        if matches!(s.get(self.i), Some(b'-' | b'+')) {
            self.i += 1;
        }
        let mut dot = false;
        let mut digits = false;
        while let Some(&c) = s.get(self.i) {
            if c.is_ascii_digit() {
                digits = true;
            } else if c == b'.' && !dot {
                dot = true;
            } else {
                break;
            }
            self.i += 1;
        }
        if matches!(s.get(self.i), Some(b'e' | b'E')) && digits {
            let save = self.i;
            self.i += 1;
            if matches!(s.get(self.i), Some(b'-' | b'+')) {
                self.i += 1;
            }
            let exp_start = self.i;
            while matches!(s.get(self.i), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
            if self.i == exp_start {
                self.i = save;
            }
        }
        if !digits {
            self.i = start;
            return None;
        }
        core::str::from_utf8(&s[start..self.i]).ok()?.parse().ok()
    }

    /// An arc's flag: a single 0 or 1, which may run into the next number ("a1 1 0 01...").
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let c = *self.s.get(self.i)?;
        self.i += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// Appends the path data `d` to `pb`. Returns false if the data is malformed (the part before
/// the error is kept, as the specification asks).
pub fn parse(d: &str, pb: &mut PathBuilder) -> bool {
    let mut lx = Lexer::new(d);
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    // The last control point, for S and T.
    let mut last_c: Option<(f32, f32)> = None;
    let mut last_q: Option<(f32, f32)> = None;
    let mut cmd = match lx.command() {
        Some(c @ (b'M' | b'm')) => c,
        None if d.trim().is_empty() => return true,
        _ => return false,
    };
    loop {
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        macro_rules! num {
            () => {
                match lx.number() {
                    Some(v) => v,
                    None => return false,
                }
            };
        }
        let mut c_ctrl = None;
        let mut q_ctrl = None;
        match cmd.to_ascii_uppercase() {
            b'M' => {
                x = ox + num!();
                y = oy + num!();
                pb.move_to(x, y);
                sx = x;
                sy = y;
                // Further pairs after a moveto are linetos.
                cmd = if rel { b'l' } else { b'L' };
            }
            b'L' => {
                x = ox + num!();
                y = oy + num!();
                pb.line_to(x, y);
            }
            b'H' => {
                x = ox + num!();
                pb.line_to(x, y);
            }
            b'V' => {
                y = oy + num!();
                pb.line_to(x, y);
            }
            b'C' => {
                let (x1, y1) = (ox + num!(), oy + num!());
                let (x2, y2) = (ox + num!(), oy + num!());
                x = ox + num!();
                y = oy + num!();
                pb.cubic_to(x1, y1, x2, y2, x, y);
                c_ctrl = Some((x2, y2));
            }
            b'S' => {
                let (x1, y1) = match last_c {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                let (x2, y2) = (ox + num!(), oy + num!());
                x = ox + num!();
                y = oy + num!();
                pb.cubic_to(x1, y1, x2, y2, x, y);
                c_ctrl = Some((x2, y2));
            }
            b'Q' => {
                let (x1, y1) = (ox + num!(), oy + num!());
                x = ox + num!();
                y = oy + num!();
                pb.quad_to(x1, y1, x, y);
                q_ctrl = Some((x1, y1));
            }
            b'T' => {
                let (x1, y1) = match last_q {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                x = ox + num!();
                y = oy + num!();
                pb.quad_to(x1, y1, x, y);
                q_ctrl = Some((x1, y1));
            }
            b'A' => {
                let (rx, ry, rot) = (num!(), num!(), num!());
                let (Some(large), Some(sweep)) = (lx.flag(), lx.flag()) else {
                    return false;
                };
                let (x2, y2) = (ox + num!(), oy + num!());
                arc(pb, (x, y), rx, ry, rot, large, sweep, (x2, y2));
                x = x2;
                y = y2;
            }
            b'Z' => {
                pb.close();
                x = sx;
                y = sy;
            }
            _ => return false,
        }
        last_c = c_ctrl;
        last_q = q_ctrl;
        // A command's letter may be left out when it repeats (not after Z).
        if !cmd.eq_ignore_ascii_case(&b'Z') && lx.at_number() {
            continue;
        }
        match lx.command() {
            Some(c) => cmd = c,
            None => {
                lx.skip_separators();
                return lx.i >= lx.s.len();
            }
        }
    }
}

/// An elliptical arc as cubic Béziers: SVG 1.1 F.6.5 (endpoint to centre) and F.6.6 (radii
/// correction), then one cubic per quarter turn at most.
#[allow(clippy::too_many_arguments)]
fn arc(
    pb: &mut PathBuilder,
    (x1, y1): (f32, f32),
    rx: f32,
    ry: f32,
    angle: f32,
    large: bool,
    sweep: bool,
    (x2, y2): (f32, f32),
) {
    let (mut rx, mut ry) = (rx.abs() as f64, ry.abs() as f64);
    if rx == 0.0 || ry == 0.0 || (x1 == x2 && y1 == y2) {
        pb.line_to(x2, y2);
        return;
    }
    let phi = (angle as f64).to_radians();
    let (sin, cos) = phi.sin_cos();
    let (x1d, y1d, x2d, y2d) = (x1 as f64, y1 as f64, x2 as f64, y2 as f64);
    let dx = (x1d - x2d) / 2.0;
    let dy = (y1d - y2d) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut k = (num / den).max(0.0).sqrt();
    if large == sweep {
        k = -k;
    }
    let cxp = k * rx * y1p / ry;
    let cyp = -k * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x1d + x2d) / 2.0;
    let cy = sin * cxp + cos * cyp + (y1d + y2d) / 2.0;
    let angle_of = |ux: f64, uy: f64| uy.atan2(ux);
    let t1 = angle_of((x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = angle_of((-x1p - cxp) / rx, (-y1p - cyp) / ry) - t1;
    let tau = core::f64::consts::TAU;
    if sweep && dt < 0.0 {
        dt += tau;
    } else if !sweep && dt > 0.0 {
        dt -= tau;
    }
    let n = (dt.abs() / core::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = dt / n as f64;
    let alpha = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        (
            cx + rx * c * cos - ry * s * sin,
            cy + rx * c * sin + ry * s * cos,
        )
    };
    let deriv = |t: f64| {
        let (s, c) = t.sin_cos();
        (-rx * s * cos - ry * c * sin, -rx * s * sin + ry * c * cos)
    };
    let mut t = t1;
    for _ in 0..n {
        let (p0, d0) = (point(t), deriv(t));
        let (p3, d3) = (point(t + step), deriv(t + step));
        pb.cubic_to(
            (p0.0 + alpha * d0.0) as f32,
            (p0.1 + alpha * d0.1) as f32,
            (p3.0 - alpha * d3.0) as f32,
            (p3.1 - alpha * d3.1) as f32,
            p3.0 as f32,
            p3.1 as f32,
        );
        t += step;
    }
}

/// A transform list: `matrix`, `translate`, `scale`, `rotate`, `skewX`, `skewY`, applied left
/// to right (the first is outermost).
pub fn transform(s: &str) -> Transform {
    let mut out = Transform::identity();
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let name = rest[..open].trim_matches(|c: char| c.is_whitespace() || c == ',');
        let Some(close) = rest[open..].find(')') else {
            break;
        };
        let mut lx = Lexer::new(&rest[open + 1..open + close]);
        let mut v = Vec::new();
        while let Some(n) = lx.number() {
            v.push(n);
        }
        let t = match (name, v.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Transform::from_row(*a, *b, *c, *d, *e, *f),
            ("translate", [x]) => Transform::from_translate(*x, 0.0),
            ("translate", [x, y]) => Transform::from_translate(*x, *y),
            ("scale", [s]) => Transform::from_scale(*s, *s),
            ("scale", [x, y]) => Transform::from_scale(*x, *y),
            ("rotate", [a]) => Transform::from_rotate(*a),
            ("rotate", [a, cx, cy]) => Transform::from_translate(*cx, *cy)
                .pre_concat(Transform::from_rotate(*a))
                .pre_translate(-cx, -cy),
            ("skewX", [a]) => Transform::from_row(1.0, 0.0, a.to_radians().tan(), 1.0, 0.0, 0.0),
            ("skewY", [a]) => Transform::from_row(1.0, a.to_radians().tan(), 0.0, 1.0, 0.0, 0.0),
            _ => Transform::identity(),
        };
        out = out.pre_concat(t);
        rest = &rest[open + close + 1..];
    }
    out
}

/// A list of numbers (`points`, `viewBox`).
pub fn numbers(s: &str) -> Vec<f32> {
    let mut lx = Lexer::new(s);
    let mut v = Vec::new();
    while let Some(n) = lx.number() {
        v.push(n);
    }
    v
}

/// A paint colour: `#rgb`, `#rrggbb`, `rgb(r, g, b)` or a few names; `None` for `none` or what
/// isn't understood.
pub fn color(s: &str) -> Option<[u8; 3]> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix('#') {
        let v = u32::from_str_radix(hex, 16).ok()?;
        return match hex.len() {
            3 => Some([
                ((v >> 8) & 15) as u8 * 17,
                ((v >> 4) & 15) as u8 * 17,
                (v & 15) as u8 * 17,
            ]),
            6 => Some([(v >> 16) as u8, (v >> 8) as u8, v as u8]),
            _ => None,
        };
    }
    if let Some(inner) = s.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<_> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() == 3 {
            let mut out = [0u8; 3];
            for (o, p) in out.iter_mut().zip(parts) {
                *o = match p.strip_suffix('%') {
                    Some(pc) => (pc.parse::<f32>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8,
                    None => p.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8,
                };
            }
            return Some(out);
        }
        return None;
    }
    match s {
        "black" => Some([0, 0, 0]),
        "white" => Some([255, 255, 255]),
        "red" => Some([255, 0, 0]),
        "green" => Some([0, 128, 0]),
        "blue" => Some([0, 0, 255]),
        "gray" | "grey" => Some([128, 128, 128]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds(d: &str) -> (f32, f32, f32, f32) {
        let mut pb = PathBuilder::new();
        assert!(parse(d, &mut pb), "{d}");
        let p = pb.finish().unwrap();
        let b = p.bounds();
        (b.left(), b.top(), b.right(), b.bottom())
    }

    #[test]
    fn relative_moves_lines_and_closes() {
        assert_eq!(bounds("m 10,10 5,0 0,5 z"), (10.0, 10.0, 15.0, 15.0));
        assert_eq!(bounds("M0 0H4V3h-4z"), (0.0, 0.0, 4.0, 3.0));
    }

    #[test]
    fn numbers_run_together() {
        assert_eq!(numbers("1.5.5-2e1,3"), vec![1.5, 0.5, -20.0, 3.0]);
    }

    #[test]
    fn potrace_curves() {
        let (l, t, r, b) = bounds("m 2257.9,217 -17.6,0.5 -4.6,3.8 c -5.9,4.8 -6,6.9 -0.4,10.3 z");
        assert!(
            l > 2225.0 && r < 2260.0 && t >= 217.0 && b < 232.0,
            "{l} {t} {r} {b}"
        );
    }

    #[test]
    fn arcs_reach_their_end() {
        let (l, _, r, _) = bounds("M0 0 A5 5 0 0 1 10 0 Z");
        assert!(l.abs() < 0.01 && (r - 10.0).abs() < 0.01);
    }

    #[test]
    fn transforms_compose_left_to_right() {
        let t = transform("translate(10,0) scale(2)");
        let mut p = [tiny_skia::Point::from_xy(1.0, 1.0)];
        t.map_points(&mut p);
        assert_eq!((p[0].x, p[0].y), (12.0, 2.0));
    }

    #[test]
    fn colours() {
        assert_eq!(color("#fffffe"), Some([255, 255, 254]));
        assert_eq!(color("#0f0"), Some([0, 255, 0]));
        assert_eq!(color("none"), None);
    }
}
