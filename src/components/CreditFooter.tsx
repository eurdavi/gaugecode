import { openUrl } from "@tauri-apps/plugin-opener";

const GITHUB_PROFILE = "https://github.com/eurdavi";

/** Quiet byline. The handle stays a link; the sentence is what gets translated. */
export function CreditFooter({ credit }: { credit: string }) {
  return (
    <p className="text-center text-[10px] leading-tight text-neutral-400 dark:text-neutral-600">
      {credit}
      {" — "}
      <button
        type="button"
        className="hover:text-neutral-600 dark:hover:text-neutral-400"
        onClick={() => void openUrl(GITHUB_PROFILE)}
      >
        @eurdavi
      </button>
    </p>
  );
}
