# Spec fixtures

The two specifications ezmark's parser is tested against, as their authors
publish them. Do not edit these files: `crates/mdcore/tests/conformance.rs`
reads the examples out of them, and the test is only worth something if the
examples are the official ones.

| File | What | Source | SHA-256 |
|---|---|---|---|
| `commonmark-0.31.2.txt` | CommonMark spec 0.31.2, 652 examples | `spec.txt` at tag `0.31.2` of [commonmark/commonmark-spec](https://github.com/commonmark/commonmark-spec) | `257c41ad946f7a1414a499aca402a1aa8fdac3678532266611348c1cf54f4b80` |
| `gfm.txt` | GitHub Flavored Markdown spec 0.29-gfm, 672 examples, 2 of them marked `disabled` | `test/spec.txt` at commit `2f1b2ae84edf10fc3f3a2138d69dc4ceaf8e81f1` of [kivikakk/cmark-gfm](https://github.com/kivikakk/cmark-gfm), the copy comrak 0.59.0 tests itself against | `db86aa739f0259ce8207d84b99a087878086b09e2b71ce0eee2d28529be1cf5b` |

Fetched on 2026-11-14.

## Licence

Both specifications are licensed
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/), not under
ezmark's MIT OR Apache-2.0. CommonMark: copyright John MacFarlane. GitHub
Flavored Markdown: copyright GitHub, Inc., based on the CommonMark spec. They
are here unmodified, as test data, and are not compiled into the viewer.

## How the examples are read

The same way the specs' own `spec_tests.py` reads them. An example opens with
a line of 32 backticks and the word `example`, optionally followed by the
extensions it needs (`table`, `strikethrough`, `autolink`) or by `disabled`.
A line holding only `.` separates the markdown from the expected HTML, and
`→` stands for a tab. Disabled examples are skipped but still counted in the
numbering, so example numbers match the published specs.

The parser runs with its defaults plus raw HTML kept, and only the extensions
an example names. For the GFM file the parser's `gfm_quirks` option is on, as
it is in comrak's own run of that file, because the expected HTML there
records cmark-gfm's behaviour.

One GFM example (652, the tag filter) has `<IGNORE>` as its expected HTML.
The spec's runner passes it without comparing, and so does this one. What
ezmark does with that input is pinned in `tests/pipeline/gfm.snap`.

## Updating

Replace a file with the new published version, update the table above and the
example counts in `conformance.rs`, run the tests, then regenerate and review
the pipeline snapshots (`tests/pipeline/README.md`).
