//! RGBA pictures: artwork decoded from PNG, resampled to the size it is shown at.

/// An 8-bit RGBA picture, not premultiplied.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Rows top to bottom, 4 bytes a pixel.
    pub pixels: Vec<u8>,
}

impl Image {
    /// A picture of one colour.
    pub fn filled(width: usize, height: usize, rgba: [u8; 4]) -> Image {
        Image {
            width,
            height,
            pixels: rgba.repeat(width * height),
        }
    }

    /// Decodes a PNG of any colour type and depth into RGBA8.
    pub fn from_png(data: &[u8]) -> Result<Image, String> {
        let mut dec = png::Decoder::new(data);
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = dec.read_info().map_err(|e| e.to_string())?;
        let mut buf = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
        let (w, h) = (info.width as usize, info.height as usize);
        let src = &buf[..info.buffer_size()];
        let pixels = match info.color_type {
            png::ColorType::Rgba => src.to_vec(),
            png::ColorType::Rgb => src
                .chunks_exact(3)
                .flat_map(|p| [p[0], p[1], p[2], 255])
                .collect(),
            png::ColorType::Grayscale => src.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            png::ColorType::GrayscaleAlpha => src
                .chunks_exact(2)
                .flat_map(|p| [p[0], p[0], p[0], p[1]])
                .collect(),
            png::ColorType::Indexed => return Err("indexed PNG was not expanded".into()),
        };
        Ok(Image {
            width: w,
            height: h,
            pixels,
        })
    }

    /// Encodes as PNG.
    pub fn to_png(&self) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, self.width as u32, self.height as u32);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            if let Ok(mut w) = enc.write_header() {
                let _ = w.write_image_data(&self.pixels);
            }
        }
        out
    }

    /// The pixel at (x, y).
    pub fn at(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width + x) * 4;
        [
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ]
    }

    /// Mirrored left to right.
    pub fn flipped_x(&self) -> Image {
        let mut out = self.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let (s, d) = (
                    (y * self.width + x) * 4,
                    (y * self.width + self.width - 1 - x) * 4,
                );
                out.pixels[d..d + 4].copy_from_slice(&self.pixels[s..s + 4]);
            }
        }
        out
    }

    /// Mirrored top to bottom.
    pub fn flipped_y(&self) -> Image {
        let mut out = self.clone();
        let row = self.width * 4;
        for y in 0..self.height {
            let d = (self.height - 1 - y) * row;
            out.pixels[d..d + row].copy_from_slice(&self.pixels[y * row..y * row + row]);
        }
        out
    }

    /// Turned a quarter clockwise.
    pub fn rotated_90(&self) -> Image {
        let (w, h) = (self.height, self.width);
        let mut out = Image::filled(w, h, [0; 4]);
        for y in 0..self.height {
            for x in 0..self.width {
                let s = (y * self.width + x) * 4;
                let (dx, dy) = (self.height - 1 - y, x);
                let d = (dy * w + dx) * 4;
                out.pixels[d..d + 4].copy_from_slice(&self.pixels[s..s + 4]);
            }
        }
        out
    }

    /// Resampled to `width` × `height`: a triangle filter as wide as the scale needs, so a big
    /// picture shrinks without aliasing and a small one grows smoothly. Colour is weighted by
    /// alpha, so transparent pixels don't darken the edges.
    pub fn resized(&self, width: usize, height: usize) -> Image {
        if width == self.width && height == self.height {
            return self.clone();
        }
        let wx = weights(self.width, width);
        let wy = weights(self.height, height);
        // Horizontal pass into premultiplied f32.
        let mut tmp = vec![0f32; width * self.height * 4];
        for y in 0..self.height {
            for (x, (start, ws)) in wx.iter().enumerate() {
                let mut acc = [0f32; 4];
                for (k, w) in ws.iter().enumerate() {
                    let p = self.at(start + k, y);
                    let a = p[3] as f32 * w;
                    acc[0] += p[0] as f32 * a;
                    acc[1] += p[1] as f32 * a;
                    acc[2] += p[2] as f32 * a;
                    acc[3] += a;
                }
                tmp[(y * width + x) * 4..][..4].copy_from_slice(&acc);
            }
        }
        let mut out = Image::filled(width, height, [0; 4]);
        for (y, (start, ws)) in wy.iter().enumerate() {
            for x in 0..width {
                let mut acc = [0f32; 4];
                for (k, w) in ws.iter().enumerate() {
                    let t = &tmp[((start + k) * width + x) * 4..][..4];
                    for c in 0..4 {
                        acc[c] += t[c] * w;
                    }
                }
                let a = acc[3];
                let px = &mut out.pixels[(y * width + x) * 4..][..4];
                if a > 0.0 {
                    for c in 0..3 {
                        px[c] = (acc[c] / a).round().clamp(0.0, 255.0) as u8;
                    }
                }
                px[3] = a.round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    }
}

/// For each output pixel, the first source pixel it reads and the weights from there.
fn weights(from: usize, to: usize) -> Vec<(usize, Vec<f32>)> {
    let scale = from as f32 / to as f32;
    let support = scale.max(1.0);
    (0..to)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale;
            let lo = ((center - support).floor().max(0.0)) as usize;
            let hi = ((center + support).ceil() as usize).min(from);
            let mut ws: Vec<f32> = (lo..hi)
                .map(|j| (1.0 - ((j as f32 + 0.5 - center) / support).abs()).max(0.0))
                .collect();
            let sum: f32 = ws.iter().sum();
            if sum > 0.0 {
                ws.iter_mut().for_each(|w| *w /= sum);
            } else {
                let nearest = (center as usize).min(from - 1);
                return (nearest, vec![1.0]);
            }
            (lo, ws)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resizing_keeps_a_flat_colour() {
        let img = Image::filled(37, 23, [10, 200, 30, 255]);
        let r = img.resized(11, 50);
        assert!(r.pixels.chunks(4).all(|p| p == [10, 200, 30, 255]));
    }

    #[test]
    fn png_round_trip() {
        let mut img = Image::filled(3, 2, [1, 2, 3, 4]);
        img.pixels[0] = 99;
        assert_eq!(Image::from_png(&img.to_png()).unwrap(), img);
    }

    #[test]
    fn rotation_and_flips() {
        let mut img = Image::filled(2, 1, [0; 4]);
        img.pixels[0] = 1;
        assert_eq!(img.rotated_90().at(0, 0)[0], 1);
        assert_eq!(img.flipped_x().at(1, 0)[0], 1);
    }
}
