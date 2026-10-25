//! An LCD screen drawn as SVG: one group per segment, its `<title>` the segment's address
//! `x.y.z` (output row, column, common), holding the segment's filled shapes.
//!
//! Only what the screens use is read (docs/design.md, "Screen SVGs"): `g`, `path`, `rect`,
//! `polygon`, `polyline`, `circle`, `ellipse`, `transform`, fill by attribute or `style`, fill
//! and group opacity, `display: none`. Strokes are not drawn (the screens have none of width).
//! Shapes outside any addressed group are counted in [`Screen::loose`] and not drawn: on the
//! units' screens they are the canvas's own background.

use crate::path;
use tiny_skia::{Path, PathBuilder, Rect, Transform};

/// One segment: its address and the shapes that draw it.
#[derive(Clone, Debug)]
pub struct Segment {
    /// The LCD output: row x, column y, common z.
    pub addr: [u8; 3],
    /// Its colour when on.
    pub color: [u8; 3],
    /// Its opacity when fully on (the screens draw ink at about 0.85).
    pub opacity: f32,
    /// Its shapes in the screen's coordinates (the viewBox's).
    pub path: Path,
    /// Filled by the even-odd rule (else non-zero, SVG's default).
    pub even_odd: bool,
}

/// A screen: its coordinate space and its segments.
#[derive(Clone, Debug)]
pub struct Screen {
    /// The viewBox: x, y, width, height.
    pub view: [f32; 4],
    /// The segments, in drawing order.
    pub segments: Vec<Segment>,
    /// Shapes drawn outside any segment (not drawn).
    pub loose: usize,
}

impl Screen {
    /// Width over height.
    pub fn aspect(&self) -> f32 {
        self.view[2] / self.view[3]
    }
}

#[derive(Clone, Copy)]
struct Style {
    fill: Option<[u8; 3]>,
    even_odd: bool,
    fill_opacity: f32,
    opacity: f32,
    hidden: bool,
}

/// Reads a screen.
pub fn parse(text: &str) -> Result<Screen, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err("not an SVG file".into());
    }
    let view = match root.attribute("viewBox").map(path::numbers) {
        Some(v) if v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 => [v[0], v[1], v[2], v[3]],
        _ => {
            let num = |a: &str| {
                root.attribute(a)
                    .and_then(|v| path::numbers(v).first().copied())
                    .unwrap_or(0.0)
            };
            let (w, h) = (num("width"), num("height"));
            if w <= 0.0 || h <= 0.0 {
                return Err("no viewBox or size".into());
            }
            [0.0, 0.0, w, h]
        }
    };
    let mut build = Build::default();
    let style = Style {
        fill: Some([0, 0, 0]),
        even_odd: false,
        fill_opacity: 1.0,
        opacity: 1.0,
        hidden: false,
    };
    walk(root, Transform::identity(), style, None, &mut build);
    let segments = build
        .segments
        .into_iter()
        .filter_map(|(addr, color, opacity, path, even_odd)| {
            Some(Segment {
                addr,
                color,
                opacity,
                path: path?,
                even_odd,
            })
        })
        .collect();
    Ok(Screen {
        view,
        segments,
        loose: build.loose,
    })
}

/// A segment being read: address, colour, opacity, shapes so far, fill rule.
type Partial = ([u8; 3], [u8; 3], f32, Option<Path>, bool);

#[derive(Default)]
struct Build {
    segments: Vec<Partial>,
    loose: usize,
}

/// The address in a `<title>` that is exactly three numbers.
fn address(node: roxmltree::Node) -> Option<[u8; 3]> {
    let title = node.children().find(|c| c.has_tag_name("title"))?;
    let text = title.text()?.trim();
    let mut parts = text.split('.');
    let mut out = [0u8; 3];
    for o in &mut out {
        *o = parts.next()?.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

fn styled(node: roxmltree::Node, parent: Style) -> Style {
    let mut s = Style {
        opacity: 1.0,
        ..parent
    };
    let mut apply = |key: &str, value: &str| match key.trim() {
        "fill" => s.fill = path::color(value),
        "fill-opacity" => s.fill_opacity = value.trim().parse().unwrap_or(1.0),
        "fill-rule" => s.even_odd = value.trim() == "evenodd",
        "opacity" => s.opacity = value.trim().parse().unwrap_or(1.0),
        "display" => s.hidden |= value.trim() == "none",
        "visibility" => s.hidden |= value.trim() == "hidden",
        _ => {}
    };
    for a in [
        "fill",
        "fill-opacity",
        "fill-rule",
        "opacity",
        "display",
        "visibility",
    ] {
        if let Some(v) = node.attribute(a) {
            apply(a, v);
        }
    }
    if let Some(st) = node.attribute("style") {
        for decl in st.split(';') {
            if let Some((k, v)) = decl.split_once(':') {
                apply(k, v);
            }
        }
    }
    s
}

/// Walks the tree. `current` is the segment being drawn into and the product of the group
/// opacities between it and the shapes.
fn walk(
    node: roxmltree::Node,
    parent: Transform,
    parent_style: Style,
    current: Option<(usize, f32)>,
    build: &mut Build,
) {
    for child in node.children().filter(|c| c.is_element()) {
        let name = child.tag_name().name();
        if matches!(
            name,
            "defs" | "title" | "desc" | "metadata" | "namedview" | "style"
        ) {
            continue;
        }
        let ts = match child.attribute("transform") {
            Some(t) => parent.pre_concat(path::transform(t)),
            None => parent,
        };
        let style = styled(child, parent_style);
        if style.hidden {
            continue;
        }
        if name == "g" || name == "svg" {
            let into = match address(child) {
                Some(addr) => {
                    build
                        .segments
                        .push((addr, style.fill.unwrap_or([0, 0, 0]), 1.0, None, false));
                    Some((build.segments.len() - 1, style.opacity))
                }
                None => current.map(|(i, o)| (i, o * style.opacity)),
            };
            walk(child, ts, style, into, build);
            continue;
        }
        let (Some(shape), Some(fill)) = (shape(child), style.fill) else {
            continue;
        };
        let Some(shape) = shape.transform(ts) else {
            continue;
        };
        // A shape may carry its own address, as a group does.
        let current = match address(child) {
            Some(addr) => {
                build.segments.push((addr, fill, 1.0, None, false));
                Some((build.segments.len() - 1, 1.0))
            }
            None => current,
        };
        match current {
            Some((i, group_opacity)) => {
                let seg = &mut build.segments[i];
                seg.1 = fill;
                seg.2 = (style.fill_opacity * style.opacity * group_opacity).clamp(0.0, 1.0);
                seg.4 = style.even_odd;
                seg.3 = Some(match seg.3.take() {
                    Some(p) => join(&p, &shape),
                    None => shape,
                });
            }
            None => build.loose += 1,
        }
    }
}

fn join(a: &Path, b: &Path) -> Path {
    let mut pb = PathBuilder::new();
    pb.push_path(a);
    pb.push_path(b);
    pb.finish().unwrap_or_else(|| a.clone())
}

fn shape(node: roxmltree::Node) -> Option<Path> {
    let num = |a: &str| {
        node.attribute(a)
            .and_then(|v| path::numbers(v).first().copied())
    };
    let mut pb = PathBuilder::new();
    match node.tag_name().name() {
        "path" => {
            path::parse(node.attribute("d")?, &mut pb);
        }
        "rect" => {
            let r = Rect::from_xywh(
                num("x").unwrap_or(0.0),
                num("y").unwrap_or(0.0),
                num("width")?,
                num("height")?,
            )?;
            pb.push_rect(r);
        }
        "polygon" | "polyline" => {
            let pts = path::numbers(node.attribute("points")?);
            let mut it = pts.chunks_exact(2);
            let first = it.next()?;
            pb.move_to(first[0], first[1]);
            for p in it {
                pb.line_to(p[0], p[1]);
            }
            pb.close();
        }
        "circle" => {
            let r = num("r")?;
            pb.push_circle(num("cx").unwrap_or(0.0), num("cy").unwrap_or(0.0), r);
        }
        "ellipse" => {
            let (cx, cy) = (num("cx").unwrap_or(0.0), num("cy").unwrap_or(0.0));
            pb.push_oval(Rect::from_xywh(
                cx - num("rx")?,
                cy - num("ry")?,
                2.0 * num("rx")?,
                2.0 * num("ry")?,
            )?);
        }
        _ => return None,
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">
      <rect width="100" height="50" style="fill:#ffffff"/>
      <g style="fill:#000000;fill-opacity:0.8">
        <g><title>0.1.2</title><desc>x</desc><path d="m 10,10 10,0 0,10 z"/></g>
        <g transform="translate(50,0)"><title>2.0.3</title>
          <path d="M0 0h5v5h-5z"/><rect x="10" y="10" width="2" height="2"/></g>
        <path d="M0 40h3v3z" style="fill-rule:evenodd"><title>3.4.1</title></path>
      </g>
      <g style="display:none"><title>1.1.1</title><path d="M0 0h1v1z"/></g>
    </svg>"##;

    #[test]
    fn segments_by_title() {
        let s = parse(SAMPLE).unwrap();
        assert_eq!(s.view, [0.0, 0.0, 100.0, 50.0]);
        assert_eq!(s.loose, 1);
        let addrs: Vec<_> = s.segments.iter().map(|g| g.addr).collect();
        assert_eq!(addrs, vec![[0, 1, 2], [2, 0, 3], [3, 4, 1]]);
        assert!(s.segments[2].even_odd && !s.segments[0].even_odd);
        assert!((s.segments[0].opacity - 0.8).abs() < 1e-6);
        let b = s.segments[1].path.bounds();
        assert_eq!((b.left(), b.right()), (50.0, 62.0));
    }
}
