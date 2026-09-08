//! Tray icon, native menu and the popup it toggles (SPEC §9.2).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_positioner::{Position, WindowExt};

use crate::i18n::{self, fill, Language, Strings};
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
    notch_item: MenuItem<Wry>,
    bar_item: MenuItem<Wry>,
    settings_item: MenuItem<Wry>,
    quit_item: MenuItem<Wry>,
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
    // The menu is built before any window exists, so these strings come from the
    // Rust catalogue and are re-applied by `update` whenever the language changes.
    let text = app.state::<Arc<AppState>>().language().strings();

    let refresh_item = MenuItem::with_id(app, "refresh", text.menu_refresh, true, None::<&str>)?;
    let notch_item = MenuItem::with_id(app, "notch", text.menu_hide_notch, true, None::<&str>)?;
    // Only offered where there is a taskbar to sit on.
    let bar_item = MenuItem::with_id(
        app,
        "bar",
        text.menu_show_bar,
        crate::bar::supported(),
        None::<&str>,
    )?;
    let settings_item = MenuItem::with_id(app, "settings", text.menu_settings, true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", text.menu_quit, true, None::<&str>)?;
    let first_separator = PredefinedMenuItem::separator(app)?;
    let second_separator = PredefinedMenuItem::separator(app)?;

    let menu = Menu::with_items(
        app,
        &[
            &refresh_item as &dyn IsMenuItem<Wry>,
            &notch_item,
            &bar_item,
            &first_separator,
            &settings_item,
            &second_separator,
            &quit_item,
        ],
    )?;

    app.manage(TrayState {
        refresh_item,
        notch_item,
        bar_item,
        settings_item,
        quit_item,
        last_hidden: Mutex::new(None),
    });

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
            "notch" => {
                let state = app.state::<Arc<AppState>>();
                let visible = !state.prefs().notch_visible;
                state.set_notch_visible(visible);
                crate::notch::apply(app, crate::notch::NotchMode::Folded);
                crate::commands::publish_prefs(app);
                update(app);
            }
            "bar" => {
                let state = app.state::<Arc<AppState>>();
                let visible = !state.prefs().bar_visible;
                state.set_bar_visible(visible);
                crate::bar::apply(app);
                crate::commands::publish_prefs(app);
                update(app);
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
/// tooltip and every menu label — the labels are re-applied because the language
/// can change while the app is running.
pub fn update(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>();
    let language = state.language();
    let text = language.strings();

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
        let _ = tray.set_tooltip(Some(tooltip(&state, language)));
    }

    let prefs = state.prefs();
    let tray_state = app.state::<TrayState>();
    let _ = tray_state.settings_item.set_text(text.menu_settings);
    let _ = tray_state.quit_item.set_text(text.menu_quit);
    let _ = tray_state.notch_item.set_text(if prefs.notch_visible {
        text.menu_hide_notch
    } else {
        text.menu_show_notch
    });
    let _ = tray_state.bar_item.set_text(if prefs.bar_visible {
        text.menu_hide_bar
    } else {
        text.menu_show_bar
    });

    match state.backoff_until(prefs.primary) {
        Some(until) => {
            let local = until.with_timezone(&Local).format("%H:%M").to_string();
            let _ = tray_state.refresh_item.set_text(fill(text.menu_waiting_until, &local));
            let _ = tray_state.refresh_item.set_enabled(false);
        }
        None => {
            let _ = tray_state.refresh_item.set_text(text.menu_refresh);
            let _ = tray_state.refresh_item.set_enabled(true);
        }
    }
}

fn tooltip(state: &AppState, language: Language) -> String {
    let text = language.strings();
    // Whatever provider the icon is showing, so the number and the name always
    // describe the same thing.
    let primary = state.tray_provider();
    let name = primary.display_name();

    let Some(snapshot) = state.snapshot(primary) else {
        return format!("GaugeCode — {}", fill(text.tooltip_no_reading, name));
    };
    let Some(window) = snapshot.primary_window() else {
        return format!("GaugeCode — {}", fill(text.tooltip_nothing_metered, name));
    };

    let percent = (window.used_fraction * 100.0).round() as i64;
    let label = i18n::window_label(language, &window.id, &window.label);
    let mut line = format!("{name} — {label} {percent}%");
    if let Some(reset) = humanize_reset(window.resets_at, Utc::now(), text) {
        line.push_str(&format!(" · {reset}"));
    }
    if let Some(age) = stale_age(state, primary, text) {
        line.push_str(&format!(" · {age}"));
    }
    line
}

fn stale_age(
    state: &AppState,
    provider: crate::model::ProviderId,
    text: &Strings,
) -> Option<String> {
    match state.status(provider)? {
        crate::model::ProviderStatus::Stale { age_secs } => {
            Some(fill(text.age_old, &humanize_minutes((age_secs / 60) as i64, text)))
        }
        crate::model::ProviderStatus::NeedsAuth => Some(text.needs_sign_in.into()),
        crate::model::ProviderStatus::Error { message } => Some(message),
        _ => None,
    }
}

fn humanize_reset(
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    text: &Strings,
) -> Option<String> {
    let at = resets_at?;
    let minutes = at.signed_duration_since(now).num_minutes();
    if minutes <= 0 {
        return Some(text.resetting_now.into());
    }
    Some(fill(text.resets_in, &humanize_minutes(minutes, text)))
}

/// Durations stay numeric on purpose: `2h 13m` reads the same in all three
/// languages, so only the "under a minute" case needs translating.
fn humanize_minutes(minutes: i64, text: &Strings) -> String {
    if minutes < 1 {
        return text.under_a_minute.into();
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

    fn en() -> &'static Strings {
        Language::English.strings()
    }

    #[test]
    fn minutes_are_humanized_in_the_expected_steps() {
        assert_eq!(humanize_minutes(0, en()), "under a minute");
        assert_eq!(humanize_minutes(1, en()), "1m");
        assert_eq!(humanize_minutes(59, en()), "59m");
        assert_eq!(humanize_minutes(60, en()), "1h 00m");
        assert_eq!(humanize_minutes(133, en()), "2h 13m");
        assert_eq!(humanize_minutes(60 * 24, en()), "1d 0h");
        assert_eq!(humanize_minutes(60 * 50, en()), "2d 2h");
    }

    #[test]
    fn a_past_reset_reads_as_resetting_and_a_missing_one_is_omitted() {
        let now = DateTime::parse_from_rfc3339("2026-09-07T12:00:00Z").unwrap().with_timezone(&Utc);
        let at = |offset_minutes: i64| Some(now + chrono::Duration::minutes(offset_minutes));

        assert_eq!(humanize_reset(None, now, en()), None);
        assert_eq!(humanize_reset(at(-5), now, en()).as_deref(), Some("resetting now"));
        assert_eq!(humanize_reset(at(0), now, en()).as_deref(), Some("resetting now"));
        assert_eq!(humanize_reset(at(133), now, en()).as_deref(), Some("resets in 2h 13m"));
        assert_eq!(humanize_reset(at(45), now, en()).as_deref(), Some("resets in 45m"));
    }

    #[test]
    fn the_reset_line_follows_the_chosen_language() {
        let now = DateTime::parse_from_rfc3339("2026-09-07T12:00:00Z").unwrap().with_timezone(&Utc);
        let at = Some(now + chrono::Duration::minutes(133));

        let pt = Language::BrazilianPortuguese.strings();
        assert_eq!(humanize_reset(at, now, pt).as_deref(), Some("reseta em 2h 13m"));
        let es = Language::Spanish.strings();
        assert_eq!(humanize_reset(at, now, es).as_deref(), Some("se reinicia en 2h 13m"));
        // The numeric part is language independent, so it must not be translated.
        assert_eq!(humanize_minutes(133, pt), "2h 13m");
    }
}
