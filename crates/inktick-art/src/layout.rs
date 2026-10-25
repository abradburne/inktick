//! MAME layout files, as the artwork packs ship them (`default.lay`).
//!
//! Read from MAME's layout file documentation (`docs/source/techspecs/layout_files.rst`; its
//! user documentation, not its source): elements built of image and rectangle components,
//! each component optionally drawn only in one element state; views of element and screen
//! items with bounds, a modifier colour, a blend mode and an orientation, drawn in file order
//! (the first at the back); groups instantiated into bounds; collections, shown by default
//! unless `visible="no"`. Parameters, repeats and element types other than `image` and `rect`
//! are not read; nothing in the units' artwork uses them.

use crate::path;
use std::collections::HashMap;

/// A rectangle in layout coordinates.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bounds {
    /// Left.
    pub x: f32,
    /// Top.
    pub y: f32,
    /// Width.
    pub w: f32,
    /// Height.
    pub h: f32,
}

impl Bounds {
    /// The unit square, every bounds' default.
    pub const UNIT: Bounds = Bounds {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// The smallest rectangle holding both.
    pub fn union(self, o: Bounds) -> Bounds {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let r = (self.x + self.w).max(o.x + o.w);
        let b = (self.y + self.h).max(o.y + o.h);
        Bounds {
            x,
            y,
            w: r - x,
            h: b - y,
        }
    }

    /// Whether they overlap.
    pub fn overlaps(self, o: Bounds) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }

    /// The area.
    pub fn area(self) -> f32 {
        self.w * self.h
    }

    /// `inner` (in the coordinates `from` spans) placed into `self`.
    fn map(self, from: Bounds, inner: Bounds) -> Bounds {
        let sx = if from.w > 0.0 { self.w / from.w } else { 1.0 };
        let sy = if from.h > 0.0 { self.h / from.h } else { 1.0 };
        Bounds {
            x: self.x + (inner.x - from.x) * sx,
            y: self.y + (inner.y - from.y) * sy,
            w: inner.w * sx,
            h: inner.h * sy,
        }
    }
}

/// An RGBA modifier, each channel 0 to 1 (omitted channels are 1).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color(pub [f32; 4]);

impl Color {
    /// No change.
    pub const WHITE: Color = Color([1.0; 4]);

    fn times(self, o: Color) -> Color {
        let mut c = self.0;
        for (a, b) in c.iter_mut().zip(o.0) {
            *a *= b;
        }
        Color(c)
    }
}

/// How an item is combined with what is behind it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blend {
    /// Replaces.
    None,
    /// Alpha blending (the default for elements, and here for screens).
    Alpha,
    /// RGB multiplication.
    Multiply,
    /// Additive.
    Add,
}

/// Rotation then mirroring.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Orientation {
    /// Clockwise quarter turns, 0-3.
    pub quarter_turns: u8,
    /// Mirrored along the diagonal.
    pub swap_xy: bool,
    /// Mirrored left to right.
    pub flip_x: bool,
    /// Mirrored top to bottom.
    pub flip_y: bool,
}

/// A part of an element.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    /// A picture file in the artwork archive.
    Image {
        /// Its file.
        file: String,
        /// A file whose brightness becomes the picture's alpha.
        alpha_file: Option<String>,
    },
    /// A rectangle of the component's colour.
    Rect,
}

/// A component of an element.
#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    /// What it draws.
    pub part: Part,
    /// Where, in the element's coordinates.
    pub bounds: Bounds,
    /// Its colour.
    pub color: Color,
    /// Drawn only in this element state, if given.
    pub state: Option<i32>,
}

/// An element: components drawn in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    /// The components.
    pub components: Vec<Component>,
    /// The state when not connected to anything.
    pub default_state: i32,
}

impl Element {
    /// The element's own coordinate space: its components' union.
    pub fn bounds(&self) -> Bounds {
        self.components
            .iter()
            .map(|c| c.bounds)
            .reduce(Bounds::union)
            .unwrap_or(Bounds::UNIT)
    }
}

/// What a view item shows.
#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// An element, by name, in a state.
    Element {
        /// Its name.
        name: String,
        /// The state it is drawn in.
        state: i32,
    },
    /// Screen number n.
    Screen(usize),
}

/// An item tied to one of the machine's inputs (a clickable button in the artwork).
#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    /// The input port's tag (`IN.2`, `BA`, `B`).
    pub tag: String,
    /// The bit within it.
    pub mask: u32,
}

/// One item of a view, flattened out of groups and collections.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The input it is drawn for, if any.
    pub input: Option<Input>,
    /// What it shows.
    pub what: What,
    /// Where, in view coordinates.
    pub bounds: Bounds,
    /// Its modifier colour.
    pub color: Color,
    /// Its blend mode.
    pub blend: Blend,
    /// Its orientation.
    pub orientation: Orientation,
}

/// A view: a way to show the unit.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    /// Its name, as the menu shows it.
    pub name: String,
    /// The part of the view shown (the items' union when the file gives none).
    pub bounds: Bounds,
    /// The items, back to front.
    pub items: Vec<Item>,
}

impl View {
    /// The screens it shows.
    pub fn screens(&self) -> Vec<usize> {
        let mut s: Vec<usize> = self
            .items
            .iter()
            .filter_map(|i| match i.what {
                What::Screen(n) => Some(n),
                _ => None,
            })
            .collect();
        s.sort_unstable();
        s.dedup();
        s
    }

    /// The bounds of screen `n` (the first item showing it).
    pub fn screen_bounds(&self, n: usize) -> Option<Bounds> {
        self.items
            .iter()
            .find(|i| i.what == What::Screen(n))
            .map(|i| i.bounds)
    }
}

/// A layout file.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// Elements by name.
    pub elements: HashMap<String, Element>,
    /// The views, in file order.
    pub views: Vec<View>,
}

impl Layout {
    /// Reads a layout file.
    pub fn parse(text: &str) -> Result<Layout, String> {
        let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
        let root = doc.root_element();
        if root.tag_name().name() != "mamelayout" {
            return Err("not a MAME layout".into());
        }
        let mut layout = Layout::default();
        let mut groups = HashMap::new();
        for node in root.children().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "element" => {
                    if let Some(name) = node.attribute("name") {
                        layout.elements.insert(name.to_string(), element(node));
                    }
                }
                "group" => {
                    if let Some(name) = node.attribute("name") {
                        groups.insert(name.to_string(), node);
                    }
                }
                _ => {}
            }
        }
        for node in root.children().filter(|n| n.has_tag_name("view")) {
            let mut items = Vec::new();
            collect(node, &groups, &layout.elements, &mut items, 0);
            let bounds = child_bounds(node)
                .or_else(|| items.iter().map(|i| i.bounds).reduce(Bounds::union))
                .unwrap_or(Bounds::UNIT);
            layout.views.push(View {
                name: node.attribute("name").unwrap_or("").to_string(),
                bounds,
                items,
            });
        }
        Ok(layout)
    }

    /// The view named `name`, or else the first.
    pub fn view(&self, name: &str) -> Option<&View> {
        self.views
            .iter()
            .find(|v| v.name == name)
            .or(self.views.first())
    }

    /// Every picture file the layout draws.
    pub fn files(&self) -> Vec<&str> {
        let mut out: Vec<&str> = self
            .elements
            .values()
            .flat_map(|e| e.components.iter())
            .flat_map(|c| match &c.part {
                Part::Image { file, alpha_file } => {
                    let mut v = vec![file.as_str()];
                    v.extend(alpha_file.as_deref());
                    v
                }
                Part::Rect => vec![],
            })
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn element(node: roxmltree::Node) -> Element {
    let mut components = Vec::new();
    for c in node.children().filter(|c| c.is_element()) {
        let part = match c.tag_name().name() {
            "image" => match c.attribute("file") {
                Some(f) => Part::Image {
                    file: f.to_string(),
                    alpha_file: c.attribute("alphafile").map(str::to_string),
                },
                None => continue,
            },
            "rect" => Part::Rect,
            _ => continue,
        };
        components.push(Component {
            part,
            bounds: child_bounds(c).unwrap_or(Bounds::UNIT),
            color: child_color(c),
            state: c.attribute("state").and_then(|s| s.parse().ok()),
        });
    }
    Element {
        components,
        default_state: node
            .attribute("defstate")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
    }
}

/// Appends a view's or group's items, flattening groups and visible collections.
fn collect(
    node: roxmltree::Node,
    groups: &HashMap<String, roxmltree::Node>,
    elements: &HashMap<String, Element>,
    out: &mut Vec<Item>,
    depth: usize,
) {
    if depth > 16 {
        return;
    }
    for c in node.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "element" => {
                let Some(name) = c.attribute("ref") else {
                    continue;
                };
                // Items tied to an input or output show their state 0 (nothing pressed);
                // others their element's default state.
                let connected = c.attribute("inputtag").is_some() || c.attribute("name").is_some();
                let state = if connected {
                    0
                } else {
                    elements.get(name).map_or(0, |e| e.default_state)
                };
                out.push(item(
                    c,
                    What::Element {
                        name: name.to_string(),
                        state,
                    },
                    Blend::Alpha,
                ));
            }
            "screen" => {
                let Some(n) = c.attribute("index").and_then(|s| s.parse().ok()) else {
                    continue;
                };
                out.push(item(c, What::Screen(n), Blend::Alpha));
            }
            "collection" => {
                if c.attribute("visible") != Some("no") {
                    collect(c, groups, elements, out, depth + 1);
                }
            }
            "group" => {
                let Some(g) = c.attribute("ref").and_then(|r| groups.get(r)) else {
                    continue;
                };
                let mut inner = Vec::new();
                collect(*g, groups, elements, &mut inner, depth + 1);
                let own = child_bounds(*g)
                    .or_else(|| inner.iter().map(|i| i.bounds).reduce(Bounds::union))
                    .unwrap_or(Bounds::UNIT);
                let dest = child_bounds(c);
                let tint = child_color(c);
                for mut i in inner {
                    if let Some(d) = dest {
                        i.bounds = d.map(own, i.bounds);
                    }
                    i.color = i.color.times(tint);
                    out.push(i);
                }
            }
            _ => {}
        }
    }
}

fn item(c: roxmltree::Node, what: What, default_blend: Blend) -> Item {
    let blend = match c.attribute("blend") {
        Some("none") => Blend::None,
        Some("alpha") => Blend::Alpha,
        Some("multiply") => Blend::Multiply,
        Some("add") => Blend::Add,
        _ => default_blend,
    };
    let orientation = c
        .children()
        .find(|n| n.has_tag_name("orientation"))
        .map(|o| Orientation {
            quarter_turns: match o.attribute("rotate") {
                Some("90") => 1,
                Some("180") => 2,
                Some("270") => 3,
                _ => 0,
            },
            swap_xy: o.attribute("swapxy") == Some("yes"),
            flip_x: o.attribute("flipx") == Some("yes"),
            flip_y: o.attribute("flipy") == Some("yes"),
        })
        .unwrap_or_default();
    let input = c.attribute("inputtag").map(|tag| Input {
        tag: tag.to_string(),
        mask: c
            .attribute("inputmask")
            .and_then(|m| u32::from_str_radix(m.trim_start_matches("0x"), 16).ok())
            .unwrap_or(0),
    });
    Item {
        input,
        what,
        bounds: child_bounds(c).unwrap_or(Bounds::UNIT),
        color: child_color(c),
        blend,
        orientation,
    }
}

fn child_bounds(node: roxmltree::Node) -> Option<Bounds> {
    let b = node.children().find(|n| n.has_tag_name("bounds"))?;
    let f = |a: &str| {
        b.attribute(a)
            .and_then(|v| path::numbers(v).first().copied())
    };
    let (x, w) = match (f("x"), f("xc"), f("left"), f("right"), f("width")) {
        (Some(x), _, _, _, w) => (x, w.unwrap_or(1.0)),
        (None, Some(xc), _, _, w) => {
            let w = w.unwrap_or(1.0);
            (xc - w / 2.0, w)
        }
        (None, None, Some(l), r, _) => (l, r.map_or(1.0, |r| r - l)),
        _ => (0.0, f("width").unwrap_or(1.0)),
    };
    let (y, h) = match (f("y"), f("yc"), f("top"), f("bottom"), f("height")) {
        (Some(y), _, _, _, h) => (y, h.unwrap_or(1.0)),
        (None, Some(yc), _, _, h) => {
            let h = h.unwrap_or(1.0);
            (yc - h / 2.0, h)
        }
        (None, None, Some(t), b, _) => (t, b.map_or(1.0, |b| b - t)),
        _ => (0.0, f("height").unwrap_or(1.0)),
    };
    Some(Bounds { x, y, w, h })
}

fn child_color(node: roxmltree::Node) -> Color {
    let Some(c) = node.children().find(|n| n.has_tag_name("color")) else {
        return Color::WHITE;
    };
    let f = |a: &str| {
        c.attribute(a)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0)
    };
    Color([f("red"), f("green"), f("blue"), f("alpha")].map(|v| v.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAY: &str = r#"<?xml version="1.0"?><mamelayout version="2">
      <element name="bg"><image file="bg.png"/></element>
      <element name="black"><rect><color red="0" green="0" blue="0"/></rect></element>
      <element name="btn" defstate="1"><image file="b.png" state="1"/></element>
      <group name="g"><element ref="black"><bounds x="0" y="0" width="10" height="10"/></element></group>
      <view name="One">
        <bounds x="0" y="0" width="100" height="50"/>
        <element ref="bg"><bounds left="0" top="0" right="100" bottom="50"/></element>
        <screen index="0" blend="multiply"><bounds xc="50" yc="25" width="40" height="20"/></screen>
        <element ref="btn" inputtag="IN.0" inputmask="0x01"><bounds x="1" y="1" width="2" height="2"/></element>
        <collection name="Off" visible="no"><element ref="bg"/></collection>
        <group ref="g"><bounds x="90" y="40" width="5" height="5"/><color alpha="0.5"/></group>
      </view>
    </mamelayout>"#;

    #[test]
    fn views_items_and_groups() {
        let l = Layout::parse(LAY).unwrap();
        let v = l.view("One").unwrap();
        assert_eq!(
            v.bounds,
            Bounds {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 50.0
            }
        );
        assert_eq!(v.items.len(), 4);
        assert_eq!(v.items[1].what, What::Screen(0));
        assert_eq!(v.items[1].blend, Blend::Multiply);
        assert_eq!(
            v.items[1].bounds,
            Bounds {
                x: 30.0,
                y: 15.0,
                w: 40.0,
                h: 20.0
            }
        );
        assert_eq!(
            v.items[2].what,
            What::Element {
                name: "btn".into(),
                state: 0
            }
        );
        assert_eq!(
            v.items[3].bounds,
            Bounds {
                x: 90.0,
                y: 40.0,
                w: 5.0,
                h: 5.0
            }
        );
        assert_eq!(v.items[3].color.0[3], 0.5);
        assert_eq!(
            l.elements["black"].components[0].color.0,
            [0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(l.files(), vec!["b.png", "bg.png"]);
    }
}
