import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isDimmed, primaryWindow, useMessages, usePrefs, useProviders } from "../lib/usage";
import { BAR_EVENT, PREFS_EVENT, type BarView, type ProviderView } from "../types/usage";
import { UsageBar } from "./UsageBar";

/** A taskbar is around 48 px tall; two rows of 9 px type is what fits. */
const MAX_ROWS = 2;

/**
 * The taskbar strip. Rust owns its size and position (`src-tauri/src/bar.rs`).
 *
 * Deliberately without a background: it sits *inside* the taskbar, and a panel
 * of its own would read as a rectangle pasted on top. What makes it legible is
 * the type and the blocks, not a plate behind them.
 */
export function TaskbarBar() {
  const { providers } = useProviders();
  const prefs = usePrefs();
  const messages = useMessages(prefs);
  const [bar, setBar] = useState<BarView | null>(null);

  useEffect(() => {
    let cancelled = false;
    const load = () =>
      invoke<BarView>("get_bar")
        .then(setBar)
        .catch(() => {
          /* not managed yet; the event delivers it */
        });

    const unlisten = Promise.all([
      listen<BarView>(BAR_EVENT, ({ payload }) => setBar(payload)),
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

  if (!bar?.visible || providers === null || prefs === null) return null;

  const view = trayProvider(providers, prefs.primary);
  const windows = view?.snapshot?.windows.slice(0, MAX_ROWS) ?? [];
  const dimmed = view ? isDimmed(view.status) : true;

  return (
    <div className="flex h-screen w-screen items-center gap-1 overflow-hidden pl-0.5 pr-1 text-neutral-100">
      <Grip label={messages.bar.drag} />

      <button
        type="button"
        className="flex min-w-0 flex-1 flex-col justify-center gap-[3px] rounded text-left hover:bg-white/5"
        onClick={() => void invoke("open_popup")}
        title={view?.name ?? messages.app.name}
      >
        {windows.length > 0 ? (
          windows.slice(0, 6).map((limit) => (
            <UsageBar key={limit.id} window={limit} messages={messages} dimmed={dimmed} compact />
          ))
        ) : (
          <span className="truncate px-1 text-[9px] text-neutral-400">
            {view ? messages.status.waiting : messages.notch.noProvider}
          </span>
        )}
      </button>
    </div>
  );
}

/**
 * Two columns of dots, the usual "drag me" affordance.
 *
 * Not `data-tauri-drag-region`: that asks Windows to run its own move loop,
 * which refuses on a window that cannot be activated. Rust follows the pointer
 * instead, so all this has to do is say when the grip went down.
 */
function Grip({ label }: { label: string }) {
  return (
    <div
      className="flex h-full shrink-0 cursor-grab items-center px-1.5 active:cursor-grabbing"
      title={label}
      aria-label={label}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        void invoke("begin_bar_drag");
      }}
    >
      <svg width="4" height="14" viewBox="0 0 4 14" aria-hidden="true">
        {[2, 5, 8, 11].map((y) =>
          [0.75, 3.25].map((x) => (
            <circle key={`${x}-${y}`} cx={x} cy={y} r="0.75" fill="rgb(255 255 255 / 0.35)" />
          )),
        )}
      </svg>
    </div>
  );
}

/**
 * Mirrors `AppState::tray_provider`: the one the user picked when it has a
 * reading, otherwise the first enabled provider that does. The two must agree
 * or the strip would describe a different number from the tray icon.
 */
function trayProvider(providers: ProviderView[], primary: string): ProviderView | null {
  const hasReading = (view: ProviderView) => view.enabled && primaryWindow(view.snapshot) !== null;
  const picked = providers.find((view) => view.id === primary);
  if (picked && hasReading(picked)) return picked;
  return providers.find(hasReading) ?? picked ?? null;
}
