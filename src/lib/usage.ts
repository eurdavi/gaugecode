import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { messagesFor, type Messages } from "../i18n";
import {
  NOTCH_EVENT,
  PREFS_EVENT,
  SNAPSHOT_EVENT,
  STATUS_EVENT,
  type Band,
  type LimitWindow,
  type NotchView,
  type PrefUpdate,
  type PrefsView,
  type ProviderSnapshot,
  type ProviderStatus,
  type ProviderView,
  type StatusEvent,
} from "../types/usage";

/**
 * Loads the current state once and then follows the Rust events. The UI never
 * fetches anything itself (SPEC §4).
 */
export function useProviders() {
  const [providers, setProviders] = useState<ProviderView[] | null>(null);

  const reload = useCallback(async () => {
    setProviders(await invoke<ProviderView[]>("get_state").catch(() => null));
  }, []);

  useEffect(() => {
    let cancelled = false;

    const unlisten = Promise.all([
      // Windows exist before the Rust state does, so the first `get_state` can
      // come back empty. `prefs:changed` fires once the backend is ready.
      listen(PREFS_EVENT, () => void reload()),
      listen<ProviderSnapshot>(SNAPSHOT_EVENT, ({ payload }) => {
        setProviders((current) => {
          if (!current) {
            void reload();
            return current;
          }
          return current.map((view) =>
            view.id === payload.provider
              ? { ...view, snapshot: payload, account: payload.account ?? view.account }
              : view,
          );
        });
      }),
      listen<StatusEvent>(STATUS_EVENT, ({ payload }) => {
        setProviders((current) => {
          if (!current) {
            void reload();
            return current;
          }
          return current.map((view) =>
            view.id === payload.provider ? { ...view, status: payload.status } : view,
          );
        });
      }),
    ]).then(async (fns) => {
      // Subscribe first, then read: an event that fires while invoke is in
      // flight is applied, not dropped.
      if (!cancelled) await reload();
      return fns;
    });

    return () => {
      cancelled = true;
      void unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [reload]);

  return { providers, reload };
}

/**
 * Preferences, kept in step across every window: Rust emits `prefs:changed`
 * whenever any of them moves, so the notch and Settings never disagree.
 */
export function usePrefs() {
  const [prefs, setPrefs] = useState<PrefsView | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlisten = listen<PrefsView>(PREFS_EVENT, ({ payload }) => setPrefs(payload)).then(
      async (fn) => {
        if (!cancelled) {
          try {
            setPrefs(await invoke<PrefsView>("get_prefs"));
          } catch {
            /* not managed yet; the event above delivers it */
          }
        }
        return fn;
      },
    );
    return () => {
      cancelled = true;
      void unlisten.then((fn) => fn());
    };
  }, []);

  return prefs;
}

/** The message catalogue for the language currently in use. */
export function useMessages(prefs: PrefsView | null): Messages {
  return useMemo(() => messagesFor(prefs?.language ?? "en"), [prefs?.language]);
}

export async function refreshNow(): Promise<string | null> {
  return invoke<string | null>("refresh_now");
}

export async function setPref(update: PrefUpdate): Promise<void> {
  await invoke("set_pref", { update });
}

/**
 * Follows the notch mode Rust decides (pointer watch, tray toggle, edge
 * change). The UI only draws it; it never moves the window itself.
 */
export function useNotch() {
  const [notch, setNotch] = useState<NotchView | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () =>
      invoke<NotchView>("get_notch")
        .then(setNotch)
        .catch(() => {
          /* not managed yet; `apply` emits the state at the end of setup */
        });

    const unlisten = Promise.all([
      listen<NotchView>(NOTCH_EVENT, ({ payload }) => setNotch(payload)),
      listen(PREFS_EVENT, () => void load()),
    ]).then(async (fns) => {
      if (!cancelled) await load();
      return fns;
    });

    return () => {
      cancelled = true;
      void unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);

  return notch;
}

export async function toggleNotchPin(): Promise<void> {
  await invoke("toggle_notch_pin");
}

export function bandOf(usedFraction: number): Band {
  if (usedFraction < 0.5) return "ok";
  if (usedFraction <= 0.8) return "warn";
  return "hot";
}

export const BAND_STROKE: Record<Band, string> = {
  ok: "var(--color-band-ok)",
  warn: "var(--color-band-warn)",
  hot: "var(--color-band-hot)",
  off: "var(--color-band-off)",
};

/** The window the main ring shows: `session` when there is one (SPEC §5). */
export function primaryWindow(snapshot: ProviderSnapshot | null): LimitWindow | null {
  if (!snapshot) return null;
  return snapshot.windows.find((window) => window.id === "session") ?? snapshot.windows[0] ?? null;
}

export function formatPercent(usedFraction: number): string {
  return `${Math.round(usedFraction * 100)}%`;
}

/** Numbers read the same in every language, so only the smallest case needs one. */
function humanizeMinutes(minutes: number, messages: Messages): string {
  if (minutes < 1) return messages.window.underAMinute;
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours < 24) return `${hours}h ${String(rest).padStart(2, "0")}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

export function formatReset(
  resetsAt: string | null,
  messages: Messages,
  now = Date.now(),
): string | null {
  if (!resetsAt) return null;
  const minutes = Math.floor((new Date(resetsAt).getTime() - now) / 60_000);
  if (minutes <= 0) return messages.window.resettingNow;
  return messages.window.resetsIn(humanizeMinutes(minutes, messages));
}

/** Short, human description of a status. `null` status means "no reading yet". */
export function describeStatus(status: ProviderStatus | null, messages: Messages): string {
  if (!status) return messages.status.waiting;
  switch (status.kind) {
    case "fresh":
      return messages.status.fresh;
    case "stale":
      return messages.status.old(humanizeMinutes(Math.floor(status.age_secs / 60), messages));
    case "needs_auth":
      return messages.status.needsAuth;
    case "error":
      return status.message;
    case "disabled":
      return messages.status.off;
  }
}

export function isDimmed(status: ProviderStatus | null): boolean {
  return status?.kind !== "fresh";
}
