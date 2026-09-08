//! Tray icon, native menu and the popup it toggles (SPEC §9.2).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::icon::{self, ICON_SIZE};
use crate::state::AppState;

pub const TRAY_ID: &str = "gaugecode";
pub const POPUP_WINDOW: &str = "popup";
pub const SETTINGS_WINDOW: &str = "settings";

/// Clicking the tray icon while the popup has focus first blurs it (which hides
/// it), so a toggle right after a hide must be ignored or the popup can never be
/// closed from the tray.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

pub struct TrayState {
    refresh_item: MenuItem<Wry>,
    last_hidden: Mutex<Option<Instant>>,
}

impl TrayState {
    pub fn note_popup_hidden(&self) {
        if let Ok(mut last) = self.last_hidden.lock() {
            *last = Some(Instant::now());
        }
    }

    fn hidden_just_now(&self) -> bool {
        self.last_hidden
            .lock()
            .ok()
            .and_then(|last| *last)
            .is_some_and(|at| at.elapsed() < REOPEN_GUARD)
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let refresh_item = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit GaugeCode", true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &refresh_item as &dyn IsMenuItem<Wry>,
            &first_separator,
            &settings_item,
            &second_separator,
            &quit_item,
        ],
    )?;

    app.manage(TrayState { refresh_item, last_hidden: Mutex::new(None) });

    let initial = icon::render(None, crate::model::Band::Off, false, cfg!(target_os = "macos"));

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(initial, ICON_SIZE, ICON_SIZE))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip("GaugeCode")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "refresh" => {
                app.state::<Arc<AppState>>().request_refresh();
            }
            "settings" => show_settings(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            // Feeds the plugin the tray rect it needs to place the popup.
            tauri_plugin_positioner::on_tray_event(app, &event);
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_popup(app);
            }
        })
        .build(app)?;

    Ok(())
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS_WINDOW) {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn toggle_popup(app: &AppHandle) {
    let Some(window) = app.get_webview_window(POPUP_WINDOW) else { return };

    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if app.state::<TrayState>().hidden_just_now() {
        return;
    }

    // `TrayCenter` sits the popup directly above the tray icon (below it when
    // there is no room), and `constrained` keeps it on screen.
    if let Err(error) = window.move_window_constrained(Position::TrayCenter) {
        tracing::debug!(%error, "could not anchor popup to the tray; showing at its last position");
    }
    let _ = window.show();
    let _ = window.set_focus();
}

/// Repaints the icon when the whole percent changed, and always refreshes the
/// tooltip and the "Refresh now" label.
pub fn update(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();

    if let Some(face) = state.face_if_changed() {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let rgba =
                icon::render(face.percent, face.band, face.dimmed, cfg!(target_os = "macos"));
            if let Err(error) = tray.set_icon(Some(Image::new_owned(rgba, ICON_SIZE, ICON_SIZE))) {
                tracing::warn!(%error, "could not update tray icon");
            }
        }
    }

    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip(&state)));
    }

    let primary = state.prefs().primary;
    let tray_state = app.state::<TrayState>();
    match state.backoff_until(primary) {
        Some(until) => {
            let local = until.with_timezone(&Local).format("%H:%M");
            let _ = tray_state.refresh_item.set_text(format!("Waiting until {local}"));
            let _ = tray_state.refresh_item.set_enabled(false);
        }
        None => {
            let _ = tray_state.refresh_item.set_text("Refresh now");
            let _ = tray_state.refresh_item.set_enabled(true);
        }
    }
}

fn tooltip(state: &AppState) -> String {
    let primary = state.prefs().primary;
    let name = primary.display_name();

    let Some(snapshot) = state.snapshot(primary) else {
        return format!("GaugeCode — {name}: no reading yet");
    };
    let Some(window) = snapshot.primary_window() else {
        return format!("GaugeCode — {name}: nothing metered");
    };

    let percent = (window.used_fraction * 100.0).round() as i64;
    let mut line = format!("{name} — {} {percent}%", window.label);
    if let Some(reset) = humanize_reset(window.resets_at, Utc::now()) {
        line.push_str(&format!(" · {reset}"));
    }
    if let Some(age) = stale_age(state, primary) {
        line.push_str(&format!(" · {age}"));
    }
    line
}

fn stale_age(state: &AppState, provider: crate::model::ProviderId) -> Option<String> {
    match state.status(provider)? {
        crate::model::ProviderStatus::Stale { age_secs } => {
            Some(format!("{} old", humanize_minutes((age_secs / 60) as i64)))
        }
        crate::model::ProviderStatus::NeedsAuth => Some("needs sign-in".into()),
        crate::model::ProviderStatus::Error { message } => Some(message),
        _ => None,
    }
}

fn humanize_reset(resets_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> Option<String> {
    let at = resets_at?;
    let minutes = at.signed_duration_since(now).num_minutes();
    if minutes <= 0 {
        return Some("resetting now".into());
    }
    Some(format!("resets in {}", humanize_minutes(minutes)))
}

fn humanize_minutes(minutes: i64) -> String {
    if minutes < 1 {
        return "under a minute".into();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let (hours, rest) = (minutes / 60, minutes % 60);
    if hours < 24 {
        return format!("{hours}h {rest:02}m");
    }
    format!("{}d {}h", hours / 24, hours % 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minutes_are_humanized_in_the_expected_steps() {
        assert_eq!(humanize_minutes(0), "under a minute");
        assert_eq!(humanize_minutes(1), "1m");
        assert_eq!(humanize_minutes(59), "59m");
        assert_eq!(humanize_minutes(60), "1h 00m");
        assert_eq!(humanize_minutes(133), "2h 13m");
        assert_eq!(humanize_minutes(60 * 24), "1d 0h");
        assert_eq!(humanize_minutes(60 * 50), "2d 2h");
    }

    #[test]
    fn a_past_reset_reads_as_resetting_and_a_missing_one_is_omitted() {
        let now = DateTime::parse_from_rfc3339("2026-09-07T12:00:00Z").unwrap().with_timezone(&Utc);
        let at = |offset_minutes: i64| Some(now + chrono::Duration::minutes(offset_minutes));

        assert_eq!(humanize_reset(None, now), None);
        assert_eq!(humanize_reset(at(-5), now).as_deref(), Some("resetting now"));
        assert_eq!(humanize_reset(at(0), now).as_deref(), Some("resetting now"));
        assert_eq!(humanize_reset(at(133), now).as_deref(), Some("resets in 2h 13m"));
        assert_eq!(humanize_reset(at(45), now).as_deref(), Some("resets in 45m"));
    }
}
