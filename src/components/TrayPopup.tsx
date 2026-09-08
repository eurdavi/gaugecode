import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { refreshNow, useMessages, usePrefs, useProviders } from "../lib/usage";
import { ProviderCard } from "./ProviderCard";

/** Popup anchored to the tray icon (SPEC §9.2). */
export function TrayPopup() {
  const { providers } = useProviders();
  const prefs = usePrefs();
  const messages = useMessages(prefs);
  const [notice, setNotice] = useState<string | null>(null);

  async function onRefresh() {
    setNotice(await refreshNow());
  }

  return (
    <main className="flex h-screen flex-col bg-white text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="flex items-center justify-between border-b border-neutral-200 px-4 py-2.5 dark:border-neutral-800">
        <h1 className="flex items-center gap-2 text-xs font-semibold uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
          {messages.app.name}
          {prefs?.demo && (
            <span className="rounded-full bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal text-amber-800 dark:bg-amber-950/60 dark:text-amber-200">
              {messages.app.demo}
            </span>
          )}
        </h1>
        <div className="flex items-center gap-3 text-xs">
          <button
            type="button"
            className="text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
            onClick={() => void onRefresh()}
          >
            {messages.app.refresh}
          </button>
          <button
            type="button"
            className="text-neutral-500 hover:text-neutral-900 dark:text-neutral-400 dark:hover:text-neutral-100"
            onClick={() => void invoke("open_settings")}
          >
            {messages.app.settings}
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
          <p className="px-4 py-6 text-center text-xs text-neutral-400">{messages.app.loading}</p>
        ) : (
          <div className="divide-y divide-neutral-100 dark:divide-neutral-900">
            {providers
              .filter((view) => view.enabled)
              .map((view) => (
                <ProviderCard key={view.id} view={view} messages={messages} />
              ))}
          </div>
        )}
      </div>
    </main>
  );
}
