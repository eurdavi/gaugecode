import { openUrl } from "@tauri-apps/plugin-opener";
import { describeStatus, setPref, useNotch, useProviders } from "../lib/usage";
import type { NotchEdge, ProviderView, SignInHint } from "../types/usage";

const EDGES: { value: NotchEdge; label: string }[] = [
  { value: "top", label: "Top" },
  { value: "bottom", label: "Bottom" },
  { value: "left", label: "Left" },
  { value: "right", label: "Right" },
];

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

/** Show/hide the overlay and pick the screen edge it hugs (SPEC §9.1). */
function NotchSection() {
  const notch = useNotch();

  return (
    <div className="rounded-lg border border-neutral-200 bg-white px-4 py-3 dark:border-neutral-800 dark:bg-neutral-900">
      <label className="flex items-center justify-between gap-3">
        <span className="text-sm font-medium">Show the notch overlay</span>
        <input
          type="checkbox"
          className="size-4 accent-neutral-700"
          checked={notch?.visible ?? false}
          disabled={notch === null}
          onChange={(event) => {
            void setPref({ key: "notch_visible", visible: event.currentTarget.checked });
          }}
        />
      </label>

      <div className="mt-3 flex items-center justify-between gap-3">
        <span className="text-[11px] text-neutral-500 dark:text-neutral-400">Screen edge</span>
        <div className="flex gap-1">
          {EDGES.map((edge) => (
            <button
              key={edge.value}
              type="button"
              disabled={notch === null || !notch.visible}
              aria-pressed={notch?.edge === edge.value}
              className={`rounded-md px-2 py-1 text-[11px] disabled:opacity-40 ${
                notch?.edge === edge.value
                  ? "bg-neutral-800 text-neutral-50 dark:bg-neutral-100 dark:text-neutral-900"
                  : "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
              }`}
              onClick={() => void setPref({ key: "notch_edge", edge: edge.value })}
            >
              {edge.label}
            </button>
          ))}
        </div>
      </div>

      <p className="mt-2 text-[11px] text-neutral-400 dark:text-neutral-500">
        Folded it is click-through and sits inside the work area, so it never covers the taskbar or
        the Dock. Point at it to peek; click to keep it open.
      </p>
    </div>
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

      <section aria-labelledby="notch-heading" className="mt-5">
        <h2
          id="notch-heading"
          className="mb-2 text-[11px] font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400"
        >
          Notch
        </h2>
        <NotchSection />
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
