# Frontend (Phase 6 — not started)

Intentionally empty. Per the implementation plan, the visualization is built only after the
Rust engine produces reliable dependency/impact reports.

Planned stack: Next.js + TypeScript, `@xyflow/react`, Tailwind CSS. Data flow: the CLI writes a
JSON report (`changegraph analyze . --format json -o report.json`) and the UI loads it.
The UI must never fabricate graph relationships or hide analysis warnings.
