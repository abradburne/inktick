//! A unit's screens and artwork, drawn.
//!
//! - [`svg`]: an SVG screen, read into its segments, each a filled shape addressed `x.y.z` by
//!   its `<title>`, as the units' screen files are made.
//! - [`layout`]: a MAME layout file (`default.lay`): elements, views and their items, read from
//!   MAME's published layout-file documentation.
//! - [`scene`]: a view laid out for an output size, composed with the screens' segments, their
//!   optics (fade-in, fade-out, shadow) and the artwork.

pub mod image;
pub mod layout;
pub mod optics;
mod path;
pub mod scene;
pub mod svg;

pub use image::Image;
pub use optics::{Look, Optics};
pub use scene::{Lcd, LcdSegment, Rect, Scene, LCD_FIXED};
pub use svg::Screen;
