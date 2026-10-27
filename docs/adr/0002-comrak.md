# ADR 0002: comrak as the markdown parser

- Status: accepted
- Date: 2026-10-27
- Evidence: [engine research](../research/03-markdown-engine.md) sections 2 and 3

## Context

The product promise is rendering that matches GitHub: tables, task lists,
footnotes, alerts, heading anchors, front matter, and the sanitized subset of
raw HTML that READMEs rely on (`<details>`, `<picture>`, centered images).
Parse speed matters less than fidelity: DOM build, highlighting, and layout
dominate the cost of showing a large file.

## Decision

Use comrak (0.59.x), the Rust port of cmark-gfm, with raw HTML preserved at
parse time (`render.unsafe = true`) and every byte of its output passed
through the ammonia sanitizer before it leaves `mdcore`.

## Why

- cmark-gfm is the parser GitHub's pipeline is built on, and comrak tracks it
  closely, including its edge cases. Its README reports 652/652 CommonMark and
  670/670 GFM spec tests.
- Every extension v1 needs is in one crate: GFM, footnotes, alerts, header
  IDs, front matter, shortcodes, plus URL rewriter hooks and a highlighter
  adapter.
- pulldown-cmark is about 1.9x faster in comrak's own benchmark and covers
  most extensions now, but it does not aim at cmark-gfm parity, and parse time
  is not the bottleneck.
- markdown-rs warns about very large and pathological inputs; cmark-gfm itself
  would add C FFI to the Windows and macOS builds.

## Consequences

- The sanitizer, not the parser, decides what HTML survives. Parser
  conformance (spec settings, no sanitizer) and pipeline expectations (parse,
  rewrite, sanitize) are separate test sets from M1 on, so a working
  sanitizer never fails a spec test.
- comrak has had a quadratic-parse CVE (fixed in 0.17). Parsing runs off the
  UI thread, inputs above 50 MB are refused, and the pipeline is fuzzed after
  M3 (plan section 9).
- Bug-for-bug parity is a feature: where comrak differs from GitHub, the
  difference is documented in the pipeline expectations rather than patched
  around.
