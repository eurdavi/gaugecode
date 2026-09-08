import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isDimmed, primaryWindow, useMessages, usePrefs, useProviders } from "../lib/usage";
import { BAR_EVENT, type BarView, type ProviderView } from "../types/usage";
import { UsageBar } from "./UsageBar";

/** The strip is one taskbar tall; two rows is what fits legibly. */
const MAX_ROWS = 2;

/**
 * The taskbar strip. Rust owns its size and position (`src-tauri/src/bar.rs`);
 * this draws the tray provider's limit windows as segmented bars. The grip on
 * the left drags it; everything else opens the popup, because there is no room
 * here for anything more than the headline.
 */
export function TaskbarBar() {
  const { providers } = useProviders();
  const prefs = usePrefs();
  const messages = useMessages(prefs);
  const [bar, setBar] = useState<BarView | null>(null);

  useEffect(() => {
    void invoke<BarView>("get_bar")
      .then(setBar)
      .catch(() => {
        /* not managed yet; the event delivers it */
      });
    const unlisten = listen<BarView>(BAR_EVENT, ({ payload }) => setBar(payload));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  if (!bar?.visible || providers === null || prefs === null) return null;

  const view = trayProvider(providers, prefs.primary);
  const windows = view?.snapshot?.windows.slice(0, MAX_ROWS) ?? [];
  const dimmed = view ? isDimmed(view.status) : true;

  return (
    <div
      className={`flex h-screen w-screen items-center gap-2 overflow-hidden bg-neutral-950/80 px-1.5 text-neutral-100 backdrop-blur-sm ${
        bar.horizontal ? "flex-row" : "flex-col justify-center"
      }`}
    >
      <div
        data-tauri-drag-region
        className="flex h-full w-3 shrink-0 cursor-grab items-center justify-center text-neutral-600 active:cursor-grabbing"
        title={messages.bar.drag}
        aria-label={messages.bar.drag}
      >
        <span aria-hidden="true" className="select-none text-[10px] leading-none tracking-tighter">
          ⋮⋮
        </span>
      </div>

      <button
        type="button"
        className="flex min-w-0 flex-1 flex-col justify-center gap-1 text-left"
        onClick={() => void invoke("open_popup")}
        title={view?.name ?? messages.app.name}
      >
        {view && windows.length > 0 ? (
          windows.map((limit) => (
            <UsageBar key={limit.id} window={limit} messages={messages} dimmed={dimmed} compact />
          ))
        ) : (
          <span className="truncate text-[10px] text-neutral-400">
            {view ? messages.status.waiting : messages.notch.noProvider}
          </span>
        )}
      </button>
    </div>
  );
}

/**
 * Mirrors `AppState::tray_provider`: the one the user picked when it has a
 * reading, otherwise the first enabled provider that does. The names must agree
 * with the tray icon or the strip would describe a different number.
 */
function trayProvider(providers: ProviderView[], primary: string): ProviderView | null {
  const hasReading = (view: ProviderView) => view.enabled && primaryWindow(view.snapshot) !== null;
  const picked = providers.find((view) => view.id === primary);
  if (picked && hasReading(picked)) return picked;
  return providers.find(hasReading) ?? picked ?? null;
}
