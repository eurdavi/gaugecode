import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { refreshNow, useProviders } from "../lib/usage";
import { ProviderCard } from "./ProviderCard";

/** Popup anchored to the tray icon (SPEC §9.2). */
export function TrayPopup() {
  const { providers } = useProviders();
  const [notice, setNotice] = useState<string | null>(null);

  async function onRefresh() {
    setNotice(await refreshNow());
  }

  return (
    <main className="flex h-screen flex-col bg-white text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="flex items-center justify-between border-b border-neutral-200 px-4 py-2.5 dark:border-neutral-800">
        <h1 className="text-xs font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
          GaugeCode
        </h1>
        <div className="flex items-center gap-3 text-xs">
          <button
            type="button"
            className="text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
            onClick={() => void onRefresh()}
          >
            Refresh
          </button>
          <button
            type="button"
            className="text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
            onClick={() => void invoke("open_settings")}
          >
            Settings
          </button>
        </div>
      </header>

      {notice && (
        <p className="border-b border-amber-200 bg-amber-50 px-4 py-2 text-[11px] text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/40 dark:text-amber-200">
          {notice}
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        {providers === null ? (
          <p className="px-4 py-6 text-center text-xs text-neutral-400">loading…</p>
        ) : (
          <div className="divide-y divide-neutral-100 dark:divide-neutral-900">
            {providers
              .filter((view) => view.enabled)
              .map((view) => (
                <ProviderCard key={view.id} view={view} />
              ))}
          </div>
        )}
      </div>
    </main>
  );
}
