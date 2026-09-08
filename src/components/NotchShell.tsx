import { useEffect, useState } from "react";
import { windowLabel } from "../i18n";
import {
  BAND_STROKE,
  bandOf,
  describeStatus,
  formatReset,
  isDimmed,
  primaryWindow,
  toggleNotchPin,
  useMessages,
  useNotch,
  usePrefs,
  useProviders,
} from "../lib/usage";
import type { NotchEdge, NotchView, ProviderView } from "../types/usage";
import { Ring } from "./Ring";
import { UsageBar } from "./UsageBar";

/** Long enough to read as a movement, short enough not to feel laggy. */
const DURATION_MS = 220;
/** Slightly quicker on the way out, which is what makes it feel responsive. */
const DURATION_OUT_MS = 160;
const EASING = "cubic-bezier(0.22, 1, 0.36, 1)";

/** Where the card sits while folded, so it slides in from its own edge. */
const OFFSCREEN: Record<NotchEdge, string> = {
  right: "translateX(101%)",
  left: "translateX(-101%)",
  top: "translateY(-101%)",
  bottom: "translateY(101%)",
};

/**
 * The overlay. Rust owns the window's size, position and click-through (see
 * `src-tauri/src/notch.rs`); the window keeps its expanded footprint at all
 * times, and what moves is the card drawn inside it — that is what lets the
 * transition be smooth instead of a series of window resizes.
 */
export function NotchShell() {
  const { providers } = useProviders();
  const prefs = usePrefs();
  const notch = useNotch();
  const messages = useMessages(prefs);
  const reducedMotion = usePrefersReducedMotion();

  if (!notch?.visible || providers === null) return null;

  const shown = providers.filter((view) => view.enabled);
  const expanded = notch.mode !== "folded";
  // "None" and the OS-level reduced-motion setting both mean: just switch.
  const animated = notch.animation !== "instant" && !reducedMotion;
  const duration = animated ? (expanded ? DURATION_MS : DURATION_OUT_MS) : 0;
  const transition = `transform ${duration}ms ${EASING}, opacity ${duration}ms ease-out`;

  return (
    <div className="relative h-screen w-screen overflow-hidden">
      <FoldedPill view={notch} views={shown} expanded={expanded} transition={transition} />

      <button
        type="button"
        aria-label={notch.mode === "pinned" ? messages.notch.unpin : messages.notch.pin}
        title={notch.mode === "pinned" ? messages.notch.unpin : messages.notch.pin}
        onClick={() => void toggleNotchPin()}
        className="absolute inset-0 p-[7px] text-left"
        style={{
          transition,
          // Fade keeps the card in place; slide walks it out of its edge.
          transform: expanded || !animated ? "none" : OFFSCREEN[notch.edge],
          opacity: expanded ? 1 : 0,
          // Folded, the window is click-through anyway; this keeps the card from
          // swallowing the pointer during the fade out.
          pointerEvents: expanded ? "auto" : "none",
        }}
      >
        <div
          className={`flex h-full w-full items-stretch overflow-hidden rounded-2xl border border-white/10 bg-neutral-900/85 text-neutral-100 shadow-xl backdrop-blur-md ${
            isVertical(notch.edge) ? "flex-col divide-y" : "flex-row divide-x"
          } divide-white/10`}
        >
          {shown.length === 0 ? (
            <p className="m-auto px-4 text-[11px] text-neutral-400">{messages.notch.noProvider}</p>
          ) : notch.style === "bars" ? (
            shown.map((view) => (
              <BarsRow key={view.id} view={view} messages={messages} />
            ))
          ) : (
            shown.map((view) => (
              <ExpandedRow
                key={view.id}
                view={view}
                vertical={isVertical(notch.edge)}
                messages={messages}
              />
            ))
          )}
        </div>
      </button>
    </div>
  );
}

function isVertical(edge: NotchEdge): boolean {
  return edge === "left" || edge === "right";
}

/**
 * Folded, the notch is a colour-coded sliver: one segment per enabled provider,
 * no digits — those live in the tray icon. Its size comes from Rust so it lands
 * exactly where the pointer watch is waiting for it.
 */
function FoldedPill({
  view,
  views,
  expanded,
  transition,
}: {
  view: NotchView;
  views: ProviderView[];
  expanded: boolean;
  transition: string;
}) {
  const vertical = isVertical(view.edge);
  const { folded_thickness: thickness, folded_length: length } = view;

  // Round only the corners that point away from the screen edge, and pin the
  // sliver to that edge, centred along it.
  const placement: Record<NotchEdge, string> = {
    right: "right-0 top-1/2 -translate-y-1/2 rounded-l-full",
    left: "left-0 top-1/2 -translate-y-1/2 rounded-r-full",
    top: "top-0 left-1/2 -translate-x-1/2 rounded-b-full",
    bottom: "bottom-0 left-1/2 -translate-x-1/2 rounded-t-full",
  };

  return (
    <div
      className={`absolute flex gap-px overflow-hidden bg-neutral-900/70 ${placement[view.edge]} ${
        vertical ? "flex-col" : "flex-row"
      }`}
      style={{
        width: vertical ? thickness : length,
        height: vertical ? length : thickness,
        // Only the opacity animates: the placement classes already use a
        // transform to centre the sliver, and animating it too would drift.
        transition,
        opacity: expanded ? 0 : 1,
      }}
    >
      {views.map((provider) => {
        const main = primaryWindow(provider.snapshot);
        const colour = main ? BAND_STROKE[bandOf(main.used_fraction)] : BAND_STROKE.off;
        return (
          <div
            key={provider.id}
            className="flex-1"
            style={{ backgroundColor: colour, opacity: isDimmed(provider.status) ? 0.4 : 0.95 }}
          />
        );
      })}
    </div>
  );
}

/**
 * The bar style: the provider's name once, then every limit window it reported
 * as its own row. More lines than the ring style, but each number is spelled
 * out rather than encoded in an arc.
 */
function BarsRow({ view, messages }: { view: ProviderView; messages: ReturnType<typeof useMessages> }) {
  const dimmed = isDimmed(view.status);
  const windows = view.snapshot?.windows ?? [];

  return (
    <div className="flex min-w-0 flex-1 flex-col justify-center gap-1 px-3 py-1">
      <div className="flex items-baseline justify-between gap-2">
        <p className="truncate text-[11px] font-medium">{view.name}</p>
        {windows.length === 0 && (
          <p className="shrink-0 text-[10px] text-neutral-400">
            {view.implemented
              ? describeStatus(view.status, messages)
              : messages.status.notImplemented}
          </p>
        )}
      </div>
      {windows.map((limit) => (
        <UsageBar key={limit.id} window={limit} messages={messages} dimmed={dimmed} />
      ))}
    </div>
  );
}

function ExpandedRow({
  view,
  vertical,
  messages,
}: {
  view: ProviderView;
  vertical: boolean;
  messages: ReturnType<typeof useMessages>;
}) {
  const main = primaryWindow(view.snapshot);
  const dimmed = isDimmed(view.status);
  const reset = main ? formatReset(main.resets_at, messages) : null;

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
          {main
            ? windowLabel(messages, main.id, main.label)
            : view.implemented
              ? describeStatus(view.status, messages)
              : messages.status.notImplemented}
        </p>
        {reset && <p className="truncate text-[10px] text-neutral-500">{reset}</p>}
      </div>
    </div>
  );
}

/** Honours the OS "reduce motion" setting whatever the app preference says. */
function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);

  useEffect(() => {
    const query = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(query.matches);
    const onChange = (event: MediaQueryListEvent) => setReduced(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  return reduced;
}
