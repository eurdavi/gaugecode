import { shortWindowLabel, windowLabel, type Messages } from "../i18n";
import { BAND_STROKE, bandOf, formatReset } from "../lib/usage";
import type { LimitWindow } from "../types/usage";

/** Blocks rather than a smooth bar: countable at a glance, and they read at
 *  overlay sizes where a two-pixel gradient would not. */
const SEGMENTS = 10;

/**
 * One limit window as a compact row: name, segmented scale and percentage.
 *
 * `compact` is for the taskbar strip, where there is room for a two-character
 * name and nothing else — the reset time and the full name live in the popup.
 */
export function UsageBar({
  window: limit,
  messages,
  dimmed,
  compact = false,
}: {
  window: LimitWindow;
  messages: Messages;
  dimmed: boolean;
  compact?: boolean;
}) {
  const percent = Math.round(limit.used_fraction * 100);
  const filled = Math.round(Math.min(limit.used_fraction, 1) * SEGMENTS);
  const colour = BAND_STROKE[bandOf(limit.used_fraction)];
  const full = windowLabel(messages, limit.id, limit.label);
  const label = compact ? shortWindowLabel(messages, limit.id, limit.label) : full;
  const reset = compact ? null : formatReset(limit.resets_at, messages);

  return (
    <div
      className="flex items-center gap-1.5 leading-none"
      style={{ opacity: dimmed ? 0.55 : 1 }}
      title={`${full} · ${percent}%`}
    >
      <span
        className={`shrink-0 tabular-nums text-neutral-400 ${
          compact ? "w-9 text-right text-[9px]" : "w-16 truncate text-[10px]"
        }`}
      >
        {label}
      </span>

      <div
        className="flex shrink-0 gap-px"
        role="img"
        aria-label={`${full} ${percent}%`}
      >
        {Array.from({ length: SEGMENTS }, (_, index) => (
          <span
            key={index}
            className={`rounded-full ${compact ? "h-2 w-1" : "h-2.5 w-[5px]"}`}
            style={{
              backgroundColor: index < filled ? colour : "rgb(255 255 255 / 0.18)",
            }}
          />
        ))}
      </div>

      <span
        className={`shrink-0 text-right font-mono tabular-nums text-neutral-200 ${
          compact ? "w-7 text-[9px]" : "text-[10px]"
        }`}
      >
        {percent}%
      </span>
      {reset && <span className="truncate text-[10px] text-neutral-500">· {reset}</span>}
    </div>
  );
}
