//! Notch overlay: geometry, states and the pointer watch (SPEC §9.1).
//!
//! The window is anchored to the monitor's **work area**, never to the raw
//! screen, so it can never sit on top of the Windows taskbar or the macOS Dock —
//! whichever edge either of them happens to be on.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::model::ProviderId;
use crate::state::AppState;

pub const NOTCH_WINDOW: &str = "notch";
pub const MODE_EVENT: &str = "notch:state";

/// How often the pointer is sampled while the notch is folded.
const WATCH_INTERVAL: Duration = Duration::from_millis(100);
/// Delay before a peeking notch folds back after the pointer leaves.
const COLLAPSE_DELAY: Duration = Duration::from_millis(600);
/// Extra pixels around the folded pill that still count as "the pointer arrived".
const HOT_ZONE_PADDING: f64 = 6.0;

// Logical (pre-scale) dimensions.
const FOLDED_THICKNESS: f64 = 10.0;
const FOLDED_LENGTH_PER_PROVIDER: f64 = 26.0;
const FOLDED_MIN_LENGTH: f64 = 54.0;
const EXPANDED_PADDING: f64 = 14.0;
/// Vertical edges stack the providers; horizontal edges lay them side by side.
const EXPANDED_WIDTH_VERTICAL: f64 = 196.0;
const EXPANDED_LENGTH_PER_PROVIDER_VERTICAL: f64 = 74.0;
const EXPANDED_HEIGHT_HORIZONTAL: f64 = 104.0;
const EXPANDED_LENGTH_PER_PROVIDER_HORIZONTAL: f64 = 150.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchEdge {
    #[default]
    Right,
    Left,
    Top,
    Bottom,
}

impl NotchEdge {
    /// Left and right edges stack providers vertically.
    fn is_vertical(self) -> bool {
        matches!(self, NotchEdge::Left | NotchEdge::Right)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchMode {
    /// A thin pill showing only the band colour. Click-through.
    #[default]
    Folded,
    /// Expanded because the pointer reached the edge; folds back on its own.
    Peek,
    /// Expanded because the user clicked; stays until clicked again.
    Pinned,
}

impl NotchMode {
    pub fn is_expanded(self) -> bool {
        !matches!(self, NotchMode::Folded)
    }
}

/// A plain rectangle, so the geometry can be unit tested without a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn right(&self) -> i32 {
        self.x + self.width as i32
    }

    fn bottom(&self) -> i32 {
        self.y + self.height as i32
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x as f64 && x < self.right() as f64 && y >= self.y as f64 && y < self.bottom() as f64
    }

    fn inflated(&self, by: f64) -> Rect {
        let by = by.round() as i32;
        Rect {
            x: self.x - by,
            y: self.y - by,
            width: self.width + (by as u32) * 2,
            height: self.height + (by as u32) * 2,
        }
    }
}

/// Places the notch flush against `edge` of `work_area`, centred along it.
///
/// Everything is computed from the work area, so the result can never overlap
/// the taskbar or the Dock — that is the M2 done criterion (SPEC §12).
pub fn layout(work_area: Rect, scale: f64, edge: NotchEdge, mode: NotchMode, providers: u32) -> Rect {
    let providers = providers.max(1) as f64;
    let px = |logical: f64| (logical * scale).round().max(1.0) as u32;

    let (width, height) = match (edge.is_vertical(), mode.is_expanded()) {
        (true, false) => (
            px(FOLDED_THICKNESS),
            px((FOLDED_LENGTH_PER_PROVIDER * providers).max(FOLDED_MIN_LENGTH)),
        ),
        (true, true) => (
            px(EXPANDED_WIDTH_VERTICAL),
            px(EXPANDED_PADDING + EXPANDED_LENGTH_PER_PROVIDER_VERTICAL * providers),
        ),
        (false, false) => (
            px((FOLDED_LENGTH_PER_PROVIDER * providers).max(FOLDED_MIN_LENGTH)),
            px(FOLDED_THICKNESS),
        ),
        (false, true) => (
            px(EXPANDED_PADDING + EXPANDED_LENGTH_PER_PROVIDER_HORIZONTAL * providers),
            px(EXPANDED_HEIGHT_HORIZONTAL),
        ),
    };

    // Never wider or taller than the space we are allowed to use.
    let width = width.min(work_area.width);
    let height = height.min(work_area.height);

    let (x, y) = match edge {
        NotchEdge::Right => (
            work_area.right() - width as i32,
            centre(work_area.y, work_area.height, height),
        ),
        NotchEdge::Left => (work_area.x, centre(work_area.y, work_area.height, height)),
        NotchEdge::Top => (centre(work_area.x, work_area.width, width), work_area.y),
        NotchEdge::Bottom => (
            centre(work_area.x, work_area.width, width),
            work_area.bottom() - height as i32,
        ),
    };

    Rect { x, y, width, height }
}

fn centre(start: i32, available: u32, size: u32) -> i32 {
    start + ((available.saturating_sub(size)) / 2) as i32
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize)]
pub struct NotchView {
    pub mode: NotchMode,
    pub edge: NotchEdge,
    pub visible: bool,
}

pub struct NotchState {
    inner: Mutex<Runtime>,
}

#[derive(Default)]
struct Runtime {
    mode: NotchMode,
    /// When the pointer left an expanded notch; drives the collapse delay.
    left_at: Option<Instant>,
    last_work_area: Option<Rect>,
}

impl NotchState {
    pub fn new() -> Self {
        Self { inner: Mutex::new(Runtime::default()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Runtime> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn mode(&self) -> NotchMode {
        self.lock().mode
    }
}

impl Default for NotchState {
    fn default() -> Self {
        Self::new()
    }
}

fn work_area_of(app: &AppHandle) -> Option<(Rect, f64)> {
    // Follow the monitor the pointer is on, falling back to the primary one.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;

    let area = monitor.work_area();
    Some((
        Rect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        },
        monitor.scale_factor(),
    ))
}

fn enabled_provider_count(app: &AppHandle) -> u32 {
    let prefs = app.state::<Arc<AppState>>().prefs();
    ProviderId::ALL.iter().filter(|id| prefs.is_enabled(**id)).count().max(1) as u32
}

/// Applies the current mode to the window: geometry, click-through and
/// visibility, then tells the UI what to draw.
pub fn apply(app: &AppHandle, mode: NotchMode) {
    let Some(window) = app.get_webview_window(NOTCH_WINDOW) else { return };
    let prefs = app.state::<Arc<AppState>>().prefs();

    {
        let state = app.state::<NotchState>();
        let mut runtime = state.lock();
        runtime.mode = mode;
        if mode.is_expanded() {
            runtime.left_at = None;
        }
    }

    if !prefs.notch_visible {
        let _ = window.hide();
        emit(app, mode, prefs.notch_edge, false);
        return;
    }

    if let Some((work_area, scale)) = work_area_of(app) {
        let rect = layout(work_area, scale, prefs.notch_edge, mode, enabled_provider_count(app));
        let _ = window.set_size(PhysicalSize::new(rect.width, rect.height));
        let _ = window.set_position(PhysicalPosition::new(rect.x, rect.y));
        app.state::<NotchState>().lock().last_work_area = Some(work_area);
    }

    // An overlay must never pull focus away from the editor underneath it.
    let _ = window.set_focusable(false);
    // Folded, the notch must not eat clicks meant for whatever is behind it.
    if let Err(error) = window.set_ignore_cursor_events(!mode.is_expanded()) {
        tracing::warn!(%error, "click-through is unavailable; the folded notch will take clicks");
    }
    let _ = window.show();
    emit(app, mode, prefs.notch_edge, true);
}

fn emit(app: &AppHandle, mode: NotchMode, edge: NotchEdge, visible: bool) {
    let _ = app.emit(MODE_EVENT, NotchView { mode, edge, visible });
}

pub fn view(app: &AppHandle) -> NotchView {
    let prefs = app.state::<Arc<AppState>>().prefs();
    NotchView {
        mode: app.state::<NotchState>().mode(),
        edge: prefs.notch_edge,
        visible: prefs.notch_visible,
    }
}

/// Samples the pointer so a folded, click-through notch can still notice that
/// the pointer reached its edge (SPEC §9.1).
pub fn spawn_pointer_watch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(WATCH_INTERVAL).await;
            tick(&app);
        }
    });
}

fn tick(app: &AppHandle) {
    let prefs = app.state::<Arc<AppState>>().prefs();
    if !prefs.notch_visible {
        return;
    }

    let Some((work_area, scale)) = work_area_of(app) else { return };
    let state = app.state::<NotchState>();
    let mode = state.mode();

    // The taskbar moved, the resolution changed, or the pointer crossed to
    // another monitor: re-anchor before deciding anything.
    let moved = { state.lock().last_work_area } != Some(work_area);
    if moved {
        tracing::debug!(?work_area, "work area changed; re-anchoring notch");
        apply(app, mode);
        return;
    }

    if mode == NotchMode::Pinned {
        return;
    }

    let Ok(cursor) = app.cursor_position() else { return };
    let providers = enabled_provider_count(app);
    let rect = layout(work_area, scale, prefs.notch_edge, mode, providers);

    match mode {
        NotchMode::Folded => {
            if rect.inflated(HOT_ZONE_PADDING).contains(cursor.x, cursor.y) {
                apply(app, NotchMode::Peek);
            }
        }
        NotchMode::Peek => {
            if rect.contains(cursor.x, cursor.y) {
                state.lock().left_at = None;
                return;
            }
            let should_collapse = {
                let mut runtime = state.lock();
                let left_at = runtime.left_at.get_or_insert_with(Instant::now);
                left_at.elapsed() >= COLLAPSE_DELAY
            };
            if should_collapse {
                apply(app, NotchMode::Folded);
            }
        }
        NotchMode::Pinned => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK_AREA: Rect = Rect { x: 0, y: 0, width: 1920, height: 1032 };

    fn all_edges() -> [NotchEdge; 4] {
        [NotchEdge::Right, NotchEdge::Left, NotchEdge::Top, NotchEdge::Bottom]
    }

    fn contains_rect(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right()
            && inner.bottom() <= outer.bottom()
    }

    #[test]
    fn the_notch_always_stays_inside_the_work_area() {
        for edge in all_edges() {
            for mode in [NotchMode::Folded, NotchMode::Peek, NotchMode::Pinned] {
                for providers in 1..=3 {
                    for scale in [1.0, 1.25, 1.5, 2.0] {
                        let rect = layout(WORK_AREA, scale, edge, mode, providers);
                        assert!(
                            contains_rect(WORK_AREA, rect),
                            "{edge:?}/{mode:?} x{scale} with {providers} providers escaped: {rect:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_taskbar_on_any_side_is_never_covered() {
        // The work area is what is left after the taskbar; anchoring to it is the
        // whole point. One case per taskbar position.
        let areas = [
            Rect { x: 0, y: 0, width: 1920, height: 1032 },   // taskbar at the bottom
            Rect { x: 0, y: 48, width: 1920, height: 1032 },  // at the top
            Rect { x: 72, y: 0, width: 1848, height: 1080 },  // on the left
            Rect { x: 0, y: 0, width: 1848, height: 1080 },   // on the right
        ];
        for area in areas {
            for edge in all_edges() {
                for mode in [NotchMode::Folded, NotchMode::Pinned] {
                    let rect = layout(area, 1.0, edge, mode, 3);
                    assert!(contains_rect(area, rect), "{edge:?}/{mode:?} left {area:?}: {rect:?}");
                }
            }
        }
    }

    #[test]
    fn folded_hugs_its_edge_and_expanding_keeps_it_there() {
        let folded = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let expanded = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert_eq!(folded.right(), WORK_AREA.right());
        assert_eq!(expanded.right(), WORK_AREA.right());
        assert!(expanded.width > folded.width);

        let folded = layout(WORK_AREA, 1.0, NotchEdge::Left, NotchMode::Folded, 3);
        assert_eq!(folded.x, WORK_AREA.x);

        let folded = layout(WORK_AREA, 1.0, NotchEdge::Top, NotchMode::Folded, 3);
        assert_eq!(folded.y, WORK_AREA.y);

        let folded = layout(WORK_AREA, 1.0, NotchEdge::Bottom, NotchMode::Folded, 3);
        assert_eq!(folded.bottom(), WORK_AREA.bottom());
    }

    #[test]
    fn folded_is_a_thin_pill_along_the_edge() {
        let vertical = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        assert_eq!(vertical.width, FOLDED_THICKNESS as u32);
        assert!(vertical.height > vertical.width);

        let horizontal = layout(WORK_AREA, 1.0, NotchEdge::Top, NotchMode::Folded, 3);
        assert_eq!(horizontal.height, FOLDED_THICKNESS as u32);
        assert!(horizontal.width > horizontal.height);
    }

    #[test]
    fn the_notch_is_centred_along_its_edge() {
        let rect = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let gap_above = rect.y - WORK_AREA.y;
        let gap_below = WORK_AREA.bottom() - rect.bottom();
        assert!((gap_above - gap_below).abs() <= 1, "not centred: {gap_above} vs {gap_below}");
    }

    #[test]
    fn more_providers_make_the_notch_longer_not_thicker() {
        let one = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 1);
        let three = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert_eq!(one.width, three.width);
        assert!(three.height > one.height);
    }

    #[test]
    fn scaling_grows_the_notch_in_physical_pixels() {
        let at_one = layout(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let at_two = layout(WORK_AREA, 2.0, NotchEdge::Right, NotchMode::Folded, 3);
        assert_eq!(at_two.width, at_one.width * 2);
    }

    #[test]
    fn a_work_area_smaller_than_the_notch_clamps_instead_of_overflowing() {
        let tiny = Rect { x: 10, y: 10, width: 120, height: 90 };
        let rect = layout(tiny, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert!(contains_rect(tiny, rect), "{rect:?} escaped {tiny:?}");
    }

    #[test]
    fn the_hot_zone_is_slightly_larger_than_the_pill() {
        let rect = Rect { x: 100, y: 100, width: 10, height: 60 };
        let hot = rect.inflated(HOT_ZONE_PADDING);
        assert!(!rect.contains(96.0, 130.0), "pointer is outside the pill");
        assert!(hot.contains(96.0, 130.0), "but inside the hot zone");
        assert!(!hot.contains(80.0, 130.0), "and far away is still outside");
    }
}
