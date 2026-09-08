import { openUrl } from "@tauri-apps/plugin-opener";
import { BAND_STROKE, bandOf, describeStatus, formatReset, isDimmed, primaryWindow } from "../lib/usage";
import type { ProviderView } from "../types/usage";
import { Ring } from "./Ring";

/** One provider: its ring, every limit window it reported, and where it stands. */
export function ProviderCard({ view }: { view: ProviderView }) {
  const main = primaryWindow(view.snapshot);
  const dimmed = isDimmed(view.status);

  return (
    <article className="flex gap-4 px-4 py-3">
      <Ring
        usedFraction={main?.used_fraction ?? null}
        dimmed={dimmed}
        label={`${view.name} usage`}
      />

      <div className="min-w-0 flex-1">
        <header className="flex items-baseline justify-between gap-2">
          <h3 className="truncate text-sm font-semibold">{view.name}</h3>
          <span className="shrink-0 text-[11px] text-neutral-500 dark:text-neutral-400">
            {view.implemented ? describeStatus(view.status) : "not in this build yet"}
          </span>
        </header>

        {view.snapshot ? (
          <ul className="mt-2 space-y-1.5">
            {view.snapshot.windows.map((window) => (
              <li key={window.id} className="text-xs">
                <div className="flex items-baseline justify-between gap-2">
                  <span className="truncate text-neutral-600 dark:text-neutral-300">
                    {window.label}
                  </span>
                  <span className="shrink-0 font-mono tabular-nums">
                    {Math.round(window.used_fraction * 100)}%
                  </span>
                </div>
                <div className="mt-1 h-1 overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-800">
                  <div
                    className="h-full rounded-full"
                    style={{
                      width: `${Math.min(window.used_fraction, 1) * 100}%`,
                      backgroundColor: BAND_STROKE[bandOf(window.used_fraction)],
                      opacity: dimmed ? 0.5 : 1,
                    }}
                  />
                </div>
                {formatReset(window.resets_at) && (
                  <p className="mt-0.5 text-[11px] text-neutral-400 dark:text-neutral-500">
                    {formatReset(window.resets_at)}
                  </p>
                )}
              </li>
            ))}
          </ul>
        ) : (
          <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">
            {view.status?.kind === "needs_auth" && view.sign_in_hint?.kind === "text"
              ? view.sign_in_hint.text
              : describeStatus(view.status)}
          </p>
        )}

        <footer className="mt-2 flex items-center gap-2 text-[11px] text-neutral-400 dark:text-neutral-500">
          {view.snapshot && <span>{view.snapshot.fidelity}</span>}
          {view.account?.plan && <span>· {view.account.plan}</span>}
          {view.manage_url && (
            <button
              type="button"
              className="ml-auto underline underline-offset-2 hover:text-neutral-600 dark:hover:text-neutral-300"
              onClick={() => void openUrl(view.manage_url as string)}
            >
              manage
            </button>
          )}
        </footer>
      </div>
    </article>
  );
}
