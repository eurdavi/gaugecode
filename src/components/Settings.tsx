import type { ProviderId } from "../types/usage";

const PROVIDERS: { id: ProviderId; name: string; milestone: string }[] = [
  { id: "claude", name: "Claude Code", milestone: "M1" },
  { id: "cursor", name: "Cursor", milestone: "M3" },
  { id: "codex", name: "Codex", milestone: "M3" },
];

/**
 * M0 placeholder. Real toggles, account rows and sign-in hints arrive with the
 * provider adapters (SPEC §9.3).
 */
export function Settings() {
  return (
    <main className="min-h-screen bg-neutral-50 p-6 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="mb-6">
        <h1 className="text-xl font-semibold tracking-tight">GaugeCode</h1>
        <p className="mt-1 text-sm text-neutral-500 dark:text-neutral-400">
          Session usage for Claude Code, Cursor and Codex. Read-only; nothing
          leaves this machine except the provider's own usage endpoint.
        </p>
      </header>

      <section aria-labelledby="providers-heading">
        <h2
          id="providers-heading"
          className="mb-2 text-xs font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400"
        >
          Providers
        </h2>
        <ul className="divide-y divide-neutral-200 rounded-lg border border-neutral-200 bg-white dark:divide-neutral-800 dark:border-neutral-800 dark:bg-neutral-900">
          {PROVIDERS.map((p) => (
            <li
              key={p.id}
              className="flex items-center justify-between px-4 py-3"
            >
              <div className="flex items-center gap-3">
                <span
                  aria-hidden
                  className="inline-block size-2.5 rounded-full bg-band-off"
                />
                <span className="text-sm font-medium">{p.name}</span>
              </div>
              <span className="font-mono text-xs text-neutral-400 dark:text-neutral-500">
                not wired yet · {p.milestone}
              </span>
            </li>
          ))}
        </ul>
      </section>
    </main>
  );
}
