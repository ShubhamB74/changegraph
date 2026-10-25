import type { ChangeEvent } from "react";

export function EmptyState({ onFile, busy }: { onFile: (f: File) => void; busy: boolean }) {
  return (
    <div className="mx-auto flex h-full max-w-2xl flex-col justify-center gap-5 px-6 py-10">
      <h1 className="text-2xl font-semibold">{busy ? "Loading…" : "Open a ChangeGraph report"}</h1>
      {!busy && (
        <>
          <p className="text-ink-dim">
            This viewer reads a JSON report produced by the <span className="font-mono">changegraph</span> command line tool.
            Nothing is uploaded; the file is read in your browser.
          </p>
          <pre className="overflow-x-auto rounded border border-ink-line bg-ink-panel p-3 font-mono text-sm">
            changegraph analyze . --base main --format json -o report.json
          </pre>
          <div>
            <FileButton onFile={onFile} label="Choose report.json" />
            <span className="ml-3 text-sm text-ink-dim">or drop the file anywhere on this page</span>
          </div>
        </>
      )}
    </div>
  );
}

export function FileButton({ onFile, label }: { onFile: (f: File) => void; label: string }) {
  return (
    <label className="inline-block cursor-pointer rounded border border-ink-line bg-ink-raised px-3 py-1.5 text-sm hover:bg-ink-line focus-within:outline focus-within:outline-2 focus-within:outline-[#7fb2ff]">
      {label}
      <input
        type="file"
        accept=".json,application/json"
        className="sr-only"
        onChange={(e: ChangeEvent<HTMLInputElement>) => {
          const f = e.target.files?.[0];
          if (f) onFile(f);
          e.target.value = "";
        }}
      />
    </label>
  );
}

export function ErrorPanel({ errors, onDismiss }: { errors: string[]; onDismiss: () => void }) {
  return (
    <div role="alert" className="border-b border-signal-limit bg-ink-panel px-4 py-3 text-sm">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="font-medium text-signal-limit">This file could not be opened as a ChangeGraph report.</p>
          <ul className="mt-1 list-disc pl-5 text-ink-dim">
            {errors.slice(0, 8).map((e, i) => (
              <li key={i} className="break-words">
                {e}
              </li>
            ))}
          </ul>
          {errors.length > 8 && <p className="mt-1 text-xs text-ink-faint">and {errors.length - 8} more problems.</p>}
        </div>
        <button type="button" onClick={onDismiss} className="shrink-0 rounded border border-ink-line px-2 py-1 hover:bg-ink-raised">
          Dismiss
        </button>
      </div>
    </div>
  );
}
