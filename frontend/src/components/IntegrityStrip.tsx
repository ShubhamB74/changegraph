import type { IntegrityNote } from "../lib/integrity.ts";
import { STANDING_CAVEAT } from "../lib/integrity.ts";

const MARK: Record<IntegrityNote["level"], { glyph: string; className: string; label: string }> = {
  limit: { glyph: "▲", className: "text-signal-limit", label: "Limit" },
  caution: { glyph: "◐", className: "text-signal-changed", label: "Caution" },
  info: { glyph: "i", className: "text-ink-dim", label: "Note" },
};

const VISIBLE = 2;

/** Always-visible statement of what this report cannot guarantee. */
export function IntegrityStrip({ notes }: { notes: IntegrityNote[] }) {
  const shown = notes.slice(0, VISIBLE);
  const rest = notes.slice(VISIBLE);
  return (
    <section aria-label="Analysis integrity" className="border-b border-ink-line bg-ink-panel px-4 py-2 text-sm">
      <ul className="flex flex-col gap-1">
        {shown.map((n) => (
          <NoteRow key={n.id} note={n} />
        ))}
      </ul>
      {rest.length > 0 && (
        <details className="mt-1">
          <summary className="cursor-pointer text-ink-dim hover:text-ink-text">
            {rest.length} more {rest.length === 1 ? "limitation" : "limitations"}
          </summary>
          <ul className="mt-1 flex flex-col gap-1">
            {rest.map((n) => (
              <NoteRow key={n.id} note={n} />
            ))}
          </ul>
        </details>
      )}
      <p className="mt-1 text-xs text-ink-faint">{STANDING_CAVEAT}</p>
    </section>
  );
}

function NoteRow({ note }: { note: IntegrityNote }) {
  const m = MARK[note.level];
  return (
    <li className="flex gap-2">
      <span className={`w-4 shrink-0 text-center ${m.className}`} aria-label={m.label} role="img">
        {m.glyph}
      </span>
      <span>{note.text}</span>
    </li>
  );
}
