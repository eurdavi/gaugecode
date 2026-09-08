import {
  BAND_STROKE,
  bandOf,
  describeStatus,
  formatReset,
  isDimmed,
  primaryWindow,
  toggleNotchPin,
  useNotch,
  useProviders,
} from "../lib/usage";
import type { NotchEdge, ProviderView } from "../types/usage";
import { Ring } from "./Ring";

/**
 * The overlay itself. Rust owns the window's size, position and click-through
 * (see `src-tauri/src/notch.rs`); this component only draws the mode it is
 * told about, so the two must agree on which providers are on screen.
 */
export function NotchShell() {
  const { providers } = useProviders();
  const notch = useNotch();

  if (!notch?.visible || providers === null) return null;

  const shown = providers.filter((view) => view.enabled);
  const vertical = notch.edge === "left" || notch.edge === "right";

  if (notch.mode === "folded") {
    return <FoldedPill views={shown} vertical={vertical} edge={notch.edge} />;
  }

  return (
    <div
      className="h-screen w-screen p-[7px]"
      onClick={() => void toggleNotchPin()}
      role="button"
      tabIndex={0}
      title={notch.mode === "pinned" ? "Click to unpin" : "Click to keep open"}
    >
      <div
        className={`flex h-full w-full items-stretch overflow-hidden rounded-2xl border border-white/10 bg-neutral-900/85 text-neutral-100 shadow-xl backdrop-blur-md ${
          vertical ? "flex-col divide-y" : "flex-row divide-x"
        } divide-white/10`}
      >
        {shown.length === 0 ? (
          <p className="m-auto px-4 text-[11px] text-neutral-400">no provider enabled</p>
        ) : (
          shown.map((view) => <ExpandedRow key={view.id} view={view} vertical={vertical} />)
        )}
      </div>
    </div>
  );
}

/**
 * Folded, the notch is a colour-coded sliver: one segment per enabled
 * provider, no digits — those live in the tray icon.
 */
function FoldedPill({
  views,
  vertical,
  edge,
}: {
  views: ProviderView[];
  vertical: boolean;
  edge: NotchEdge;
}) {
  // Round only the corners that point away from the screen edge.
  const rounding = {
    right: "rounded-l-full",
    left: "rounded-r-full",
    top: "rounded-b-full",
    bottom: "rounded-t-full",
  }[edge];

  return (
    <div
      className={`flex h-screen w-screen gap-px overflow-hidden bg-neutral-900/70 ${rounding} ${
        vertical ? "flex-col" : "flex-row"
      }`}
    >
      {views.map((view) => {
        const main = primaryWindow(view.snapshot);
        const colour =
          main === null || main === undefined
            ? BAND_STROKE.off
            : BAND_STROKE[bandOf(main.used_fraction)];
        return (
          <div
            key={view.id}
            className="flex-1"
            style={{ backgroundColor: colour, opacity: isDimmed(view.status) ? 0.4 : 0.95 }}
          />
        );
      })}
    </div>
  );
}

function ExpandedRow({ view, vertical }: { view: ProviderView; vertical: boolean }) {
  const main = primaryWindow(view.snapshot);
  const dimmed = isDimmed(view.status);
  const reset = main ? formatReset(main.resets_at) : null;

  return (
    <div
      className={`flex min-w-0 flex-1 items-center gap-3 px-3 ${vertical ? "" : "justify-center"}`}
    >
      <Ring
        usedFraction={main?.used_fraction ?? null}
        size={44}
        dimmed={dimmed}
        label={`${view.name} usage`}
      />
      <div className="min-w-0">
        <p className="truncate text-xs font-medium">{view.name}</p>
        <p className="truncate text-[10px] text-neutral-400">
          {main?.label ?? (view.implemented ? describeStatus(view.status) : "not in this build yet")}
        </p>
        {reset && <p className="truncate text-[10px] text-neutral-500">{reset}</p>}
      </div>
    </div>
  );
}
