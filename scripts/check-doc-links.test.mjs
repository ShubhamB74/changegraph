import test from "node:test";
import assert from "node:assert/strict";
import { isLocal, linksIn } from "./check-doc-links.mjs";

test("finds markdown links but ignores code blocks and inline code", () => {
  const md = "See [a](docs/a.md) and [b](../b.md#sec \"title\").\n```\n[x](nope.md)\n```\nInline `[y](nope2.md)` too.\n";
  assert.deepEqual(linksIn(md), ["docs/a.md", "../b.md#sec"]);
});

test("only relative targets are checked", () => {
  assert.equal(isLocal("docs/a.md"), true);
  assert.equal(isLocal("../a.md#x"), true);
  assert.equal(isLocal("https://example.com"), false);
  assert.equal(isLocal("mailto:a@b.c"), false);
  assert.equal(isLocal("#anchor"), false);
});
