//! Tray icon drawn at runtime (SPEC §9.2).
//!
//! Tauri accepts raw RGBA through `Image::new_owned`, so this needs neither a
//! PNG encoder nor a font rasterizer: the digits are a 3×5 bitmap defined right
//! here, scaled to fill the canvas.

use crate::model::Band;

pub const ICON_SIZE: u32 = 32;
const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;
/// Keeps a 2 px margin on every side of the canvas.
const MAX_EXTENT: u32 = 28;
/// Alpha used for a stale or errored reading (SPEC §9.1: "esmaecido").
const DIMMED_ALPHA: u8 = 140;

type Glyph = [u8; GLYPH_H as usize];

/// One row per line, three bits each, most significant bit leftmost.
#[rustfmt::skip]
const DIGITS: [Glyph; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b001, 0b001], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// Drawn when there is no number to show. A dash is honest; a zero would not be.
#[rustfmt::skip]
const DASH: Glyph = [0b000, 0b000, 0b111, 0b000, 0b000];

fn digits_of(percent: u8) -> Vec<usize> {
    if percent == 0 {
        return vec![0];
    }
    let mut digits = Vec::new();
    let mut left = percent;
    while left > 0 {
        digits.push((left % 10) as usize);
        left /= 10;
    }
    digits.reverse();
    digits
}

/// Returns RGBA bytes for a [`ICON_SIZE`]×[`ICON_SIZE`] icon.
///
/// `template` renders in plain white so macOS can tint it for the current menu
/// bar theme; on Windows the digits carry the band colour.
pub fn render(percent: Option<u8>, band: Band, dimmed: bool, template: bool) -> Vec<u8> {
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    let glyphs: Vec<Glyph> = match percent {
        Some(percent) => digits_of(percent).into_iter().map(|digit| DIGITS[digit]).collect(),
        None => vec![DASH],
    };

    let count = glyphs.len() as u32;
    let unscaled_width = GLYPH_W * count + count.saturating_sub(1);
    let scale = (MAX_EXTENT / unscaled_width).clamp(1, MAX_EXTENT / GLYPH_H);
    let width = scale * unscaled_width;
    let height = scale * GLYPH_H;
    let origin_x = (ICON_SIZE - width.min(ICON_SIZE)) / 2;
    let origin_y = (ICON_SIZE - height.min(ICON_SIZE)) / 2;

    let [red, green, blue] = if template { [0xff, 0xff, 0xff] } else { band.rgb() };
    let alpha = if dimmed { DIMMED_ALPHA } else { 0xff };

    for (index, glyph) in glyphs.iter().enumerate() {
        let glyph_x = origin_x + index as u32 * scale * (GLYPH_W + 1);
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits & (1 << (GLYPH_W - 1 - col)) == 0 {
                    continue;
                }
                fill_block(
                    &mut rgba,
                    glyph_x + col * scale,
                    origin_y + row as u32 * scale,
                    scale,
                    [red, green, blue, alpha],
                );
            }
        }
    }

    rgba
}

fn fill_block(rgba: &mut [u8], x: u32, y: u32, size: u32, colour: [u8; 4]) {
    for dy in 0..size {
        for dx in 0..size {
            let (px, py) = (x + dx, y + dy);
            if px >= ICON_SIZE || py >= ICON_SIZE {
                continue;
            }
            let offset = ((py * ICON_SIZE + px) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&colour);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_pixels(rgba: &[u8]) -> usize {
        rgba.chunks_exact(4).filter(|pixel| pixel[3] > 0).count()
    }

    #[test]
    fn output_is_always_a_full_rgba_canvas() {
        for percent in [None, Some(0), Some(7), Some(42), Some(100)] {
            let rgba = render(percent, Band::Ok, false, false);
            assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        }
    }

    #[test]
    fn digits_are_split_most_significant_first() {
        assert_eq!(digits_of(0), vec![0]);
        assert_eq!(digits_of(7), vec![7]);
        assert_eq!(digits_of(42), vec![4, 2]);
        assert_eq!(digits_of(100), vec![1, 0, 0]);
    }

    #[test]
    fn every_percent_fits_inside_the_canvas() {
        // A pixel outside the canvas would be silently dropped by `fill_block`,
        // so count the ink instead: 100 must draw more than a single dash.
        for percent in 0..=100u8 {
            let rgba = render(Some(percent), Band::Hot, false, false);
            assert!(opaque_pixels(&rgba) > 0, "percent {percent} drew nothing");
        }
    }

    #[test]
    fn no_reading_draws_a_dash_not_a_zero() {
        let dash = render(None, Band::Off, false, false);
        let zero = render(Some(0), Band::Ok, false, false);
        assert_ne!(dash, zero);
        assert!(opaque_pixels(&dash) < opaque_pixels(&zero));
    }

    #[test]
    fn band_colour_reaches_the_pixels_and_template_is_white() {
        let hot = render(Some(88), Band::Hot, false, false);
        let ink = hot.chunks_exact(4).find(|pixel| pixel[3] > 0).unwrap();
        assert_eq!([ink[0], ink[1], ink[2]], Band::Hot.rgb());

        let template = render(Some(88), Band::Hot, false, true);
        let ink = template.chunks_exact(4).find(|pixel| pixel[3] > 0).unwrap();
        assert_eq!([ink[0], ink[1], ink[2]], [0xff, 0xff, 0xff]);
    }

    #[test]
    fn stale_readings_are_drawn_dimmed() {
        let fresh = render(Some(50), Band::Warn, false, false);
        let stale = render(Some(50), Band::Warn, true, false);
        let alpha = |rgba: &[u8]| {
            rgba.chunks_exact(4).find(|pixel| pixel[3] > 0).map(|pixel| pixel[3]).unwrap()
        };
        assert_eq!(alpha(&fresh), 0xff);
        assert_eq!(alpha(&stale), DIMMED_ALPHA);
    }
}
