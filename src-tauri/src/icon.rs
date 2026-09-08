//! Tray icon drawn at runtime (SPEC §9.2).
//!
//! Tauri accepts raw RGBA through `Image::new_owned`, so this needs no PNG
//! encoder and no font: the icon is the GaugeCode mark itself — a ring whose
//! arc fills with the usage and takes the band colour.
//!
//! Why a ring and not the percentage in digits: the tray icon is 16×16 points.
//! Digits at that size read like a frame counter overlay, and there is no room
//! for a number *and* a mark. A filling arc gives the same at-a-glance answer,
//! the exact figure lives one hover away in the tooltip, and the icon still
//! looks like an application rather than a debug readout.

use crate::model::Band;

pub const ICON_SIZE: u32 = 32;
/// Leaves a 2 px margin, so the ring is not clipped by the tray's own padding.
const RADIUS: f32 = 14.0;
const STROKE: f32 = 4.0;
/// Alpha of the unfilled part of the ring.
const TRACK_ALPHA: u8 = 70;
/// Alpha used for a stale or errored reading (SPEC §9.1: "esmaecido").
const DIMMED_ALPHA: u8 = 140;
/// Samples per pixel per axis. Nothing here is antialiased by a library, so the
/// edges are smoothed by counting how much of each pixel falls inside the ring.
const SAMPLES: u32 = 3;

/// Returns RGBA bytes for a [`ICON_SIZE`]×[`ICON_SIZE`] icon.
///
/// `percent` of `None` draws the bare track: no arc at all, because a zero-length
/// arc and "nothing to report" must not look the same.
///
/// `template` renders in plain white so macOS can tint it for the current menu
/// bar theme; on Windows the arc carries the band colour.
pub fn render(percent: Option<u8>, band: Band, dimmed: bool, template: bool) -> Vec<u8> {
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    let [red, green, blue] = if template { [0xff, 0xff, 0xff] } else { band.rgb() };
    let peak = if dimmed { DIMMED_ALPHA } else { 0xff };
    let track = (TRACK_ALPHA as u32 * peak as u32 / 0xff) as u8;
    let fraction = percent.map(|percent| percent.min(100) as f32 / 100.0);

    let centre = ICON_SIZE as f32 / 2.0;
    let (inner, outer) = (RADIUS - STROKE, RADIUS);

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (mut on_track, mut on_arc) = (0u32, 0u32);

            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32 - centre;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32 - centre;
                    let distance = (px * px + py * py).sqrt();
                    if distance < inner || distance > outer {
                        continue;
                    }
                    on_track += 1;
                    if fraction.is_some_and(|fraction| turn_at(px, py) <= fraction) {
                        on_arc += 1;
                    }
                }
            }

            if on_track == 0 {
                continue;
            }
            let total = SAMPLES * SAMPLES;
            // The arc sits on top of the track, so coverage of each is weighted
            // by how much of the pixel it actually covers.
            let alpha = (on_arc * peak as u32 + (on_track - on_arc) * track as u32) / total;
            let offset = ((y * ICON_SIZE + x) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&[red, green, blue, alpha as u8]);
        }
    }

    rgba
}

/// Position around the ring as 0.0..1.0, starting at twelve o'clock and going
/// clockwise — the direction every progress ring in the UI turns.
fn turn_at(px: f32, py: f32) -> f32 {
    let angle = px.atan2(-py);
    let turn = angle / std::f32::consts::TAU;
    if turn < 0.0 {
        turn + 1.0
    } else {
        turn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ink(rgba: &[u8]) -> u32 {
        rgba.chunks_exact(4).map(|pixel| pixel[3] as u32).sum()
    }

    fn strongest_alpha(rgba: &[u8]) -> u8 {
        rgba.chunks_exact(4).map(|pixel| pixel[3]).max().unwrap()
    }

    #[test]
    fn output_is_always_a_full_rgba_canvas() {
        for percent in [None, Some(0), Some(7), Some(42), Some(100)] {
            let rgba = render(percent, Band::Ok, false, false);
            assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        }
    }

    #[test]
    fn the_arc_grows_with_the_percentage() {
        let at = |percent| ink(&render(Some(percent), Band::Ok, false, false));
        assert!(at(0) < at(25), "0% should be almost all track");
        assert!(at(25) < at(50));
        assert!(at(50) < at(75));
        assert!(at(75) < at(100));
    }

    #[test]
    fn no_reading_draws_the_bare_track_not_a_zero_arc() {
        // Visually near-identical, but they must not be the same image: one
        // means "nothing to report", the other means "you have used none of it".
        let nothing = render(None, Band::Off, false, false);
        let zero = render(Some(0), Band::Ok, false, false);
        assert_ne!(nothing, zero);
        assert_eq!(strongest_alpha(&nothing), TRACK_ALPHA);
    }

    #[test]
    fn the_ring_stays_inside_the_canvas_and_leaves_its_centre_clear() {
        let rgba = render(Some(100), Band::Hot, false, false);
        let alpha_at = |x: u32, y: u32| rgba[((y * ICON_SIZE + x) * 4 + 3) as usize];

        // Corners and edges are outside a ring of radius 14 in a 32 px canvas.
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31), (16, 0), (0, 16)] {
            assert_eq!(alpha_at(x, y), 0, "ring reached ({x}, {y})");
        }
        // And the middle is a hole, which is what makes it read as a ring.
        assert_eq!(alpha_at(16, 16), 0);
    }

    #[test]
    fn half_used_fills_the_right_half_of_the_ring() {
        let rgba = render(Some(50), Band::Ok, false, false);
        let alpha_at = |x: u32, y: u32| rgba[((y * ICON_SIZE + x) * 4 + 3) as usize];
        assert!(alpha_at(28, 16) > TRACK_ALPHA, "right side should be filled");
        assert_eq!(alpha_at(3, 16), TRACK_ALPHA, "left side should still be track");
    }

    #[test]
    fn the_arc_starts_at_the_top_and_turns_clockwise() {
        // A small reading should have inked just clockwise of twelve o'clock,
        // and nothing anticlockwise of it.
        let rgba = render(Some(10), Band::Ok, false, false);
        let alpha_at = |x: u32, y: u32| rgba[((y * ICON_SIZE + x) * 4 + 3) as usize];
        assert!(alpha_at(18, 4) > TRACK_ALPHA, "just clockwise of the top should be filled");
        assert_eq!(alpha_at(14, 4), TRACK_ALPHA, "anticlockwise of the top must stay track");
    }

    #[test]
    fn band_colour_reaches_the_pixels_and_template_is_white() {
        let hot = render(Some(88), Band::Hot, false, false);
        let pixel = hot.chunks_exact(4).find(|pixel| pixel[3] > 0).unwrap();
        assert_eq!([pixel[0], pixel[1], pixel[2]], Band::Hot.rgb());

        let template = render(Some(88), Band::Hot, false, true);
        let pixel = template.chunks_exact(4).find(|pixel| pixel[3] > 0).unwrap();
        assert_eq!([pixel[0], pixel[1], pixel[2]], [0xff, 0xff, 0xff]);
    }

    #[test]
    fn stale_readings_are_drawn_dimmed() {
        let fresh = render(Some(50), Band::Warn, false, false);
        let stale = render(Some(50), Band::Warn, true, false);
        assert_eq!(strongest_alpha(&fresh), 0xff);
        assert_eq!(strongest_alpha(&stale), DIMMED_ALPHA);
        assert!(ink(&stale) < ink(&fresh));
    }

    #[test]
    fn a_percentage_over_one_hundred_is_clamped_rather_than_wrapping() {
        // A wrapped arc would read as a *lower* number than the truth.
        assert_eq!(
            render(Some(100), Band::Hot, false, false),
            render(Some(255), Band::Hot, false, false)
        );
    }
}
