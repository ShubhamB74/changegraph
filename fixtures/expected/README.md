# Expected outputs

Hand-labeled expected results for fixtures live here, starting in Phase 2
(expected dependency edges per fixture) and used by the evaluation harness in Phase 7.

For `simple-ts-project`, the intended dependency edges (`A -> B` = A imports B) are:

```
src/api/profile.ts    -> src/auth/session.ts
src/index.ts          -> src/api/profile.ts   (re-export)
src/index.ts          -> src/auth/session.ts  (re-export)
tests/profile.test.ts -> src/api/profile.ts
tests/session.test.ts -> src/auth/session.ts
```

These are *labels for future tests*; Phase 1 does not extract edges yet.
