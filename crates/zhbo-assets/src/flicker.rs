//! The in-game flicker check (`scripts/autotest-mac.sh --flicker`): consecutive captures of a still
//! scene, or of one nudged by a fraction of a pixel a frame, must agree. A correct picture changes
//! by a fraction of a pixel's contrast at its edges; z-fighting (coplanar or near-coplanar surfaces
//! without the depth precision or bias to separate them) flips whole patches between two surfaces
//! from one frame to the next, at full contrast.

use std::path::Path;

/// A capture's pixels, RGB.
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
}

/// The region compared, in pixels: `[x0, y0, x1, y1)`.
pub type Rect = [usize; 4];

/// How much a channel must change for a pixel to count as flipped (of 255). A fifth of a pixel's
/// nudge moves an edge pixel by about a fifth of the edge's contrast; a flipped patch changes by
/// the difference between the two surfaces, far more than this.
pub const FLIP_THRESHOLD: u8 = 64;

/// Read a PNG capture (8-bit RGB or RGBA).
pub fn read_png(path: &Path) -> Result<Frame, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    let (width, height) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        other => return Err(format!("{}: colour type {other:?}", path.display())),
    };
    let mut rgb = Vec::with_capacity(width * height * 3);
    for px in buf[..width * height * channels].chunks_exact(channels) {
        match channels {
            1 | 2 => rgb.extend_from_slice(&[px[0], px[0], px[0]]),
            _ => rgb.extend_from_slice(&px[..3]),
        }
    }
    Ok(Frame { width, height, rgb })
}

/// How many pixels in `rect` changed by more than `threshold` in any channel between two frames.
/// Frames of different sizes, or a rect past either frame, compare as every pixel flipped.
pub fn flipped(a: &Frame, b: &Frame, rect: Rect, threshold: u8) -> usize {
    let [x0, y0, x1, y1] = rect;
    if a.width != b.width || a.height != b.height || x1 > a.width || y1 > a.height || x0 >= x1 || y0 >= y1 {
        return usize::MAX;
    }
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * a.width + x) * 3;
            if (0..3).any(|c| a.rgb[i + c].abs_diff(b.rgb[i + c]) > threshold) {
                n += 1;
            }
        }
    }
    n
}

/// How many pixels in `rect` glow the tritium's green: bright (green at least 160) and saturated
/// (green at least 60 over red and blue). The MP7's sight dots are this colour and nothing else in
/// the flicker scene is (its grass is dark and desaturated); a dot that loses to the sight under it
/// drops out of the count.
pub fn tritium_glow(f: &Frame, rect: Rect) -> usize {
    let [x0, y0, x1, y1] = rect;
    let (x1, y1) = (x1.min(f.width), y1.min(f.height));
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * f.width + x) * 3;
            let (r, g, b) = (f.rgb[i], f.rgb[i + 1], f.rgb[i + 2]);
            if g >= 160 && g.saturating_sub(r.max(b)) >= 60 {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(f: impl Fn(usize, usize) -> u8) -> Frame {
        let (width, height) = (32, 16);
        let mut rgb = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let v = f(x, y);
                rgb.extend_from_slice(&[v, v, v]);
            }
        }
        Frame { width, height, rgb }
    }

    #[test]
    fn a_nudged_edge_is_not_a_flip_but_a_flipped_patch_is() {
        // An edge at x = 16 moved by a fifth of a pixel: the edge column changes by a fifth of the
        // contrast.
        let a = frame(|x, _| if x < 16 { 40 } else { 200 });
        let b = frame(|x, _| if x < 16 { 40 } else if x == 16 { 168 } else { 200 });
        assert_eq!(flipped(&a, &b, [0, 0, 32, 16], FLIP_THRESHOLD), 0);
        // A patch where the other surface won this frame (road over grass): every pixel of it.
        let c = frame(|x, y| if (4..8).contains(&x) && (2..6).contains(&y) { 200 } else if x < 16 { 40 } else { 200 });
        assert_eq!(flipped(&a, &c, [0, 0, 32, 16], FLIP_THRESHOLD), 16);
        // Only the rect counts.
        assert_eq!(flipped(&a, &c, [8, 0, 32, 16], FLIP_THRESHOLD), 0);
        // The tritium's green counts; the grass and the gun do not.
        let mut g = frame(|_, _| 60);
        for (i, px) in g.rgb.as_chunks_mut::<3>().0.iter_mut().enumerate() {
            if i % 32 < 4 && i / 32 < 2 {
                *px = [80, 220, 90];
            } else if i % 32 > 20 {
                *px = [90, 140, 60];
            }
        }
        assert_eq!(tritium_glow(&g, [0, 0, 32, 16]), 8);
        // A mismatch never passes.
        assert_eq!(flipped(&a, &c, [0, 0, 64, 16], FLIP_THRESHOLD), usize::MAX);
    }
}
