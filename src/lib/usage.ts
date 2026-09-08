import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  SNAPSHOT_EVENT,
  STATUS_EVENT,
  type Band,
  type LimitWindow,
  type PrefUpdate,
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
    setProviders(await invoke<ProviderView[]>("get_state"));
  }, []);

  useEffect(() => {
    void reload();

    const unlisten = Promise.all([
      listen<ProviderSnapshot>(SNAPSHOT_EVENT, ({ payload }) => {
        setProviders((current) =>
          current?.map((view) =>
            view.id === payload.provider
              ? { ...view, snapshot: payload, account: payload.account ?? view.account }
              : view,
          ) ?? current,
        );
      }),
      listen<StatusEvent>(STATUS_EVENT, ({ payload }) => {
        setProviders((current) =>
          current?.map((view) =>
            view.id === payload.provider ? { ...view, status: payload.status } : view,
          ) ?? current,
        );
      }),
    ]);

    return () => {
      void unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [reload]);

  return { providers, reload };
}

export async function refreshNow(): Promise<string | null> {
  return invoke<string | null>("refresh_now");
}

export async function setPref(update: PrefUpdate): Promise<void> {
  await invoke("set_pref", { update });
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

function humanizeMinutes(minutes: number): string {
  if (minutes < 1) return "under a minute";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours < 24) return `${hours}h ${String(rest).padStart(2, "0")}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

export function formatReset(resetsAt: string | null, now = Date.now()): string | null {
  if (!resetsAt) return null;
  const minutes = Math.floor((new Date(resetsAt).getTime() - now) / 60_000);
  if (minutes <= 0) return "resetting now";
  return `resets in ${humanizeMinutes(minutes)}`;
}

/** Short, human description of a status. `null` status means "no reading yet". */
export function describeStatus(status: ProviderStatus | null): string {
  if (!status) return "waiting for first reading";
  switch (status.kind) {
    case "fresh":
      return "up to date";
    case "stale":
      return `${humanizeMinutes(Math.floor(status.age_secs / 60))} old`;
    case "needs_auth":
      return "needs sign-in";
    case "error":
      return status.message;
    case "disabled":
      return "off";
  }
}

export function isDimmed(status: ProviderStatus | null): boolean {
  return status?.kind !== "fresh";
}
