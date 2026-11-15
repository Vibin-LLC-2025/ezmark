# Pipeline snapshots

`tests/spec/` checks the parser against the specs. These files check
everything after it. Each `.snap` holds what the full pipeline
(`mdcore::render`: parse, block markers, front matter, highlighting, URL
policy, sanitizer) makes of every example in one spec file, exactly as the
viewer would put it in the page.

| File | Input |
|---|---|
| `commonmark-0.31.2.snap` | the 652 examples of `tests/spec/commonmark-0.31.2.txt` |
| `gfm.snap` | the 670 enabled examples of `tests/spec/gfm.txt` |

The test (`crates/mdcore/tests/conformance.rs`) fails if the pipeline's output
for any example differs from the saved one by a single byte. The pass
condition is "nothing changed since a person last looked", not "equals the
spec", because the pipeline differs from the spec on purpose.

## Accepting a change

```
EZMARK_BLESS=1 cargo test -p mdcore --test conformance
git diff tests/pipeline
```

Read the diff before committing it. A change you did not intend is a bug, and
a change you did intend should be the only one in the diff.

## Reading a snapshot

Each example is headed by its number and section in the spec and says whether
the output is the same as the spec's HTML once the block markers are set
aside. As of 2026-11-14: 354 of 652 CommonMark examples and 354 of 670 GFM
examples are the same; the rest differ in the ways listed below.

The snapshots have not yet been reviewed example by example by a person. They
were generated on 2026-11-14 and classified by script into the kinds below; a
full read is still owed before M1 closes.

## Deliberate differences from the spec

Every difference falls under one of these. Counts are for the CommonMark
file; the GFM file has the same ones plus its extension examples.

| Difference | Why | Examples |
|---|---|---|
| A marker, `<ezmark-block key="…">`, before each top-level block | Content keys for live reload (`crates/mdcore/src/blocks.rs`). The view removes them. | all |
| `rel="noopener noreferrer"` on every link | The sanitizer adds it. No link navigates the webview, so it is belt and braces. | about 140 |
| An `id` on every heading and an anchor link inside it, both prefixed `user-content-` | GitHub's heading anchors. The prefix keeps a document's ids from shadowing the app's. | about 40 |
| `<br>`, `<hr>`, `<img>` without ` /`, and attributes re-quoted | The sanitizer parses the HTML and writes it out again in HTML5 form. | about 55 |
| Characters written as themselves where the spec has an entity (`"` for `&quot;`), and the other way round in attributes | Same re-serialization. The parsed document is identical. | about 30 |
| An image's `src` replaced by `ezmark-blocked:<reason>:<hex>` | No image in the examples exists on disk, and remote images never load. The view turns the marker into a sentence. | about 22 |
| Raw HTML changed or removed | The sanitizer: unknown elements and attributes go, unclosed elements are closed, comments, processing instructions, declarations, and CDATA are dropped. This is the largest group, and the point of the pipeline. | about 60 (the "HTML blocks" and "Raw HTML" sections) |
| A link with no `href` | Its scheme is not `http`, `https`, or `mailto` (`irc:`, `ftp:`, a made-up one). The text stays. | a few, mostly in the "Autolinks" section |
| `#fragment` links become `#user-content-fragment` | To match the prefixed ids they point at. | 1 |
| Fenced code in a known language contains `<span class="hl-…">` | Highlighting. | 2 |
| A leading `---` block becomes a "Front matter" block | The parser option for front matter is on. The spec reads the same lines as a thematic break and a setext heading. | 1 (example 96) |
| Bare URLs and `www.` addresses become links, `~~text~~` is struck through, and pipe tables are tables, in every example | The GFM extensions are always on in the pipeline; the spec runs turn each on only for the examples that name it. | a handful |
| Task list items get classes and a disabled checkbox | The task list extension is on. | where a list item starts with `[ ]` or `[x]` |

If you find a difference that fits none of these, it is a bug or a missing
row. Either way it should not be blessed until one of the two is fixed.
