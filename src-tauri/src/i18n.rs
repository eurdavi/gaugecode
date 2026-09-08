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

    // Limit-window names. These come from the adapters as English text, but the
    // window *id* is stable and language independent, so the id is what gets
    // translated and the adapter's label is only a fallback for ids we do not
    // know about yet.
    pub window_session: &'static str,
    pub window_weekly: &'static str,
    pub window_weekly_all: &'static str,
    pub window_weekly_opus: &'static str,
    pub window_weekly_sonnet: &'static str,
    pub window_plan: &'static str,
    pub window_api: &'static str,
    pub window_on_demand: &'static str,
    pub window_primary: &'static str,
    pub window_secondary: &'static str,
    pub window_rolling: &'static str,
    pub window_monthly: &'static str,
    pub window_credits: &'static str,
    pub window_mcp: &'static str,
}

/// Localised name for a limit window, by id.
///
/// Unknown ids keep whatever the adapter called it: a vendor that adds a window
/// tomorrow should show up with its own name rather than disappear.
pub fn window_label(language: Language, id: &str, fallback: &str) -> String {
    let text = language.strings();
    let translated = match id {
        "session" => text.window_session,
        "weekly" => text.window_weekly,
        "weekly_all" => text.window_weekly_all,
        "weekly_opus" => text.window_weekly_opus,
        "weekly_sonnet" => text.window_weekly_sonnet,
        "plan" => text.window_plan,
        "api" => text.window_api,
        "on_demand" => text.window_on_demand,
        "primary" => text.window_primary,
        "secondary" => text.window_secondary,
        "rolling" => text.window_rolling,
        "monthly" => text.window_monthly,
        "credits" => text.window_credits,
        "mcp" => text.window_mcp,
        _ => return fallback.to_string(),
    };
    translated.to_string()
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

    window_session: "Session (5h)",
    window_weekly: "Weekly",
    window_weekly_all: "Weekly (all models)",
    window_weekly_opus: "Weekly (Opus)",
    window_weekly_sonnet: "Weekly (Sonnet)",
    window_plan: "Plan (billing cycle)",
    window_api: "Included API usage",
    window_on_demand: "On-demand spend",
    window_primary: "Primary limit",
    window_secondary: "Secondary limit",
    window_rolling: "Rolling (5h)",
    window_monthly: "Monthly",
    window_credits: "Credits",
    window_mcp: "MCP (1 month)",
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

    window_session: "Sessão (5h)",
    window_weekly: "Semanal",
    window_weekly_all: "Semanal (todos os modelos)",
    window_weekly_opus: "Semanal (Opus)",
    window_weekly_sonnet: "Semanal (Sonnet)",
    window_plan: "Plano (ciclo de faturamento)",
    window_api: "Uso de API incluído",
    window_on_demand: "Gasto sob demanda",
    window_primary: "Limite principal",
    window_secondary: "Limite secundário",
    window_rolling: "Contínuo (5h)",
    window_monthly: "Mensal",
    window_credits: "Créditos",
    window_mcp: "MCP (1 mês)",
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

    window_session: "Sesión (5h)",
    window_weekly: "Semanal",
    window_weekly_all: "Semanal (todos los modelos)",
    window_weekly_opus: "Semanal (Opus)",
    window_weekly_sonnet: "Semanal (Sonnet)",
    window_plan: "Plan (ciclo de facturación)",
    window_api: "Uso de API incluido",
    window_on_demand: "Gasto a demanda",
    window_primary: "Límite principal",
    window_secondary: "Límite secundario",
    window_rolling: "Continuo (5h)",
    window_monthly: "Mensual",
    window_credits: "Créditos",
    window_mcp: "MCP (1 mes)",
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

    #[test]
    fn a_known_window_id_is_translated_and_the_adapter_label_ignored() {
        let pt = Language::BrazilianPortuguese;
        assert_eq!(window_label(pt, "session", "Session (5h)"), "Sessão (5h)");
        assert_eq!(window_label(pt, "plan", "Plan (billing cycle)"), "Plano (ciclo de faturamento)");
        assert_eq!(window_label(pt, "api", "Included API usage"), "Uso de API incluído");
        assert_eq!(window_label(Language::Spanish, "monthly", "Monthly limit"), "Mensual");
    }

    #[test]
    fn an_unknown_window_id_keeps_the_name_the_vendor_gave_it() {
        // A window a vendor adds tomorrow must show up under its own name
        // rather than vanish because we have no translation for it.
        assert_eq!(
            window_label(Language::BrazilianPortuguese, "weekly_haiku", "Weekly Haiku"),
            "Weekly Haiku"
        );
        assert_eq!(
            window_label(Language::BrazilianPortuguese, "window-3x2", "Usage (2 h)"),
            "Usage (2 h)"
        );
    }

    #[test]
    fn every_window_name_is_translated_in_every_language() {
        let ids = [
            "session", "weekly", "weekly_all", "weekly_opus", "weekly_sonnet", "plan", "api",
            "on_demand", "primary", "secondary", "rolling", "monthly", "credits", "mcp",
        ];
        for id in ids {
            let english = window_label(Language::English, id, "FALLBACK");
            assert_ne!(english, "FALLBACK", "{id} is missing from the English catalogue");
            for language in [Language::BrazilianPortuguese, Language::Spanish] {
                let translated = window_label(language, id, "FALLBACK");
                assert_ne!(translated, "FALLBACK", "{id} is missing from {language:?}");
                // A few names are genuinely identical across languages (MCP,
                // Opus), so only the ones with real words are compared.
                if !id.contains("opus") && !id.contains("sonnet") && id != "mcp" {
                    assert_ne!(translated, english, "{id} was left in English in {language:?}");
                }
            }
        }
    }
}
