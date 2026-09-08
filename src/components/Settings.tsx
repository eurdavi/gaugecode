import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { LANGUAGES, connectSteps, windowLabel, type Messages } from "../i18n";
import { describeStatus, setPref, useMessages, usePrefs, useProviders } from "../lib/usage";
import { Onboarding } from "./Onboarding";
import {
  UPDATE_EVENT,
  type AvailableUpdate,
  type Language,
  type NotchAnimation,
  type NotchEdge,
  type NotchStyle,
  type PrefsView,
  type ProviderView,
} from "../types/usage";

const EDGES: NotchEdge[] = ["top", "bottom", "left", "right"];
const ANIMATIONS: NotchAnimation[] = ["slide", "fade", "instant"];
const STYLES: NotchStyle[] = ["rings", "bars"];
/** Round numbers between the bounds Rust enforces. */
const POLL_CHOICES = [30, 60, 120, 300, 900];

const README_ANCHOR =
  "https://github.com/eurdavi/gaugecode#what-the-app-reads-and-what-it-never-does";

// ---------------------------------------------------------------------------
// Small shared pieces
// ---------------------------------------------------------------------------

function GearIcon() {
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
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-5">
      <h2 className="mb-2 text-[11px] font-medium uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
        {title}
      </h2>
      {children}
    </section>
  );
}

function Card({ children }: { children: React.ReactNode }) {
  return (
    <div className="rounded-lg border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-900">
      {children}
    </div>
  );
}

function Toggle({
  label,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 px-4 py-3">
      <span className="text-sm">{label}</span>
      <input
        type="checkbox"
        className="size-4 shrink-0 accent-neutral-700"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.currentTarget.checked)}
      />
    </label>
  );
}

/** A row of mutually exclusive choices — used for the edge, animation and language. */
function ChoiceRow<T extends string>({
  label,
  options,
  selected,
  disabled,
  onSelect,
}: {
  label: string;
  options: { value: T; label: string }[];
  selected: T;
  disabled?: boolean;
  onSelect: (value: T) => void;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3">
      <span className="text-sm">{label}</span>
      <div className="flex flex-wrap gap-1">
        {options.map((option) => (
          <button
            key={option.value}
            type="button"
            disabled={disabled}
            aria-pressed={selected === option.value}
            className={`rounded-md px-2 py-1 text-[11px] disabled:opacity-40 ${
              selected === option.value
                ? "bg-neutral-800 text-neutral-50 dark:bg-neutral-100 dark:text-neutral-900"
                : "bg-neutral-100 text-neutral-600 hover:bg-neutral-200 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
            }`}
            onClick={() => onSelect(option.value)}
          >
            {option.label}
          </button>
        ))}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Providers
// ---------------------------------------------------------------------------

function ProviderRow({
  view,
  messages,
  onChanged,
}: {
  view: ProviderView;
  messages: Messages;
  onChanged: () => void;
}) {
  const [showSteps, setShowSteps] = useState(false);
  const [showWindows, setShowWindows] = useState(false);
  // Offer the instructions before the user has to hunt for them.
  const needsAuth = view.status?.kind === "needs_auth";
  // Never let the last one be unticked: hiding everything would leave the ring
  // with nothing to draw, and Rust would ignore the filter anyway.
  const visibleCount = view.windows.filter((window) => !window.hidden).length;

  return (
    <li className="px-4 py-3">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium">{view.name}</p>
          <p className="mt-0.5 text-[11px] text-neutral-500 dark:text-neutral-400">
            {!view.implemented
              ? messages.status.notImplemented
              : [
                  view.account?.email,
                  view.account?.plan,
                  view.enabled ? describeStatus(view.status, messages) : messages.status.off,
                ]
                  .filter(Boolean)
                  .join(" · ")}
          </p>
        </div>

        <div className="flex shrink-0 items-center gap-3">
          {view.enabled && view.windows.length > 1 && (
            <button
              type="button"
              aria-label={messages.settings.windows}
              aria-expanded={showWindows}
              title={messages.settings.windows}
              className="text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
              onClick={() => setShowWindows((open) => !open)}
            >
              <GearIcon />
            </button>
          )}

          <label
            className="flex items-center gap-1.5 text-[11px] text-neutral-500 dark:text-neutral-400"
            title={messages.settings.trayHint}
          >
            <input
              type="radio"
              name="primary"
              className="accent-neutral-700"
              checked={view.is_primary}
              disabled={!view.enabled || !view.implemented}
              onChange={() => {
                void setPref({ key: "primary", provider: view.id }).then(onChanged);
              }}
            />
            {messages.settings.tray}
          </label>

          <input
            type="checkbox"
            aria-label={messages.settings.enable(view.name)}
            className="size-4 accent-neutral-700"
            checked={view.enabled}
            disabled={!view.implemented}
            onChange={(event) => {
              void setPref({
                key: "provider_enabled",
                provider: view.id,
                enabled: event.currentTarget.checked,
              }).then(onChanged);
            }}
          />
        </div>
      </div>

      {showWindows && view.windows.length > 0 && (
        <fieldset className="mt-2 rounded-md bg-neutral-100 px-3 py-2 dark:bg-neutral-950">
          <legend className="px-1 text-[10px] uppercase tracking-wide text-neutral-500 dark:text-neutral-400">
            {messages.settings.windows}
          </legend>
          {view.windows.map((window) => (
            <label
              key={window.id}
              className="flex items-center gap-2 py-0.5 text-[11px] text-neutral-600 dark:text-neutral-300"
            >
              <input
                type="checkbox"
                className="size-3.5 accent-neutral-700"
                checked={!window.hidden}
                disabled={!window.hidden && visibleCount <= 1}
                onChange={(event) => {
                  void setPref({
                    key: "window_hidden",
                    provider: view.id,
                    window: window.id,
                    hidden: !event.currentTarget.checked,
                  }).then(onChanged);
                }}
              />
              {windowLabel(messages, window.id, window.label)}
            </label>
          ))}
          <p className="mt-1 px-1 text-[10px] text-neutral-400 dark:text-neutral-500">
            {messages.settings.windowsHint}
          </p>
        </fieldset>
      )}

      {view.enabled && view.implemented && (
        <>
          <div className="mt-2 flex items-center gap-3 text-[11px]">
            <button
              type="button"
              className="text-neutral-500 underline underline-offset-2 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
              onClick={() => setShowSteps((open) => !open)}
            >
              {messages.settings.connect}
            </button>
            {view.manage_url && (
              <button
                type="button"
                className="text-neutral-500 underline underline-offset-2 hover:text-neutral-800 dark:text-neutral-400 dark:hover:text-neutral-200"
                onClick={() => void openUrl(view.manage_url as string)}
              >
                {messages.settings.manage}
              </button>
            )}
          </div>

          {(showSteps || needsAuth) && (
            <ol className="mt-2 list-decimal space-y-1 rounded-md bg-neutral-100 py-2 pl-7 pr-3 text-[11px] text-neutral-600 dark:bg-neutral-950 dark:text-neutral-300">
              {connectSteps(messages, view.id).map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
          )}
        </>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Updates
// ---------------------------------------------------------------------------

function UpdateSection({ prefs, messages }: { prefs: PrefsView; messages: Messages }) {
  const [available, setAvailable] = useState<AvailableUpdate | null>(null);
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState(false);

  useEffect(() => {
    void invoke<AvailableUpdate | null>("update_status").then(setAvailable);
    const unlisten = listen<AvailableUpdate>(UPDATE_EVENT, ({ payload }) => setAvailable(payload));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  async function onCheck() {
    setChecking(true);
    try {
      setAvailable(await invoke<AvailableUpdate | null>("check_update_now"));
      setChecked(true);
    } finally {
      setChecking(false);
    }
  }

  return (
    <Card>
      <Toggle
        label={messages.settings.autoUpdate}
        checked={prefs.auto_update}
        onChange={(enabled) => void setPref({ key: "auto_update", enabled })}
      />
      <div className="flex flex-wrap items-center justify-between gap-2 border-t border-neutral-200 px-4 py-3 dark:border-neutral-800">
        <p className="text-[11px] text-neutral-500 dark:text-neutral-400">
          {available
            ? messages.settings.updateAvailable(available.version)
            : checked
              ? messages.settings.upToDate
              : messages.app.version(prefs.version)}
        </p>
        <div className="flex gap-2">
          <button
            type="button"
            disabled={checking}
            className="rounded-md bg-neutral-100 px-2 py-1 text-[11px] text-neutral-600 hover:bg-neutral-200 disabled:opacity-40 dark:bg-neutral-800 dark:text-neutral-300 dark:hover:bg-neutral-700"
            onClick={() => void onCheck()}
          >
            {messages.settings.checkNow}
          </button>
          {available && (
            <button
              type="button"
              className="rounded-md bg-neutral-800 px-2 py-1 text-[11px] text-neutral-50 hover:bg-neutral-700 dark:bg-neutral-100 dark:text-neutral-900 dark:hover:bg-white"
              onClick={() => void invoke("install_update")}
            >
              {messages.settings.installUpdate}
            </button>
          )}
        </div>
      </div>
    </Card>
  );
}

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------

export function Settings() {
  const { providers, reload } = useProviders();
  const prefs = usePrefs();
  const messages = useMessages(prefs);
  const [dismissed, setDismissed] = useState(false);

  if (prefs && !prefs.onboarded && !dismissed) {
    return (
      <Onboarding
        messages={messages}
        providers={providers}
        onChanged={() => void reload()}
        onDone={() => setDismissed(true)}
      />
    );
  }

  const languageOptions: { value: Language | "system"; label: string }[] = [
    { value: "system", label: messages.settings.languageSystem },
    ...LANGUAGES.map((language) => ({ value: language, label: messages.language[language] })),
  ];

  return (
    <main className="min-h-screen bg-neutral-50 p-6 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
      <header>
        <h1 className="text-xl font-semibold tracking-tight">{messages.app.name}</h1>
        <p className="mt-1 text-xs text-neutral-500 dark:text-neutral-400">
          {messages.settings.readOnly}
        </p>
      </header>

      <Section title={messages.settings.providers}>
        <Card>
          <p className="border-b border-neutral-200 px-4 py-3 text-[11px] text-neutral-500 dark:border-neutral-800 dark:text-neutral-400">
            {messages.connect.intro}
          </p>
          <ul className="divide-y divide-neutral-200 dark:divide-neutral-800">
            {providers === null ? (
              <li className="px-4 py-6 text-center text-xs text-neutral-400">
                {messages.app.loading}
              </li>
            ) : (
              providers.map((view) => (
                <ProviderRow
                  key={view.id}
                  view={view}
                  messages={messages}
                  onChanged={() => void reload()}
                />
              ))
            )}
          </ul>
        </Card>
      </Section>

      <Section title={messages.settings.refresh}>
        <Card>
          <ChoiceRow
            label={messages.settings.refreshEvery}
            options={POLL_CHOICES.filter(
              (seconds) =>
                prefs === null ||
                (seconds >= prefs.poll_seconds_min && seconds <= prefs.poll_seconds_max),
            ).map((seconds) => ({
              value: String(seconds),
              label:
                seconds < 60
                  ? messages.settings.seconds(seconds)
                  : messages.settings.minutes(seconds / 60),
            }))}
            selected={String(prefs?.poll_seconds ?? 60)}
            disabled={prefs === null}
            onSelect={(value) => void setPref({ key: "poll_seconds", seconds: Number(value) })}
          />
          <p className="border-t border-neutral-200 px-4 py-3 text-[11px] text-neutral-400 dark:border-neutral-800 dark:text-neutral-500">
            {messages.settings.refreshHint}
          </p>
        </Card>
      </Section>

      <Section title={messages.settings.notch}>
        <Card>
          {prefs && !prefs.notch_supported && (
            <p className="border-b border-amber-200 bg-amber-50 px-4 py-3 text-[11px] text-amber-800 dark:border-amber-900/50 dark:bg-amber-950/40 dark:text-amber-200">
              {messages.settings.notchUnsupported}
            </p>
          )}
          <Toggle
            label={messages.settings.notchVisible}
            checked={(prefs?.notch_visible ?? false) && (prefs?.notch_supported ?? true)}
            disabled={prefs === null || !prefs.notch_supported}
            onChange={(visible) => void setPref({ key: "notch_visible", visible })}
          />
          {prefs && prefs.notch_supported && (
            <>
              <div className="border-t border-neutral-200 dark:border-neutral-800">
                <ChoiceRow
                  label={messages.settings.notchEdge}
                  options={EDGES.map((edge) => ({ value: edge, label: messages.edge[edge] }))}
                  selected={prefs.notch_edge}
                  disabled={!prefs.notch_visible}
                  onSelect={(edge) => void setPref({ key: "notch_edge", edge })}
                />
              </div>
              <div className="border-t border-neutral-200 dark:border-neutral-800">
                <ChoiceRow
                  label={messages.settings.notchStyle}
                  options={STYLES.map((style) => ({
                    value: style,
                    label: messages.notchStyle[style],
                  }))}
                  selected={prefs.notch_style}
                  disabled={!prefs.notch_visible}
                  onSelect={(style) => void setPref({ key: "notch_style", style })}
                />
              </div>
              <div className="border-t border-neutral-200 dark:border-neutral-800">
                <ChoiceRow
                  label={messages.settings.animation}
                  options={ANIMATIONS.map((animation) => ({
                    value: animation,
                    label: messages.animation[animation],
                  }))}
                  selected={prefs.notch_animation}
                  disabled={!prefs.notch_visible}
                  onSelect={(animation) => void setPref({ key: "notch_animation", animation })}
                />
              </div>
              <div className="border-t border-neutral-200 dark:border-neutral-800">
                <Toggle
                  label={messages.settings.overTaskbar}
                  checked={prefs.notch_over_taskbar}
                  disabled={!prefs.notch_visible}
                  onChange={(over) => void setPref({ key: "notch_over_taskbar", over })}
                />
                <p className="px-4 pb-3 text-[11px] text-neutral-400 dark:text-neutral-500">
                  {messages.settings.overTaskbarHint}
                </p>
              </div>
            </>
          )}
          <p className="border-t border-neutral-200 px-4 py-3 text-[11px] text-neutral-400 dark:border-neutral-800 dark:text-neutral-500">
            {messages.settings.notchHint}
          </p>
        </Card>
      </Section>

      <Section title={messages.settings.appearance}>
        <Card>
          <ChoiceRow
            label={messages.settings.language}
            options={languageOptions}
            selected={prefs?.language_override ?? "system"}
            disabled={prefs === null}
            onSelect={(value) =>
              void setPref({ key: "language", language: value === "system" ? null : value })
            }
          />
          <div className="border-t border-neutral-200 dark:border-neutral-800">
            <Toggle
              label={messages.settings.autostart}
              checked={prefs?.autostart ?? false}
              disabled={prefs === null}
              onChange={(enabled) => void setPref({ key: "autostart", enabled })}
            />
          </div>
        </Card>
      </Section>

      <Section title={messages.settings.updates}>
        {prefs && <UpdateSection prefs={prefs} messages={messages} />}
      </Section>

      <footer className="mt-5 text-[11px] text-neutral-400 dark:text-neutral-500">
        <button
          type="button"
          className="underline underline-offset-2 hover:text-neutral-600 dark:hover:text-neutral-300"
          onClick={() => void openUrl(README_ANCHOR)}
        >
          {messages.settings.howItWorks}
        </button>
      </footer>
    </main>
  );
}
