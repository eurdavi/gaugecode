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

/// Logical width of the strip. Sized for a two-character window name, ten
/// blocks and a percentage — anything wider is empty space on the taskbar.
const WIDTH: f64 = 152.0;
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
    inner: Mutex<Runtime>,
}

#[derive(Default)]
struct Runtime {
    /// The position `apply` last set, so a `Moved` event can tell a drag apart
    /// from our own placement.
    last_applied: Option<PhysicalPosition<i32>>,
    /// Distance from the pointer to the strip's leading edge when it was
    /// grabbed, so the strip does not jump under the cursor.
    grab_gap: Option<i32>,
}

impl BarState {
    fn lock(&self) -> std::sync::MutexGuard<'_, Runtime> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn is_dragging(&self) -> bool {
        self.lock().grab_gap.is_some()
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
    let leading = if taskbar.is_horizontal() { position.x } else { position.y };
    offset_of_leading(taskbar, leading, scale)
}

/// The same, from just the coordinate along the taskbar's own axis.
pub fn offset_of_leading(taskbar: Taskbar, leading: i32, scale: f64) -> i32 {
    let width = (WIDTH * scale).round() as i32;
    let end = if taskbar.is_horizontal() {
        taskbar.rect.x + taskbar.rect.width as i32
    } else {
        taskbar.rect.y + taskbar.rect.height as i32
    };
    end - leading - width
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
    app.state::<BarState>().lock().last_applied = Some(position);
    let _ = window.set_size(PhysicalSize::new(rect.width, rect.height));
    let _ = window.set_position(position);
    let _ = window.set_focusable(false);
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.show();
    let _ = app.emit(BAR_EVENT, BarView { visible: true, horizontal: taskbar.is_horizontal() });
}

/// Puts the strip back above the taskbar.
///
/// Both windows are "always on top", and within that band Windows orders by
/// activation. The strip is never activated — it must not steal focus — so
/// Explorer's taskbar quietly ends up above it and hides it.
///
/// `SetWindowPos` re-asserts the position in that band directly. Going through
/// tao's `set_always_on_top` instead does not work: it diffs window flags and
/// skips the call when the flag is already set, so it has to be toggled off and
/// on — and that off, however brief, is a visible flicker.
#[cfg(target_os = "windows")]
pub fn raise(app: &AppHandle) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };

    let Some(window) = app.get_webview_window(BAR_WINDOW) else { return };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    // Tauri hands back the `windows` crate's HWND; `windows-sys` wants the bare
    // pointer inside it.
    let Ok(hwnd) = window.hwnd() else { return };

    // SWP_NOACTIVATE matters as much as the rest: raising the strip must not
    // take focus from whatever the user is typing in.
    unsafe {
        SetWindowPos(
            hwnd.0,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(target_os = "windows"))]
pub fn raise(_app: &AppHandle) {}

/// Whether the primary mouse button is still held.
///
/// The strip is not focusable, so the usual window move loop refuses to run and
/// a `pointerup` in the webview cannot be relied on either — the pointer spends
/// the drag outside the window. Asking the OS directly is what works.
#[cfg(target_os = "windows")]
fn primary_button_down() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    // The high bit means "currently down".
    unsafe { GetAsyncKeyState(VK_LBUTTON as i32) < 0 }
}

#[cfg(not(target_os = "windows"))]
fn primary_button_down() -> bool {
    false
}

/// Called from the window's `Moved` event. Our own `set_position` fires it too,
/// so only a position we did not set counts as a move worth remembering.
pub fn note_moved(app: &AppHandle, position: PhysicalPosition<i32>) {
    let state = app.state::<BarState>();
    if state.lock().last_applied == Some(position) {
        return;
    }
    let Some((taskbar, scale)) = current_taskbar(app) else { return };
    let offset = offset_of(taskbar, position, scale);
    app.state::<Arc<AppState>>().set_bar_offset(Some(offset));
    state.lock().last_applied = Some(position);
}

/// Starts a drag: records where along the strip it was grabbed.
///
/// The webview only says "the grip went down"; the follow loop and the release
/// are handled here, because a non-focusable window gets neither the OS move
/// loop nor a reliable `pointerup`.
pub fn begin_drag(app: &AppHandle) {
    let Some((taskbar, _)) = current_taskbar(app) else { return };
    let Ok(cursor) = app.cursor_position() else { return };
    let Some(window) = app.get_webview_window(BAR_WINDOW) else { return };
    let Ok(position) = window.outer_position() else { return };

    let gap = if taskbar.is_horizontal() {
        cursor.x as i32 - position.x
    } else {
        cursor.y as i32 - position.y
    };
    app.state::<BarState>().lock().grab_gap = Some(gap);
}

/// One step of a drag. Returns false once the button is up, so the caller can
/// stop the follow loop.
pub fn drag_step(app: &AppHandle) -> bool {
    let state = app.state::<BarState>();
    let Some(gap) = state.lock().grab_gap else { return false };

    if !primary_button_down() {
        state.lock().grab_gap = None;
        // Saved here rather than through `note_moved`: every drag step records
        // its own position as `last_applied`, so `note_moved` would see the
        // final position as one of ours and skip persisting it — which is
        // exactly why a dragged strip used to jump home on the next change.
        if let Some((taskbar, scale)) = current_taskbar(app) {
            if let Some(window) = app.get_webview_window(BAR_WINDOW) {
                if let Ok(position) = window.outer_position() {
                    let offset = offset_of(taskbar, position, scale);
                    tracing::debug!(offset, "remembering where the bar was dropped");
                    app.state::<Arc<AppState>>().set_bar_offset(Some(offset));
                }
            }
        }
        return false;
    }

    let Some((taskbar, scale)) = current_taskbar(app) else { return false };
    let Ok(cursor) = app.cursor_position() else { return true };
    let Some(window) = app.get_webview_window(BAR_WINDOW) else { return false };

    // Reuse `place`, so a drag is clamped by exactly the same rule that keeps
    // the strip on the taskbar at launch.
    let leading = if taskbar.is_horizontal() { cursor.x as i32 } else { cursor.y as i32 } - gap;
    let rect = place(taskbar, scale, Some(offset_of_leading(taskbar, leading, scale)));
    let position = PhysicalPosition::new(rect.x, rect.y);
    state.lock().last_applied = Some(position);
    let _ = window.set_position(position);
    true
}

pub fn cancel_drag(app: &AppHandle) {
    app.state::<BarState>().lock().grab_gap = None;
}

pub fn is_dragging(app: &AppHandle) -> bool {
    app.try_state::<BarState>().is_some_and(|state| state.is_dragging())
}

/// Where the popup should sit when it was opened from the strip: alongside it,
/// on the inner side of the taskbar, and clamped so it stays on screen.
///
/// Taking the strip's own rectangle rather than the taskbar's means the popup
/// follows wherever the strip was dragged to.
pub fn popup_position(
    strip: Rect,
    taskbar: Taskbar,
    work_area: Rect,
    popup: (u32, u32),
) -> PhysicalPosition<i32> {
    let (popup_width, popup_height) = popup;

    let (x, y) = if taskbar.is_horizontal() {
        // Centred on the strip, and on whichever side of the bar the desktop is.
        let x = strip.x + strip.width as i32 / 2 - popup_width as i32 / 2;
        let y = match taskbar.side {
            TaskbarSide::Bottom => taskbar.rect.y - popup_height as i32,
            _ => taskbar.rect.y + taskbar.rect.height as i32,
        };
        (x, y)
    } else {
        let y = strip.y + strip.height as i32 / 2 - popup_height as i32 / 2;
        let x = match taskbar.side {
            TaskbarSide::Left => taskbar.rect.x + taskbar.rect.width as i32,
            _ => taskbar.rect.x - popup_width as i32,
        };
        (x, y)
    };

    // Never off the edge of the usable desktop.
    let max_x = work_area.x + work_area.width as i32 - popup_width as i32;
    let max_y = work_area.y + work_area.height as i32 - popup_height as i32;
    PhysicalPosition::new(
        x.clamp(work_area.x, max_x.max(work_area.x)),
        y.clamp(work_area.y, max_y.max(work_area.y)),
    )
}

/// Shows the tray popup beside the strip. Falls back to the caller's own
/// placement when there is no taskbar to work from.
pub fn show_popup_beside(app: &AppHandle) -> bool {
    let Some(popup) = app.get_webview_window(crate::tray::POPUP_WINDOW) else { return false };
    let Some(strip) = app.get_webview_window(BAR_WINDOW) else { return false };
    let Some((monitor, work_area, _)) = monitor_and_work_area(app) else { return false };
    let Some(taskbar) = taskbar_of(monitor, work_area) else { return false };

    let (Ok(position), Ok(size), Ok(popup_size)) =
        (strip.outer_position(), strip.outer_size(), popup.outer_size())
    else {
        return false;
    };
    let strip_rect =
        Rect { x: position.x, y: position.y, width: size.width, height: size.height };

    let at = popup_position(
        strip_rect,
        taskbar,
        work_area,
        (popup_size.width, popup_size.height),
    );
    let _ = popup.set_position(at);
    let _ = popup.show();
    let _ = popup.set_focus();
    true
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
    fn dragging_by_the_grip_lands_the_leading_edge_under_the_pointer() {
        // A drag records the gap between pointer and leading edge, then feeds
        // the pointer's position back through `place`. Grabbing 20 px in and
        // moving to x=700 should put the leading edge at 680.
        let taskbar = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        let grab_gap = 20;
        let leading = 700 - grab_gap;
        let rect = place(taskbar, 1.0, Some(offset_of_leading(taskbar, leading, 1.0)));
        assert_eq!(rect.x, 680);
    }

    #[test]
    fn the_strip_can_be_dragged_to_the_far_right_of_the_taskbar() {
        // The complaint that started this: it would not go right. The clamp has
        // to allow the strip's trailing edge within a margin of the screen edge.
        let taskbar = taskbar_of(MONITOR, Rect { x: 0, y: 0, width: 1440, height: 852 }).unwrap();
        let far_right = place(taskbar, 1.0, Some(offset_of_leading(taskbar, 9999, 1.0)));
        assert_eq!(far_right.x + far_right.width as i32, 1440 - EDGE_MARGIN as i32);
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

    const WORK: Rect = Rect { x: 0, y: 0, width: 1440, height: 852 };

    #[test]
    fn the_popup_opens_beside_the_strip_not_over_by_the_tray() {
        let taskbar = taskbar_of(MONITOR, WORK).unwrap();
        // A strip dragged to the middle of the taskbar.
        let strip = Rect { x: 600, y: 852, width: 152, height: 48 };
        let at = popup_position(strip, taskbar, WORK, (340, 420));

        // Centred on the strip: 600 + 76 - 170.
        assert_eq!(at.x, 506);
        // Sitting on top of the taskbar, not over it.
        assert_eq!(at.y, 852 - 420);
    }

    #[test]
    fn the_popup_follows_the_strip_when_it_moves() {
        let taskbar = taskbar_of(MONITOR, WORK).unwrap();
        let left = popup_position(Rect { x: 300, y: 852, width: 152, height: 48 }, taskbar, WORK, (340, 420));
        let right = popup_position(Rect { x: 900, y: 852, width: 152, height: 48 }, taskbar, WORK, (340, 420));
        assert_eq!(right.x - left.x, 600);
    }

    #[test]
    fn the_popup_never_hangs_off_the_screen() {
        let taskbar = taskbar_of(MONITOR, WORK).unwrap();
        // A strip parked hard against the right edge would centre a popup
        // half-way off the desktop.
        let at = popup_position(
            Rect { x: 1284, y: 852, width: 152, height: 48 },
            taskbar,
            WORK,
            (340, 420),
        );
        assert_eq!(at.x + 340, 1440, "clamped to the right edge");

        let at = popup_position(Rect { x: 0, y: 852, width: 152, height: 48 }, taskbar, WORK, (340, 420));
        assert_eq!(at.x, 0, "and to the left edge");
    }

    #[test]
    fn the_popup_opens_below_a_taskbar_at_the_top() {
        let work = Rect { x: 0, y: 48, width: 1440, height: 852 };
        let taskbar = taskbar_of(MONITOR, work).unwrap();
        let at = popup_position(Rect { x: 600, y: 0, width: 152, height: 48 }, taskbar, work, (340, 420));
        assert_eq!(at.y, 48, "below the bar, not above the screen");
    }

    #[test]
    fn the_popup_opens_beside_a_vertical_taskbar() {
        let work = Rect { x: 72, y: 0, width: 1368, height: 900 };
        let taskbar = taskbar_of(MONITOR, work).unwrap();
        let at = popup_position(Rect { x: 0, y: 400, width: 72, height: 152 }, taskbar, work, (340, 420));
        assert_eq!(at.x, 72, "to the right of a left-hand bar");
        // Centred on the strip: 400 + 76 - 210.
        assert_eq!(at.y, 266);
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
