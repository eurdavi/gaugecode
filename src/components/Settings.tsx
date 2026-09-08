import { openUrl } from "@tauri-apps/plugin-opener";
import { describeStatus, setPref, useProviders } from "../lib/usage";
import type { ProviderView, SignInHint } from "../types/usage";

function hintText(hint: SignInHint | null): string | null {
  if (!hint) return null;
  switch (hint.kind) {
    case "text":
      return hint.text;
    case "open_app":
      return `Open ${hint.app} and sign in.`;
    case "open_url":
      return hint.url;
  }
}

function ProviderRow({ view, onChanged }: { view: ProviderView; onChanged: () => void }) {
  const hint = hintText(view.sign_in_hint);
  const needsAuth = view.status?.kind === "needs_auth";

  return (
    <li className="px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{view.name}</p>
          <p className="mt-0.5 text-[11px] text-neutral-500 dark:text-neutral-400">
            {!view.implemented
              ? "adapter lands in M3"
              : [view.account?.plan, view.enabled ? describeStatus(view.status) : "off"]
                  .filter(Boolean)
                  .join(" · ")}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-3">
          <label className="flex items-center gap-1.5 text-[11px] text-neutral-500 dark:text-neutral-400">
            <input
              type="radio"
              name="primary"
              className="accent-neutral-700"
              checked={view.is_primary}
              disabled={!view.enabled || !view.implemented}
              onChange={() => {
                void setPref({ key: "primary", provider: view.id }).then(onChanged);
              }}
            />
            tray
          </label>

          <input
            type="checkbox"
            aria-label={`Enable ${view.name}`}
            className="size-4 accent-neutral-700"
            checked={view.enabled}
            disabled={!view.implemented}
            onChange={(event) => {
              void setPref({
                key: "provider_enabled",
                provider: view.id,
                enabled: event.currentTarget.checked,
              }).then(onChanged);
            }}
          />
        </div>
      </div>

      {view.enabled && needsAuth && hint && (
        <p className="mt-2 rounded-md bg-neutral-100 px-2.5 py-1.5 text-[11px] text-neutral-600 dark:bg-neutral-900 dark:text-neutral-300">
          {hint}
        </p>
      )}
    </li>
  );
}

export function Settings() {
  const { providers, reload } = useProviders();

  return (
    <main className="min-h-screen bg-neutral-50 p-6 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="mb-5">
        <h1 className="text-xl font-semibold tracking-tight">GaugeCode</h1>
        <p className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">
          Read-only. GaugeCode reads the credential each tool already left on this machine and asks
          that vendor's own usage endpoint. It never signs you in and never sends anything anywhere
          else.
        </p>
      </header>

      <section aria-labelledby="providers-heading">
        <h2
          id="providers-heading"
          className="mb-2 text-[11px] font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400"
        >
          Providers
        </h2>
        <ul className="divide-y divide-neutral-200 rounded-lg border border-neutral-200 bg-white dark:divide-neutral-800 dark:border-neutral-800 dark:bg-neutral-900">
          {providers === null ? (
            <li className="px-4 py-6 text-center text-xs text-neutral-400">loading…</li>
          ) : (
            providers.map((view) => (
              <ProviderRow key={view.id} view={view} onChanged={() => void reload()} />
            ))
          )}
        </ul>
      </section>

      <footer className="mt-5 text-[11px] text-neutral-400 dark:text-neutral-500">
        <button
          type="button"
          className="underline underline-offset-2 hover:text-neutral-600 dark:hover:text-neutral-300"
          onClick={() => void openUrl("https://github.com/eurdavi/gaugecode#what-the-app-reads-and-what-it-never-does")}
        >
          How this works / what the app reads
        </button>
      </footer>
    </main>
  );
}
