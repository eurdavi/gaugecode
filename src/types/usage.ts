/**
 * Mirror of `src-tauri/src/model.rs` and the `ProviderView` in
 * `src-tauri/src/commands.rs`.
 * If you change one side, change the other in the same commit (AGENTS.md).
 *
 * Serialization conventions (serde):
 *  - enums without payload → lowercase string
 *  - `ProviderStatus` / `SignInHint` → externally tagged with `kind`, snake_case
 *  - `DateTime<Utc>` → RFC 3339 string
 */

export type ProviderId = "claude" | "cursor" | "codex";

export type Fidelity = "official" | "derived" | "manual";

export type Band = "ok" | "warn" | "hot" | "off";

export interface LimitWindow {
  /** "session", "weekly_all", "weekly_opus", "primary", ... */
  id: string;
  label: string;
  /** 0.0..=1.0 — always taken from the provider response, never estimated. */
  used_fraction: number;
  /** RFC 3339, or null when the provider did not report a reset time. */
  resets_at: string | null;
}

export interface ProviderAccount {
  email: string | null;
  plan: string | null;
}

export interface ProviderSnapshot {
  provider: ProviderId;
  fidelity: Fidelity;
  /** Ordered: `session` first, then the rest in provider order. */
  windows: LimitWindow[];
  /** RFC 3339 */
  fetched_at: string;
  account: ProviderAccount | null;
}

export type ProviderStatus =
  | { kind: "fresh" }
  | { kind: "stale"; age_secs: number }
  | { kind: "needs_auth" }
  | { kind: "error"; message: string }
  | { kind: "disabled" };

export type SignInHint =
  | { kind: "open_url"; url: string }
  | { kind: "open_app"; app: string }
  | { kind: "text"; text: string };

/** One row of the UI, as returned by the `get_state` command. */
export interface ProviderView {
  id: ProviderId;
  name: string;
  enabled: boolean;
  /** `null` means "no reading yet" — show a placeholder, never a zero. */
  status: ProviderStatus | null;
  snapshot: ProviderSnapshot | null;
  account: ProviderAccount | null;
  sign_in_hint: SignInHint | null;
  manage_url: string | null;
  /** False for providers whose adapter is not in this build yet. */
  implemented: boolean;
  is_primary: boolean;
}

/** Payload of the `usage:status` event. */
export interface StatusEvent {
  provider: ProviderId;
  status: ProviderStatus;
}

export type PrefUpdate =
  | { key: "provider_enabled"; provider: ProviderId; enabled: boolean }
  | { key: "primary"; provider: ProviderId }
  | { key: "notch_visible"; visible: boolean }
  | { key: "notch_edge"; edge: NotchEdge }
  | { key: "notch_animation"; animation: NotchAnimation }
  /** `null` goes back to following the operating system. */
  | { key: "language"; language: Language | null }
  | { key: "autostart"; enabled: boolean }
  | { key: "auto_update"; enabled: boolean };

/** Mirror of `NotchEdge` / `NotchMode` / `NotchAnimation` in `src-tauri/src/notch.rs`. */
export type NotchEdge = "right" | "left" | "top" | "bottom";

export type NotchMode = "folded" | "peek" | "pinned";

export type NotchAnimation = "slide" | "fade" | "instant";

/** Mirror of `Language` in `src-tauri/src/i18n.rs`. */
export type Language = "en" | "pt-BR" | "es";

export interface NotchView {
  mode: NotchMode;
  edge: NotchEdge;
  visible: boolean;
  animation: NotchAnimation;
  /** CSS pixels; the sliver the pointer watch is waiting for. */
  folded_thickness: number;
  folded_length: number;
}

/** Mirror of `PrefsView` in `src-tauri/src/commands.rs`. */
export interface PrefsView {
  /** Already resolved: never null, unlike `language_override`. */
  language: Language;
  language_override: Language | null;
  primary: ProviderId;
  notch_visible: boolean;
  notch_edge: NotchEdge;
  notch_animation: NotchAnimation;
  autostart: boolean;
  auto_update: boolean;
  demo: boolean;
  version: string;
}

/** Mirror of `AvailableUpdate` in `src-tauri/src/updater.rs`. */
export interface AvailableUpdate {
  version: string;
  notes: string | null;
}

export const SNAPSHOT_EVENT = "usage:snapshot";
export const STATUS_EVENT = "usage:status";
export const NOTCH_EVENT = "notch:state";
export const PREFS_EVENT = "prefs:changed";
export const UPDATE_EVENT = "update:available";
