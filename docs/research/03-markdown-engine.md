# 03: Markdown parsing and rendering engine

Researched 2026-10-26. Scope: parser, extensions, security, rendering pipeline, performance, file watching, and parity testing for a read-only, cross-platform (Windows + macOS) markdown preview tool whose target is "renders like GitHub". GUI-framework independent.

Confidence key used below: **[primary]** = read from the project's own repo/docs/spec; **[third-party]** = aggregator, blog, or vendor comparison; **[approx]** = number not verified against a primary source on 2026-10-26, verify before quoting publicly.

---

## 1. Summary and recommended pipeline

1. **Parse (Rust, off the UI thread):** comrak (cmark-gfm port, 652/652 CommonMark + 670/670 GFM spec tests) with GFM extensions + alerts + footnotes + header IDs + front matter + math syntax + shortcodes + wikilinks enabled, `render.unsafe = true`.
2. **Rewrite URLs during parse:** comrak `image_url_rewriter` / `link_url_rewriter` turn relative paths into a scoped custom asset protocol URL, `.md` links into in-app navigation, and remote images into blocked placeholders (opt-in to load).
3. **Sanitize (Rust):** ammonia with a GitHub-shaped allowlist (adds `input[type=checkbox][disabled]`, `picture`/`source`, `class` for a known set, `id` on headings). Never ship unsanitized HTML to the webview.
4. **Highlight code:** syntect (Rust, at render time, CSS classes) for v1; consider arborium (tree-sitter) later. No JS highlighter on the critical path.
5. **Webview:** strict CSP (no inline/remote script, `img-src` only the asset scheme + `data:`), sanitized HTML morphed into the DOM with idiomorph to keep scroll and `<details>` state.
6. **Post-render (JS, trusted libraries, lazy):** KaTeX for math (`trust: false`), Mermaid loaded only when a mermaid fence exists (`securityLevel: 'strict'`).
7. **Watch:** watch the file's parent directory (notify + notify-debouncer-full, or chokidar with `atomic`), match by path, debounce ~75 ms, re-read, full re-parse, morph.
8. **JS-only alternative** (if the shell has no Rust side): markdown-it + plugins + DOMPurify, or comrak compiled to WASM. Parity tiebreaker: comrak is bug-for-bug cmark-gfm; nothing in JS is.
9. **Biggest performance risk:** not parsing, but DOM build + highlighting + layout on 1 to 10 MB files, plus pathological inputs. Parse in a worker with a cancel path; highlight lazily; cap synchronous work.
10. **Parity tests:** CommonMark + GFM spec suites, then snapshot diffs against GitHub's `POST /markdown` (mode `gfm`) over a README corpus.

---

## 2. Parser comparison

| Parser | Lang | CommonMark / GFM compliance | Speed evidence | Extensions relevant to us | Output control | WASM | Maintenance (as of 2026-10-26) |
|---|---|---|---|---|---|---|---|
| **comrak** 0.59.0 | Rust | Targets CommonMark 0.31.2; README badges 652/652 CommonMark, 670/670 GFM spec tests [primary]. Port of cmark-gfm, maintainer says cmark-gfm bugs are likely reproduced (that is a feature for parity) | Comrak CI hyperfine run on 2026-10-21 (Release v0.57.0, PR #879): comrak ~102 ms, cmark-gfm ~107 ms, pulldown-cmark ~53.9 ms (1.9x faster), markdown-it.rs (Rust port, NOT JS markdown-it) ~274 ms [primary, GitHub Actions runner, whole bench corpus, noisy]. README: "not and will not be the fastest" | All GFM (table, tasklist, strikethrough, autolink), footnotes + inline footnotes, alerts, Obsidian callouts, header IDs, front matter, math (dollars, LaTeX, code), shortcodes (emoji), wikilinks, description lists, highlight, sub/superscript, URL rewriters for links and images, syntect plugin adapter, sourcepos | Mutable AST (arena) + HTML/CommonMark/XML renderers; custom highlighter trait | Yes, JS/TS bindings listed in README (verify package health before relying) | Very active: 0.57 to 0.59 in Oct 2026. Used by crates.io, docs.rs, GitLab, Deno doc, Reddit (fork) |
| **pulldown-cmark** 0.13.5 | Rust | Goal is 100% CommonMark; GFM not claimed bug-for-bug [primary] | Fastest Rust option in comrak's bench (1.9x comrak). SIMD scanners opt-in | Tables, footnotes (GFM style), strikethrough, tasklists, heading attributes, YAML/`+++` metadata blocks, math events, `ENABLE_GFM` (alerts: `[!NOTE]` etc.), definition lists, super/subscript, wikilinks [primary, docs.rs] | Pull-parser event stream (no AST), offset iterator for source maps | Pure Rust, compiles to wasm32 | Active; used by rustdoc and mdBook. Event API has had breaking changes between minors |
| **markdown-rs** (`markdown` crate) | Rust | 100% CommonMark, full GFM, MDX, front matter, math claimed; 2,300+ tests, fuzzed [primary] | No published numbers. README warns huge inputs (100 MB) and many unclosed constructs can be slow or crash; suggests ~500 KB cap and a killable thread [primary] | GFM, math, frontmatter, MDX | mdast AST with positions, or HTML | Yes | Slower release cadence; ~1.6k stars |
| **cmark-gfm** | C | Reference GFM implementation (GitHub's fork of cmark) [primary] | ~same as comrak in comrak's bench | table, strikethrough, autolink, tagfilter, tasklist, footnotes; no alerts in this repo (GitHub adds those elsewhere) | C AST + HTML | Community emscripten ports (cmark-gfm-js) | Active repo, no formal releases page. FFI + build complexity on Windows/macOS is a cost for us |
| **markdown-it** v15 | JS | 100% CommonMark claimed; GFM tables/strikethrough built in; tasks, footnotes, alerts, anchors via plugins; GFM autolinks via linkify-it (not spec-identical) | No numbers in README. Used by VS Code's preview [primary] | Large plugin ecosystem: markdown-it-footnote, markdown-it-anchor, markdown-it-github-alerts (antfu, GitHub-matching output + CSS), task lists, front matter, KaTeX plugins | Token stream + HTML string | N/A (JS) | Very widely used, ~22k stars |
| **micromark / remark / rehype** | JS | 100% CommonMark, GFM extensions described as 100% compliant [primary] | "Small, pretty fast", ~14 kB core; no numbers. Same DoS warning as markdown-rs [primary] | GFM, math, frontmatter, directives; remark/rehype plugins for everything else (rehype-sanitize, rehype-slug) | mdast/hast ASTs; best for tree transforms | N/A | Active (unified collective) |
| **goldmark** v2 | Go | CommonMark 0.31.2 [primary] | Author bench: ~5.7 ms/op on the CommonMark spec doc [primary, hardware unstated] | GFM bundle (table, strikethrough, linkify, tasklist), footnotes, definition lists, typographer; GFM ext does NOT filter disallowed tags | AST + HTML | Go WASM (large) | v2 "early release". Hugo uses it (generally known, not confirmed in this session). Wrong language for a Rust or JS shell |

**Recommendation:** comrak for a Rust-backed shell (Tauri or similar). The discriminating constraint is "renders like GitHub": comrak is a port of the parser GitHub's pipeline is built on, has every extension we need in one crate, and exposes URL rewriters and a highlighter hook. pulldown-cmark is ~2x faster and now covers alerts, math, and wikilinks, but parse time is not our bottleneck (see section 5), and it does not aim at cmark-gfm edge-case parity. For a JS-only shell (Electron without a Rust core): markdown-it (what VS Code uses) + plugins, sanitized with DOMPurify; or comrak's WASM build if parity matters more than bundle size.

---

## 3. Extension-by-extension recommendation

| Feature | Decision | How | Reason |
|---|---|---|---|
| Tables | **Ship v1** | comrak `table` | Core GFM |
| Task lists | **Ship v1** | comrak `tasklist`; allow `input[type=checkbox][disabled]` in sanitizer | Core GFM. Ammonia's default tags do NOT include `input`, so it must be allowlisted explicitly |
| Strikethrough | **Ship v1** | comrak `strikethrough` | Core GFM |
| Autolinks (bare URLs, www.) | **Ship v1** | comrak `autolink` | Core GFM |
| Footnotes | **Ship v1** | comrak `footnotes` | GitHub renders them in .md files |
| GitHub alerts (`> [!NOTE]`, TIP, IMPORTANT, WARNING, CAUTION) | **Ship v1** | comrak `alerts`; style with GitHub-like CSS + icons | Very common in modern READMEs; GitHub renders as plain blockquote when the rule is not met (top-level, first line marker) |
| Heading anchors / auto IDs | **Ship v1** | comrak `header_id_prefix` (use `""` or a short prefix and match GitHub's slug: lowercase, spaces to `-`, strip punctuation, dedupe with `-1`, `-2`) | Needed for in-document `#anchor` links and TOC; GitHub's slug rules are documented |
| Front matter (YAML/TOML) | **Ship v1 (hide or show as a collapsible table)** | comrak `front_matter_delimiter = "---"` (omitted from HTML); parse separately for display | Unhandled front matter renders as an ugly `<hr>` + paragraph. GitHub renders YAML front matter as a table; collapsible table is a nice touch |
| Emoji shortcodes (`:+1:`) | **Ship v1** | comrak `shortcodes` feature | Cheap (data table in Rust), GitHub renders them |
| Syntax highlighting | **Ship v1** | syntect via comrak `SyntectAdapter`, class-based output, themes as CSS (light/dark). Load from binary dump; syntect docs: ~23 ms to load all syntaxes from a dump with lazy regex compilation vs ~138 ms from source [primary, older measurement] | Server-side highlighting keeps JS bundle small and removes a highlighting flash. Highlight lazily or skip above a per-block size limit |
| Math (`$...$`, `$$...$$`, ` ```math `) | **Ship v1, lazy** | comrak `math_dollars` + `math_code` emit marked spans; KaTeX renders them client-side only if present, `trust: false`, `throwOnError: false` | GitHub supports math; VS Code's preview uses KaTeX. KaTeX is synchronous and much faster/smaller than MathJax (third-party claims ~100x and ~250 KB vs ~1.6 MB gzip [third-party, approx]). Coverage gap vs MathJax (no `\require`, fewer packages) is acceptable for a viewer |
| Mermaid diagrams | **Ship v1, lazy-loaded only** | Detect ` ```mermaid ` fences; dynamically import Mermaid only then; `securityLevel: 'strict'`; render into the block | Very common in READMEs and AI-generated docs, so expected. Cost is large: bundlephobia lists mermaid 12.2.0 at 693.6 kB min / 169.5 kB gzip for the entry [primary], and one project reports ~3.4 MB vendored with diagram chunks [third-party]. Never on the startup path |
| Wiki links (`[[Page]]`, `[[Page\|label]]`) | **Ship v1 (behind a setting, default on)** | comrak `wikilinks_title_after_pipe` (Obsidian order); resolve to a sibling `.md` by name | Obsidian users expect it; cheap to support. Leave `![[embed]]` as text in v1 (`wikilinks_ignore_embeds`) |
| Obsidian callouts (`> [!tip]+ Title`, foldable) | **Defer to v1.x** | comrak `ofm_callouts` exists | Nice for Obsidian users, but diverges from GitHub rendering; ship after alerts look right |
| Raw HTML (`<details>`, `<kbd>`, `<picture>`, `<img width>`, `<p align=center>`) | **Ship v1 (sanitized)** | `unsafe = true` + ammonia allowlist | Comrak's default scrubs ALL raw HTML, which makes READMEs look broken (centered logos, `<details>`, dark-mode `<picture>`). GitHub allows a sanitized subset, so we must too |
| Definition lists, superscript, subscript, highlight `==x==`, spoilers, underline | **Skip** (not GFM) | | Each changes how valid GFM renders (e.g. single tilde subscript overrides strikethrough). Off for parity |
| Smart punctuation / typographer | **Skip** | | GitHub does not do it |
| GitHub-only references (`#123`, `@user`, SHA links) | **Skip** | | Needs repo context and network; not part of file rendering |
| Diagram formats beyond Mermaid (PlantUML, GeoJSON/TopoJSON, STL) | **Skip / defer** | | GitHub renders some, but rare in local files and heavy |
| TOC sidebar from headings | **Defer** (UI workstream) | Walk comrak AST headings | Belongs to UX; data is free from the AST |

---

## 4. Security

**Threat model.** A viewer opens arbitrary files (downloaded repos, email attachments, AI output). A `.md` file can contain `<script>`, inline event handlers (`<img onerror>`), `javascript:`/`data:`/`vbscript:` URLs, `<iframe>`/`<object>`/`<embed>`, `<form>`, `<style>` (UI spoofing, CSS exfiltration), `<meta http-equiv=refresh>`, `<base href>`, SVG with script, and remote images (tracking pixels, IP leak, NTLM leak via `file://`/UNC paths on Windows). Mermaid and KaTeX also run over attacker-controlled text.

**How others do it.**
- **GitHub:** convert to HTML, then "aggressively" sanitize (scripts, inline styles removed), then highlight, then add emoji/task list/anchor filters, and proxy images through its CDN (camo) [primary: github/markup README]. cmark-gfm's `tagfilter` (escapes `title, textarea, style, xmp, iframe, noembed, noframes, script, plaintext`) is defense in depth only; it cannot catch malicious attributes [third-party: cmark-gfm-js].
- **VS Code preview:** default "Strict" disables scripts and only loads `https` resources; "Allow insecure content" allows `http`; "Disable" allows scripts [primary]. Built on markdown-it.
- **Parsers' own defaults:** comrak, cmark-gfm, goldmark, markdown-rs, micromark all drop raw HTML and dangerous URLs by default [primary]. Every one of them recommends a real sanitizer if you turn raw HTML on.

**Our stance (defense in depth, four layers):**
1. **Sanitizer is mandatory, in Rust:** ammonia 4.2.x (html5ever-based allowlist, parses like a browser). Defaults are a good base: ~80 tags including `details`/`summary`, URL schemes limited (http, https, mailto, etc.; no `javascript:`), `rel="noopener noreferrer"` on links, `script`/`style` content removed [primary]. Required additions: `input` (checkbox, `disabled`, `checked`), `picture` + `source` (`srcset`, `media`) for GitHub dark-mode images, `id` on headings and footnote elements, `class` for a fixed set (alert, footnote, language-*, highlighter token classes, math markers), `align` on `p`/`div`, `width`/`height` on `img`. Do not allow `style`, `svg`, `iframe`, `form`, `object`, `embed`, `base`, `meta`. Use ammonia's `url_relative` (`Custom` or `RewriteWithBase`) and `attribute_filter` as a second check on URLs after comrak's rewriters.
   - JS equivalent: DOMPurify 3.4.x with `USE_PROFILES: { html: true }`, `FORBID_TAGS: ['style']`, URL rewriting in `uponSanitizeAttribute` hook. DOMPurify warns that modifying HTML after sanitizing can void it, so do URL rewriting inside the hook, not after [primary].
2. **CSP in the webview** (the backstop if the sanitizer has a bug): `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'` (KaTeX and Mermaid emit inline styles; tighten with hashes later) `; img-src <asset-scheme>: data:; font-src 'self'; connect-src <ipc only>; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`. Tauri injects hashes/nonces for bundled scripts automatically and its docs say keep CSP "as restricted as possible" and avoid remote content [primary]. Add `'wasm-unsafe-eval'` only if WASM is used in the webview.
3. **Remote content blocked by default:** remote `http(s)` images replaced with a placeholder ("Load remote images for this file / folder") mirroring VS Code Strict and mail clients. Block `file://` and UNC (`\\server\share`) URLs outright on Windows (credential leak). `http:` (non-TLS) never loads.
4. **Navigation control:** all link clicks intercepted; `.md` links open in the viewer, `#anchors` scroll, external `http(s)`/`mailto` open in the system browser after the OS handler, anything else is dropped. The webview must never navigate away from the app page. Disable devtools, context-menu "open in new window", and drag-drop navigation in release builds.

**Trusted post-processors.** KaTeX and Mermaid run after sanitization on text content the sanitizer already passed. Configure them safely: KaTeX `trust: false` (blocks `\href`, `\includegraphics` with arbitrary URLs), `maxExpand` default; Mermaid `securityLevel: 'strict'` (encodes HTML in labels, disables click callbacks). Pin versions and track their advisories, since both have had XSS fixes historically.

**Relative link and image resolution.**
- Resolve against the directory of the open file. Normalize, then **reject anything outside an allowed root** (default: the file's own directory tree, or the git repo root if found by walking up for `.git`). This blocks `![](../../../../Users/me/.ssh/id_rsa)`-style probes from loading local files into the page (exfiltration is already blocked by CSP, but defense in depth matters).
- Serve images through a custom scheme (e.g. `mdview://asset/<token>/<path>` or Tauri's scoped asset protocol, which appears as `asset:` / `http://asset.localhost` in CSP) whose handler re-checks the path scope, serves only image MIME types (png, jpg, gif, webp, svg as `<img>` only, avif), and sets `X-Content-Type-Options: nosniff`. SVG loaded via `<img>` cannot run script, so that is safe; never inline SVG markup from files.
- `/path` links: GitHub resolves from repo root; do the same when a repo root is found, else the file's directory.
- Links to `.md`/`.markdown` files (with optional `#anchor`) become in-app navigation with back/forward history. Links to other local file types: offer "open with default app" via the OS, never load in the webview.
- Wikilinks resolve by filename search within the root (Obsidian semantics: shortest path match).
- Implementation hook: comrak `link_url_rewriter` / `image_url_rewriter` (both exist on `Extension` in 0.59) do this at parse time; ammonia `url_relative` is the second gate.

---

## 5. Performance

**Where time goes.** For a 1 MB file, Rust parse + render is in the low tens of milliseconds (comrak's bench corpus runs in ~100 ms on a CI runner; pulldown-cmark ~2x faster) [primary, CI numbers, approx per-file]. The expensive parts are: syntax highlighting (syntect is regex-based and is usually the slowest stage on code-heavy docs), sanitizing (ammonia builds a full DOM; fast for small docs, ~88 µs for its docs intro in an old benchmark [primary, old]), IPC transfer of multi-MB HTML strings into the webview, and above all the browser's HTML parse + style + layout of tens of thousands of nodes.

**Time budgets (targets to validate with a benchmark harness, not measured yet):**
| File | Target open-to-first-paint | Target re-render on save |
|---|---|---|
| Typical README (< 50 KB) | < 50 ms total, < 16 ms for pipeline work | < 16 ms pipeline, morph without flicker |
| 1 MB / ~20k lines | < 300 ms | < 200 ms, no scroll jump |
| 10 MB | Progressive: first screen < 500 ms, rest streamed; highlighting off by default above a threshold | Debounced harder (300 ms+), may fall back to "file changed, click to reload" |

**Recommendations.**
- **Parse off the UI thread** with a cancellation token: if a new change arrives, drop the in-flight render. Comrak had CVE-2023-28626 (quadratic parsing inherited from cmark/cmark-gfm, fixed in 0.17.0, nested lists/emphasis) [primary]; markdown-rs and micromark both warn that huge inputs or many unclosed constructs can exhaust time/memory and recommend a killable worker and input caps [primary]. Fuzz our own pipeline with pathological inputs (deep nesting, 100k unclosed `[`, 10k `*`).
- **Full re-parse + DOM morph for v1**, not incremental parsing. Incremental markdown parsing is hard (a single fence or list change can reflow the whole document) and parse is not the bottleneck. Morph with **idiomorph** (~3.7 kB min/gz, id-set matching, keeps focus; ~10% slower than morphdom on large morphs, equal or faster on small) [primary]. Stable heading IDs give it anchors to match on. This preserves scroll position, open `<details>`, already-rendered Mermaid/KaTeX nodes (skip re-render when the source text is unchanged by keying on a hash), and image elements (no reload flash).
- **Block-level caching (v1.1):** hash each top-level block's source; reuse highlighted HTML for unchanged code blocks. This removes most of the highlighting cost on re-render.
- **Lazy work:** highlight and typeset math for blocks near the viewport first (IntersectionObserver), rest in idle callbacks. Apply `content-visibility: auto` on top-level blocks to skip layout of off-screen content (big win on 10k-line files).
- **Large-file mode:** above ~2 MB, disable highlighting and Mermaid by default, render in chunks (first N blocks immediately, rest appended), show a banner. Above a hard cap (e.g. 50 MB), offer plain-text view.
- **Startup:** keep JS on the critical path tiny (no highlighter, no KaTeX, no Mermaid in the initial bundle). Load syntect from a precompiled binary dump; consider building it once at app start on a background thread.

---

## 6. File watching (Windows + macOS)

**The atomic-save problem.** Many editors (VS Code with some settings, Vim, JetBrains "safe write", Sublime, many Obsidian/sync tools) save by writing a temp file and renaming it over the original. A watcher attached to the original file handle/inode then sees a delete (or nothing) and stops reporting. notify's docs list this exactly: editors "truncate files on save" or "create a new file and replace the old one", so precise Write/Delete/Create events are unreliable [primary].

**Recommended approach (both platforms):**
1. **Watch the parent directory non-recursively**, not the file. Filter events by the target path (case-insensitive compare on Windows and default macOS APFS). notify's docs note that to see deletion of `/a/b` you must watch `/a` [primary].
2. **Treat any event touching the path (create, modify, rename-to, remove) as "maybe changed"**, debounce ~50 to 100 ms, then `stat` + re-read. If the file is missing, wait briefly (rename gap) and retry 2 to 3 times before showing "file deleted".
3. **Compare content hash (or size + mtime then hash)** before re-rendering to drop no-op events (Windows emits several modify events per save; antivirus and indexers touch files).
4. **Handle partial writes:** if the read yields an empty or truncated file right after a truncate-then-write save, the next debounced event fixes it; never show an empty-document flash, keep the last good render until a non-empty read or a stable second read.
5. **Fallback poller:** network drives (SMB/NFS), WSL paths from Windows, and some cloud-sync folders may emit no events; notify recommends `PollWatcher` there [primary]. Offer automatic fallback (poll every 1 s) when the path is on a network volume.

**Libraries.**
- **Rust: notify 8.2.x + notify-debouncer-full 0.7.x.** The full debouncer stitches rename from/to pairs, uses file-system IDs to match renames on macOS FSEvents and Windows, suppresses duplicate creates and modify-after-create [primary]. Backends: ReadDirectoryChangesW (Windows), FSEvents (macOS, default) or kqueue (feature flag).
- **Node: chokidar v6** (Oct 2026 rewrite, Node 22.22+; native recursive `fs.watch` backend on macOS and Windows; no fsevents dependency since v4; globs removed in v4) [primary]. Its `atomic` option (default on for native backends, 100 ms) collapses unlink+add into one `change` and ignores common temp/swap files; `awaitWriteFinish` holds events until size is stable (adds latency, use only for slow writers) [primary].

**Platform quirks to plan for.**
- **macOS FSEvents:** coalesces events and reports at directory granularity with latency; event flags can be combined (created+modified+renamed in one event). Security model can hide events for files the user does not own [primary: notify docs]. Sandboxed (App Store) builds need security-scoped bookmarks to keep watching after relaunch.
- **Windows ReadDirectoryChangesW:** fixed-size buffer; heavy churn in a directory can overflow and drop events (generally known behavior of the API; not confirmed from a primary source in this session), so always re-read on any event rather than trusting event types. Watching a directory holds a handle that can block deleting/renaming that directory in Explorer; watch only the file's own directory, release on file close.
- **Both:** case-insensitive filesystems, Unicode normalization differences on macOS (NFD vs NFC paths) when comparing paths; normalize before matching.

---

## 7. Rendering parity test strategy

1. **Spec conformance (CI, every commit):** run the CommonMark spec tests (`spec.txt` for 0.31.2) and GFM extension spec tests from cmark-gfm's `test/` directory through our full pipeline (parse + sanitize), not just the parser. Comrak already passes them; the value is catching regressions our sanitizer/rewriter introduces (e.g. stripping a `class` the test expects). Normalize whitespace and attribute order before comparing.
2. **GitHub parity corpus (nightly / on demand):** collect 200 to 500 real READMEs and docs (top starred repos across languages, plus files known to use alerts, footnotes, math, mermaid, `<picture>`, `<details>`, HTML tables, emoji, CJK, RTL). Render each with GitHub's REST API `POST /markdown` with `mode: "gfm"` [primary; `/markdown/raw` caps input at 400 KB and is not GFM]. Store GitHub's output as golden snapshots (they change rarely; refresh monthly, rate limits apply).
3. **Normalize before diffing:** GitHub adds wrapper markup our viewer does not need (anchor `<a class="anchor">` elements, `user-content-` id prefixes, camo-proxied image URLs, `<markdown-accessiblity-table>` style wrappers, highlighter `<span class="pl-*">` tokens, `dir="auto"`). Strip/canonicalize those, then compare DOM trees (parse both with html5ever or a DOM parser, compare structure and text), not raw strings. Track the count of divergent files as a metric, triaged into "bug", "intentional", "GitHub-only feature".
4. **Visual regression (lighter weight):** screenshot a curated set of 20 to 30 fixtures in the real webview on Windows and macOS (Playwright for the web layer) and compare against approved baselines. Catches CSS issues that DOM diffs miss (alert styling, table overflow, dark mode).
5. **Security corpus:** a fixture suite of XSS payloads (OWASP XSS filter evasion cheat sheet style, `javascript:` variants with entities and whitespace, SVG script, `<base>`, `<meta refresh>`, mermaid/KaTeX injection attempts). Assert sanitized output contains no executable vectors and that CSP violation reports stay at zero in an E2E run.
6. **Fuzzing and perf regression:** `cargo fuzz` over the full pipeline with a time budget per input (catches quadratic blowups); a perf benchmark on fixed 50 KB, 1 MB, 10 MB fixtures in CI with a threshold alarm.
7. **Watcher tests:** scripted saves using each pattern (in-place write, truncate+write, temp+rename, delete+create, rapid 10 saves in 100 ms) on Windows and macOS CI runners, asserting exactly one final render with the final content.

---

## 8. Open questions for other workstreams

- Shell choice (Tauri vs Electron vs native) decides Rust-side vs JS-side pipeline. This doc assumes a Rust core; the JS fallback is markdown-it + DOMPurify.
- GitHub CSS: the de facto source is the `github-markdown-css` project (generally known, not verified this session); check its license and freshness.
- Whether to show front matter at all (UX call).

---

## 9. Sources (all accessed 2026-10-26)

- comrak repository and README: https://github.com/kivikakk/comrak
- comrak `Extension` options (0.59.0): https://docs.rs/comrak/latest/comrak/options/struct.Extension.html
- comrak benchmark run, Release v0.57.0 PR #879 (run dated 2026-10-21): https://github.com/kivikakk/comrak/pull/879
- comrak bench PR #854: https://github.com/kivikakk/comrak/pull/854
- comrak CVE-2023-28626 / GHSA-8hqf-xjwp-p67v: https://github.com/advisories/GHSA-8hqf-xjwp-p67v
- GitHub Security Lab report GHSL-2023-047: https://securitylab.github.com/advisories/GHSL-2023-047_GHSL-2023-049_comrak
- pulldown-cmark repository: https://github.com/pulldown-cmark/pulldown-cmark
- pulldown-cmark Options (0.13.5): https://docs.rs/pulldown-cmark/latest/pulldown_cmark/struct.Options.html
- markdown-rs: https://github.com/wooorm/markdown-rs
- markdown-it: https://github.com/markdown-it/markdown-it
- micromark: https://github.com/micromark/micromark
- goldmark: https://github.com/yuin/goldmark
- cmark-gfm: https://github.com/github/cmark-gfm
- cmark-gfm-js (notes on SAFE and tagfilter limits): https://github.com/mgenware/cmark-gfm-js
- GitHub markup rendering pipeline: https://github.com/github/markup
- GitHub basic writing and formatting syntax (alerts, anchors, relative links, footnotes, emoji): https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax
- GitHub REST API, Markdown rendering: https://docs.github.com/en/rest/markdown/markdown
- markdown-it-github-alerts: https://git.tdem.in/antfu/markdown-it-github-alerts
- @ende/markdown (alert detection rules, port): https://jsr.io/@ende/markdown
- VS Code Markdown docs (preview security, KaTeX): https://code.visualstudio.com/docs/languages/markdown
- ammonia repository: https://github.com/rust-ammonia/ammonia
- ammonia Builder (4.2.4) defaults: https://docs.rs/ammonia/latest/ammonia/struct.Builder.html
- DOMPurify: https://github.com/cure53/DOMPurify
- Tauri CSP: https://v2.tauri.app/security/csp/
- idiomorph: https://github.com/bigskysoftware/idiomorph
- notify (8.2.1) docs, known problems: https://docs.rs/notify/latest/notify/
- notify-debouncer-full (0.7.0): https://docs.rs/notify-debouncer-full/latest/notify_debouncer_full/
- chokidar: https://github.com/paulmillr/chokidar
- Shiki bundles: https://shiki.style/guide/bundles
- Shiki regex engines: https://shiki.style/guide/regex-engines
- oniguruma-to-es (JS engine size claim): https://npmjs.com/package/oniguruma-to-es
- syntect (load-time figures): https://docs.rs/crate/syntect/latest
- bat issue on syntect deserialization startup: https://github.com/sharkdp/bat/issues/1262
- arborium (tree-sitter highlighting, native + WASM): https://docs.rs/crate/arborium/2.18.1
- Mermaid size on bundlephobia (12.2.0): https://bundlephobia.com/package/mermaid
- Mermaid lazy-load vendored size report: https://github.com/openknowledge-sh/openknowledge/pull/15
- KaTeX vs MathJax figures (third-party, approximate): https://cuberoot.me/code/language/katex
- TanStack Markdown parser comparison (vendor, qualitative): https://tanstack.com/markdown/latest/docs/comparison
- ferromark (third-party throughput comparison): https://crates.io/crates/ferromark
