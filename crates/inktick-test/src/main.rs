//! inktick-test: runs units headless.
//!
//! ```text
//! inktick-test png SET OUT.png [--seconds N] [--artwork DIR] [--view NAME] [--size WxH]
//! ```

use inktick_art::scene::{Parts, Scene};
use inktick_art::{Look, Optics};
use inktick_core::{Machine, Options, CRYSTAL_HZ};
use std::path::PathBuf;

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("png") => png(&args[1..]),
        Some("lit") => lit(&args[1..]),
        Some("segments") => segments(&args[1..]),
        Some("watch") => watch(&args[1..]),
        Some("ram") => ram(&args[1..]),
        Some("trace") => trace(&args[1..]),
        Some("profile") => profile(&args[1..]),
        Some("wiring") => wiring(&args[1..]),
        Some("dis") => dis(&args[1..]),
        Some("probe") => probe(&args[1..]),
        Some("strobes") => strobe_patterns(&args[1..]),
        Some("buttons") => buttons(&args[1..]),
        Some("check") => check(&args[1..]),
        Some("wav") => wav(&args[1..]),
        Some("bench") => bench(&args[1..]),
        Some("speed") => speed(&args[1..]),
        Some("clockfind") => clockfind(&args[1..]),
        Some("clockcheck") => clockcheck(&args[1..]),
        Some("controlcheck") => controlcheck(&args[1..]),
        Some("undo") => undo(&args[1..]),
        Some("controlfind") => controlfind(&args[1..]),
        _ => Err("usage: inktick-test png SET OUT.png [--seconds N] [--artwork DIR] [--view NAME] [--size WxH]".into()),
    };
    if let Err(e) = result {
        eprintln!("inktick-test: {e}");
        std::process::exit(1);
    }
}

fn png(args: &[String]) -> Result<(), String> {
    let set_path = PathBuf::from(args.first().ok_or("no set")?);
    let out = PathBuf::from(args.get(1).ok_or("no output file")?);
    let seconds: f64 = arg(args, "--seconds")
        .map_or(Ok(2.0), |s| s.parse())
        .map_err(|_| "--seconds")?;
    let art: Vec<PathBuf> = arg(args, "--artwork")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let (w, h) = match arg(args, "--size") {
        Some(s) => {
            let (a, b) = s.split_once('x').ok_or("--size WxH")?;
            (
                a.parse().map_err(|_| "--size")?,
                b.parse().map_err(|_| "--size")?,
            )
        }
        None => (1024u32, 768u32),
    };
    let set = inktick_sets::open(&set_path, None, &art)?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let presses = presses(args)?;
    let end = (seconds * CRYSTAL_HZ as f64) as u64;
    while m.ticks() < end {
        m.set_lines(lines_at(&presses, m.ticks()));
        m.run_for(64.min(end - m.ticks()));
    }
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let addrs = Scene::addresses(&screens);
    let optics: Vec<Optics> = addrs
        .iter()
        .map(|a| {
            let mut o = Optics::new(a.len());
            o.snap(a, m.segments());
            o
        })
        .collect();
    let view_name = arg(args, "--view").unwrap_or_default();
    let view = set.artwork.as_ref().and_then(|a| a.layout.view(&view_name));
    let picture = |f: &str| set.artwork.as_ref().and_then(|a| a.picture(f));
    let parts = Parts {
        view,
        elements: set.artwork.as_ref().map(|a| &a.layout.elements),
        screens: &screens,
        viewport: None,
        picture: &picture,
        side_by_side: set.screens.iter().any(|(f, _)| f.contains("_left")),
        only: None,
    };
    let mut scene = Scene::new(&parts, w, h, Look::default());
    let (img, _) = scene.compose(&optics);
    std::fs::write(&out, img.to_png()).map_err(|e| e.to_string())?;
    let r = m.chip().registers();
    println!(
        "{}: {} {} screens, {} segments on, pc {:03x} {}",
        set.name,
        set.kind.name(),
        screens.len(),
        m.segments().count(),
        r.pc,
        if r.halted { "halted" } else { "" }
    );
    Ok(())
}

/// Draws a set's screens with only the segments matching a pattern lit (`2.*.*`, `0.3.1`).
fn lit(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let out = PathBuf::from(args.get(1).ok_or("no output file")?);
    let pattern: Vec<String> = args
        .get(2)
        .ok_or("no pattern")?
        .split('.')
        .map(str::to_string)
        .collect();
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let optics: Vec<Optics> = screens
        .iter()
        .map(|s| Optics {
            level: s
                .segments
                .iter()
                .map(|g| {
                    let ok = g
                        .addr
                        .iter()
                        .zip(&pattern)
                        .all(|(a, p)| p == "*" || p.parse() == Ok(*a));
                    ok as u8 as f32
                })
                .collect(),
        })
        .collect();
    let none = |_: &str| None;
    let parts = Parts {
        view: None,
        elements: None,
        screens: &screens,
        viewport: None,
        picture: &none,
        side_by_side: set.screens.iter().any(|(f, _)| f.contains("_left")),
        only: None,
    };
    let mut scene = Scene::new(&parts, 640, 400, Look::SHARP);
    let (img, _) = scene.compose(&optics);
    std::fs::write(&out, img.to_png()).map_err(|e| e.to_string())
}

/// Lists a set's segments: screen, address and where they are (fractions of the screen).
fn segments(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    for (n, (file, s)) in set.screens.iter().enumerate() {
        for g in &s.segments {
            let b = g.path.bounds();
            let fx = |x: f32| (x - s.view[0]) / s.view[2];
            let fy = |y: f32| (y - s.view[1]) / s.view[3];
            println!(
                "{n} {file} {}.{}.{} x {:.3}-{:.3} y {:.3}-{:.3}",
                g.addr[0],
                g.addr[1],
                g.addr[2],
                fx(b.left()),
                fx(b.right()),
                fy(b.top()),
                fy(b.bottom())
            );
        }
    }
    Ok(())
}

/// Runs a set and prints each change of one RAM nibble with the instruction that made it:
/// `watch SET ADDR [SECONDS] [--press S2K3@2.2 ...]`.
fn watch(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let at = usize::from_str_radix(args.get(1).ok_or("no RAM address (hex)")?, 16)
        .map_err(|_| "address")?;
    let seconds: f64 = args
        .get(2)
        .filter(|s| !s.starts_with("--"))
        .map_or(Ok(3.0), |s| s.parse())
        .map_err(|_| "seconds")?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let presses = presses(args)?;
    let mut last = m.chip().ram()[at];
    let mut prev_pc = m.chip().pc();
    let mut shown = 0;
    while m.ticks() < (seconds * CRYSTAL_HZ as f64) as u64 && shown < 60 {
        m.set_lines(lines_at(&presses, m.ticks()));
        let pc = m.chip().pc();
        m.run_for(1);
        let v = m.chip().ram()[at];
        if v != last {
            let r = m.chip().registers();
            println!(
                "t={:.3}s {:03x}:{:02x} (after {:03x}) {last:x} -> {v:x} acc={:x} b={}{:x}",
                m.ticks() as f64 / CRYSTAL_HZ as f64,
                pc,
                set.program[pc as usize],
                prev_pc,
                r.acc,
                r.bm,
                r.bl
            );
            last = v;
            shown += 1;
        }
        if m.chip().pc() != pc {
            prev_pc = pc;
        }
    }
    println!("final file 6: {:x?}", &m.chip().ram()[96..112]);
    Ok(())
}

/// Prints the RAM (128 nibbles) at each of the given times, with presses, to set beside another
/// run's: `ram SET T1,T2,... [--press S2K3@2.2 ...]`.
fn ram(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let times: Vec<f64> = args
        .get(1)
        .ok_or("times")?
        .split(',')
        .map(|t| t.parse().map_err(|_| "time"))
        .collect::<Result<_, _>>()?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let presses = presses(args)?;
    for t in times {
        while m.ticks() < (t * CRYSTAL_HZ as f64) as u64 {
            m.set_lines(lines_at(&presses, m.ticks()));
            m.run_for(64);
        }
        let ram: String = m.chip().ram()[..128]
            .iter()
            .map(|n| format!("{:x}", n & 15))
            .collect();
        println!("RAM {t:.3} {ram}");
    }
    Ok(())
}

/// Prints the instructions run between two times, optionally only in some pages (hex, comma
/// separated), with presses: `trace SET FROM TO [PAGES] [--press S2K3@2.2 ...]`.
fn trace(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let from: f64 = args.get(1).ok_or("from")?.parse().map_err(|_| "from")?;
    let to: f64 = args.get(2).ok_or("to")?.parse().map_err(|_| "to")?;
    let pages: Vec<u16> = args
        .get(3)
        .filter(|p| !p.starts_with("--"))
        .map(|p| {
            p.split(',')
                .filter_map(|x| u16::from_str_radix(x, 16).ok())
                .collect()
        })
        .unwrap_or_default();
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let presses = presses(args)?;
    while m.ticks() < (from * CRYSTAL_HZ as f64) as u64 {
        m.set_lines(lines_at(&presses, m.ticks()));
        m.run_for(64);
    }
    let end = (to * CRYSTAL_HZ as f64) as u64;
    let mut lines = 0;
    while m.ticks() < end && lines < 4000 {
        m.set_lines(lines_at(&presses, m.ticks()));
        let r = m.chip().registers();
        if !r.halted && (pages.is_empty() || pages.contains(&(r.pc >> 6))) {
            let (text, _) = inktick_core::disasm::instruction(set.kind, &set.program, r.pc);
            println!(
                "{} {:<12} A={:X} C={} B={}{:X} W={:02X} {}",
                inktick_core::disasm::addr(r.pc),
                text,
                r.acc,
                r.carry as u8,
                r.bm,
                r.bl,
                r.w,
                if r.gamma { "γ" } else { "" }
            );
            lines += 1;
        }
        m.step();
    }
    Ok(())
}

/// Counts the instructions run (by mnemonic, and by address for the rare ones) in a time span.
fn profile(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let to: f64 = args
        .get(1)
        .map_or(Ok(6.0), |s| s.parse())
        .map_err(|_| "seconds")?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let mut by_name: std::collections::BTreeMap<String, (u64, std::collections::BTreeSet<u16>)> =
        Default::default();
    while m.ticks() < (to * CRYSTAL_HZ as f64) as u64 {
        let r = m.chip().registers();
        if !r.halted {
            let (text, _) = inktick_core::disasm::instruction(set.kind, &set.program, r.pc);
            let name = text.split(' ').next().unwrap_or("").to_string();
            let e = by_name.entry(name).or_default();
            e.0 += 1;
            if e.1.len() < 8 {
                e.1.insert(r.pc);
            }
        }
        m.step();
    }
    for (name, (n, at)) in by_name {
        let at: Vec<String> = at.iter().map(|a| inktick_core::disasm::addr(*a)).collect();
        println!("{name:<6} {n:>8}  {}", at.join(" "));
    }
    Ok(())
}

/// For each strobe S1-S8 and K line K1-K4 (and K held high regardless, and BA and β low),
/// holds it from power-on and reports what the unit does: how many segments are on after N
/// seconds and whether it reached TIS, CEND or TMEL. Finds hard-wired lines.
fn wiring(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let secs: f64 = args
        .get(1)
        .map_or(Ok(6.0), |s| s.parse())
        .map_err(|_| "seconds")?;
    let mut cases: Vec<(String, inktick_core::Lines)> =
        vec![("none".into(), inktick_core::Lines::IDLE)];
    for s in 0..8 {
        for k in 0..4 {
            let mut l = inktick_core::Lines::IDLE;
            l.matrix[s] = 1 << k;
            cases.push((format!("S{}-K{}", s + 1, k + 1), l));
        }
    }
    for k in 0..4 {
        let mut l = inktick_core::Lines::IDLE;
        l.k = 1 << k;
        cases.push((format!("K{} high", k + 1), l));
    }
    let mut l = inktick_core::Lines::IDLE;
    l.ba = false;
    cases.push(("BA low".into(), l));
    let mut l = inktick_core::Lines::IDLE;
    l.beta = false;
    cases.push(("β low".into(), l));
    for (name, lines) in cases {
        let mut m = Machine::new(
            set.kind,
            &set.program,
            set.melody.as_deref(),
            Options::default(),
        );
        m.set_lines(lines);
        let (mut tis, mut cend, mut tmel) = (0u32, 0u32, 0u32);
        while m.ticks() < (secs * CRYSTAL_HZ as f64) as u64 {
            let r = m.chip().registers();
            if !r.halted {
                let op = set.program[r.pc as usize];
                let arg = set.program[inktick_core::rom::next(r.pc) as usize];
                tis += (op == 0x58) as u32;
                cend += (op == 0x5D) as u32;
                tmel += (set.kind.melodic() && op == 0x60 && arg == 0x32) as u32;
            }
            m.step();
        }
        println!(
            "{name:<8} segments {:>3}  TIS {tis:>5}  CEND {cend:>3}  TMEL {tmel:>5}",
            m.segments().count()
        );
    }
    Ok(())
}

/// Disassembles pages (hex, comma separated) in execution order.
fn dis(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    for page in args.get(1).ok_or("pages")?.split(',') {
        let page = u16::from_str_radix(page, 16).map_err(|_| "page")?;
        let mut skip = 0;
        for step in inktick_core::rom::steps() {
            let at = (page << 6) | step as u16;
            let (text, len) = inktick_core::disasm::instruction(set.kind, &set.program, at);
            if skip > 0 {
                skip -= 1;
                println!(
                    "{}          ({:02X})",
                    inktick_core::disasm::addr(at),
                    set.program[at as usize]
                );
                continue;
            }
            println!(
                "{} {:02X}       {text}",
                inktick_core::disasm::addr(at),
                set.program[at as usize]
            );
            skip = len - 1;
        }
    }
    Ok(())
}

/// `--press S3K3@6,BA@8.5`: each pin held for a quarter second from that time.
fn presses(args: &[String]) -> Result<Vec<(u64, Pin)>, String> {
    let mut out = Vec::new();
    // `--press A@1,B@2` or `--press A@1 --press B@2`, or both.
    let given: Vec<&String> = args
        .windows(2)
        .filter(|w| w[0] == "--press")
        .map(|w| &w[1])
        .collect();
    for p in given
        .iter()
        .flat_map(|g| g.split(','))
        .filter(|p| !p.is_empty())
    {
        let (pin, at) = p.split_once('@').ok_or("--press PIN@SECONDS")?;
        let t: f64 = at.parse().map_err(|_| "--press time")?;
        let pin = match pin {
            "BA" => Pin::Ba,
            "B" => Pin::Beta,
            _ => Pin::Matrix(
                pin[1..2].parse::<usize>().map_err(|_| "pin")? - 1,
                pin[3..4].parse::<usize>().map_err(|_| "pin")? - 1,
            ),
        };
        out.push(((t * CRYSTAL_HZ as f64) as u64, pin));
    }
    out.sort_by_key(|p| p.0);
    Ok(out)
}

/// The lines for machine time `t` under `presses`.
fn lines_at(presses: &[(u64, Pin)], t: u64) -> inktick_core::Lines {
    let held: Vec<Pin> = presses
        .iter()
        .filter(|(at, _)| t >= *at && t < at + CRYSTAL_HZ as u64 / 4)
        .map(|(_, p)| *p)
        .collect();
    pressed(&held)
}

/// An input to try: a matrix pair, a K line held high, or BA/β pulled low.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Pin {
    Matrix(usize, usize),
    Ba,
    Beta,
}

fn pressed(pins: &[Pin]) -> inktick_core::Lines {
    let mut l = inktick_core::Lines::IDLE;
    for p in pins {
        match *p {
            Pin::Matrix(s, k) => l.matrix[s] |= 1 << k,
            Pin::Ba => l.ba = false,
            Pin::Beta => l.beta = false,
        }
    }
    l
}

fn pin_name(p: Pin) -> String {
    match p {
        Pin::Matrix(s, k) => format!("S{}K{}", s + 1, k + 1),
        Pin::Ba => "BA".into(),
        Pin::Beta => "B".into(),
    }
}

/// Finds a unit's inputs by behaviour: which (strobe, K) pairs the program reads, and what
/// pressing each does, from the clock and from inside the game the first big change starts.
/// Writes a contact sheet per stage: `probe SET OUTDIR`.
fn probe(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let dir = PathBuf::from(args.get(1).ok_or("no output folder")?);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let sec = CRYSTAL_HZ as u64;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    // The strobe patterns driven when K is read, and whether BA or β are tested.
    let mut patterns: std::collections::BTreeMap<u8, u64> = Default::default();
    let (mut ba, mut beta) = (false, false);
    let kta = if set.kind.melodic() { 0x50 } else { 0x6A };
    while m.ticks() < 6 * sec {
        let r = m.chip().registers();
        if !r.halted {
            let op = set.program[r.pc as usize];
            if op == kta && m.ticks() > 2 * sec {
                *patterns.entry(r.w).or_default() += 1;
            }
            ba |= op == 0x5E;
            beta |= op == 0x51;
        }
        m.step();
    }
    let most = patterns.values().copied().max().unwrap_or(0);
    let patterns: Vec<u8> = patterns
        .into_iter()
        .filter(|(_, n)| *n * 20 >= most)
        .map(|(w, _)| w)
        .collect();
    // Strobes alike to the program (in the same patterns) are one input: keep the lowest.
    let mut seen = Vec::new();
    let mut pins: Vec<Pin> = Vec::new();
    for s in 0..8 {
        let signature: Vec<bool> = patterns.iter().map(|w| w & (1 << s) != 0).collect();
        if !signature.iter().any(|&b| b) || seen.contains(&signature) {
            continue;
        }
        seen.push(signature);
        for k in 0..4 {
            pins.push(Pin::Matrix(s, k));
        }
    }
    if ba {
        pins.push(Pin::Ba);
    }
    if beta {
        pins.push(Pin::Beta);
    }
    // `--time PIN`: the first stage from the clock that line brings up, not from power-on.
    let mut clock = m.clone();
    if let Some(t) =
        arg(args, "--time").and_then(|n| pins.iter().copied().find(|p| pin_name(*p) == n))
    {
        clock.set_lines(pressed(&[t]));
        clock.run_for(sec / 4);
        clock.set_lines(inktick_core::Lines::IDLE);
        clock.run_for(sec);
    }
    // `--held`: in the game, each line's picture is taken while it is still held.
    let held = args.iter().any(|a| a == "--held");
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let addrs = Scene::addresses(&screens);
    let none = |_: &str| None;
    let parts = Parts {
        view: None,
        elements: None,
        screens: &screens,
        viewport: None,
        picture: &none,
        side_by_side: set.screens.iter().any(|(f, _)| f.contains("_left")),
        only: None,
    };
    let mut scene = Scene::new(&parts, 240, 180, Look::SHARP);
    let mut shot = |m: &Machine| {
        let optics: Vec<Optics> = addrs
            .iter()
            .map(|a| {
                let mut o = Optics::new(a.len());
                o.snap(a, m.segments());
                o
            })
            .collect();
        scene.compose(&optics).0.clone()
    };
    let try_all =
        |from: &Machine, hold: u64, after: u64, pins: &[Pin]| -> Vec<(Pin, Machine, u32)> {
            let mut base = from.clone();
            base.run_for(hold + after);
            pins.iter()
                .map(|&p| {
                    let mut m = from.clone();
                    m.set_lines(pressed(&[p]));
                    m.run_for(hold);
                    m.set_lines(inktick_core::Lines::IDLE);
                    m.run_for(after);
                    let diff: u32 = (0..4)
                        .flat_map(|x| (0..16).map(move |y| (x, y)))
                        .map(|(x, y)| (m.segments().0[x][y] ^ base.segments().0[x][y]).count_ones())
                        .sum();
                    (p, m, diff)
                })
                .collect()
        };
    let mut report = String::new();
    // Stage 1: from the clock.
    let results = try_all(&clock, sec / 4, 2 * sec, &pins);
    let mut images = vec![shot(&{
        let mut b = clock.clone();
        b.run_for(sec / 4 + 2 * sec);
        b
    })];
    report += "clock:";
    for (p, m, d) in &results {
        report += &format!(" {}={d}", pin_name(*p));
        images.push(shot(m));
    }
    sheet(&images, &dir.join(format!("{}-clock.png", set.name)))?;
    // Stage 2: from inside the game `--start` (or the biggest change) starts, 3 s in; `--start`
    // may be several lines, pressed in turn (a game, then serving the ball).
    // Each as PIN or PIN:SECONDS held (a quarter of a second unless said).
    let named: Vec<(Pin, u64)> = arg(args, "--start")
        .map(|list| {
            list.split(',')
                .filter_map(|n| {
                    let (n, t) = n.split_once(':').unwrap_or((n, "0.25"));
                    let t = (t.parse::<f64>().ok()? * sec as f64) as u64;
                    pins.iter()
                        .copied()
                        .find(|p| pin_name(*p) == n)
                        .map(|p| (p, t))
                })
                .collect()
        })
        .unwrap_or_default();
    let start = (!named.is_empty()).then_some(named).or_else(|| {
        results
            .iter()
            .max_by_key(|r| r.2)
            .map(|r| vec![(r.0, sec / 4)])
    });
    // `--settle SECONDS`: the wait after each of them (3 s unless said).
    let settle: f64 = arg(args, "--settle")
        .and_then(|t| t.parse().ok())
        .unwrap_or(3.0);
    if let Some(start) = start {
        let mut game = clock.clone();
        for (p, t) in &start {
            game.set_lines(pressed(&[*p]));
            game.run_for(*t);
            game.set_lines(inktick_core::Lines::IDLE);
            game.run_for((settle * sec as f64) as u64);
        }
        let start = start[0].0;
        // What the program reads in the game, which may be more than at the clock.
        let mut look = game.clone();
        let mut in_game: std::collections::BTreeSet<u8> = Default::default();
        let end = look.ticks() + 2 * sec;
        while look.ticks() < end {
            let r = look.chip().registers();
            if !r.halted && set.program[r.pc as usize] == kta {
                in_game.insert(r.w);
            }
            look.step();
        }
        report += &format!(
            "\ngame strobes: {}\n",
            in_game
                .iter()
                .map(|w| format!("{w:08b}"))
                .collect::<Vec<_>>()
                .join(" ")
        );
        let mut game_pins = pins.clone();
        for s in 0..8u8 {
            let new = in_game.iter().any(|w| w & (1 << s) != 0)
                && !game_pins
                    .iter()
                    .any(|p| matches!(p, Pin::Matrix(t, _) if *t as u8 == s));
            if new {
                for k in 0..4 {
                    game_pins.push(Pin::Matrix(s as _, k));
                }
            }
        }
        let results = try_all(&game, sec / 3, if held { 0 } else { sec / 3 }, &game_pins);
        let mut images = vec![shot(&{
            let mut b = game.clone();
            b.run_for(2 * sec / 3);
            b
        })];
        report += &format!("\ngame ({}):", pin_name(start));
        for (p, m, d) in &results {
            report += &format!(" {}={d}", pin_name(*p));
            images.push(shot(m));
        }
        sheet(&images, &dir.join(format!("{}-game.png", set.name)))?;
    }
    println!("{}: {}\n{report}", set.name, set.kind.name());
    Ok(())
}

/// Lays pictures out 4 to a row with a gap, the first (the baseline) framed.
fn sheet(images: &[inktick_art::Image], out: &std::path::Path) -> Result<(), String> {
    let (w, h) = (images[0].width, images[0].height);
    let cols = 4;
    let rows = images.len().div_ceil(cols);
    let mut s = inktick_art::Image::filled(cols * (w + 6), rows * (h + 6), [60, 20, 20, 255]);
    for (i, img) in images.iter().enumerate() {
        let (ox, oy) = ((i % cols) * (w + 6) + 3, (i / cols) * (h + 6) + 3);
        for y in 0..h {
            let d = ((oy + y) * s.width + ox) * 4;
            s.pixels[d..d + w * 4].copy_from_slice(&img.pixels[y * w * 4..(y + 1) * w * 4]);
        }
    }
    std::fs::write(out, s.to_png()).map_err(|e| e.to_string())
}

/// The strobe patterns (W) the program drives when it reads K, with counts, over a run that
/// starts Game A on `--start SxKy` if given: `strobes SET [SECONDS] [--start S3K3]`.
fn strobe_patterns(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10.0);
    let start = arg(args, "--start");
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let mut counts: std::collections::BTreeMap<u8, u64> = Default::default();
    let kta = if set.kind.melodic() { 0x50 } else { 0x6A };
    let sec = CRYSTAL_HZ as u64;
    while m.ticks() < (secs * sec as f64) as u64 {
        if let Some(st) = &start {
            let t = m.ticks();
            if t == 6 * sec {
                let s: usize = st[1..2].parse().unwrap_or(1);
                let k: usize = st[3..4].parse().unwrap_or(1);
                m.set_lines(pressed(&[Pin::Matrix(s - 1, k - 1)]));
            } else if t == 6 * sec + sec / 4 {
                m.set_lines(inktick_core::Lines::IDLE);
            }
        }
        let r = m.chip().registers();
        if !r.halted && set.program[r.pc as usize] == kta && m.ticks() > 6 * sec {
            *counts.entry(r.w).or_default() += 1;
        }
        m.step();
    }
    for (w, n) in counts {
        let lines: Vec<String> = (0..8)
            .filter(|i| w & (1 << i) != 0)
            .map(|i| format!("S{}", i + 1))
            .collect();
        println!("{w:08b} {:<16} {n}", lines.join(","));
    }
    Ok(())
}

/// The artwork's clickable buttons: input tag and bit, element name, and where on the unit the
/// picture's opaque part is (fractions of the view): `buttons SET --artwork DIR [--view NAME]`.
fn buttons(args: &[String]) -> Result<(), String> {
    let art: Vec<PathBuf> = arg(args, "--artwork")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &art)?;
    let a = set.artwork.as_ref().ok_or("no artwork")?;
    let want = arg(args, "--view").unwrap_or_else(|| "Unit Only".into());
    let view = a
        .layout
        .views
        .iter()
        .find(|v| v.name == want && v.items.iter().any(|i| i.input.is_some()))
        .or_else(|| {
            a.layout.views.iter().find(|v| {
                v.items
                    .iter()
                    .any(|i| i.input.as_ref().is_some_and(|x| x.mask != 0))
            })
        })
        .ok_or("no view with buttons")?;
    let vb = view.bounds;
    let scale = 800.0 / vb.w;
    let mut seen = std::collections::BTreeSet::new();
    for item in &view.items {
        let (Some(input), inktick_art::layout::What::Element { name, .. }) =
            (&item.input, &item.what)
        else {
            continue;
        };
        if input.mask == 0 || !seen.insert((input.tag.clone(), input.mask)) {
            continue;
        }
        let Some(e) = a.layout.elements.get(name) else {
            continue;
        };
        // The element's pictures at their size in an 800-wide view, opaque pixels' box.
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let own = e.bounds();
        for c in &e.components {
            let inktick_art::layout::Part::Image { file, .. } = &c.part else {
                continue;
            };
            let Some(img) = a.picture(file) else { continue };
            let w = ((c.bounds.w / own.w) * item.bounds.w * scale).max(1.0) as usize;
            let h = ((c.bounds.h / own.h) * item.bounds.h * scale).max(1.0) as usize;
            let img = img.resized(w, h);
            let ox = item.bounds.x + (c.bounds.x - own.x) / own.w * item.bounds.w;
            let oy = item.bounds.y + (c.bounds.y - own.y) / own.h * item.bounds.h;
            for y in 0..h {
                for x in 0..w {
                    if img.at(x, y)[3] > 128 {
                        let vx = ox + x as f32 / scale;
                        let vy = oy + y as f32 / scale;
                        x0 = x0.min(vx);
                        y0 = y0.min(vy);
                        x1 = x1.max(vx);
                        y1 = y1.max(vy);
                    }
                }
            }
        }
        if x0 > x1 {
            continue;
        }
        let f = |v: f32, o: f32, len: f32| (v - o) / len;
        println!(
            "{} {:#x} {name} x {:.3}-{:.3} y {:.3}-{:.3}",
            input.tag,
            input.mask,
            f(x0, vb.x, vb.w),
            f(x1, vb.x, vb.w),
            f(y0, vb.y, vb.h),
            f(y1, vb.y, vb.h)
        );
    }
    Ok(())
}

fn pin_of(line: inktick_core::units::Line) -> Pin {
    match line {
        inktick_core::units::Line::Matrix(s, k) => Pin::Matrix(s as usize, k as usize),
        inktick_core::units::Line::Ba => Pin::Ba,
        inktick_core::units::Line::Beta => Pin::Beta,
    }
}

/// Checks a unit's database row on our core: from the clock (after Time), presses each unit
/// button; from inside game A, each control, and each line the program reads that the row
/// doesn't have. Writes `<set>-check.png` (in that order, the first of each stage the baseline)
/// and prints the order: `check SET OUTDIR`.
fn check(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(args.first().ok_or("no set")?);
    let name = inktick_sets::set_name(&path);
    let unit = inktick_core::units::by_set(&name).ok_or("not in the database")?;
    let set = inktick_sets::open(&path, unit.parent, &[])?;
    let dir = PathBuf::from(args.get(1).ok_or("no output folder")?);
    let sec = CRYSTAL_HZ as u64;
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let addrs = Scene::addresses(&screens);
    let none = |_: &str| None;
    let parts = Parts {
        view: None,
        elements: None,
        screens: &screens,
        viewport: None,
        picture: &none,
        side_by_side: set.screens.iter().any(|(f, _)| f.contains("_left")),
        only: None,
    };
    let mut scene = Scene::new(&parts, 240, 180, Look::SHARP);
    let mut shot = |m: &Machine| {
        let optics: Vec<Optics> = addrs
            .iter()
            .map(|a| {
                let mut o = Optics::new(a.len());
                o.snap(a, m.segments());
                o
            })
            .collect();
        scene.compose(&optics).0.clone()
    };
    let press = |m: &mut Machine, p: Pin, hold: u64, after: u64| {
        m.set_lines(pressed(&[p]));
        m.run_for(hold);
        m.set_lines(inktick_core::Lines::IDLE);
        m.run_for(after);
    };
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    m.run_until(3 * sec);
    let mut images = Vec::new();
    let mut order = vec!["power-on 3 s".to_string()];
    images.push(shot(&m));
    // Time first: it leaves the setting mode the units start in.
    if let Some(t) = unit.button(inktick_core::units::Role::Time) {
        press(&mut m, pin_of(t.line), sec / 4, sec);
        order.push("after TIME".into());
        images.push(shot(&m));
    }
    let clock = m.clone();
    for b in unit.buttons.iter().filter(|b| b.role.is_unit_button()) {
        let mut t = clock.clone();
        press(&mut t, pin_of(b.line), sec / 4, sec);
        order.push(format!("clock + {}", b.label));
        images.push(shot(&t));
    }
    let start = unit
        .button(inktick_core::units::Role::GameA)
        .or(unit.button(inktick_core::units::Role::Game));
    if let Some(start) = start {
        let mut game = clock.clone();
        let wait = arg(args, "--wait")
            .and_then(|w| w.parse::<u64>().ok())
            .unwrap_or(4);
        press(&mut game, pin_of(start.line), sec / 4, wait * sec);
        let mut base = game.clone();
        base.run_for(sec / 2);
        order.push("game base".into());
        images.push(shot(&base));
        let known: Vec<Pin> = unit.buttons.iter().map(|b| pin_of(b.line)).collect();
        let mut tries: Vec<(String, Pin)> = unit
            .buttons
            .iter()
            .filter(|b| !b.role.is_unit_button())
            .map(|b| (b.label.to_string(), pin_of(b.line)))
            .collect();
        for s in 0..8 {
            for k in 0..4 {
                if !known.contains(&Pin::Matrix(s, k)) {
                    tries.push((
                        format!("? {}", pin_name(Pin::Matrix(s, k))),
                        Pin::Matrix(s, k),
                    ));
                }
            }
        }
        for p in [Pin::Ba, Pin::Beta] {
            if !known.contains(&p) {
                tries.push((format!("? {}", pin_name(p)), p));
            }
        }
        for (label, p) in tries {
            let mut t = game.clone();
            press(&mut t, p, sec / 3, sec / 6);
            let diff: u32 = (0..4)
                .flat_map(|x| (0..16).map(move |y| (x, y)))
                .map(|(x, y)| (t.segments().0[x][y] ^ base.segments().0[x][y]).count_ones())
                .sum();
            if label.starts_with('?') && diff == 0 {
                continue;
            }
            order.push(format!("game + {label} ({diff})"));
            images.push(shot(&t));
        }
    }
    sheet(&images, &dir.join(format!("{}-check.png", set.name)))?;
    println!("{} ({}):", unit.set, unit.title);
    for (i, o) in order.iter().enumerate() {
        println!("  {i:>2} {o}");
    }
    Ok(())
}

/// Runs a set and writes its sound to a WAV file (mono, 16-bit, 48 kHz), with presses as `png`
/// takes them (`--press S3K1@3,S3K3@5`): `wav SET OUT.wav [--seconds N] [--press ...]`.
fn wav(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let out = PathBuf::from(args.get(1).ok_or("no output file")?);
    let seconds: f64 = arg(args, "--seconds")
        .map_or(Ok(10.0), |s| s.parse())
        .map_err(|_| "--seconds")?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let clean = arg(args, "--clean").is_some();
    let piezo = if clean {
        inktick_core::Piezo {
            amount: 0.0,
            ..inktick_core::Piezo::DEFAULT
        }
    } else {
        inktick_core::Piezo::DEFAULT
    };
    let mut synth = inktick_core::Synth::new(48_000, piezo);
    let presses = presses(args)?;
    let end = (seconds * CRYSTAL_HZ as f64) as u64;
    let mut samples = Vec::new();
    let step = CRYSTAL_HZ as u64 / 1000;
    while m.ticks() < end {
        m.set_lines(lines_at(&presses, m.ticks()));
        m.run_for(step);
        m.sound_into(&mut synth);
        let n = synth.ready(m.ticks());
        synth.take(n, &mut samples);
    }
    let pcm: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut w = Vec::new();
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&48_000u32.to_le_bytes());
    w.extend_from_slice(&96_000u32.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    w.extend_from_slice(&pcm);
    std::fs::write(&out, w).map_err(|e| e.to_string())?;
    let loud = samples.iter().filter(|s| s.abs() > 0.05).count();
    println!(
        "{}: {:.1} s of sound, {:.1} s above -26 dB",
        set.name,
        samples.len() as f64 / 48_000.0,
        loud as f64 / 48_000.0
    );
    Ok(())
}

/// Composes a unit's top panel frame by frame as the program does (60 a second, the default
/// look), with presses: what it costs and how much it redraws. `bench SET --artwork DIR
/// [--seconds N] [--press ...] [--view NAME]`.
/// The segments a press lit and put out.
type Change = (Vec<[u8; 3]>, Vec<[u8; 3]>);

/// What each line the game scans does, from inside a game: `--start PIN[:SECONDS],...` presses
/// them in turn from the clock, `--settle` seconds after each (3 unless said); then each line is
/// held 0.35 s against not pressing. Printed: the segments it changed, where the lit ones moved
/// on the screens (the centre of those it lit less the centre of those it put out, in pixels of
/// a 1024 x 768 plain picture per screen, y down), and the lines that did exactly the same.
fn controlfind(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(args.first().ok_or("no set")?);
    let name = inktick_sets::set_name(&path);
    let unit = inktick_core::units::by_set(&name).ok_or("not in the database")?;
    let set = inktick_sets::open(&path, unit.parent, &[])?;
    let sec = CRYSTAL_HZ as u64;
    let pin = |n: &str| -> Result<Pin, String> {
        match n.as_bytes() {
            [b'S', s, b'K', k] => Ok(Pin::Matrix((s - b'1') as usize, (k - b'1') as usize)),
            b"BA" => Ok(Pin::Ba),
            b"B" => Ok(Pin::Beta),
            _ => Err(format!("{n}: a line is SnKm, BA or B")),
        }
    };
    let hold = |m: &mut Machine, p: Option<Pin>, t: u64| {
        m.set_lines(p.map_or(inktick_core::Lines::IDLE, |p| pressed(&[p])));
        m.run_for(t);
        m.set_lines(inktick_core::Lines::IDLE);
    };
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    unit.show_clock(&mut m);
    let settle: f64 = arg(args, "--settle")
        .and_then(|t| t.parse().ok())
        .unwrap_or(3.0);
    // `--hold SECONDS`: how long each line is held (0.35 s unless said; slow games need more).
    let hold_s: f64 = arg(args, "--hold")
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.35);
    let held = (hold_s * sec as f64) as u64;
    for n in arg(args, "--start")
        .unwrap_or_default()
        .split(',')
        .filter(|n| !n.is_empty())
    {
        let (n, t) = n.split_once(':').unwrap_or((n, "0.25"));
        let t: f64 = t.parse().map_err(|_| "--start time")?;
        hold(&mut m, Some(pin(n)?), (t * sec as f64) as u64);
        m.run_for((settle * sec as f64) as u64);
    }
    // The strobes the game drives when it reads K.
    let kta = if set.kind.melodic() { 0x50 } else { 0x6A };
    let mut strobes = 0u8;
    let mut look = m.clone();
    let window: f64 = arg(args, "--window")
        .and_then(|t| t.parse().ok())
        .unwrap_or(2.0);
    let end = look.ticks() + (window * sec as f64) as u64;
    let mut patterns = std::collections::BTreeMap::<u8, u32>::new();
    while look.ticks() < end {
        let r = look.chip().registers();
        if !r.halted && set.program[r.pc as usize] == kta {
            strobes |= r.w;
            *patterns.entry(r.w).or_default() += 1;
        }
        look.step();
    }
    if args.iter().any(|a| a == "--patterns") {
        for (w, n) in &patterns {
            println!("  pattern {w:08b} read {n} times");
        }
    }
    // Each segment's centre, in a plain picture of the screens.
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let addrs = Scene::addresses(&screens);
    let parts = Parts {
        view: None,
        elements: None,
        screens: &screens,
        viewport: None,
        picture: &|_| None,
        side_by_side: set.screens.iter().any(|(f, _)| f.contains("_left")),
        only: None,
    };
    let scene = Scene::new(&parts, 1024, 768 * screens.len() as u32, Look::default());
    let mut centre = std::collections::HashMap::new();
    for g in scene.lcd().segments {
        let a = addrs[g.screen][g.segment];
        let r = g.rect;
        centre.insert(a, ((r.x + r.w / 2) as f32, (r.y + r.h / 2) as f32));
    }
    let lit = |m: &Machine| -> Vec<[u8; 3]> {
        addrs
            .iter()
            .flatten()
            .copied()
            .filter(|a| m.segments().on(a[0] as usize, a[1] as usize, a[2] as usize))
            .collect()
    };
    let mut pins: Vec<Pin> = (0..8)
        .filter(|s| strobes & (1 << s) != 0)
        .flat_map(|s| (0..4).map(move |k| Pin::Matrix(s, k)))
        .collect();
    let mean = |v: &[[u8; 3]]| {
        let pts: Vec<(f32, f32)> = v.iter().filter_map(|a| centre.get(a).copied()).collect();
        let n = pts.len().max(1) as f32;
        (
            pts.iter().map(|p| p.0).sum::<f32>() / n,
            pts.iter().map(|p| p.1).sum::<f32>() / n,
        )
    };
    // `--explore N`: N steps of a walk through the game, each holding a line that did something
    // (chosen by a fixed sequence); at each step every line is tried, and what each did is
    // totalled: how often it changed anything, and how often it moved the lit segments left,
    // right, up or down (by more than 20 pixels).
    if let Some(n) = arg(args, "--explore").and_then(|n| n.parse::<usize>().ok()) {
        #[derive(Default, Clone)]
        struct Tally {
            any: u32,
            left: u32,
            right: u32,
            up: u32,
            down: u32,
            like: std::collections::BTreeMap<String, u32>,
        }
        let mut tally = vec![Tally::default(); pins.len()];
        let mut seed = 0x9e37_79b9u32;
        let mut walk = m.clone();
        for _ in 0..n {
            let mut without = walk.clone();
            hold(&mut without, None, sec * 3 / 10);
            let base = lit(&without);
            let mut effects: Vec<Option<Change>> = Vec::new();
            for p in &pins {
                let mut with = walk.clone();
                hold(&mut with, Some(*p), sec * 3 / 10);
                let now = lit(&with);
                let on: Vec<[u8; 3]> = now.iter().filter(|a| !base.contains(a)).copied().collect();
                let off: Vec<[u8; 3]> = base.iter().filter(|a| !now.contains(a)).copied().collect();
                effects.push((!on.is_empty() || !off.is_empty()).then_some((on, off)));
            }
            for (i, e) in effects.iter().enumerate() {
                let Some((on, off)) = e else { continue };
                let t = &mut tally[i];
                t.any += 1;
                if !on.is_empty() && !off.is_empty() {
                    let (a, b) = (mean(on), mean(off));
                    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
                    if dx < -20.0 {
                        t.left += 1
                    }
                    if dx > 20.0 {
                        t.right += 1
                    }
                    if dy < -20.0 {
                        t.up += 1
                    }
                    if dy > 20.0 {
                        t.down += 1
                    }
                }
                for (j, f) in effects.iter().enumerate() {
                    if j < i && f.as_ref() == Some(&(on.clone(), off.clone())) {
                        *t.like.entry(pin_name(pins[j])).or_default() += 1;
                    }
                }
            }
            // The next step: a line that did something here, by a fixed pseudo-random choice.
            let live: Vec<usize> = (0..pins.len()).filter(|&i| effects[i].is_some()).collect();
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            match live.get(seed as usize % live.len().max(1)) {
                Some(&i) => hold(&mut walk, Some(pins[i]), sec * 3 / 10),
                None => hold(&mut walk, None, sec * 3 / 10),
            }
            walk.run_for(sec / 10);
        }
        println!("{name}: {n} steps; per line: changed, left right up down, same as");
        for (p, t) in pins.iter().zip(&tally) {
            if t.any == 0 {
                continue;
            }
            let like: Vec<String> = t
                .like
                .iter()
                .filter(|(_, c)| **c * 10 >= t.any * 9)
                .map(|(n, _)| n.clone())
                .collect();
            println!(
                "  {:<5} {:>3}   {:>3} {:>3} {:>3} {:>3}   {}",
                pin_name(*p),
                t.any,
                t.left,
                t.right,
                t.up,
                t.down,
                like.join(" ")
            );
        }
        return Ok(());
    }
    let mut without = m.clone();
    hold(&mut without, None, held);
    let base = lit(&without);
    pins.extend([Pin::Ba, Pin::Beta]);
    let mut seen: Vec<(Change, String)> = Vec::new();
    println!(
        "{name}: strobes {strobes:08b}, {} lit without a press",
        base.len()
    );
    for p in pins {
        let mut with = m.clone();
        hold(&mut with, Some(p), held);
        let now = lit(&with);
        let on: Vec<[u8; 3]> = now.iter().filter(|a| !base.contains(a)).copied().collect();
        let off: Vec<[u8; 3]> = base.iter().filter(|a| !now.contains(a)).copied().collect();
        if on.is_empty() && off.is_empty() {
            continue;
        }
        let name = pin_name(p);
        if let Some(same) = seen.iter().find(|s| s.0 .0 == on && s.0 .1 == off) {
            println!("  {name:<5} as {}", same.1);
            continue;
        }
        let moved = if !on.is_empty() && !off.is_empty() {
            let (a, b) = (mean(&on), mean(&off));
            format!("moved {:+5.0} {:+5.0}", a.0 - b.0, a.1 - b.1)
        } else if on.is_empty() {
            format!(
                "put out {} at {:.0},{:.0}",
                off.len(),
                mean(&off).0,
                mean(&off).1
            )
        } else {
            format!("lit {} at {:.0},{:.0}", on.len(), mean(&on).0, mean(&on).1)
        };
        println!("  {name:<5} +{} -{}  {moved}", on.len(), off.len());
        seen.push(((on, off), name));
    }
    Ok(())
}

/// Whether line B undoes what line A does: from the state `--start` leads to (as for `probe`,
/// `--settle` seconds after each, 3 unless said), A held 0.4 s changes some RAM nibbles against not pressing (where the player
/// is); then B held 0.4 s: the nibbles B puts back as they were without A are printed. A pair of
/// directions undo each other: `undo SET A B --start PIN,...`.
fn undo(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(args.first().ok_or("no set")?);
    let name = inktick_sets::set_name(&path);
    let unit = inktick_core::units::by_set(&name).ok_or("not in the database")?;
    let set = inktick_sets::open(&path, unit.parent, &[])?;
    let pin = |n: &str| -> Result<Pin, String> {
        let n = n.as_bytes();
        match n {
            [b'S', s, b'K', k] => Ok(Pin::Matrix((s - b'1') as usize, (k - b'1') as usize)),
            _ => Err("a line is SnKm".into()),
        }
    };
    let a = pin(args.get(1).ok_or("no line A")?)?;
    let b = pin(args.get(2).ok_or("no line B")?)?;
    let sec = CRYSTAL_HZ as u64;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    unit.show_clock(&mut m);
    let hold = |m: &mut Machine, p: Option<Pin>, t: u64| {
        m.set_lines(p.map_or(inktick_core::Lines::IDLE, |p| pressed(&[p])));
        m.run_for(t);
        m.set_lines(inktick_core::Lines::IDLE);
    };
    let settle: f64 = arg(args, "--settle")
        .and_then(|t| t.parse().ok())
        .unwrap_or(3.0);
    for n in arg(args, "--start")
        .unwrap_or_default()
        .split(',')
        .filter(|n| !n.is_empty())
    {
        hold(&mut m, Some(pin(n)?), sec / 4);
        m.run_for((settle * sec as f64) as u64);
    }
    let (mut with_a, mut without) = (m.clone(), m.clone());
    hold(&mut with_a, Some(a), sec * 2 / 5);
    hold(&mut without, None, sec * 2 / 5);
    let moved: Vec<usize> = (0..128)
        .filter(|&i| with_a.chip().ram()[i] != without.chip().ram()[i])
        .collect();
    let mut back = with_a.clone();
    hold(&mut back, Some(b), sec * 2 / 5);
    let mut still = without.clone();
    hold(&mut still, None, sec * 2 / 5);
    let undone: Vec<String> = moved
        .iter()
        .filter(|&&i| back.chip().ram()[i] == still.chip().ram()[i])
        .map(|i| format!("{i:02x}"))
        .collect();
    println!(
        "{name}: A changes {} nibbles ({}); B puts back {}: {}",
        moved.len(),
        moved
            .iter()
            .map(|i| format!("{i:02x}"))
            .collect::<Vec<_>>()
            .join(" "),
        undone.len(),
        undone.join(" ")
    );
    Ok(())
}

/// Checks that each of a unit's controls does something in a game: from inside game A (or
/// GAME), 3 s in, each control held 0.4 s, against the same 0.4 s untouched; and again after
/// each other control has been held first (a direction blocked where the game starts is free
/// after a move). Prints the controls that changed nothing either way: `controlcheck SET...`.
fn controlcheck(args: &[String]) -> Result<(), String> {
    use inktick_core::units::Role;
    let sec = CRYSTAL_HZ as u64;
    for path in args {
        let path = PathBuf::from(path);
        let name = inktick_sets::set_name(&path);
        let unit = inktick_core::units::by_set(&name).ok_or("not in the database")?;
        let set = inktick_sets::open(&path, unit.parent, &[])?;
        let mut m = Machine::new(
            set.kind,
            &set.program,
            set.melody.as_deref(),
            Options::default(),
        );
        unit.show_clock(&mut m);
        let Some(start) = unit
            .button(Role::GameA)
            .or(unit.button(Role::Game))
            .map(|b| b.role)
        else {
            println!("{name:<14} no game button");
            continue;
        };
        let hold = |m: &mut Machine, role: Option<Role>, t: u64| {
            m.set_lines(match role {
                Some(r) => unit.lines(|b| b.role == r),
                None => inktick_core::Lines::IDLE,
            });
            m.run_for(t);
            m.set_lines(inktick_core::Lines::IDLE);
        };
        hold(&mut m, Some(start), sec / 4);
        m.run_for(3 * sec);
        let controls: Vec<Role> = unit
            .buttons
            .iter()
            .filter(|b| !b.role.is_unit_button())
            .map(|b| b.role)
            .collect();
        let effect = |from: &Machine, role: Role, t: u64| {
            let mut a = from.clone();
            let mut b = from.clone();
            hold(&mut a, Some(role), t);
            hold(&mut b, None, t);
            a.segments() != b.segments() || a.chip().ram() != b.chip().ram()
        };
        // 3 s into the game, held 0.4 s; and, for games that start slowly or move on a slow
        // beat, 6 s in, held a second; and 9 s and 15 s in, after a long opening (Boxing's bell,
        // Bomb Sweeper's).
        let mut later = m.clone();
        later.run_for(3 * sec);
        let mut latest = later.clone();
        latest.run_for(3 * sec);
        let mut last = latest.clone();
        last.run_for(6 * sec);
        let tries = [
            (&m, sec * 2 / 5),
            (&later, sec),
            (&latest, sec),
            (&last, sec),
        ];
        let dead: Vec<String> = controls
            .iter()
            .filter(|&&r| {
                !tries.iter().any(|(from, t)| {
                    effect(from, r, *t)
                        || controls.iter().filter(|&&o| o != r).any(|&o| {
                            let mut first = (*from).clone();
                            hold(&mut first, Some(o), *t + sec / 10);
                            first.run_for(sec / 4);
                            effect(&first, r, *t)
                        })
                })
            })
            .map(|r| format!("{r:?}"))
            .collect();
        // Held 1.5 s, a control must not put the unit in its all-segments display (a line read
        // in another of the game's strobe patterns too, as a different button).
        let total = Scene::addresses(
            &set.screens
                .iter()
                .map(|(_, s)| s.clone())
                .collect::<Vec<_>>(),
        )
        .iter()
        .map(Vec::len)
        .sum::<usize>() as u32;
        let lit = |m: &Machine| {
            m.segments()
                .0
                .iter()
                .flatten()
                .map(|b| b.count_ones())
                .sum::<u32>()
        };
        let mut dead = dead;
        for &r in &controls {
            let mut h = m.clone();
            hold(&mut h, Some(r), sec * 3 / 2);
            if lit(&h) * 10 > total * 8 {
                dead.push(format!("{r:?} (held: every segment)"));
            }
        }
        let state = if controls.is_empty() {
            "no controls".to_string()
        } else if dead.is_empty() {
            format!("ok ({})", controls.len())
        } else {
            format!("does nothing: {}", dead.join(", "))
        };
        println!("{name:<14} {state}");
    }
    Ok(())
}

/// Checks a unit's clock entry: set to times either side of the carries, the unit counts on
/// from them as a clock does, at its clock and inside game A; and the set time is shown.
/// Prints one line per set: `clockcheck SET... [--png DIR]` (the clock set to 10:42 PM).
fn clockcheck(args: &[String]) -> Result<(), String> {
    use inktick_core::units::Role;
    let png_dir = arg(args, "--png").map(PathBuf::from);
    let sec = CRYSTAL_HZ as u64;
    let after = |(h, m, s): (u8, u8, u8), n: u32| {
        let t = (h as u32 * 3600 + m as u32 * 60 + s as u32 + n) % 86400;
        ((t / 3600) as u8, (t / 60 % 60) as u8, (t % 60) as u8)
    };
    for path in args.iter().take_while(|a| !a.starts_with("--")) {
        let path = PathBuf::from(path);
        let name = inktick_sets::set_name(&path);
        let unit = inktick_core::units::by_set(&name).ok_or("not in the database")?;
        let set = inktick_sets::open(&path, unit.parent, &[])?;
        let fresh = || {
            let mut m = Machine::new(
                set.kind,
                &set.program,
                set.melody.as_deref(),
                Options::default(),
            );
            unit.show_clock(&mut m);
            m
        };
        let mut wrong = Vec::new();
        for t in [
            (23, 59, 57),
            (11, 59, 56),
            (12, 59, 55),
            (0, 59, 58),
            (9, 59, 59),
            (21, 9, 5),
        ] {
            let mut m = fresh();
            if !unit.set_time(&mut m, t.0, t.1, t.2) {
                wrong.push("no clock".into());
                break;
            }
            m.run_for(5 * sec);
            let got = unit.time(&m);
            if got != Some(after(t, 5)) {
                wrong.push(format!("{t:?}+5s gave {got:?}"));
            }
        }
        // Inside a game: the clock keeps time, and setting it doesn't stop the game.
        let mut m = fresh();
        unit.set_time(&mut m, 22, 58, 0);
        let start = unit
            .button(Role::GameA)
            .or(unit.button(Role::Game))
            .map(|b| b.role);
        if let Some(r) = start {
            m.set_lines(unit.lines(|b| b.role == r));
            m.run_for(sec / 5);
            m.set_lines(inktick_core::Lines::IDLE);
            m.run_for(20 * sec);
            let lit = m
                .segments()
                .0
                .iter()
                .flatten()
                .map(|b| b.count_ones())
                .sum::<u32>();
            unit.set_time(&mut m, 7, 30, 0);
            m.run_for(10 * sec);
            let got = unit.time(&m);
            // Set at any point in a second, it may count one more or one less in ten.
            if !matches!(got, Some((7, 30, 9..=11))) {
                wrong.push(format!(
                    "in game: 7:30:00 +10 s gave {got:?} ({lit} segments lit)"
                ));
            }
        }
        if let Some(dir) = &png_dir {
            let mut m = fresh();
            unit.set_time(&mut m, 22, 42, 0);
            m.run_for(2 * sec);
            let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
            let parts = Parts {
                view: None,
                elements: None,
                screens: &screens,
                viewport: None,
                picture: &|_| None,
                side_by_side: false,
                only: None,
            };
            let mut scene = Scene::new(&parts, 512, 384, Look::default());
            let addrs = Scene::addresses(&screens);
            let optics: Vec<Optics> = addrs
                .iter()
                .map(|a| {
                    let mut o = Optics::new(a.len());
                    o.snap(a, m.segments());
                    o
                })
                .collect();
            scene.compose(&optics);
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{name}.png")), scene.picture().to_png())
                .map_err(|e| e.to_string())?;
        }
        println!(
            "{name:<14} {}",
            if wrong.is_empty() {
                "ok".to_string()
            } else {
                wrong.join("; ")
            }
        );
    }
    Ok(())
}

/// Finds a unit's clock in RAM: runs it a day and a bit, untouched, sampling RAM once a second,
/// and prints each nibble that changes on the clock's beats (1, 10, 60, 600, 3600 or 43200 s)
/// with the values it takes, in order, from its first change.
fn clockfind(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(args.first().ok_or("no set")?);
    let unit =
        inktick_core::units::by_set(&inktick_sets::set_name(&path)).ok_or("not in the database")?;
    let set = inktick_sets::open(&path, unit.parent, &[])?;
    let hours: u64 = arg(args, "--hours")
        .map_or(Ok(25), |s| s.parse())
        .map_err(|_| "--hours")?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let second = CRYSTAL_HZ as u64;
    // ACL, as after putting the batteries in.
    m.set_acl(true);
    m.run_for(second / 10);
    m.set_acl(false);
    m.run_for(second / 2);
    // TIME: the clock, from the all-segments test that follows ACL; or the presses given
    // (`--press PIN@SECONDS`, from then).
    let presses = presses(args)?;
    if presses.is_empty() {
        m.set_lines(unit.lines(|b| b.role == inktick_core::units::Role::Time));
        m.run_for(second / 5);
        m.set_lines(inktick_core::Lines::IDLE);
        m.run_for(second / 2);
    } else {
        let start = m.ticks();
        let end = start + presses.iter().map(|p| p.0).max().unwrap_or(0) + second;
        while m.ticks() < end {
            m.set_lines(lines_at(&presses, m.ticks() - start));
            m.run_for(64);
        }
        m.set_lines(inktick_core::Lines::IDLE);
    }
    let mut samples: Vec<[u8; 128]> = Vec::new();
    for _ in 0..hours * 3600 {
        samples.push(*m.chip().ram());
        m.run_for(second);
        m.take_edges();
    }
    for at in 0..128 {
        let v: Vec<u8> = samples.iter().map(|r| r[at]).collect();
        let changes: Vec<usize> = (1..v.len()).filter(|&i| v[i] != v[i - 1]).collect();
        if changes.len() < 2 {
            continue;
        }
        let gaps: std::collections::BTreeSet<usize> =
            changes.windows(2).map(|w| w[1] - w[0]).collect();
        let beat = [1usize, 10, 60, 600, 3600, 43200]
            .into_iter()
            .find(|b| gaps.iter().all(|g| g % b == 0) && gaps.contains(b));
        if args.iter().any(|a| a == "--all") {
            let g: Vec<String> = gaps.iter().take(8).map(|g| g.to_string()).collect();
            println!("{at:02x} changes {:>6} gaps {}", changes.len(), g.join(","));
        }
        let Some(beat) = beat else { continue };
        // The values in order, one per beat, from the first change.
        let first = changes[0];
        let seq: Vec<String> = (0..)
            .map(|k| first + k * beat)
            .take_while(|&i| i < v.len())
            .take(if beat >= 3600 { 26 } else { 24 })
            .map(|i| format!("{:x}", v[i]))
            .collect();
        println!(
            "{:02x} beat {beat:>5} first {first:>5}  {}",
            at,
            seq.join("")
        );
    }
    Ok(())
}

/// How long the core alone takes per emulated second, and how much of the time it is halted.
fn speed(args: &[String]) -> Result<(), String> {
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &[])?;
    let seconds: f64 = arg(args, "--seconds")
        .map_or(Ok(60.0), |s| s.parse())
        .map_err(|_| "--seconds")?;
    let presses = presses(args)?;
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let frame = CRYSTAL_HZ as u64 / 60;
    let (mut halted, mut steps) = (0u64, 0u64);
    let t = std::time::Instant::now();
    while m.ticks() < (seconds * CRYSTAL_HZ as f64) as u64 {
        m.set_lines(lines_at(&presses, m.ticks()));
        m.run_for(frame);
        m.take_edges();
    }
    let spent = t.elapsed();
    let mut m2 = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    while m2.ticks() < (seconds.min(10.0) * CRYSTAL_HZ as f64) as u64 {
        m2.set_lines(lines_at(&presses, m2.ticks()));
        halted += m2.chip().registers().halted as u64;
        steps += 1;
        m2.step();
    }
    println!(
        "{:.3} ms per emulated second; halted {:.1}% of steps",
        spent.as_secs_f64() * 1000.0 / seconds,
        halted as f64 * 100.0 / steps as f64
    );
    Ok(())
}

fn bench(args: &[String]) -> Result<(), String> {
    let art: Vec<PathBuf> = arg(args, "--artwork")
        .map(PathBuf::from)
        .into_iter()
        .collect();
    let set = inktick_sets::open(&PathBuf::from(args.first().ok_or("no set")?), None, &art)?;
    let seconds: f64 = arg(args, "--seconds")
        .map_or(Ok(20.0), |s| s.parse())
        .map_err(|_| "--seconds")?;
    let presses = presses(args)?;
    let screens: Vec<_> = set.screens.iter().map(|(_, s)| s.clone()).collect();
    let addrs = Scene::addresses(&screens);
    let mut optics: Vec<Optics> = addrs.iter().map(|a| Optics::new(a.len())).collect();
    let view_name = arg(args, "--view").unwrap_or_else(|| "Background Only".into());
    let view = set.artwork.as_ref().and_then(|a| a.layout.view(&view_name));
    let picture = |f: &str| set.artwork.as_ref().and_then(|a| a.picture(f));
    let parts = Parts {
        view,
        elements: set.artwork.as_ref().map(|a| &a.layout.elements),
        screens: &screens,
        viewport: view.and_then(|v| {
            (screens.len() > 1)
                .then(|| inktick_art::scene::screen_viewport(v, 0))
                .flatten()
        }),
        picture: &picture,
        side_by_side: false,
        only: None,
    };
    let t = std::time::Instant::now();
    let mut scene = Scene::new(&parts, 1024, 768, Look::default());
    let build = t.elapsed();
    let mut m = Machine::new(
        set.kind,
        &set.program,
        set.melody.as_deref(),
        Options::default(),
    );
    let look = Look::default();
    let frame = CRYSTAL_HZ as u64 / 60;
    let (mut frames, mut changed, mut pixels) = (0u64, 0u64, 0u64);
    let mut spent = std::time::Duration::ZERO;
    while m.ticks() < (seconds * CRYSTAL_HZ as f64) as u64 {
        m.set_lines(lines_at(&presses, m.ticks()));
        m.run_for(frame);
        let mut any = false;
        for (o, a) in optics.iter_mut().zip(&addrs) {
            any |= o.update(a, m.segments(), 1000.0 / 60.0, &look);
        }
        frames += 1;
        if any {
            let t = std::time::Instant::now();
            let (_, rects) = scene.compose(&optics);
            spent += t.elapsed();
            changed += 1;
            pixels += rects.iter().map(|r| (r.w * r.h) as u64).sum::<u64>();
        }
    }
    println!(
        "{}: built in {:.0} ms; {frames} frames, {changed} changed ({:.0}%), {:.0} px redrawn a changed frame, {:.3} ms a changed frame",
        set.name,
        build.as_secs_f64() * 1000.0,
        changed as f64 * 100.0 / frames as f64,
        pixels as f64 / changed.max(1) as f64,
        spent.as_secs_f64() * 1000.0 / changed.max(1) as f64
    );
    Ok(())
}
