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
const STROKE: f32 = 4.5;
/// Alpha of the unfilled part of the dial.
const TRACK_ALPHA: u8 = 70;
/// Alpha used for a stale or errored reading (SPEC §9.1: "esmaecido").
const DIMMED_ALPHA: u8 = 140;
/// Samples per pixel per axis. Nothing here is antialiased by a library, so the
/// edges are smoothed by counting how much of each pixel falls inside a shape.
const SAMPLES: u32 = 3;

// The mark's palette (see `app-icon.svg`). White where there is room left, red
// where there is not — the gradient carries the severity, so the icon still
// reads for someone who cannot tell red from green.
const COLOUR_LOW: [f32; 3] = [255.0, 255.0, 255.0];
const COLOUR_HIGH: [f32; 3] = [232.0, 53.0, 47.0];
/// Light rather than dark: the Windows taskbar and the macOS menu bar are both
/// dark by default, and a dark track would simply vanish.
const COLOUR_TRACK: [f32; 3] = [154.0, 154.0, 160.0];

/// Needle geometry, as a share of the inner radius.
// Deliberately chunky: the tray draws this at half size, so a hairline needle
// would disappear in the downscale.
const NEEDLE_INNER: f32 = 0.30;
const NEEDLE_OUTER: f32 = 0.86;
const NEEDLE_HALF_WIDTH: f32 = 1.4;
const HUB_RADIUS: f32 = 3.0;

/// Colour of the dial at a given position around it.
fn dial_colour(turn: f32) -> [f32; 3] {
    let mix = turn.clamp(0.0, 1.0);
    [
        COLOUR_LOW[0] + (COLOUR_HIGH[0] - COLOUR_LOW[0]) * mix,
        COLOUR_LOW[1] + (COLOUR_HIGH[1] - COLOUR_LOW[1]) * mix,
        COLOUR_LOW[2] + (COLOUR_HIGH[2] - COLOUR_LOW[2]) * mix,
    ]
}

/// Returns RGBA bytes for a [`ICON_SIZE`]×[`ICON_SIZE`] icon: the GaugeCode
/// mark, with its dial filled to `percent` and its needle pointing there.
///
/// `percent` of `None` draws the bare dial with no needle, because a needle at
/// zero and "nothing to report" must not look the same.
///
/// `template` renders in plain white so macOS can tint it for the current menu
/// bar theme; the gradient then survives as an alpha ramp rather than a hue.
pub fn render(percent: Option<u8>, band: Band, dimmed: bool, template: bool) -> Vec<u8> {
    let mut rgba = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];

    let peak = if dimmed { DIMMED_ALPHA } else { 0xff } as f32;
    let track_alpha = TRACK_ALPHA as f32 * peak / 255.0;
    let fraction = percent.map(|percent| percent.min(100) as f32 / 100.0);
    // Without a reading there is nothing to point at, and Band::Off means the
    // same thing arriving by a different route.
    let needle_at = fraction.filter(|_| band != Band::Off);

    let centre = ICON_SIZE as f32 / 2.0;
    let (inner, outer) = (RADIUS - STROKE, RADIUS);
    let total = (SAMPLES * SAMPLES) as f32;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            // Accumulated premultiplied colour, so a pixel straddling the arc,
            // the track and the needle blends the three instead of picking one.
            let mut weighted = [0.0f32; 3];
            let mut alpha = 0.0f32;

            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let px = x as f32 + (sx as f32 + 0.5) / SAMPLES as f32 - centre;
                    let py = y as f32 + (sy as f32 + 0.5) / SAMPLES as f32 - centre;
                    let distance = (px * px + py * py).sqrt();

                    let on_needle = needle_at
                        .is_some_and(|needle| on_needle(px, py, distance, needle, inner));

                    let sample = if on_needle {
                        // Always white: it has to read where it points into the
                        // red end of the dial.
                        Some((COLOUR_LOW, peak))
                    } else if distance >= inner && distance <= outer {
                        let turn = turn_at(px, py);
                        match fraction {
                            Some(fraction) if turn <= fraction => {
                                Some((dial_colour(turn), peak))
                            }
                            _ => Some((COLOUR_TRACK, track_alpha)),
                        }
                    } else {
                        None
                    };

                    if let Some((colour, sample_alpha)) = sample {
                        for channel in 0..3 {
                            weighted[channel] += colour[channel] * sample_alpha;
                        }
                        alpha += sample_alpha;
                    }
                }
            }

            if alpha <= 0.0 {
                continue;
            }
            let [red, green, blue] = if template {
                [0xff, 0xff, 0xff]
            } else {
                [
                    (weighted[0] / alpha).round() as u8,
                    (weighted[1] / alpha).round() as u8,
                    (weighted[2] / alpha).round() as u8,
                ]
            };
            let offset = ((y * ICON_SIZE + x) * 4) as usize;
            rgba[offset..offset + 4]
                .copy_from_slice(&[red, green, blue, (alpha / total).round() as u8]);
        }
    }

    rgba
}

/// Whether a sample falls on the needle or its hub, for a needle pointing at
/// `fraction` of the way round the dial.
fn on_needle(px: f32, py: f32, distance: f32, fraction: f32, inner: f32) -> bool {
    if distance <= HUB_RADIUS {
        return true;
    }
    if distance < inner * NEEDLE_INNER || distance > inner * NEEDLE_OUTER {
        return false;
    }
    let angle = fraction * std::f32::consts::TAU;
    let (axis_x, axis_y) = (angle.sin(), -angle.cos());
    // Behind the hub is the *opposite* reading, so the needle is a ray and not
    // a line: without this the mark grows a second pointer 180° away.
    if px * axis_x + py * axis_y <= 0.0 {
        return false;
    }
    (px * axis_y - py * axis_x).abs() <= NEEDLE_HALF_WIDTH
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
    fn the_mark_stays_inside_the_canvas_with_a_gap_around_the_hub() {
        let rgba = render(Some(100), Band::Hot, false, false);
        let alpha_at = |x: u32, y: u32| rgba[((y * ICON_SIZE + x) * 4 + 3) as usize];

        // Corners and edges are outside a ring of radius 14 in a 32 px canvas.
        for (x, y) in [(0, 0), (31, 0), (0, 31), (31, 31), (16, 0), (0, 16)] {
            assert_eq!(alpha_at(x, y), 0, "the mark reached ({x}, {y})");
        }
        // The gap between hub and dial is what stops it reading as a solid
        // disc. At 100% the needle points straight up, so sample sideways.
        assert_eq!(alpha_at(22, 16), 0, "hub and dial should not touch");
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
    fn the_template_variant_is_pure_white_so_macos_can_tint_it() {
        // The gradient survives as an alpha ramp there, not as a hue.
        let template = render(Some(88), Band::Hot, false, true);
        for pixel in template.chunks_exact(4).filter(|pixel| pixel[3] > 0) {
            assert_eq!([pixel[0], pixel[1], pixel[2]], [0xff, 0xff, 0xff]);
        }
    }

    #[test]
    fn the_gradient_runs_from_white_to_red() {
        assert_eq!(dial_colour(0.0), COLOUR_LOW);
        assert_eq!(dial_colour(1.0), COLOUR_HIGH);
        // Monotonic, so a rising reading never looks like a falling one.
        let green = |turn| dial_colour(turn)[1];
        assert!(green(0.25) > green(0.5));
        assert!(green(0.5) > green(0.75));
        // Out-of-range input is clamped rather than extrapolated into nonsense.
        assert_eq!(dial_colour(-1.0), COLOUR_LOW);
        assert_eq!(dial_colour(9.0), COLOUR_HIGH);
    }

    #[test]
    fn stale_readings_are_drawn_dimmed() {
        let fresh = render(Some(50), Band::Warn, false, false);
        let stale = render(Some(50), Band::Warn, true, false);
        assert_eq!(strongest_alpha(&fresh), 0xff);
        assert_eq!(strongest_alpha(&stale), DIMMED_ALPHA);
        assert!(ink(&stale) < ink(&fresh));
    }

    /// Prints the icon as text so its shape can be eyeballed without an image
    /// decoder. `cargo test -- --nocapture shape_of_the_mark`.
    #[test]
    fn shape_of_the_mark() {
        for percent in [Some(18), Some(63), Some(97), None] {
            println!("\n--- {percent:?} ---");
            let rgba = render(percent, Band::Ok, false, false);
            for y in 0..ICON_SIZE {
                let row: String = (0..ICON_SIZE)
                    .map(|x| {
                        let pixel = &rgba[((y * ICON_SIZE + x) * 4) as usize..][..4];
                        match (pixel[3], pixel[1]) {
                            (0, _) => ' ',
                            // Red has a low green channel; the track is grey.
                            (_, green) if green < 120 => '#',
                            (alpha, _) if alpha > TRACK_ALPHA + 40 => 'O',
                            _ => '.',
                        }
                    })
                    .collect();
                println!("{row}");
            }
        }
    }

    #[test]
    fn the_needle_points_where_the_dial_stops() {
        let alpha_at = |rgba: &Vec<u8>, x: u32, y: u32| {
            rgba[((y * ICON_SIZE + x) * 4 + 3) as usize]
        };

        // A quarter used points the needle right, so the pixels just right of
        // centre are inked and the ones just left of it are not.
        let quarter = render(Some(25), Band::Ok, false, false);
        assert!(alpha_at(&quarter, 20, 16) > 0, "needle should reach right of centre");
        assert_eq!(alpha_at(&quarter, 11, 16), 0, "nothing should be left of centre");

        // Three quarters points it left, and the two must not look the same.
        let three_quarters = render(Some(75), Band::Ok, false, false);
        assert!(alpha_at(&three_quarters, 11, 16) > 0, "needle should reach left of centre");
        assert_ne!(quarter, three_quarters);
    }

    #[test]
    fn no_reading_draws_no_needle() {
        // A needle resting at zero would be a claim; an empty dial is not.
        let nothing = render(None, Band::Off, false, false);
        let alpha_at = |x: u32, y: u32| nothing[((y * ICON_SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha_at(16, 16), 0, "the hub is part of the needle");
        assert_eq!(strongest_alpha(&nothing), TRACK_ALPHA);
    }

    #[test]
    fn red_only_appears_once_the_dial_has_filled() {
        // Red has a low green channel, so the least green pixel on the canvas
        // says how far into the red the mark has gone.
        let reddest = |percent: u8| {
            render(Some(percent), Band::Ok, false, false)
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .map(|pixel| pixel[1])
                .min()
                .unwrap()
        };
        assert!(reddest(10) > 130, "a low reading should carry no red at all");
        assert!(reddest(95) < 100, "a high reading should be plainly red");
        assert!(reddest(95) < reddest(50));
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
