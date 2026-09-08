//! Native-side translations: the tray menu, the tooltip and the few strings a
//! command hands back to the UI.
//!
//! The webview strings live in `src/i18n/`. These are separate on purpose — the
//! tray menu and the tooltip are drawn by the OS before any window exists, so
//! they cannot come from JavaScript.
//!
//! Every catalogue is one `Strings` value, so a missing translation is a
//! compile error rather than a key that silently falls back to English.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "pt-BR")]
    BrazilianPortuguese,
    #[serde(rename = "es")]
    Spanish,
}

impl Language {
    pub const ALL: [Language; 3] =
        [Language::English, Language::BrazilianPortuguese, Language::Spanish];

    /// Maps a BCP-47 tag from the OS onto a language we ship. Only the primary
    /// subtag matters, except for Portuguese where we only have pt-BR — close
    /// enough for pt-PT that showing it beats falling back to English.
    pub fn from_bcp47(tag: &str) -> Option<Self> {
        let primary = tag.split(['-', '_']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Language::English),
            "pt" => Some(Language::BrazilianPortuguese),
            "es" => Some(Language::Spanish),
            _ => None,
        }
    }

    /// The language to use when the user has not chosen one. Anything we do not
    /// ship lands on English.
    pub fn from_os() -> Self {
        tauri_plugin_os::locale()
            .and_then(|tag| Self::from_bcp47(&tag))
            .unwrap_or(Language::English)
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Language::English => &EN,
            Language::BrazilianPortuguese => &PT_BR,
            Language::Spanish => &ES,
        }
    }
}

/// Strings drawn by the OS. `{}` marks the single interpolation slot; use
/// [`fill`] to substitute it.
pub struct Strings {
    pub menu_refresh: &'static str,
    /// `{}` = local clock time the penalty ends.
    pub menu_waiting_until: &'static str,
    pub menu_hide_notch: &'static str,
    pub menu_show_notch: &'static str,
    pub menu_settings: &'static str,
    pub menu_quit: &'static str,

    /// `{}` = provider name.
    pub tooltip_no_reading: &'static str,
    /// `{}` = provider name.
    pub tooltip_nothing_metered: &'static str,
    /// `{}` = humanized duration.
    pub resets_in: &'static str,
    pub resetting_now: &'static str,
    pub under_a_minute: &'static str,
    /// `{}` = humanized duration.
    pub age_old: &'static str,
    pub needs_sign_in: &'static str,
    /// `{}` = local clock time the penalty ends.
    pub rate_limited_until: &'static str,
}

/// Replaces the single `{}` slot in a template.
pub fn fill(template: &str, value: &str) -> String {
    template.replacen("{}", value, 1)
}

static EN: Strings = Strings {
    menu_refresh: "Refresh now",
    menu_waiting_until: "Waiting until {}",
    menu_hide_notch: "Hide notch",
    menu_show_notch: "Show notch",
    menu_settings: "Settings…",
    menu_quit: "Quit GaugeCode",

    tooltip_no_reading: "{}: no reading yet",
    tooltip_nothing_metered: "{}: nothing metered",
    resets_in: "resets in {}",
    resetting_now: "resetting now",
    under_a_minute: "under a minute",
    age_old: "{} old",
    needs_sign_in: "needs sign-in",
    rate_limited_until: "Rate limited — waiting until {}",
};

static PT_BR: Strings = Strings {
    menu_refresh: "Atualizar agora",
    menu_waiting_until: "Aguardando até {}",
    menu_hide_notch: "Ocultar notch",
    menu_show_notch: "Mostrar notch",
    menu_settings: "Preferências…",
    menu_quit: "Sair do GaugeCode",

    tooltip_no_reading: "{}: sem leitura ainda",
    tooltip_nothing_metered: "{}: nada medido",
    resets_in: "reseta em {}",
    resetting_now: "resetando agora",
    under_a_minute: "menos de um minuto",
    age_old: "há {}",
    needs_sign_in: "precisa de login",
    rate_limited_until: "Limite atingido — aguardando até {}",
};

static ES: Strings = Strings {
    menu_refresh: "Actualizar ahora",
    menu_waiting_until: "Esperando hasta {}",
    menu_hide_notch: "Ocultar notch",
    menu_show_notch: "Mostrar notch",
    menu_settings: "Preferencias…",
    menu_quit: "Salir de GaugeCode",

    tooltip_no_reading: "{}: sin lectura todavía",
    tooltip_nothing_metered: "{}: nada medido",
    resets_in: "se reinicia en {}",
    resetting_now: "reiniciándose ahora",
    under_a_minute: "menos de un minuto",
    age_old: "hace {}",
    needs_sign_in: "requiere inicio de sesión",
    rate_limited_until: "Límite alcanzado — esperando hasta {}",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_locale_tag_maps_onto_a_language_we_ship() {
        assert_eq!(Language::from_bcp47("pt-BR"), Some(Language::BrazilianPortuguese));
        assert_eq!(Language::from_bcp47("pt_BR"), Some(Language::BrazilianPortuguese));
        // We only ship pt-BR; European Portuguese is closer to it than to English.
        assert_eq!(Language::from_bcp47("pt-PT"), Some(Language::BrazilianPortuguese));
        assert_eq!(Language::from_bcp47("es-419"), Some(Language::Spanish));
        assert_eq!(Language::from_bcp47("en-GB"), Some(Language::English));
        assert_eq!(Language::from_bcp47("ja-JP"), None);
        assert_eq!(Language::from_bcp47(""), None);
    }

    #[test]
    fn language_survives_a_json_round_trip() {
        for language in Language::ALL {
            let json = serde_json::to_string(&language).unwrap();
            assert_eq!(serde_json::from_str::<Language>(&json).unwrap(), language);
        }
        assert_eq!(serde_json::to_string(&Language::BrazilianPortuguese).unwrap(), "\"pt-BR\"");
    }

    #[test]
    fn every_language_translates_every_string() {
        // The struct makes a missing key impossible; this catches a copy-paste
        // that left an English string behind in another catalogue.
        for language in [Language::BrazilianPortuguese, Language::Spanish] {
            let translated = language.strings();
            assert_ne!(translated.menu_refresh, EN.menu_refresh);
            assert_ne!(translated.menu_quit, EN.menu_quit);
            assert_ne!(translated.resets_in, EN.resets_in);
            assert_ne!(translated.needs_sign_in, EN.needs_sign_in);
        }
    }

    #[test]
    fn fill_replaces_the_slot_once() {
        assert_eq!(fill("resets in {}", "2h 13m"), "resets in 2h 13m");
        assert_eq!(fill("no slot", "x"), "no slot");
    }
}
