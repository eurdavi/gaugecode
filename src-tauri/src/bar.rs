//! Taskbar bar: a third surface, independent of the notch and the tray.
//!
//! A small always-on-top strip that sits *on* the taskbar — in the band between
//! the monitor's edge and its work area, which is exactly the space the notch
//! promises never to enter. It shows the tray provider's limit windows as
//! segmented bars, the way a status widget would.
//!
//! Where along the taskbar it sits cannot be computed: the width of the
//! notification area varies by machine and by what is pinned. So the bar is
//! draggable, and the position the user leaves it in is remembered.

use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

use crate::notch::{self, Rect};
use crate::state::AppState;

pub const BAR_WINDOW: &str = "bar";
pub const BAR_EVENT: &str = "bar:state";

/// Logical width of the strip.
const WIDTH: f64 = 236.0;
/// Where the strip lands before the user has ever moved it: this far from the
/// taskbar's trailing end, which clears a typical notification area and clock.
const DEFAULT_OFFSET: f64 = 372.0;
/// Keeps the strip from being dragged so far it leaves the taskbar.
const EDGE_MARGIN: f64 = 4.0;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BarView {
    pub visible: bool,
    /// Whether the taskbar runs left-to-right (bottom/top) or top-to-bottom.
    pub horizontal: bool,
}

#[derive(Default)]
pub struct BarState {
    /// The position `apply` last set, so a `Moved` event can tell a drag apart
    /// from our own placement.
    last_applied: Mutex<Option<PhysicalPosition<i32>>>,
}

impl BarState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<PhysicalPosition<i32>>> {
        self.last_applied.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The bar needs a taskbar to sit on and a session that lets us place windows.
/// macOS has a menu bar, not a taskbar; that surface is the tray item itself.
pub fn supported() -> bool {
    !cfg!(target_os = "macos") && notch::positioning_supported()
}

/// Which side of the monitor the taskbar is on, and the strip it occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskbarSide {
    Bottom,
    Top,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Taskbar {
    pub rect: Rect,
    pub side: TaskbarSide,
}

impl Taskbar {
    pub fn is_horizontal(&self) -> bool {
        matches!(self.side, TaskbarSide::Bottom | TaskbarSide::Top)
    }
}

/// The taskbar is whatever the work area leaves out of the monitor. A monitor
/// with an auto-hidden or absent taskbar leaves nothing, and gets no bar.
pub fn taskbar_of(monitor: Rect, work_area: Rect) -> Option<Taskbar> {
    let bottom_gap = (monitor.y + monitor.height as i32) - (work_area.y + work_area.height as i32);
    let top_gap = work_area.y - monitor.y;
    let left_gap = work_area.x - monitor.x;
    let right_gap = (monitor.x + monitor.width as i32) - (work_area.x + work_area.width as i32);

    let (side, rect) = if bottom_gap > 0 {
        (
            TaskbarSide::Bottom,
            Rect {
                x: monitor.x,
                y: work_area.y + work_area.height as i32,
                width: monitor.width,
                height: bottom_gap as u32,
            },
        )
    } else if top_gap > 0 {
        (
            TaskbarSide::Top,
            Rect { x: monitor.x, y: monitor.y, width: monitor.width, height: top_gap as u32 },
        )
    } else if right_gap > 0 {
        (
            TaskbarSide::Right,
            Rect {
                x: work_area.x + work_area.width as i32,
                y: monitor.y,
                width: right_gap as u32,
                height: monitor.height,
            },
        )
    } else if left_gap > 0 {
        (
            TaskbarSide::Left,
            Rect { x: monitor.x, y: monitor.y, width: left_gap as u32, height: monitor.height },
        )
    } else {
        return None;
    };
    Some(Taskbar { rect, side })
}

/// Where the strip goes on a given taskbar, `offset` physical pixels in from the
/// taskbar's trailing end (right, or bottom for a vertical bar), clamped so it
/// can never be dragged off the taskbar.
pub fn place(taskbar: Taskbar, scale: f64, offset: Option<i32>) -> Rect {
    let px = |logical: f64| (logical * scale).round() as i32;
    let width = px(WIDTH);
    let margin = px(EDGE_MARGIN);
    let offset = offset.unwrap_or(px(DEFAULT_OFFSET));

    if taskbar.is_horizontal() {
        let max_x = taskbar.rect.x + taskbar.rect.width as i32 - width - margin;
        let min_x = taskbar.rect.x + margin;
        let x = (taskbar.rect.x + taskbar.rect.width as i32 - offset - width).clamp(min_x, max_x.max(min_x));
        Rect { x, y: taskbar.rect.y, width: width as u32, height: taskbar.rect.height }
    } else {
        // A vertical taskbar gets the strip turned on its side: as wide as the
        // taskbar, as tall as the strip is normally wide.
        let max_y = taskbar.rect.y + taskbar.rect.height as i32 - width - margin;
        let min_y = taskbar.rect.y + margin;
        let y = (taskbar.rect.y + taskbar.rect.height as i32 - offset - width).clamp(min_y, max_y.max(min_y));
        Rect { x: taskbar.rect.x, y, width: taskbar.rect.width, height: width as u32 }
    }
}

/// Offset to remember after the user dragged the strip to `position`.
pub fn offset_of(taskbar: Taskbar, position: PhysicalPosition<i32>, scale: f64) -> i32 {
    let width = (WIDTH * scale).round() as i32;
    if taskbar.is_horizontal() {
        taskbar.rect.x + taskbar.rect.width as i32 - position.x - width
    } else {
        taskbar.rect.y + taskbar.rect.height as i32 - position.y - width
    }
}

fn monitor_and_work_area(app: &AppHandle) -> Option<(Rect, Rect, f64)> {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|point| app.monitor_from_point(point.x, point.y).ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten())?;
    let position = monitor.position();
    let size = monitor.size();
    let area = monitor.work_area();
    Some((
        Rect { x: position.x, y: position.y, width: size.width, height: size.height },
        Rect {
            x: area.position.x,
            y: area.position.y,
            width: area.size.width,
            height: area.size.height,
        },
        monitor.scale_factor(),
    ))
}

fn current_taskbar(app: &AppHandle) -> Option<(Taskbar, f64)> {
    let (monitor, work_area, scale) = monitor_and_work_area(app)?;
    taskbar_of(monitor, work_area).map(|taskbar| (taskbar, scale))
}

/// Shows the strip where the preferences say, or hides it.
pub fn apply(app: &AppHandle) {
    let Some(window) = app.get_webview_window(BAR_WINDOW) else { return };
    let prefs = app.state::<Arc<AppState>>().prefs();

    let Some((taskbar, scale)) = current_taskbar(app).filter(|_| prefs.bar_visible && supported())
    else {
        let _ = window.hide();
        let _ = app.emit(BAR_EVENT, BarView { visible: false, horizontal: true });
        return;
    };

    let rect = place(taskbar, scale, prefs.bar_offset);
    let position = PhysicalPosition::new(rect.x, rect.y);
    *app.state::<BarState>().lock() = Some(position);
    let _ = window.set_size(PhysicalSize::new(rect.width, rect.height));
    let _ = window.set_position(position);
    let _ = window.set_focusable(false);
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.show();
    let _ = app.emit(BAR_EVENT, BarView { visible: true, horizontal: taskbar.is_horizontal() });
}

/// Called from the window's `Moved` event. Our own `set_position` fires it too,
/// so only a position we did not set counts as a drag worth remembering.
pub fn note_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    let state = app.state::<BarState>();
    if *state.lock() == Some(position) {
        return;
    }
    let Some((taskbar, scale)) = current_taskbar(app) else { return };
    let offset = offset_of(taskbar, position, scale);
    app.state::<Arc<AppState>>().set_bar_offset(Some(offset));
    *state.lock() = Some(position);
}

pub fn view(app: &AppHandle) -> BarView {
    let prefs = app.state::<Arc<AppState>>().prefs();
    let horizontal = current_taskbar(app).is_none_or(|(taskbar, _)| taskbar.is_horizontal());
    BarView { visible: prefs.bar_visible && supported(), horizontal }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MONITOR: Rect = Rect { x: 0, y: 0, width: 1440, height: 900 };

    #[test]
    fn the_taskbar_is_whatever_the_work_area_leaves_out() {
        let bottom = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        assert_eq!(bottom.side, TaskbarSide::Bottom);
        assert_eq!(bottom.rect, Rect { x: 0, y: 852, width: 1440, height: 48 });

        let top = taskbar_of(MONITOR, Rect { x: 0, y: 48, width: 1440, height: 852 }).unwrap();
        assert_eq!(top.side, TaskbarSide::Top);
        assert_eq!(top.rect.height, 48);

        let left = taskbar_of(MONITOR, Rect { x: 72, y: 0, width: 1368, height: 900 }).unwrap();
        assert_eq!(left.side, TaskbarSide::Left);
        assert_eq!(left.rect.width, 72);

        let right = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1368, height: 900 }).unwrap();
        assert_eq!(right.side, TaskbarSide::Right);
        assert_eq!(right.rect.x, 1368);
    }

    #[test]
    fn no_taskbar_means_no_bar() {
        // Auto-hidden taskbar: the work area is the whole monitor.
        assert_eq!(taskbar_of(MONITOR, MONITOR), None);
    }

    #[test]
    fn the_strip_lands_inside_the_taskbar_and_clear_of_the_tray_by_default() {
        let taskbar = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        let rect = place(taskbar, 1.0, None);
        assert_eq!(rect.y, 852);
        assert_eq!(rect.height, 48, "as tall as the taskbar");
        assert_eq!(rect.width, WIDTH as u32);
        // Right edge sits the default offset in from the screen edge.
        assert_eq!(rect.x + rect.width as i32, 1440 - DEFAULT_OFFSET as i32);
    }

    #[test]
    fn a_dragged_position_round_trips_through_the_remembered_offset() {
        let taskbar = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        let dragged_to = PhysicalPosition::new(600, 852);
        let offset = offset_of(taskbar, dragged_to, 1.0);
        let placed = place(taskbar, 1.0, Some(offset));
        assert_eq!(placed.x, 600, "the strip should come back where it was left");
    }

    #[test]
    fn the_strip_cannot_be_dragged_off_the_taskbar() {
        let taskbar = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        let too_far_right = place(taskbar, 1.0, Some(-500));
        assert!(too_far_right.x + too_far_right.width as i32 <= 1440);
        let too_far_left = place(taskbar, 1.0, Some(5000));
        assert!(too_far_left.x >= 0);
    }

    #[test]
    fn a_vertical_taskbar_turns_the_strip_on_its_side() {
        let taskbar = taskbar_of(MONITOR, Rect { x: 72, y: 0, width: 1368, height: 900 }).unwrap();
        let rect = place(taskbar, 1.0, None);
        assert_eq!(rect.x, 0);
        assert_eq!(rect.width, 72, "as wide as the taskbar");
        assert_eq!(rect.height, WIDTH as u32);
    }

    #[test]
    fn scaling_grows_the_strip_in_physical_pixels() {
        let taskbar = taskbar_of(
            Rect { x: 0, y: 0, width: 2880, height: 1800 },
            Rect { x: 0, y: 0, width: 2880, height: 1704 },
        )
        .unwrap();
        let rect = place(taskbar, 2.0, None);
        assert_eq!(rect.width, (WIDTH * 2.0) as u32);
    }
}
