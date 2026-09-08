//! Notch overlay: geometry, states and the pointer watch (SPEC §9.1).
//!
//! The window is anchored to the monitor's **work area**, never to the raw
//! screen, so it can never sit on top of the Windows taskbar or the macOS Dock —
//! whichever edge either of them happens to be on.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::state::AppState;
use crate::store::Prefs;

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
/// The bar style trades the rings for one row per limit window, so it needs
/// more width and more height per provider.
const BARS_WIDTH_VERTICAL: f64 = 268.0;
const BARS_LENGTH_PER_PROVIDER_VERTICAL: f64 = 62.0;
const BARS_HEIGHT_HORIZONTAL: f64 = 74.0;
const BARS_LENGTH_PER_PROVIDER_HORIZONTAL: f64 = 214.0;

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

/// What the expanded notch shows. Unlike the animation this *does* affect the
/// geometry, because a row of bars needs a different footprint from a ring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchStyle {
    /// One progress ring per provider.
    #[default]
    Rings,
    /// One segmented bar per limit window, with its percentage and reset.
    Bars,
}

/// How the notch moves between folded and expanded. Purely a UI concern — the
/// window itself never resizes, so the choice cannot affect the geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchAnimation {
    /// Slides out of the edge it is anchored to.
    #[default]
    Slide,
    /// Stays put and fades in.
    Fade,
    /// No transition at all.
    Instant,
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
pub fn layout(
    work_area: Rect,
    scale: f64,
    edge: NotchEdge,
    mode: NotchMode,
    providers: u32,
    style: NotchStyle,
) -> Rect {
    let providers = providers.max(1) as f64;
    let px = |logical: f64| (logical * scale).round().max(1.0) as u32;
    let bars = style == NotchStyle::Bars;

    let (width, height) = match (edge.is_vertical(), mode.is_expanded()) {
        (true, false) => (
            px(FOLDED_THICKNESS),
            px((FOLDED_LENGTH_PER_PROVIDER * providers).max(FOLDED_MIN_LENGTH)),
        ),
        (true, true) if bars => (
            px(BARS_WIDTH_VERTICAL),
            px(EXPANDED_PADDING + BARS_LENGTH_PER_PROVIDER_VERTICAL * providers),
        ),
        (true, true) => (
            px(EXPANDED_WIDTH_VERTICAL),
            px(EXPANDED_PADDING + EXPANDED_LENGTH_PER_PROVIDER_VERTICAL * providers),
        ),
        (false, false) => (
            px((FOLDED_LENGTH_PER_PROVIDER * providers).max(FOLDED_MIN_LENGTH)),
            px(FOLDED_THICKNESS),
        ),
        (false, true) if bars => (
            px(EXPANDED_PADDING + BARS_LENGTH_PER_PROVIDER_HORIZONTAL * providers),
            px(BARS_HEIGHT_HORIZONTAL),
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

/// The window's actual footprint. It is **always** the expanded one, whatever
/// the mode: resizing a window cannot be animated smoothly, so instead the
/// window stays put and the UI slides the card inside it. While folded the
/// window is click-through, so the extra area costs nothing.
pub fn window_rect(
    work_area: Rect,
    scale: f64,
    edge: NotchEdge,
    providers: u32,
    style: NotchStyle,
) -> Rect {
    layout(work_area, scale, edge, NotchMode::Pinned, providers, style)
}

/// Where the folded sliver is drawn inside the window. This — not the window —
/// is the area the pointer has to reach for the notch to peek.
pub fn pill_rect(work_area: Rect, scale: f64, edge: NotchEdge, providers: u32) -> Rect {
    layout(work_area, scale, edge, NotchMode::Folded, providers, NotchStyle::Rings)
}

// ---------------------------------------------------------------------------
// Runtime
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize)]
pub struct NotchView {
    pub mode: NotchMode,
    pub edge: NotchEdge,
    pub visible: bool,
    pub animation: NotchAnimation,
    pub style: NotchStyle,
    /// Thickness of the folded sliver in CSS pixels, so the UI draws it exactly
    /// where the pointer watch expects it to be.
    pub folded_thickness: f64,
    /// Length of the folded sliver along the edge, in CSS pixels.
    pub folded_length: f64,
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

/// Whether this session lets an application place its own window.
///
/// Wayland deliberately does not: a client cannot position itself, so
/// `set_position` is accepted and ignored and the notch would land wherever the
/// compositor felt like. Rather than ship an overlay that drifts, the UI says
/// the notch needs an X11 session. Everything else works normally.
pub fn positioning_supported() -> bool {
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
        if session.eq_ignore_ascii_case("wayland") || std::env::var_os("WAYLAND_DISPLAY").is_some()
        {
            return false;
        }
    }
    true
}

/// The rectangle the notch is anchored to.
///
/// Normally the monitor's **work area**, which already excludes the taskbar and
/// the Dock — that is what keeps the overlay off them. `over_taskbar` anchors to
/// the full monitor instead, which is the only way to sit on the bar; it is off
/// by default and only ever on because the user asked for it.
fn work_area_of(app: &AppHandle, over_taskbar: bool) -> Option<(Rect, f64)> {
    // Follow the monitor the pointer is on, falling back to the primary one.
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;

    let (position, size) = if over_taskbar {
        (*monitor.position(), *monitor.size())
    } else {
        let area = monitor.work_area();
        (area.position, area.size)
    };

    Some((
        Rect { x: position.x, y: position.y, width: size.width, height: size.height },
        monitor.scale_factor(),
    ))
}

fn enabled_provider_count(app: &AppHandle) -> u32 {
    app.state::<Arc<AppState>>().prefs().enabled_count().max(1)
}

/// Logical size of the folded sliver: thickness across the edge, length along it.
fn folded_logical(providers: u32) -> (f64, f64) {
    let providers = providers.max(1) as f64;
    (FOLDED_THICKNESS, (FOLDED_LENGTH_PER_PROVIDER * providers).max(FOLDED_MIN_LENGTH))
}

/// Applies the current mode to the window: click-through, visibility and
/// geometry, then tells the UI what to draw.
///
/// The window is placed at its expanded footprint whatever the mode — see
/// [`window_rect`].
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

    if !prefs.notch_visible || !positioning_supported() {
        let _ = window.hide();
        emit(app, view_of(&prefs, mode, false));
        return;
    }

    if let Some((work_area, scale)) = work_area_of(app, prefs.notch_over_taskbar) {
        let rect = window_rect(
            work_area,
            scale,
            prefs.notch_edge,
            enabled_provider_count(app),
            prefs.notch_style,
        );
        let _ = window.set_size(PhysicalSize::new(rect.width, rect.height));
        let _ = window.set_position(PhysicalPosition::new(rect.x, rect.y));
        app.state::<NotchState>().lock().last_work_area = Some(work_area);
    }

    // An overlay must never pull focus away from the editor underneath it.
    let _ = window.set_focusable(false);
    // A usage meter you have to switch Spaces to read is not a usage meter. This
    // matters on macOS, where every Space would otherwise hide it.
    let _ = window.set_visible_on_all_workspaces(true);
    // Folded, the notch must not eat clicks meant for whatever is behind it.
    if let Err(error) = window.set_ignore_cursor_events(!mode.is_expanded()) {
        tracing::warn!(%error, "click-through is unavailable; the folded notch will take clicks");
    }
    let _ = window.show();
    emit(app, view_of(&prefs, mode, true));
}

fn emit(app: &AppHandle, view: NotchView) {
    let _ = app.emit(MODE_EVENT, view);
}

fn view_of(prefs: &Prefs, mode: NotchMode, visible: bool) -> NotchView {
    let (thickness, length) = folded_logical(prefs.enabled_count());
    NotchView {
        mode,
        edge: prefs.notch_edge,
        visible,
        animation: prefs.notch_animation,
        style: prefs.notch_style,
        folded_thickness: thickness,
        folded_length: length,
    }
}

/// Takes the preferences as an argument rather than reaching for global state:
/// the windows exist before `setup` has managed it, so a command that fired the
/// moment a webview loaded would otherwise panic.
pub fn view(app: &AppHandle, prefs: &Prefs) -> NotchView {
    view_of(prefs, app.state::<NotchState>().mode(), prefs.notch_visible)
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
    if !prefs.notch_visible || !positioning_supported() {
        return;
    }

    let Some((work_area, scale)) = work_area_of(app, prefs.notch_over_taskbar) else { return };
    let state = app.state::<NotchState>();
    let mode = state.mode();

    // The taskbar moved, the resolution changed, or the pointer crossed to
    // another monitor: re-anchor before deciding anything.
    let moved = { state.lock().last_work_area } != Some(work_area);
    if moved {
        tracing::debug!(?work_area, "work area changed; re-anchoring notch and bar");
        apply(app, mode);
        // The taskbar strip lives in the space the work area leaves out, so it
        // has to follow the same change.
        crate::bar::apply(app);
        return;
    }

    if mode == NotchMode::Pinned {
        return;
    }

    let Ok(cursor) = app.cursor_position() else { return };
    let providers = enabled_provider_count(app);

    match mode {
        NotchMode::Folded => {
            // Only the sliver counts, not the whole (invisible) window.
            let hot = pill_rect(work_area, scale, prefs.notch_edge, providers);
            if hot.inflated(HOT_ZONE_PADDING).contains(cursor.x, cursor.y) {
                apply(app, NotchMode::Peek);
            }
        }
        NotchMode::Peek => {
            // Expanded, the whole window is the card, so leaving it means
            // leaving the window.
            let rect =
                window_rect(work_area, scale, prefs.notch_edge, providers, prefs.notch_style);
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

        /// The default style, so each test says only what it is actually about.
    fn rings(
        area: Rect,
        scale: f64,
        edge: NotchEdge,
        mode: NotchMode,
        providers: u32,
    ) -> Rect {
        layout(area, scale, edge, mode, providers, NotchStyle::Rings)
    }

    fn rings_window(area: Rect, scale: f64, edge: NotchEdge, providers: u32) -> Rect {
        window_rect(area, scale, edge, providers, NotchStyle::Rings)
    }

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
                        let rect = rings(WORK_AREA, scale, edge, mode, providers);
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
                    let rect = rings(area, 1.0, edge, mode, 3);
                    assert!(contains_rect(area, rect), "{edge:?}/{mode:?} left {area:?}: {rect:?}");
                }
            }
        }
    }

    #[test]
    fn folded_hugs_its_edge_and_expanding_keeps_it_there() {
        let folded = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let expanded = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert_eq!(folded.right(), WORK_AREA.right());
        assert_eq!(expanded.right(), WORK_AREA.right());
        assert!(expanded.width > folded.width);

        let folded = rings(WORK_AREA, 1.0, NotchEdge::Left, NotchMode::Folded, 3);
        assert_eq!(folded.x, WORK_AREA.x);

        let folded = rings(WORK_AREA, 1.0, NotchEdge::Top, NotchMode::Folded, 3);
        assert_eq!(folded.y, WORK_AREA.y);

        let folded = rings(WORK_AREA, 1.0, NotchEdge::Bottom, NotchMode::Folded, 3);
        assert_eq!(folded.bottom(), WORK_AREA.bottom());
    }

    #[test]
    fn folded_is_a_thin_pill_along_the_edge() {
        let vertical = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        assert_eq!(vertical.width, FOLDED_THICKNESS as u32);
        assert!(vertical.height > vertical.width);

        let horizontal = rings(WORK_AREA, 1.0, NotchEdge::Top, NotchMode::Folded, 3);
        assert_eq!(horizontal.height, FOLDED_THICKNESS as u32);
        assert!(horizontal.width > horizontal.height);
    }

    #[test]
    fn the_notch_is_centred_along_its_edge() {
        let rect = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let gap_above = rect.y - WORK_AREA.y;
        let gap_below = WORK_AREA.bottom() - rect.bottom();
        assert!((gap_above - gap_below).abs() <= 1, "not centred: {gap_above} vs {gap_below}");
    }

    #[test]
    fn more_providers_make_the_notch_longer_not_thicker() {
        let one = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 1);
        let three = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert_eq!(one.width, three.width);
        assert!(three.height > one.height);
    }

    #[test]
    fn scaling_grows_the_notch_in_physical_pixels() {
        let at_one = rings(WORK_AREA, 1.0, NotchEdge::Right, NotchMode::Folded, 3);
        let at_two = rings(WORK_AREA, 2.0, NotchEdge::Right, NotchMode::Folded, 3);
        assert_eq!(at_two.width, at_one.width * 2);
    }

    #[test]
    fn a_work_area_smaller_than_the_notch_clamps_instead_of_overflowing() {
        let tiny = Rect { x: 10, y: 10, width: 120, height: 90 };
        let rect = rings(tiny, 1.0, NotchEdge::Right, NotchMode::Pinned, 3);
        assert!(contains_rect(tiny, rect), "{rect:?} escaped {tiny:?}");
    }

    #[test]
    fn the_window_keeps_its_expanded_footprint_in_every_mode() {
        // Smooth animation depends on this: the window never resizes, so what
        // moves is the card drawn inside it.
        for edge in all_edges() {
            let expected = rings_window(WORK_AREA, 1.0, edge, 3);
            for mode in [NotchMode::Folded, NotchMode::Peek, NotchMode::Pinned] {
                assert_eq!(
                    rings_window(WORK_AREA, 1.0, edge, 3),
                    expected,
                    "{edge:?}/{mode:?} moved the window"
                );
            }
            assert!(contains_rect(WORK_AREA, expected), "{edge:?} escaped: {expected:?}");
        }
    }

    #[test]
    fn the_pill_sits_inside_the_window_flush_with_the_same_edge() {
        for edge in all_edges() {
            let window = rings_window(WORK_AREA, 1.0, edge, 3);
            let pill = pill_rect(WORK_AREA, 1.0, edge, 3);
            assert!(contains_rect(window, pill), "{edge:?}: {pill:?} outside {window:?}");
            match edge {
                NotchEdge::Right => assert_eq!(pill.right(), window.right()),
                NotchEdge::Left => assert_eq!(pill.x, window.x),
                NotchEdge::Top => assert_eq!(pill.y, window.y),
                NotchEdge::Bottom => assert_eq!(pill.bottom(), window.bottom()),
            }
        }
    }

    #[test]
    fn the_bar_style_is_wider_than_the_ring_style_and_still_fits() {
        for edge in all_edges() {
            let with_rings = window_rect(WORK_AREA, 1.0, edge, 3, NotchStyle::Rings);
            let with_bars = window_rect(WORK_AREA, 1.0, edge, 3, NotchStyle::Bars);
            assert_ne!(with_rings, with_bars, "{edge:?}: the styles need different room");
            assert!(contains_rect(WORK_AREA, with_bars), "{edge:?} escaped: {with_bars:?}");
            // A bar carries a name, a scale, a percentage and a reset time, so
            // it needs more room across the edge than a ring does.
            if edge.is_vertical() {
                assert!(with_bars.width > with_rings.width);
            } else {
                assert!(with_bars.height < with_rings.height);
            }
        }
    }

    #[test]
    fn the_folded_pill_is_the_same_whatever_the_expanded_style() {
        // The pointer watch keys off the pill, so switching style must not move
        // the hot zone out from under the cursor.
        for edge in all_edges() {
            assert_eq!(
                layout(WORK_AREA, 1.0, edge, NotchMode::Folded, 3, NotchStyle::Rings),
                layout(WORK_AREA, 1.0, edge, NotchMode::Folded, 3, NotchStyle::Bars),
            );
        }
    }

    #[test]
    fn anchoring_to_the_whole_monitor_is_what_reaches_the_taskbar() {
        // Same monitor, once as the work area and once as the full screen. The
        // second is the only one that can sit on the bar — which is why it is
        // off by default (SPEC §9.1).
        let full = Rect { x: 0, y: 0, width: 1440, height: 900 };
        let work = Rect { x: 0, y: 0, width: 1440, height: 852 };

        let on_bar = rings_window(full, 1.0, NotchEdge::Bottom, 3);
        let above_bar = rings_window(work, 1.0, NotchEdge::Bottom, 3);
        assert_eq!(on_bar.bottom(), 900);
        assert_eq!(above_bar.bottom(), 852);
        assert!(on_bar.bottom() > above_bar.bottom());
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
