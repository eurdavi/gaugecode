import { BAND_STROKE, bandOf, formatPercent } from "../lib/usage";

interface RingProps {
  /** 0..1, or null when there is no reading — the ring then shows a dash. */
  usedFraction: number | null;
  size?: number;
  dimmed?: boolean;
  label?: string;
}

/**
 * Progress ring for one provider. Always shows the provider's primary window
 * (SPEC §5) and never renders a number it was not given.
 */
export function Ring({ usedFraction, size = 64, dimmed = false, label }: RingProps) {
  const stroke = Math.max(4, Math.round(size / 10));
  const radius = (size - stroke) / 2;
  const circumference = 2 * Math.PI * radius;
  const fraction = usedFraction ?? 0;
  const colour = usedFraction === null ? BAND_STROKE.off : BAND_STROKE[bandOf(usedFraction)];

  return (
    <div
      className="relative shrink-0"
      style={{ width: size, height: size, opacity: dimmed ? 0.45 : 1 }}
      role="img"
      aria-label={
        label ?? (usedFraction === null ? "no reading" : `${formatPercent(usedFraction)} used`)
      }
    >
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
        <circle
          cx={size / 2}
          cy={size / 2}
          r={radius}
          fill="none"
          strokeWidth={stroke}
          className="stroke-neutral-200 dark:stroke-neutral-800"
        />
        {usedFraction !== null && (
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={colour}
            strokeWidth={stroke}
            strokeLinecap="round"
            strokeDasharray={circumference}
            strokeDashoffset={circumference * (1 - Math.min(fraction, 1))}
          />
        )}
      </svg>
      <span
        className="absolute inset-0 flex items-center justify-center font-mono tabular-nums"
        style={{ fontSize: Math.round(size / 4) }}
      >
        {usedFraction === null ? "—" : Math.round(usedFraction * 100)}
      </span>
    </div>
  );
}
