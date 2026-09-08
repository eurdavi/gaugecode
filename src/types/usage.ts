/**
 * Mirror of `src-tauri/src/model.rs`.
 * If you change one side, change the other in the same commit (AGENTS.md).
 *
 * Serialization conventions (serde):
 *  - enums without payload → lowercase string
 *  - `ProviderStatus` → externally tagged with `kind`, snake_case
 *  - `DateTime<Utc>` → RFC 3339 string
 */

export type ProviderId = "claude" | "cursor" | "codex";

export type Fidelity = "official" | "derived" | "manual";

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

/** Payload of the `usage:snapshot` event. */
export interface SnapshotEvent {
  snapshot: ProviderSnapshot;
}

/** Payload of the `usage:status` event. */
export interface StatusEvent {
  provider: ProviderId;
  status: ProviderStatus;
}
