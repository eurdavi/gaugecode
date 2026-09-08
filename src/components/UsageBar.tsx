import { windowLabel, type Messages } from "../i18n";
import { BAND_STROKE, bandOf, formatReset } from "../lib/usage";
import type { LimitWindow } from "../types/usage";

/** Blocks rather than a smooth bar: countable at a glance, and it reads at
 *  overlay sizes where a 2 px gradient would not. */
const SEGMENTS = 12;

/**
 * One limit window as a compact row: short name, segmented scale, percentage
 * and time to reset. This is the bar style's unit (SPEC §9.1).
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
  /** Drops the reset time, for when there is no room for it. */
  compact?: boolean;
}) {
  const filled = Math.round(Math.min(limit.used_fraction, 1) * SEGMENTS);
  const colour = BAND_STROKE[bandOf(limit.used_fraction)];
  const reset = compact ? null : formatReset(limit.resets_at, messages);
  const label = windowLabel(messages, limit.id, limit.label);

  return (
    <div
      className="flex items-center gap-2 text-[10px] leading-none"
      style={{ opacity: dimmed ? 0.5 : 1 }}
    >
      <span className="w-16 shrink-0 truncate text-neutral-300" title={label}>
        {label}
      </span>

      <div className="flex shrink-0 gap-[2px]" role="img" aria-label={`${label} ${Math.round(limit.used_fraction * 100)}%`}>
        {Array.from({ length: SEGMENTS }, (_, index) => (
          <span
            key={index}
            className="h-2.5 w-[5px] rounded-[1px]"
            style={{
              backgroundColor: index < filled ? colour : "rgb(255 255 255 / 0.14)",
            }}
          />
        ))}
      </div>

      <span className="shrink-0 font-mono tabular-nums text-neutral-100">
        {Math.round(limit.used_fraction * 100)}%
      </span>
      {reset && <span className="truncate text-neutral-500">· {reset}</span>}
    </div>
  );
}
