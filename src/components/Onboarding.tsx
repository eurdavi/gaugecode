import { useState } from "react";
import type { Messages } from "../i18n";
import { setPref } from "../lib/usage";
import type { ProviderView } from "../types/usage";
import { Ring } from "./Ring";

/**
 * First-run walkthrough, shown in the Settings window because a tray app with
 * no window gives a first-time user nothing to look at. It appears exactly
 * once; dismissing it — by finishing *or* skipping — records that.
 */
export function Onboarding({
  messages,
  providers,
  onChanged,
  onDone,
}: {
  messages: Messages;
  providers: ProviderView[] | null;
  onChanged: () => void;
  onDone: () => void;
}) {
  const [step, setStep] = useState(0);

  function finish() {
    void setPref({ key: "onboarded", onboarded: true }).then(onDone);
  }

  const steps = [
    {
      title: messages.onboarding.welcomeTitle,
      body: messages.onboarding.welcomeBody,
      extra: <RingSampler />,
    },
    {
      title: messages.onboarding.providersTitle,
      body: messages.onboarding.providersBody,
      extra: <ProviderPicker messages={messages} providers={providers} onChanged={onChanged} />,
    },
    {
      title: messages.onboarding.surfacesTitle,
      body: messages.onboarding.surfacesBody,
      extra: null,
    },
    {
      title: messages.onboarding.honestTitle,
      body: messages.onboarding.honestBody,
      extra: null,
    },
  ];

  const current = steps[step];
  const isLast = step === steps.length - 1;

  return (
    <main className="flex min-h-screen flex-col bg-neutral-50 p-6 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header className="flex items-baseline justify-between">
        <p className="text-[11px] uppercase tracking-wide text-neutral-400 dark:text-neutral-500">
          {messages.onboarding.step(step + 1, steps.length)}
        </p>
        <button
          type="button"
          className="text-[11px] text-neutral-400 underline underline-offset-2 hover:text-neutral-600 dark:hover:text-neutral-300"
          onClick={finish}
        >
          {messages.onboarding.skip}
        </button>
      </header>

      <div className="mt-6 flex-1">
        <h1 className="text-xl font-semibold tracking-tight">{current.title}</h1>
        <p className="mt-2 max-w-prose text-sm leading-relaxed text-neutral-600 dark:text-neutral-300">
          {current.body}
        </p>
        {current.extra && <div className="mt-5">{current.extra}</div>}
      </div>

      <footer className="mt-6 flex items-center justify-between">
        <div className="flex gap-1.5" aria-hidden="true">
          {steps.map((entry, index) => (
            <span
              key={entry.title}
              className={`size-1.5 rounded-full ${
                index === step
                  ? "bg-neutral-700 dark:bg-neutral-200"
                  : "bg-neutral-300 dark:bg-neutral-700"
              }`}
            />
          ))}
        </div>

        <div className="flex gap-2">
          {step > 0 && (
            <button
              type="button"
              className="rounded-md bg-neutral-100 px-3 py-1.5 text-xs text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
              onClick={() => setStep((value) => value - 1)}
            >
              {messages.onboarding.back}
            </button>
          )}
          <button
            type="button"
            className="rounded-md bg-neutral-800 px-3 py-1.5 text-xs text-neutral-50 hover:bg-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-white"
            onClick={() => (isLast ? finish() : setStep((value) => value + 1))}
          >
            {isLast ? messages.onboarding.done : messages.onboarding.next}
          </button>
        </div>
      </footer>
    </main>
  );
}

/** Shows what each band colour means, using the same ring the app draws. */
function RingSampler() {
  return (
    <div className="flex items-center gap-5">
      {[0.24, 0.66, 0.93].map((fraction) => (
        <Ring key={fraction} usedFraction={fraction} size={52} />
      ))}
      <Ring usedFraction={null} size={52} />
    </div>
  );
}

function ProviderPicker({
  messages,
  providers,
  onChanged,
}: {
  messages: Messages;
  providers: ProviderView[] | null;
  onChanged: () => void;
}) {
  if (providers === null) {
    return <p className="text-xs text-neutral-400">{messages.app.loading}</p>;
  }

  return (
    <ul className="grid grid-cols-2 gap-2">
      {providers.map((view) => (
        <li key={view.id}>
          <label
            className={`flex cursor-pointer items-center gap-2 rounded-lg border px-3 py-2 text-sm ${
              view.enabled
                ? "border-neutral-800 bg-white dark:border-neutral-100 dark:bg-neutral-900"
                : "border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900"
            }`}
          >
            <input
              type="checkbox"
              className="size-4 accent-neutral-700"
              checked={view.enabled}
              onChange={(event) => {
                void setPref({
                  key: "provider_enabled",
                  provider: view.id,
                  enabled: event.currentTarget.checked,
                }).then(onChanged);
              }}
            />
            {view.name}
          </label>
        </li>
      ))}
    </ul>
  );
}
