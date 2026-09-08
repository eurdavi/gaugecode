//! Renders the tray icon in a few states and dumps the raw RGBA strip, so the
//! icon can be eyeballed without taking a screenshot of anyone's desktop.
//!
//! `cargo run --example preview_icon -- <out.rgba>` prints `width height`.

use gaugecode_lib::icon::{render, ICON_SIZE};
use gaugecode_lib::model::Band;

fn main() {
    let states: [(Option<u8>, Band, bool); 6] = [
        (None, Band::Off, false),
        (Some(7), Band::Ok, false),
        (Some(42), Band::Ok, false),
        (Some(67), Band::Warn, false),
        (Some(93), Band::Hot, false),
        (Some(100), Band::Hot, true),
    ];

    let size = ICON_SIZE as usize;
    let width = size * states.len();
    let mut strip = vec![0u8; width * size * 4];

    for (index, (percent, band, dimmed)) in states.iter().enumerate() {
        let tile = render(*percent, *band, *dimmed, false);
        for y in 0..size {
            let from = y * size * 4;
            let to = (y * width + index * size) * 4;
            strip[to..to + size * 4].copy_from_slice(&tile[from..from + size * 4]);
        }
    }

    let path = std::env::args().nth(1).unwrap_or_else(|| "icon-preview.rgba".to_string());
    std::fs::write(&path, &strip).expect("write preview");
    println!("{width} {size}");
}
