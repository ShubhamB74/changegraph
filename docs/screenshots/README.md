# Screenshots

No screenshots are committed. They were not produced because the viewer's React Flow canvas has not
been run in the environment where this documentation was written, and a screenshot of anything else
would be misleading.

To add them:

1. Run `scripts/demo.sh demo-report.json` and `cd frontend && npm run dev`.
2. Open `demo-report.json` in the viewer and capture:
   - `overview.png`: nothing selected, legend and integrity strip visible;
   - `inspector-chain.png`: `tests/profile.test.ts` selected, chain highlighted;
   - `untested-module.png`: `src/index.ts` selected, showing the "no test is linked" warning.
3. Save them in this folder and reference them from `README.md`.

Capture at the default zoom with the bundled sample or the demo report; do not retouch.
