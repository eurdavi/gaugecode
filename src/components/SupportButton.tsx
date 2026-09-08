import { openUrl } from "@tauri-apps/plugin-opener";

export const SUPPORT_URL = "https://buymeacoffee.com/eurdavi";

function CoffeeIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M17 8h1a4 4 0 1 1 0 8h-1" />
      <path d="M3 8h14v9a4 4 0 0 1-4 4H7a4 4 0 0 1-4-4Z" />
      <path d="M6 2v2M10 2v2M14 2v2" />
    </svg>
  );
}

/** Opens the Buy Me a Coffee page. Compact is just the cup, for tight headers. */
export function SupportButton({ label, compact = false }: { label: string; compact?: boolean }) {
  return (
    <button
      type="button"
      title={label}
      className={
        compact
          ? "text-neutral-400 hover:text-amber-600 dark:text-neutral-500 dark:hover:text-amber-400"
          : "inline-flex items-center gap-1.5 rounded-full bg-[#ffdd00] px-2.5 py-1 text-[11px] font-medium text-neutral-900 hover:bg-[#ffe44d]"
      }
      onClick={() => void openUrl(SUPPORT_URL)}
    >
      <CoffeeIcon />
      {compact ? <span className="sr-only">{label}</span> : label}
    </button>
  );
}
