import type { ReactNode } from "react";
import { splitPath } from "../lib/format.ts";
import type { ReportModel } from "../lib/model.ts";
import { STATE_STYLE, TEST_COLOR } from "../lib/theme.ts";

const CAP = 200;

interface Props {
  model: ReportModel;
  selected: string | null;
  onSelect: (path: string) => void;
}

function PathLabel({ path }: { path: string }) {
  const { dir, base } = splitPath(path);
  return (
    <span className="min-w-0 flex-1 truncate font-mono text-xs" title={path}>
      <span className="text-ink-text">{base}</span>
      {dir && <span className="ml-1 text-ink-faint">{dir}</span>}
    </span>
  );
}

/** Keyboard-friendly alternative to clicking nodes on the canvas. */
export function FileLists({ model, selected, onSelect }: Props) {
  const affected = model.report.impact.affected;
  const tests = model.report.test_recommendations.recommendations;
  const item = (path: string, active: boolean, children: ReactNode) => (
    <li key={path}>
      <button
        type="button"
        onClick={() => onSelect(path)}
        aria-current={active ? "true" : undefined}
        className={`flex w-full items-center gap-2 rounded px-2 py-1 text-left hover:bg-ink-raised ${active ? "bg-ink-raised" : ""}`}
      >
        {children}
      </button>
    </li>
  );

  return (
    <div className="px-2 py-3">
      <section aria-labelledby="aff-h" className="mb-4">
        <h2 id="aff-h" className="mb-1 px-2 text-sm font-semibold">
          Affected files, ranked
        </h2>
        {affected.length === 0 ? (
          <p className="px-2 text-sm text-ink-dim">
            No file depends on the changed files in this report. That says nothing about code the graph cannot see; check the
            notes above.
          </p>
        ) : (
          <ol>
            {affected.slice(0, CAP).map((a) =>
              item(
                a.path,
                selected === a.path,
                <>
                  <span className="w-6 text-right text-xs tabular-nums text-ink-faint">{a.rank}</span>
                  <span aria-hidden style={{ color: STATE_STYLE[a.classification].color }}>
                    {STATE_STYLE[a.classification].glyph}
                  </span>
                  <PathLabel path={a.path} />
                </>,
              ),
            )}
          </ol>
        )}
        {affected.length > CAP && (
          <p className="px-2 pt-1 text-xs text-ink-faint">
            Showing the first {CAP} of {affected.length}. The graph and JSON have all of them.
          </p>
        )}
      </section>

      <section aria-labelledby="tests-h">
        <h2 id="tests-h" className="mb-1 px-2 text-sm font-semibold">
          Recommended tests
        </h2>
        {tests.length === 0 ? (
          <p className="px-2 text-sm text-ink-dim">
            No tests were identified. This is not evidence that no tests are needed.
          </p>
        ) : (
          <ol>
            {tests.slice(0, CAP).map((t) =>
              item(
                t.path,
                selected === t.path,
                <>
                  <span className="w-6 text-right text-xs tabular-nums text-ink-faint">{t.rank}</span>
                  <span aria-hidden style={{ color: TEST_COLOR }}>
                    ▶
                  </span>
                  <PathLabel path={t.path} />
                  <span className={`text-xs ${t.basis === "known" ? "text-ink-dim" : "text-signal-limit"}`}>
                    {t.basis === "known" ? "known" : "guess"}
                  </span>
                </>,
              ),
            )}
          </ol>
        )}
      </section>
    </div>
  );
}
