//! The two screens, on OpenGL ES 3 (desktop GL 3.3 on a computer).
//!
//! A unit's LCD is drawn by the GPU from data the scene prepares once (`inktick_art::Scene::lcd`):
//! the picture with every segment off, and each segment's cubic difference over its place, in
//! an atlas of 16-bit integer textures, with a quad per segment in a buffer made at the same
//! time. A frame draws the off picture and then the quads, each at its segment's level (an array
//! of uniforms, the only thing that changes): the brightening parts added, the darkening ones
//! subtracted (GL's ADD and REVERSE_SUBTRACT blends); an unlit segment's quad is put outside the
//! screen. The CPU does no pixel work and uploads nothing but the levels. On the handheld the
//! LCD is drawn straight into the panel's window, one render pass a frame; where the window
//! shows it at another size (a computer), it is drawn into a 1024 × 768 target first and that
//! is scaled smoothly. Menus and the touch screen's buttons are pictures made on the CPU and
//! uploaded when they change. Nothing is drawn or swapped while nothing changes, which on a
//! unit showing the time is all but once a second.
//!
//! Each screen is a window of its own where nibOS has a display per panel; on the stock OS,
//! whose Weston makes the two panels one display (the bottom on the right), one window covers
//! both and each screen is half of it; on a computer, one window shows both, stacked, at half
//! size, as nibkit lays a computer out.

use glow::HasContext;
use inktick_art::scene::Rect;
use nibkit::screens::{self, Screen};
use sdl2::video::{GLContext, GLProfile, SwapInterval, Window};

const VERTEX: &str = r#"
in vec2 corner;
uniform vec4 rect;      // x, y, w, h in clip space
out vec2 uv;
void main() {
    uv = vec2(corner.x, 1.0 - corner.y);
    gl_Position = vec4(rect.xy + corner * rect.zw, 0.0, 1.0);
}
"#;

/// Picture pixels, rows top first, to clip space. `flip` is 1 into the target, whose row 0 is
/// the picture's top row (as an uploaded picture's is, so both are shown by the same flipped
/// quad), and −1 straight into a window.
const LCD_VERTEX: &str = r#"
in vec2 corner;
uniform vec4 rect;      // x, y, w, h in picture pixels
uniform vec2 size;      // the picture's size
uniform float flip;
out vec2 local;
void main() {
    local = corner * rect.zw;
    vec2 c = (rect.xy + local) / size * 2.0 - 1.0;
    gl_Position = vec4(c.x, flip * c.y, 0.0, 1.0);
}
"#;

const OFF: &str = r#"
in vec2 local;
out vec4 colour;
uniform sampler2D image;
void main() {
    colour = vec4(texelFetch(image, ivec2(local), 0).rgb, 1.0);
}
"#;

/// A segment's share: its cubic in t = 3L, `t·d1 + t(t−1)/2·d2 + t(t−1)(t−2)/6·d3`; `sign`
/// picks the part to draw (1: what brightens, −1: what darkens). Every segment's quad is in one
/// buffer, made once: where it goes, where its differences are in the atlas, and which level is
/// its own; drawn in two calls a frame, whatever the number of segments. `LEVELS` (the number
/// of vec4s the levels take) is defined for each LCD.
const SEGMENT_VERTEX: &str = r#"
layout(location = 0) in vec2 pos;
layout(location = 1) in vec2 texel;
layout(location = 2) in float index;
uniform vec2 size;
uniform float flip;
uniform vec4 levels[LEVELS];
out vec2 atlas;
flat out vec3 weights;
void main() {
    int i = int(index);
    float l = levels[i / 4][i % 4];
    float t = 3.0 * l;
    weights = vec3(t, t * (t - 1.0) / 2.0, t * (t - 1.0) * (t - 2.0) / 6.0);
    atlas = texel;
    vec2 c = pos / size * 2.0 - 1.0;
    gl_Position = l > 0.0 ? vec4(c.x, flip * c.y, 0.0, 1.0) : vec4(2.0, 2.0, 2.0, 1.0);
}
"#;

const SEGMENT: &str = r#"
in vec2 atlas;
flat in vec3 weights;
out vec4 colour;
uniform highp isampler2D d1;
uniform highp isampler2D d2;
uniform highp isampler2D d3;
uniform float sign;
uniform float fixed_scale;
void main() {
    ivec2 p = ivec2(atlas);
    vec3 c = weights.x * vec3(texelFetch(d1, p, 0).rgb)
           + weights.y * vec3(texelFetch(d2, p, 0).rgb)
           + weights.z * vec3(texelFetch(d3, p, 0).rgb);
    colour = vec4(max(sign * c / fixed_scale, 0.0), 0.0);
}
"#;

/// The atlas's width: segments are packed in rows (shelves) across it.
const ATLAS_W: i32 = 2048;

/// One panel's LCD on the GPU.
struct LcdGpu {
    /// The target, for drawing at another size and for reading back.
    fbo: glow::Framebuffer,
    target: glow::Texture,
    off: glow::Texture,
    atlas: [glow::Texture; 3],
    /// The segments' quads, and their number of vertices.
    quads: (glow::VertexArray, glow::Buffer, i32),
    /// Its own program: the levels array is the size of its segments.
    program: glow::Program,
    at_levels: Option<glow::UniformLocation>,
    at_sign: Option<glow::UniformLocation>,
    at_flip: Option<glow::UniformLocation>,
    /// Which (screen, segment) each level is, and the levels, padded to whole vec4s.
    order: Vec<(usize, usize)>,
    levels: Vec<f32>,
}

const PLAIN: &str = r#"
in vec2 uv;
out vec4 colour;
uniform sampler2D image;
void main() {
    colour = texture(image, uv);
}
"#;

struct Target {
    window: Window,
    /// Where the screen is in the window (GL's viewport).
    viewport: (i32, i32, i32, i32),
}

/// The screens and what draws on them.
pub struct Video {
    gl: glow::Context,
    context: GLContext,
    top: Target,
    /// `None` where the top window shows both screens.
    bottom: Option<Target>,
    /// Where the bottom screen is in the top's window, when they share one.
    shared_bottom: Option<(i32, i32, i32, i32)>,
    desktop: bool,
    plain: glow::Program,
    off: glow::Program,
    off_flip: Option<glow::UniformLocation>,
    es: bool,
    quad: glow::VertexArray,
    textures: [glow::Texture; 2],
    /// Each screen's LCD, where it shows one.
    lcd: [Option<LcdGpu>; 2],
    /// Whether each screen shows its LCD (else its uploaded picture).
    showing_lcd: [bool; 2],
}

fn compile(gl: &glow::Context, es: bool, vs: &str, fs: &str) -> Result<glow::Program, String> {
    let header = if es {
        "#version 300 es\nprecision highp float;\nprecision highp int;\n"
    } else {
        "#version 330 core\n"
    };
    // SAFETY: plain GL calls on the current context, with sources this module owns.
    unsafe {
        let program = gl.create_program()?;
        for (kind, src) in [(glow::VERTEX_SHADER, vs), (glow::FRAGMENT_SHADER, fs)] {
            let shader = gl.create_shader(kind)?;
            gl.shader_source(shader, &format!("{header}{src}"));
            gl.compile_shader(shader);
            if !gl.get_shader_compile_status(shader) {
                return Err(gl.get_shader_info_log(shader));
            }
            gl.attach_shader(program, shader);
        }
        gl.bind_attrib_location(program, 0, "corner");
        gl.link_program(program);
        if !gl.get_program_link_status(program) {
            return Err(gl.get_program_info_log(program));
        }
        Ok(program)
    }
}

fn texture(gl: &glow::Context, w: i32, h: i32, smooth: bool) -> Result<glow::Texture, String> {
    let filter = if smooth { glow::LINEAR } else { glow::NEAREST } as i32;
    // SAFETY: GL calls on the current context; the texture is filled later.
    unsafe {
        let t = gl.create_texture()?;
        gl.bind_texture(glow::TEXTURE_2D, Some(t));
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, filter);
        gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, filter);
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            None,
        );
        Ok(t)
    }
}

impl Video {
    /// Opens the screens: a window on each panel on the handheld, one window on a computer.
    pub fn open(video: &sdl2::VideoSubsystem) -> Result<Video, String> {
        let bounds: Vec<screens::Rect> = (0..video.num_video_displays()?)
            .filter_map(|i| video.display_bounds(i).ok())
            .map(|r| screens::Rect::new(r.x(), r.y(), r.width(), r.height()))
            .collect();
        let placement = screens::place(&bounds, false).filter(|_| nibkit::on_device());
        let es = placement.is_some();
        {
            let attr = video.gl_attr();
            if es {
                attr.set_context_profile(GLProfile::GLES);
                attr.set_context_version(3, 0);
            } else {
                attr.set_context_profile(GLProfile::Core);
                attr.set_context_version(3, 3);
            }
            attr.set_double_buffer(true);
        }
        let window = |r: screens::Rect| {
            video
                .window("inktick", r.w, r.h)
                .position(r.x, r.y)
                .fullscreen_desktop()
                .opengl()
                .build()
                .map_err(|e| e.to_string())
        };
        let full = (0, 0, screens::WIDTH as i32, screens::HEIGHT as i32);
        let (top, bottom, shared_bottom, desktop) = match placement {
            Some(p) if !p.shared => {
                let top = window(p.region(Screen::Top).rect)?;
                let bottom = window(p.region(Screen::Bottom).rect)?;
                (
                    Target {
                        window: top,
                        viewport: full,
                    },
                    Some(Target {
                        window: bottom,
                        viewport: full,
                    }),
                    None,
                    false,
                )
            }
            Some(p) => {
                let (t, b) = (p.region(Screen::Top).rect, p.region(Screen::Bottom).rect);
                let (x0, y0) = (t.x.min(b.x), t.y.min(b.y));
                let x1 = (t.x + t.w as i32).max(b.x + b.w as i32);
                let y1 = (t.y + t.h as i32).max(b.y + b.h as i32);
                let union = screens::Rect::new(x0, y0, (x1 - x0) as u32, (y1 - y0) as u32);
                let window = window(union)?;
                let (dw, dh) = window.drawable_size();
                let (sx, sy) = (dw as f32 / union.w as f32, dh as f32 / union.h as f32);
                // GL counts rows from the bottom of the window.
                let place = |r: screens::Rect| {
                    let (x, y) = (r.x - union.x, r.y - union.y);
                    let (w, h) = ((r.w as f32 * sx) as i32, (r.h as f32 * sy) as i32);
                    (
                        (x as f32 * sx) as i32,
                        dh as i32 - (y as f32 * sy) as i32 - h,
                        w,
                        h,
                    )
                };
                (
                    Target {
                        window,
                        viewport: place(t),
                    },
                    None,
                    Some(place(b)),
                    false,
                )
            }
            None => {
                let (w, h) = (screens::WIDTH / 2, screens::HEIGHT / 2);
                let window = video
                    .window("inktick", w, h * 2)
                    .opengl()
                    .allow_highdpi()
                    .resizable()
                    .build()
                    .map_err(|e| e.to_string())?;
                let (dw, dh) = window.drawable_size();
                (
                    Target {
                        window,
                        viewport: (0, dh as i32 / 2, dw as i32, dh as i32 / 2),
                    },
                    None,
                    Some((0, 0, dw as i32, dh as i32 / 2)),
                    true,
                )
            }
        };
        let context = top.window.gl_create_context()?;
        if let Some(b) = &bottom {
            b.window.gl_make_current(&context)?;
            let _ = video.gl_set_swap_interval(SwapInterval::Immediate);
        }
        top.window.gl_make_current(&context)?;
        // Swaps don't wait: the loop paces itself by the panel's vblank, and draws only what
        // changed, so a swap never has a frame queued behind it.
        let _ = video.gl_set_swap_interval(SwapInterval::Immediate);
        // SAFETY: the loader returns GL entry points for the context just made current.
        let gl = unsafe {
            glow::Context::from_loader_function(|s| video.gl_get_proc_address(s) as *const _)
        };
        let plain = compile(&gl, es, VERTEX, PLAIN)?;
        let off = compile(&gl, es, LCD_VERTEX, OFF)?;
        let (w, h) = (screens::WIDTH as f32, screens::HEIGHT as f32);
        // SAFETY: GL calls on the current context; the off program's fixed uniforms, set once.
        let off_flip = unsafe {
            gl.use_program(Some(off));
            let u = |n| gl.get_uniform_location(off, n);
            gl.uniform_1_i32(u("image").as_ref(), 0);
            gl.uniform_2_f32(u("size").as_ref(), w, h);
            gl.uniform_4_f32(u("rect").as_ref(), 0.0, 0.0, w, h);
            u("flip")
        };
        // SAFETY: GL calls on the current context.
        let quad = unsafe {
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let corners: [f32; 8] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0, 1.0];
            let bytes: Vec<u8> = corners.iter().flat_map(|f| f.to_ne_bytes()).collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 8, 0);
            vao
        };
        let (w, h) = (screens::WIDTH as i32, screens::HEIGHT as i32);
        // On a computer the half-size window shrinks the pictures: smooth them.
        let textures = [texture(&gl, w, h, !es)?, texture(&gl, w, h, !es)?];
        Ok(Video {
            gl,
            context,
            top,
            bottom,
            shared_bottom,
            desktop,
            plain,
            off,
            off_flip,
            es,
            quad,
            textures,
            lcd: [None, None],
            showing_lcd: [false, false],
        })
    }

    /// Where a point in the window is on the bottom screen, as 0..1 (mouse clicks on a computer,
    /// where the bottom screen is the window's lower half).
    pub fn bottom_point(&self, x: i32, y: i32) -> Option<(f32, f32)> {
        if !self.desktop {
            return None;
        }
        let (w, h) = self.top.window.size();
        let y = y as f32 - h as f32 / 2.0;
        (y >= 0.0).then(|| (x as f32 / w as f32, y / (h as f32 / 2.0)))
    }

    /// Uploads the changed rectangles of a screen's picture (1024 × 768 RGBA).
    pub fn upload(&mut self, screen: Screen, rgba: &[u8], rects: &[Rect]) {
        let t = self.textures[(screen == Screen::Bottom) as usize];
        let gl = &self.gl;
        let w = screens::WIDTH as i32;
        // SAFETY: GL calls on the current context; each rectangle lies inside the picture.
        unsafe {
            self.top.window.gl_make_current(&self.context).ok();
            gl.bind_texture(glow::TEXTURE_2D, Some(t));
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, w);
            for r in rects {
                if r.w <= 0 || r.h <= 0 {
                    continue;
                }
                let start = ((r.y * w + r.x) * 4) as usize;
                gl.tex_sub_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    r.x,
                    r.y,
                    r.w,
                    r.h,
                    glow::RGBA,
                    glow::UNSIGNED_BYTE,
                    glow::PixelUnpackData::Slice(&rgba[start..]),
                );
            }
            gl.pixel_store_i32(glow::UNPACK_ROW_LENGTH, 0);
        }
    }

    /// Gives a screen its LCD (or takes it away): the scene's off picture and segments, packed
    /// for the GPU. The screen shows it from then on.
    pub fn set_lcd(
        &mut self,
        screen: Screen,
        lcd: Option<&inktick_art::Lcd>,
    ) -> Result<(), String> {
        let i = (screen == Screen::Bottom) as usize;
        let gl = &self.gl;
        self.top.window.gl_make_current(&self.context).ok();
        if let Some(old) = self.lcd[i].take() {
            // SAFETY: deleting this module's own objects on the current context.
            unsafe {
                gl.delete_framebuffer(old.fbo);
                gl.delete_texture(old.target);
                gl.delete_texture(old.off);
                for t in old.atlas {
                    gl.delete_texture(t);
                }
                gl.delete_vertex_array(old.quads.0);
                gl.delete_buffer(old.quads.1);
                gl.delete_program(old.program);
            }
        }
        self.showing_lcd[i] = lcd.is_some();
        let Some(lcd) = lcd else { return Ok(()) };
        let (w, h) = (lcd.width as i32, lcd.height as i32);
        // Shelf packing: segments left to right, a new row when one doesn't fit; each one's quad
        // (two triangles: where it goes, where it is in the atlas, its level's index) with it.
        let mut quads: Vec<f32> = Vec::new();
        let (mut x, mut y, mut row) = (0, 0, 0);
        let mut places = Vec::new();
        for (n, seg) in lcd.segments.iter().enumerate() {
            let r = seg.rect;
            let (sw, sh) = (r.w.min(ATLAS_W), r.h);
            if x + sw > ATLAS_W {
                x = 0;
                y += row;
                row = 0;
            }
            places.push((x, y));
            let (x0, y0, x1, y1) = (r.x as f32, r.y as f32, (r.x + sw) as f32, (r.y + sh) as f32);
            let (a0, b0) = (x as f32, y as f32);
            let (a1, b1) = (a0 + sw as f32, b0 + sh as f32);
            for (px, py, a, b) in [
                (x0, y0, a0, b0),
                (x1, y0, a1, b0),
                (x0, y1, a0, b1),
                (x1, y0, a1, b0),
                (x1, y1, a1, b1),
                (x0, y1, a0, b1),
            ] {
                quads.extend_from_slice(&[px, py, a, b, n as f32]);
            }
            x += sw;
            row = row.max(sh);
        }
        let atlas_h = (y + row).max(1);
        let mut planes = vec![vec![0i16; (ATLAS_W * atlas_h * 4) as usize]; 3];
        for (seg, at) in lcd.segments.iter().zip(&places) {
            for yy in 0..seg.rect.h {
                for xx in 0..seg.rect.w.min(ATLAS_W) {
                    let src = ((yy * seg.rect.w + xx) * 9) as usize;
                    let dst = (((at.1 + yy) * ATLAS_W + at.0 + xx) * 4) as usize;
                    for (k, plane) in planes.iter_mut().enumerate() {
                        for c in 0..3 {
                            plane[dst + c] = seg.rgb[src + c * 3 + k];
                        }
                    }
                }
            }
        }
        let off_rgba: Vec<u8> = lcd
            .off
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect();
        let vec4s = lcd.segments.len().div_ceil(4).max(1);
        // SAFETY: GL calls on the current context; every buffer is the size its texture needs.
        let made = unsafe {
            let most = gl.get_parameter_i32(glow::MAX_VERTEX_UNIFORM_VECTORS) as usize;
            if vec4s + 8 > most {
                return Err(format!(
                    "the LCD has {} segments, more than this GPU takes ({})",
                    lcd.segments.len(),
                    (most - 8) * 4
                ));
            }
            let program = compile(
                gl,
                self.es,
                &format!("#define LEVELS {vec4s}\n{SEGMENT_VERTEX}"),
                SEGMENT,
            )?;
            gl.use_program(Some(program));
            let u = |n| gl.get_uniform_location(program, n);
            gl.uniform_1_i32(u("d1").as_ref(), 0);
            gl.uniform_1_i32(u("d2").as_ref(), 1);
            gl.uniform_1_i32(u("d3").as_ref(), 2);
            gl.uniform_2_f32(u("size").as_ref(), w as f32, h as f32);
            gl.uniform_1_f32(u("fixed_scale").as_ref(), inktick_art::LCD_FIXED);
            let vao = gl.create_vertex_array()?;
            gl.bind_vertex_array(Some(vao));
            let vbo = gl.create_buffer()?;
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let bytes: Vec<u8> = quads.iter().flat_map(|f| f.to_ne_bytes()).collect();
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, &bytes, glow::STATIC_DRAW);
            let stride = 5 * 4;
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 2, glow::FLOAT, false, stride, 8);
            gl.enable_vertex_attrib_array(2);
            gl.vertex_attrib_pointer_f32(2, 1, glow::FLOAT, false, stride, 16);
            gl.bind_vertex_array(None);
            let target = texture(gl, w, h, !nibkit::on_device())?;
            let fbo = gl.create_framebuffer()?;
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(target),
                0,
            );
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            let off = texture(gl, w, h, false)?;
            gl.bind_texture(glow::TEXTURE_2D, Some(off));
            gl.tex_sub_image_2d(
                glow::TEXTURE_2D,
                0,
                0,
                0,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelUnpackData::Slice(&off_rgba),
            );
            let mut atlas = Vec::new();
            for plane in &planes {
                let t = gl.create_texture()?;
                gl.bind_texture(glow::TEXTURE_2D, Some(t));
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MIN_FILTER,
                    glow::NEAREST as i32,
                );
                gl.tex_parameter_i32(
                    glow::TEXTURE_2D,
                    glow::TEXTURE_MAG_FILTER,
                    glow::NEAREST as i32,
                );
                let bytes: Vec<u8> = plane.iter().flat_map(|v| v.to_ne_bytes()).collect();
                gl.tex_image_2d(
                    glow::TEXTURE_2D,
                    0,
                    glow::RGBA16I as i32,
                    ATLAS_W,
                    atlas_h,
                    0,
                    glow::RGBA_INTEGER,
                    glow::SHORT,
                    Some(&bytes),
                );
                atlas.push(t);
            }
            LcdGpu {
                fbo,
                target,
                off,
                atlas: [atlas[0], atlas[1], atlas[2]],
                quads: (vao, vbo, (quads.len() / 5) as i32),
                program,
                at_levels: u("levels"),
                at_sign: u("sign"),
                at_flip: u("flip"),
                order: lcd.segments.iter().map(|s| (s.screen, s.segment)).collect(),
                levels: vec![0.0; vec4s * 4],
            }
        };
        self.lcd[i] = Some(made);
        Ok(())
    }

    /// What the GPU draws for a screen's LCD, rows top first (INKTICK_DEBUG=shot).
    pub fn read_lcd(&self, screen: Screen) -> Option<Vec<u8>> {
        let lcd = self.lcd[(screen == Screen::Bottom) as usize].as_ref()?;
        let (w, h) = (screens::WIDTH as i32, screens::HEIGHT as i32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        self.top.window.gl_make_current(&self.context).ok();
        // SAFETY: reads this module's framebuffer into a buffer of exactly w*h*4 bytes.
        unsafe {
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, Some(lcd.fbo));
            self.render(lcd, (0, 0, w, h), 1.0);
            self.gl.read_pixels(
                0,
                0,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut px),
            );
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        }
        // The target's row 0 is the picture's top row already.
        Some(px)
    }

    /// What a screen's window shows, drawn again and read before the swap, rows top first, where
    /// the screen has a window of its own at its own size (INKTICK_DEBUG=shot on the handheld).
    pub fn read_window(&self, screen: Screen) -> Option<Vec<u8>> {
        let (w, h) = (screens::WIDTH as i32, screens::HEIGHT as i32);
        let target = match screen {
            Screen::Top => Some(&self.top),
            Screen::Bottom => self.bottom.as_ref(),
        }?;
        if (target.viewport.2, target.viewport.3) != (w, h) {
            return None;
        }
        target.window.gl_make_current(&self.context).ok();
        let mut px = vec![0u8; (w * h * 4) as usize];
        self.draw(screen, target.viewport);
        // SAFETY: reads the window's back buffer into a buffer of exactly w*h*4 bytes.
        unsafe {
            self.gl.read_pixels(
                target.viewport.0,
                target.viewport.1,
                w,
                h,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(&mut px),
            );
        }
        // GL's rows are bottom first.
        let row = (w * 4) as usize;
        Some(px.chunks_exact(row).rev().flatten().copied().collect())
    }

    /// Shows the screen's LCD, or its uploaded picture (the menu, the touch buttons).
    pub fn show_lcd(&mut self, screen: Screen, lcd: bool) {
        let i = (screen == Screen::Bottom) as usize;
        self.showing_lcd[i] = lcd && self.lcd[i].is_some();
    }

    /// Sets a screen's LCD to the segments' darkness, `level(screen, segment)`, for the next
    /// [`Video::present`].
    pub fn draw_lcd(&mut self, screen: Screen, level: impl Fn(usize, usize) -> f32) {
        if let Some(lcd) = self.lcd[(screen == Screen::Bottom) as usize].as_mut() {
            for (l, (s, n)) in lcd.levels.iter_mut().zip(&lcd.order) {
                *l = level(*s, *n);
            }
        }
    }

    /// Draws an LCD into the bound framebuffer at `viewport`, `flip` as [`LCD_VERTEX`] has it.
    ///
    /// # Safety
    /// The context is current and `lcd` is one of this module's.
    unsafe fn render(&self, lcd: &LcdGpu, viewport: (i32, i32, i32, i32), flip: f32) {
        let gl = &self.gl;
        gl.viewport(viewport.0, viewport.1, viewport.2, viewport.3);
        gl.disable(glow::BLEND);
        gl.use_program(Some(self.off));
        gl.uniform_1_f32(self.off_flip.as_ref(), flip);
        gl.active_texture(glow::TEXTURE0);
        gl.bind_texture(glow::TEXTURE_2D, Some(lcd.off));
        gl.bind_vertex_array(Some(self.quad));
        gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        gl.use_program(Some(lcd.program));
        gl.uniform_1_f32(lcd.at_flip.as_ref(), flip);
        gl.uniform_4_f32_slice(lcd.at_levels.as_ref(), &lcd.levels);
        for (k, t) in lcd.atlas.iter().enumerate() {
            gl.active_texture(glow::TEXTURE0 + k as u32);
            gl.bind_texture(glow::TEXTURE_2D, Some(*t));
        }
        gl.bind_vertex_array(Some(lcd.quads.0));
        gl.enable(glow::BLEND);
        // Colour only: the target's alpha stays opaque.
        gl.blend_func_separate(glow::ONE, glow::ONE, glow::ZERO, glow::ONE);
        for (equation, s) in [(glow::FUNC_ADD, 1.0), (glow::FUNC_REVERSE_SUBTRACT, -1.0)] {
            gl.blend_equation(equation);
            gl.uniform_1_f32(lcd.at_sign.as_ref(), s);
            gl.draw_arrays(glow::TRIANGLES, 0, lcd.quads.2);
        }
        gl.blend_equation(glow::FUNC_ADD);
        gl.disable(glow::BLEND);
        gl.active_texture(glow::TEXTURE0);
    }

    fn draw(&self, screen: Screen, viewport: (i32, i32, i32, i32)) {
        let gl = &self.gl;
        let i = (screen == Screen::Bottom) as usize;
        let (w, h) = (screens::WIDTH as i32, screens::HEIGHT as i32);
        // SAFETY: GL calls on the current context with this module's objects.
        unsafe {
            let source = match (&self.lcd[i], self.showing_lcd[i]) {
                // At its own size, straight into the window.
                (Some(l), true) if (viewport.2, viewport.3) == (w, h) => {
                    self.render(l, viewport, -1.0);
                    return;
                }
                // At another size, through the target, scaled smoothly.
                (Some(l), true) => {
                    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(l.fbo));
                    self.render(l, (0, 0, w, h), 1.0);
                    gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                    l.target
                }
                _ => self.textures[i],
            };
            gl.viewport(viewport.0, viewport.1, viewport.2, viewport.3);
            gl.use_program(Some(self.plain));
            gl.active_texture(glow::TEXTURE0);
            gl.bind_texture(glow::TEXTURE_2D, Some(source));
            gl.uniform_1_i32(gl.get_uniform_location(self.plain, "image").as_ref(), 0);
            let at = gl.get_uniform_location(self.plain, "rect");
            gl.uniform_4_f32(at.as_ref(), -1.0, -1.0, 2.0, 2.0);
            gl.bind_vertex_array(Some(self.quad));
            gl.draw_arrays(glow::TRIANGLE_STRIP, 0, 4);
        }
    }

    /// Shows the screens whose pictures changed.
    pub fn present(&mut self, top: bool, bottom: bool) {
        if let (Some(b), true) = (&self.bottom, bottom) {
            b.window.gl_make_current(&self.context).ok();
            self.draw(Screen::Bottom, b.viewport);
            b.window.gl_swap_window();
        }
        if top || (bottom && self.shared_bottom.is_some()) {
            self.top.window.gl_make_current(&self.context).ok();
            if self.desktop {
                // The window may have been resized: both halves follow it.
                let (dw, dh) = self.top.window.drawable_size();
                self.top.viewport = (0, dh as i32 / 2, dw as i32, dh as i32 / 2);
                self.shared_bottom = Some((0, 0, dw as i32, dh as i32 / 2));
            }
            self.draw(Screen::Top, self.top.viewport);
            if let Some(v) = self.shared_bottom {
                self.draw(Screen::Bottom, v);
            }
            self.top.window.gl_swap_window();
        }
    }
}
