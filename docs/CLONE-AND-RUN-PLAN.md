# ezmark: clone-and-run plan for a lightweight markdown viewer

Written 2026-10-26. Version 6, final. Agreed in adversarial review round 6 by a GPT-6 Astra agent at medium effort after five rounds of findings. Section 15 records how every finding was handled. Builds on FEASIBILITY-REPORT.md and the five research files under research/. Where this plan and the feasibility report differ on v1 scope, this plan governs; the two known differences are that Mermaid and KaTeX move to v1.1, and remote resource loading is removed from v1 entirely rather than enabled by default.

## 1. Goal

A public GitHub repo that any developer can clone and run on Windows 11 or macOS with one command after installing stated prerequisites, producing a fast, read-only markdown viewer. No signed installers, no app stores, no yearly fees. The repo doubles as a portfolio piece, so the README, tests, measured results, and code quality are first-class deliverables.

## 2. Non-goals

- Downloadable signed installers, notarization, winget, Homebrew, auto-update. The architecture allows adding them later.
- Editing of any kind.
- Linux support in v1.
- Mermaid diagrams and math rendering in v1 (section 4). Fences for them render as labelled code blocks so the content stays readable.
- Loading any remote resource referenced by a document (images, badges). v1 blocks all of it and shows a placeholder with the URL. Opt-in remote loading is v1.1 with its own ADR, because it cannot be done without either an image CSP exception or Rust-side fetching, and either one needs a deliberate design.
- Quick Look extension, Explorer preview pane, tabs, bundled fonts, telemetry.
- Print, open-in-editor, reveal-in-file-manager, copy button on code blocks. These move to v1.1 unless M4 has slack.

## 3. Audience and success criteria

Audience: developers who read READMEs, specs, and AI-generated markdown locally, and hiring managers who will read the repo rather than run it.

v1 is done when all of these are true:

1. A clean Windows 11 virtual machine and a clean macOS machine, operated by someone other than the author, go from `git clone` to a running viewer following only the README.
2. The parser passes the CommonMark and GFM spec suites with spec settings. Separately, the full pipeline (parse, URL rewrite, sanitize) matches a reviewed set of expected outputs for the same inputs, with every deliberate deviation from the spec documented.
3. The security harness (section 11) passes on both platforms: the sanitized hostile corpus produces zero Content Security Policy violations and zero outbound connections, the enforcement corpus produces the expected blocked attempts, and out-of-boundary IPC calls are rejected.
4. Editing an open file in VS Code, Vim, and Obsidian reloads the view and the reader's anchor block stays in place (defined in section 9).
5. CI is green on the pinned Windows and macOS runner labels with format, lint, tests, and a release build.
6. README has a hero GIF, prerequisites and one command per platform, measured numbers (binary size, cold start, first-build time), a "known limitations" section, a "why not MDHero or Tinta" section, and a "never" list.

## 4. Stack and the reader-versus-contributor toolchain split

Stack as decided in the feasibility report: Tauri 2, Rust core (comrak, ammonia, syntect, notify), system webview.

**Decision: a reader needs only Rust. A contributor may optionally use Node for type-checking. Nothing beyond normal Cargo dependency downloads is fetched from the network at build time.**

- The view layer is plain HTML, CSS, and ES-module JavaScript in `ui/`, embedded into the binary at compile time (`frontendDist` set to the static folder, no `devUrl`, no `beforeDevCommand`).
- JavaScript talks to Rust through `app.withGlobalTauri = true`, which exposes the Tauri API on `window.__TAURI__` without an npm package.
- The only third-party JavaScript is idiomorph, vendored as one file in `ui/vendor/` with a `VENDOR.md` recording upstream version, commit, SHA-256, license, and the update procedure.
- Syntax highlighting happens in Rust (syntect), so no JavaScript highlighter exists.
- JavaScript is written with JSDoc types and stays small (target under 600 lines). A `package.json` with a lockfile pins TypeScript for `npx tsc --checkJs` (no dependencies beyond TypeScript). The README's contributor section documents the local command. CI runs it. Readers never need Node.
- Mermaid and KaTeX are deferred to v1.1 as a scope decision. The v1.1 approach will be decided then with its own ADR; build-time network fetching is ruled out.

Why: every extra prerequisite is a place where the one-command promise fails on someone else's machine. Node is kept out of the reader path, but contributor tooling is pinned and runnable locally so type errors are caught before a push, not after.

## 5. Prerequisites per platform (what the README says)

Windows 11:
- Rust stable via rustup, MSVC target (Tauri's supported Windows target; no MinGW).
- Visual Studio Build Tools (standalone, not the IDE) with the "Desktop development with C++" workload. The README lists the exact components the M0 rehearsal proved sufficient, the measured disk cost, and says a restart may be required before `cargo` finds the linker.
- WebView2 runtime. Preinstalled on Windows 11. The app shows a plain dialog with a download link if missing.
- Condition: Windows 11 Smart App Control, when enabled, blocks unsigned executables regardless of origin, including locally built ones. The README states this as a known limitation and does not instruct users to change security settings. Developer machines with Build Tools installed usually have it off; the M0 rehearsal records what a clean machine does.

macOS 13 or newer:
- Rust stable via rustup.
- Xcode Command Line Tools (`xcode-select --install`).
- CI runs on a pinned macOS runner label on Apple Silicon. The macOS 13 floor and Intel support are claimed only if an M5 fresh-machine test covers them; otherwise the README says "tested on" with the actual versions.

Both: git. Nothing else for readers.

## 6. The run story

Primary command, from the repo root:

```
cargo run --release -- path/to/file.md
```

Requirements for this to be true, all proven in M0:

- The workspace sets `default-members = ["crates/app"]` so `cargo run` at the root picks the app binary.
- The app crate declares `[features] default = ["custom-protocol"]` with `custom-protocol = ["tauri/custom-protocol"]`, so a release build serves embedded assets. `--release` alone does not select this.
- Tauri config paths (`frontendDist`, icons) are relative to the app crate and resolve when invoked from the root.
- Argument handling: quoted paths, spaces, Unicode, relative paths resolved against the invoking process's working directory, a missing file (clear error, not a blank window), and no argument (an empty-state window with drag-drop).
- macOS: files opened via Finder, `open`, or Dock drop arrive as `RunEvent::Opened`. Events that arrive before the webview is ready are buffered in managed state and replayed once the frontend signals readiness.
- Windows: second launches are forwarded by the single-instance plugin with argv and cwd; the first instance opens or focuses the window.
- A raw `cargo run` does not register file associations on either OS. That is the optional step below.

First compile takes several minutes; the README says so immediately above the command with the number measured in M0.

Optional second step, to get a real app with double-click file association:

```
cargo install tauri-cli --version <pinned> --locked
cargo tauri build
```

- Targets: macOS `.app` bundle, Windows NSIS `-setup.exe`. Both configured in `tauri.conf.json` with `bundle.fileAssociations` for `.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn`, role Viewer.
- macOS: `bundle.macOS.signingIdentity` is set explicitly to `"-"` (ad-hoc) so Apple Silicon runs the binary. Locally built files carry no quarantine attribute, so Gatekeeper does not block them. The README says to drag the `.app` into Applications; the association becomes available in Finder's Open With, and the user picks it as default through Get Info.
- Windows: run the NSIS setup. It is a per-user install that registers the app as an Open With candidate. The README then sends the user to Settings > Default apps to choose it. Smart App Control condition from section 5 applies.
- M5 tests cold and warm file opens after installation on both platforms.

No `justfile` or `Makefile`. Two commands do not justify another tool.

## 7. Security contract

The README and ARCHITECTURE.md state it exactly. Every line below is testable.

**Network.** The v1 binary makes no network connections of any kind. Not for updates, not for telemetry, and not for resources referenced by a document. The CSP `img-src` allows only the app's asset scheme and `data:`, so a remote image cannot load even if the sanitizer passed its URL. The placeholder shows the URL with "copy" and "open in browser" actions, where "open in browser" hands the URL to the OS. Remote loading is deferred to v1.1 (section 2).

**Sanitizer.** ammonia with an extended allowlist. Everything that leaves `mdcore` is sanitized. URL-bearing attributes (`src`, `srcset`, `poster`, `href`, `data`, `cite`) are covered on every allowed element. Only `https` and `mailto` survive for links; relative paths survive only after the rewrite step maps them into the asset scheme.

**Content Security Policy.** `default-src 'none'`; scripts only from the bundled app; styles from the bundled app plus inline (syntect emits classes, not inline styles, so this can tighten later); images from the asset scheme and `data:`; no frames, objects, forms, or `base-uri`; `connect-src` limited to Tauri's IPC origin.

**Document root and filesystem boundary.** The active root is backend state owned by Rust. Only four sources can set it, and all four originate in Rust or the OS, never in a document: a command-line argument, a drag-drop event delivered by Tauri, a Finder or Explorer open event, or the native open dialog that Rust shows. Root is the enclosing git repository root if one is found by walking up, otherwise the file's directory. No IPC command can set the root, and no IPC command accepts a "confirmed" or "trusted" flag; the frontend is untrusted.

Link following is one command, `resolve_link(href, generation)`. Rust performs these steps in order: reject the generation if stale; percent-decode exactly once, and never decode again anywhere downstream (so `%255C` becomes the literal three characters `%5C`, which are then treated as ordinary filename characters and go through the same inspection and root check as anything else; a `%` that does not form a valid escape is rejected); then, on the decoded string and before touching the filesystem, reject anything that is not a plain relative or absolute local path (UNC `\\server\share` and `//server/share`, `\\?\` and `\\.\` device paths, `file://` and any other scheme, paths containing NUL), because canonicalizing a network path can itself cause network access; join to the current document's directory; canonicalize with symlinks and junctions resolved; compare against the root. The raw undecoded input is also rejected if it contains a scheme or NUL, as a cheap first gate, but the authoritative inspection is the post-decode one. Tests feed encoded UNC and device paths through both link resolution and asset access and assert, with a filesystem call interceptor in the test build, that no filesystem or network call occurs before rejection. Inside the root: return the target and the frontend navigates. Outside the root: Rust itself shows a native confirmation dialog naming the exact file and the exact new root that would be adopted ("Open README.md? This will also allow the viewer to read files under C:\projects\other"). Rust binds the approval to that canonicalized target and the current generation, re-validates after the dialog returns, and only then commits the new root. The frontend never sees the dialog result; it only receives a new document or a typed refusal.

The asset handler performs the same decode, inspection, canonicalize, and root comparison independently through the one shared validator function, serves only image MIME types with `nosniff`, and applies the same per-request size limit as document reads. Image-header reads in `mdcore` (section 9) go through this same validator. Asset access is keyed to the current document generation; when the root changes, requests carrying an old generation are refused.

**Navigation.** All link clicks are intercepted in JavaScript and forwarded to Rust. `.md` targets go through the root check above, anchors scroll, `https` and `mailto` go to the OS, everything else is dropped. The webview never navigates away from the app page; a navigation handler in Rust denies any top-level navigation.

**IPC.** Commands are few and narrow: open a document by path, read the current document, resolve a link target, list sibling markdown files for wiki-link resolution, get and set window state. Every command is enrolled in the application's ACL and granted only to the main window's capability. Every command re-validates paths in Rust against the active root; the frontend is treated as untrusted. No shell, filesystem, or HTTP plugins are enabled.

**Test categories.** Two corpora with different pass conditions, so a working defense never fails a gate:
- Sanitized hostile corpus: payloads go through the full pipeline. Expected: no executable content in the output (string and DOM assertions), zero CSP violation reports, zero outbound connections.
- Enforcement corpus: a test-only path injects unsanitized payloads directly into the webview, bypassing `mdcore`. Expected: CSP violation reports for every payload, zero outbound connections, zero script execution side effects.
- IPC boundary: calls with traversal paths, symlinks pointing outside the root, encoded paths, and stale generations. Expected: rejection with a typed error.

## 8. Repository layout

```
.
├── README.md
├── LICENSE                 MIT OR Apache-2.0
├── ARCHITECTURE.md
├── CHANGELOG.md
├── SECURITY.md
├── CONTRIBUTING.md
├── docs/adr/               0001 tauri, 0002 comrak, 0003 security contract, 0004 reader-only-needs-rust
├── docs/media/             hero.gif, screenshots
├── docs/measurements.md    binary size, cold start, first-build time, per release
├── Cargo.toml              workspace, default-members = crates/app
├── crates/
│   ├── mdcore/             pure library: parse, rewrite URLs, sanitize, highlight, outline. No Tauri dependency.
│   └── app/                Tauri shell: windows, open events, watcher, IPC commands. Thin.
├── ui/                     index.html, app.css, app.js, vendor/idiomorph.js, vendor/VENDOR.md
├── package.json            TypeScript only, for contributors' type-check
├── tests/
│   ├── spec/               CommonMark and GFM fixtures (parser conformance)
│   ├── pipeline/           reviewed expected outputs through the full pipeline
│   ├── security/           hostile corpus
│   ├── corpus/             20 to 30 real-world files for pipeline snapshots and visual tests
│   └── visual/             Playwright scripts (CI only)
└── .github/workflows/      ci.yml, weekly-clean-build.yml
```

The split between `mdcore` and `app` is the main architectural statement: the engine is a plain Rust library with no GUI dependency, testable in isolation, and reusable by a future Quick Look extension.

## 9. Live reload: definition and design

Reading position is preserved by content anchoring, not by DOM identity alone.

**Two separate identity mechanisms.** DOM reuse and reading-position recovery are solved separately, because they have different failure costs. A wrong DOM match costs a re-rendered element; a wrong anchor match silently moves the reader.

- For DOM reuse, every top-level block gets an `id` of the form `b-<hash>-<ordinal>` (hash of normalized source text, ordinal among blocks with the same hash in document order), because idiomorph matches elements by `id` sets, not by arbitrary data attributes. Headings keep their GitHub-style slug as the `id` instead. Ordinal drift after a duplicate insertion only causes idiomorph to re-render a block; it never affects the anchor.
- For anchoring, each block also carries `data-hash`. Anchor lookup never reads the `id`.

**Anchor capture.** Before a morph, record the old document's full block hash sequence, the index of the topmost block intersecting the viewport, and the pixel offset from its top. Headings are identified by the hash of their text, never by slug, because slug suffixes are reassigned when duplicates are inserted.

**The uniqueness rule.** A hash `h` is "unique-in-both" if exactly one block in the old document and exactly one block in the new document carry it. Only unique-in-both hashes are ever matched. A hash that appears more than once in either document is never matched, in any step, because two snapshots cannot establish which copy is which: equal counts do not prove correspondence (insert one copy above and delete one below, and the count is unchanged while every occurrence number has shifted). This deliberately gives up on cases a smarter alignment might recover, in exchange for a guarantee that is actually true: the algorithm can never select a different copy of a block, because it only ever selects blocks that have exactly one copy.

**Anchor restore, in order.** Headings are ordinary blocks here; they get no separate step.
1. If the anchor block's hash is unique-in-both, match it and restore the recorded offset.
2. Otherwise walk backwards through the old block sequence from the anchor. The first block whose hash is unique-in-both is matched, at offset zero.
3. If nothing preceding the anchor is unique-in-both, preserve scroll ratio.

Opening a different document resets anchor state entirely.

Worked examples.
- Round 4 counterexample. Old `A, X, X, B`, anchor on the first `X`. Insert another `X` above: new `A, X, X, X, B`. `X` is not unique in either document, step 1 fails. Step 2 walks back to `A`, unique in both, lands at its top. Never the inserted copy.
- Round 5 counterexample. Old `A, X, B, X, C`, anchor on the first `X`. Insert an `X` above the anchor and delete the later one: new `A, X, X, B, C`. `X` appears twice in both documents, so it is never matched. Step 2 lands on `A`. Never the inserted copy.
- Ordinary case. A long document where the anchor is a paragraph that appears once, edited elsewhere: step 1 matches exactly, offset restored.
- Cost of the rule. If the reader's anchor is one of several identical blocks (common for horizontal rules or repeated boilerplate), the restore lands on the nearest unique block above, which is usually the previous paragraph or heading but is not guaranteed to be close. The plan accepts unbounded drift in that case in exchange for never landing on a wrong copy.
- What the guarantee means. It is a statement about unambiguous content correspondence between two snapshots, not about historical identity. If the only copy of a block is deleted and identical text is inserted elsewhere, the rule follows the text, because two snapshots cannot distinguish that from a move.

**Delayed layout.** All local images get `width` and `height` attributes from `mdcore`, which reads image headers (cheap, no decode) so the browser reserves space before load. Images whose dimensions cannot be read get a fixed-aspect placeholder box. Remote images are placeholders in v1, so they never resize. Code blocks are highlighted in Rust before the HTML reaches the webview, so there is no post-render reflow from highlighting. After restore, a `ResizeObserver` on the content element re-applies the anchor correction for up to 500 ms to absorb font loading and any remaining layout, and stops immediately on any user scroll input (wheel, key, touch, scrollbar drag).

**Generations.** Every render request carries a generation number. Results older than the latest request are discarded, so a slow render of an old state never overwrites a newer one, including when the user switches documents mid-render.

**Watcher.** The parent directory is watched, events for the target path are debounced (about 75 ms), the file is re-read, and a content hash is compared before any render. Intermediate states are allowed; the test oracle is eventual final content within a bounded latency, plus no render for unchanged content.

**Bounded execution.** One dedicated render thread (plain `std::thread`, no async runtime) with a single-slot mailbox: a new request replaces any pending request, so pending generations coalesce and the queue depth is at most one. Cancellation is cooperative and checked between stages (parse, URL rewrite, sanitize, per-block highlight, serialize) and between highlight blocks. The render thread is the only place `mdcore` runs, so the UI thread is never blocked.

Parsing is not interruptible, and the plan does not pretend otherwise. There is no enforced parse deadline. Parse time is bounded in practice by input limits and by fuzzing, and a stuck parse is recovered from by abandonment:
- Files above 50 MB are refused with a message and never parsed.
- Parse time per megabyte is measured in CI and recorded in `docs/measurements.md`; the 5 second figure from version 3 is a measurement target for a 50 MB file, not a guarantee.
- If the render thread has not reported back within 10 seconds, the UI shows a "still rendering" notice with a "stop" action. Stop orphans the current render thread (its result will be discarded by the generation check when it eventually finishes) and spawns a fresh render thread. At most two orphaned threads may exist; beyond that, new renders are refused with a message until one finishes. Abandonment does not stop the computation or reclaim its memory: an orphan keeps consuming CPU and whatever working memory the parser needs, which the input cap limits only indirectly. If both orphans never finish, the recovery is to restart the app, and the refusal message says so. This is a documented limitation, not a hidden one, and it is recorded in the README's known limitations.

Rendered output is bounded in bytes and nodes, not only in block count:
- Between 2 MB and 50 MB of input, large-file mode applies: highlighting is skipped; the first chunk is the smaller of 500 blocks or 1 MB of serialized HTML; further chunks are sent on scroll demand with the same cap. First paint happens after the whole-file parse completes, not before, and the README says so.
- A single block whose serialized HTML exceeds 1 MB (a giant code block or table) is truncated at 1 MB with a "show full block" control that renders the remainder as escaped plain text in a `<pre>`, itself capped at 5 MB.
- The plain-text fallback for a file that fails to parse shows the first 2 MB escaped with a "truncated" banner.

**Acceptance cases for M3, with expected outcomes.**
- Edit above the viewport: anchor block matched uniquely, offset restored exactly.
- Edit inside the anchor block: its hash no longer exists in the new document, step 1 fails, step 2 lands on the preceding unique-in-both block at offset zero.
- Delete the anchor block: same as above.
- Insert a paragraph identical to the anchor directly above it (`A X X B` to `A X X X B`): `X` not unique, step 2 lands on `A`; never the inserted copy.
- Balanced insert and delete (`A X B X C` to `A X X B C`, anchor on the first `X`): `X` not unique in either document, step 2 lands on `A`; never the inserted copy. Run once with `X` as paragraphs and once with `X` as headings.
- Insert a heading with identical text above the current section heading: that heading's hash is not unique in the new document; the preceding unique block wins; never the new heading. Also run the `A X X B` insertion case with headings.
- Anchor on one of several identical paragraphs, no edit nearby: not unique, step 2 lands on the nearest unique block above; a few lines of drift, deterministic across runs.
- Anchor on a unique paragraph, edits elsewhere: exact match, offset restored.
- Large local image above the anchor: no movement, because dimensions were emitted from the header.
- Switch documents during a slow render: old result discarded, anchor state reset, new document at top or at its own remembered position.
- Ten saves in 100 ms: one final render with the final content, within the bounded latency.
- Stop during a stuck render: fresh thread accepts the next request; orphan result discarded.

## 10. Milestones

Solo, part time, about 10 to 15 hours a week. Start 2026-11-03. Dates are targets; M0 is a spike whose findings may revise them, and that revision is written into the CHANGELOG rather than silently absorbed.

| Milestone | Target | Done when |
|---|---|---|
| M0 Spike and skeleton | 2026-11-16 | Repo, license, ADRs, CI with format, clippy, test, release build on both OS runners. `cargo run --release -- file.md` works from the root on both platforms with the feature and workspace settings above. A clean Windows 11 VM rehearsal with measured disk use and first-build time, and a clean macOS run. One real screenshot captured per OS to prove visual testing is possible. Asset protocol serves a relative image on both OSes. macOS `RunEvent::Opened` buffering proven with a cold Finder open of the bundled app. Security harness (section 11) proven on both OSes with one sanitized hostile fixture, one enforcement fixture producing an observed enforced (not report-only) CSP report naming the blocked URL and directive, one observed blocked outbound attempt with the loopback listener positively verified to accept connections, and one denied IPC call driven through the real webview IPC path so the ACL is exercised. The production release artifact is inspected to confirm the injection command, its ACL entry, and test assets are absent. |
| M1 Engine | 2026-12-07 | `mdcore` passes spec suites with spec settings. Pipeline expectations reviewed and committed. Hostile corpus passes. Highlighting, front matter, URL rewriting with canonicalized boundary checks. Adversarial performance fixtures (deep nesting, unclosed constructs, 10k-line code block) complete within budget. |
| M2 Viewer | 2026-12-28 | Opens from argv, drag-drop, Finder and Explorer. Typography spec applied (68ch measure, 17px, 1.6 line height, GitHub heading scale). Light, dark, system themes. Outline sidebar, find, zoom, keyboard scrolling, back and forward between .md files. Accessible focus order. |
| M3 Live reload | 2027-01-18 | Section 9 implemented, including the bounded render thread. All section 9 acceptance cases pass with saves from VS Code, Vim, and Obsidian. Large-file mode and the 50 MB refusal tested. |
| M4 Polish | 2027-02-01 | Window state remembered, single instance, `forced-colors` and `prefers-contrast`, visual regression suite on both runners. Items listed under section 2 as v1.1 are pulled in only if M3 landed on time. |
| External test | 2027-02-08 | Two people other than the author follow the README on clean machines, one per platform. Their failures become README fixes or issues. |
| M5 Release | 2027-02-22 | README complete with measurements and limitations. Tag v1.0.0. Optional: CI attaches unsigned builds, labelled "downloaded builds will be blocked or warned by your OS; build from source is the supported path". |

About 16 weeks. The M5 period contains real work (README, measurements, tagging), so the true contingency is the external-test week plus about 20 hours reserved and not assigned to any milestone. If M0 overruns by more than a week, the next cuts in order are: back and forward navigation between .md files, zoom persistence, the outline sidebar's current-section highlight. The window spans year-end holidays; targets assume reduced hours in the last two weeks of 2026.

## 11. Testing strategy

- Unit tests in `mdcore` for every pipeline stage.
- Parser conformance: CommonMark 0.31.2 and GFM spec fixtures run against comrak with spec settings (raw HTML preserved, no sanitizer).
- Pipeline expectations: the same inputs through the full pipeline against reviewed expected HTML, with deviations documented. Snapshot tests for the real-world corpus.
- Security harness: a `test-harness` Cargo feature builds the real app with three additions that do not exist in release builds: a `securitypolicyviolation` listener in `ui/` that forwards reports to Rust; a test-only command that injects raw HTML into the content element (used only by the enforcement corpus); and a JSON results channel on stdout. A `cargo test` integration test spawns that binary per fixture, and in the same process starts a loopback TCP listener whose address the fixtures reference as `http://127.0.0.1:<port>/pixel.png`. The assertions are that the listener receives zero connections and that each remote `img` produces a CSP report. Since the CSP blocks every non-asset scheme alike, a loopback `http` URL is a valid stand-in for a remote `https` one. IPC boundary tests call the real commands through the harness with traversal, symlink, encoded, and stale-generation inputs and assert typed errors. This uses no WebDriver and runs identically on both OS runners. The production CSP, sanitizer, and command validation paths are the ones exercised; only the reporting plumbing is test-only. What the harness proves is stated narrowly: that each fixture's attempted resource load was blocked by an enforced CSP and reached no listener. It does not prove the absence of all outbound traffic from the process. The broader "no network" claim in section 7 rests on the CSP, on the app code containing no networking calls, and on no HTTP or shell plugins being enabled; reading `Cargo.toml` supports that claim but cannot rule out dependency or system webview behaviour, so the README states the claim at that level of evidence. The zero-connection assertion excludes the listener's own positive-control connection. A Cargo feature is not automatically excluded from release profiles, so the `build` job uses an explicit feature set and M0 verifies the production artifact lacks the injection command, its ACL entry, and test assets. Proven in M0 with one fixture of each kind per OS, with the loopback listener first verified to accept a connection so a silent listener failure cannot pass as a block.
- Watcher: scripted saves in each pattern (in-place, truncate-then-write, temp-plus-rename, rapid repeats) on both OS runners, asserting eventual final content within a bounded time and no render for unchanged content. Debounce logic is unit tested with a deterministic fake clock, not wall time.
- Visual: `mdcore` output plus `ui/` CSS rendered in Playwright, WebKit engine on the macOS runner and Chromium on the Windows runner, against a small deterministic corpus with local resources only and an explicit readiness signal. Runners use explicit version labels (for example `windows-2025`, `macos-15`), not `*-latest`, and the test output records the runner's image version and browser engine versions next to each comparison. When GitHub retires a label or bumps an image, baselines are regenerated in a dedicated PR with a visual review, never silently. This is CI-only tooling and is a proxy for WKWebView and WebView2, not identical to them, and it does not test the oldest supported WebKit; real-app screenshots on both OSes are run manually before each release using the M0-proven method, and the macOS 13 claim is only made if the external tester covers it.
- Performance: stage-specific timings (parse, sanitize, highlight, morph) on fixed 50 KB, 1 MB, and 10 MB fixtures, reported as numbers in CI output and recorded in `docs/measurements.md` per release. No pass or fail thresholds on shared runners; the gate is a manual review of the trend.
- Fuzzing: `cargo fuzz` over the pipeline with a time budget, added after M3 once resource limits exist, run on a weekly schedule, with crashes filed as issues.

## 12. CI

- `lint` on `ubuntu-latest`: rustfmt check, clippy with warnings denied, `npx tsc --checkJs` on `ui/`, README link check. This is the one deliberate use of a floating label. A runner image change can alter lint diagnostics or tool versions, but the toolchain versions that matter (Rust, TypeScript) are pinned in the repo, nothing visual or platform-specific runs here, and any breakage is confined to this job.
- `test` matrix on pinned Windows and macOS runner labels: `cargo test --workspace`, watcher tests, security harness, visual tests.
- `build` matrix on the same runners: `cargo build --release` with the production feature set (no `test-harness`), uploaded as a workflow artifact.
- `weekly-clean-build.yml`: scheduled, both OS runners, no cache, from a clean checkout, to catch rot in the one-command promise.

`Swatinem/rust-cache` keeps routine CI under about 10 minutes. Third-party actions pinned to commit SHAs. Dependabot for cargo, npm, and github-actions, grouped weekly.

## 13. Repo hygiene checklist

In priority order, from the distribution research:

1. README: problem statement, hero GIF, prerequisites and one command per platform with measured first-build time, feature list, measured numbers, known limitations (Smart App Control, no Linux, no Mermaid in v1), "why not MDHero or Tinta", "never" list, troubleshooting.
2. Small focused commits with Conventional Commits messages, squash-merged PRs even when solo.
3. Tests and a CI badge.
4. rustfmt and clippy enforced.
5. LICENSE: MIT OR Apache-2.0.
6. CHANGELOG maintained by hand in v1 (release-please is v1.1), semver tags.
7. ARCHITECTURE.md: codemap, the `mdcore` and `app` boundary, the security contract as invariants.
8. ADRs 0001 to 0004.
9. Hero GIF made with vhs or Kap, under 5 MB.
10. SECURITY.md with private vulnerability reporting enabled.
11. CONTRIBUTING.md with the contributor-only Node step, issue forms, PR template, CODE_OF_CONDUCT.md.
12. Repo description, topics, social preview image, pinned on profile.

What a senior reviewer needs beyond the checklist: reproducible examples (the corpus), measured results (`docs/measurements.md`), stated limitations, and visible evidence of failures found and fixed (the external test issues, the hostile corpus history).

## 14. Risks specific to clone-and-run

| Risk | Mitigation |
|---|---|
| Windows users pick the wrong Build Tools components or skip the restart | README lists the exact components the M0 rehearsal proved sufficient, with a screenshot, disk cost, and a three-line diagnostic (does `cl.exe` resolve, does `cargo build` of a hello-world link). |
| First compile takes minutes and users assume it hung | Measured number in the README directly above the command. |
| Smart App Control blocks the locally built binary | Stated as a known limitation. No instruction to disable security. |
| Old Windows 10 machines without WebView2 | Startup detection with a plain dialog and download link. Windows 11 is the supported target. |
| `cargo tauri build` fails on a user's machine | Optional step. `cargo run` is the promise. tauri-cli version pinned. |
| The one-command promise rots | Weekly clean build on both runners. External test before release. |
| Nobody runs it, so no bug reports | Accepted. External test sessions substitute for early adopters. Unsigned CI builds with honest labels for the curious. |
| Scope creep toward an editor or toward Mermaid-in-v1 | README "never" and "later" lists, ADR 0004. Issues closed with a link. |
| Visual baselines churn with runner image updates | Explicit runner labels, recorded image versions, baselines regenerated only in a reviewed PR, per-OS baselines, a small corpus, local resources only. |
| Timeline slips | M0 spike gates the estimates. Section 2 lists what is cut first. |

## 15. Reconciliation log, review round 1

Reviewer: GPT-6 Astra, medium effort, read-only review backed by Tauri source and documentation. Eleven findings.

| # | Finding | Decision | Where |
|---|---|---|---|
| 1 | Spec suites cannot pass through a sanitizer that strips raw HTML | Accepted. Parser conformance and pipeline expectations are now separate test sets. | 3, 11 |
| 2 | "No network calls" contradicts https-images-by-default; boundary checks were lexical; security tests were string-only | Accepted. Security contract rewritten; remote images blocked by default; canonicalized boundaries; `srcset` and friends covered; real-webview tests added. | 7, 11 |
| 3 | `cargo run` needs the `custom-protocol` feature, workspace defaults, path handling, open-event buffering | Accepted. All listed as M0 proof obligations. | 6, 10 |
| 4 | Local builds are not exempt from Smart App Control; ad-hoc signing must be configured, not assumed | Accepted. `signingIdentity: "-"` explicit; Smart App Control stated as a limitation. | 5, 6 |
| 5 | Bundling is not installation; targets, associations, and CLI version unspecified | Accepted. Targets, `fileAssociations`, install steps, pinned CLI, M5 open tests. | 6 |
| 6 | CI runners do not prove the beginner Build Tools path | Accepted. Clean Windows VM rehearsal moved into M0 with measurements; MSVC kept. | 5, 10, 14 |
| 7 | No-Node shifts cost to maintainers; build-time fetch is a bad idea; CI-only type-check is late | Accepted with a different remedy than suggested: plain JS with JSDoc kept (no committed build artifacts), TypeScript pinned with a lockfile and runnable locally, build-time fetch removed, idiomorph provenance recorded, `withGlobalTauri` for IPC. Mermaid and KaTeX fences render as labelled code. | 4, 8 |
| 8 | Screenshot CI had no implementation; tauri-driver lacks macOS | Accepted. Playwright on `mdcore` output with WebKit and Chromium engines as the CI proxy; one real screenshot per OS proven in M0; manual real-app screenshots per release. | 10, 11 |
| 9 | DOM morphing is not reading-position preservation; "one final render" is a bad oracle | Accepted. Section 9 defines anchor-based preservation, generation-tagged renders, eventual-content oracle, fake-clock debounce tests. | 9, 11 |
| 10 | Performance thresholds on shared runners are noise; fuzzing needs infrastructure | Accepted. Off-UI-thread processing and large-file mode restored; stage timings recorded, not gated; fuzzing after M3 on a schedule. | 9, 11 |
| 11 | 14 weeks has no buffer; portfolio value is not automatic | Accepted. 16 weeks with an external test and a two-week M5; print, editor integration, and release automation moved to v1.1; M0 spike gates estimates; README gains measurements and limitations. | 2, 10, 13 |

### Review round 2

Verdict was NOT AGREED with four must-fix items and two nice-to-haves. Findings 1, 3, 4, 5, 6, 7, 11 were classified resolved; 2, 8, 9, 10 partially resolved.

| # | Finding | Decision | Where |
|---|---|---|---|
| R2-1 | Section 7 was not implementable: https images cannot load under the stated CSP, consent scope was ambiguous, Tauri capabilities do not scope document subtrees, the active root could be moved by a document link, and defense success could fail the test gate | Accepted. Remote loading removed from v1 entirely, so the network claim is absolute and the CSP is consistent. Root is backend state set only by explicit user opens; links outside the root require confirmation, which counts as an explicit open. Commands enrolled in the ACL with Rust-side validation. Sanitized and enforcement corpora have separate pass conditions. | 2, 7 |
| R2-2 | No security harness was proven in M0; "pinned runner images" conflicted with `*-latest` | Accepted. A `test-harness` feature build with CSP report forwarding, a test-only injection command, a loopback listener for outbound detection, and IPC boundary calls, driven by `cargo test`, no WebDriver. M0 proves one fixture of each kind per OS. Runner labels pinned explicitly, image versions recorded, baseline regeneration by reviewed PR. | 10, 11, 12 |
| R2-3 | Anchor keys collided on repeated blocks; delayed image layout was unhandled; no reset on document change | Accepted. Keys are hash plus ordinal, headings carry de-duplicated slugs, restore has a four-step fallback, `mdcore` emits image dimensions from headers, a bounded `ResizeObserver` correction stops on user scroll, document change resets anchor state. Acceptance cases listed. | 9, 10 |
| R2-4 | Cancellation was nominal; plain-text fallback above a hard cap moved the problem to the DOM | Accepted. Single render thread with a one-slot coalescing mailbox, cooperative cancellation between stages, 50 MB refusal before parse, large-file mode between 2 MB and 50 MB with on-demand blocks, plain-text fallback capped at 2 MB. No async runtime. | 9 |
| R2-5 | Nice-to-have: "nothing fetched at build time" should exclude Cargo downloads | Accepted. | 4 |
| R2-6 | Nice-to-have: reserve contingency hours and name the next cut | Accepted. About 20 unassigned hours; next cuts named in order. | 10 |

### Review round 3

Verdict was NOT AGREED with three must-fix items, four notes for M0, and one nice-to-have. R2-1 and R2-3 and R2-4 were classified partially resolved; the rest resolved.

| # | Finding | Decision | Where |
|---|---|---|---|
| R3-1 | Root change was confirmed in the frontend, which is untrusted; UNC canonicalization can touch the network before the boundary check | Accepted. Rust owns the native confirmation dialog, binds approval to the canonicalized target and generation, and shows the new root; no IPC command can set the root or carry a trusted flag. Network and device paths are rejected by string inspection before any filesystem call. Image-header reads use the same validator. | 7 |
| R3-2 | Hash-plus-ordinal keys shift on duplicate insertion, so "exact match" could land on the wrong block; idiomorph matches on `id`, not `data-key` | Accepted. DOM reuse (ids for idiomorph) and anchoring (hash plus neighbour-context triple) are now separate mechanisms; ambiguity is treated as no match; acceptance cases state expected outcomes. | 9 |
| R3-3 | A 5 second parse deadline was unenforceable on a non-interruptible parser; block-count pagination did not bound DOM size | Accepted. No deadline is claimed; the figure is a measurement target. Recovery is by abandoning and replacing the render thread, with at most two orphans, documented as a limitation. Output is bounded by serialized bytes and per-block caps, and first paint is stated to follow the whole-file parse. | 9 |
| R3-4 | M0 notes: state harness evidence narrowly, assert enforced CSP with URL and directive, verify the listener works, drive IPC tests through the real webview, verify the production artifact excludes test-harness pieces, make image-header reads obey the boundary | Accepted. All folded into the M0 row and section 11. | 7, 10, 11 |
| R3-5 | Nice-to-have: remaining `*-latest` and "pinned images" wording | Accepted. | 3, 5, 14 |

### Review round 4

Verdict was NOT AGREED with two must-fix items. R3-3 (bounded execution) and the M0 notes were classified resolved with implementation notes; R3-5 partially resolved.

| # | Finding | Decision | Where |
|---|---|---|---|
| R4-1 | String inspection ran before percent-decoding, so an encoded UNC path (`%5C%5Cserver...`) passed inspection and became a network path at canonicalization | Accepted. Decode once with a strict policy, inspect the decoded string, then canonicalize. Raw input gets a cheap first gate only. Shared validator for links, assets, and image-header reads. Tests assert no filesystem call before rejection. | 7 |
| R4-2 | Context-triple matching selects the inserted copy in `A X X B` to `A X X X B`; heading slug lookup accepts a reassigned slug; acceptance cases promised a step that ran in the wrong order | Accepted. Replaced with the count-stable rule: a hash is matched only if its occurrence count is unchanged, and then by occurrence number; a changed count is treated as no match. Headings use text hash, never slug. Fallback order is now preceding block, then heading, then ratio. The counterexample is a worked example and an acceptance fixture. | 9 |
| R4-3 | Implementation note: orphan memory is not bounded by the input cap; restart is the real recovery | Accepted. Stated as a limitation with restart as recovery, recorded in README known limitations. | 9 |
| R4-4 | M0 notes: `Cargo.toml` cannot prove absence of all networking; exclude the positive-control connection from the zero-connection assertion | Accepted. | 11 |
| R4-5 | Nice-to-have: `ubuntu-latest` in the lint job | Kept, documented as the one deliberate floating label. | 12 |

### Review round 5

Verdict was NOT AGREED with one must-fix item. R4-1, the orphan note, and the M0 notes were classified resolved.

| # | Finding | Decision | Where |
|---|---|---|---|
| R5-1 | The count-stable rule selects the wrong copy under a balanced insert and delete (`A X B X C` to `A X X B C`): equal counts do not prove correspondence | Accepted, taking the reviewer's suggested conservative direction. Replaced with the uniqueness rule: only hashes with exactly one occurrence in both documents are ever matched, in any step. Headings folded into the block walk, removing the redundant step. Both counterexamples are worked examples and acceptance fixtures. | 9 |
| R5-2 | Note: `%255C` is valid input that decodes to `%5C`; the protection is decoding exactly once and never again, not rejecting malformed escapes | Accepted, wording corrected. | 7 |
| R5-3 | Note: the heading fallback step was redundant | Accepted, removed. | 9 |
| R5-4 | Note: lint justification overstated | Softened. | 12 |

### Review round 6

Verdict: AGREED. The uniqueness rule survived three attacks (duplicate substitution, distant move, delete-plus-identical-reinsert); the reviewer noted the guarantee is about content correspondence between snapshots, not historical identity, which the plan now says. Three non-blocking clarifications were applied before finalizing.

| # | Note | Decision | Where |
|---|---|---|---|
| R6-1 | "A few lines of drift" is not a bound | Reworded: drift is unbounded in the duplicate case, accepted deliberately. | 9 |
| R6-2 | Run both counterexample fixtures with paragraphs and with headings | Added to acceptance cases. | 9 |
| R6-3 | `%5C` is three characters, and such a filename could exist; lint-runner justification still too absolute | Both reworded. | 7, 12 |

## 16. Open items for the author

1. App name: decided 2026-10-27, **ezmark** (pronounced "easy mark"). Binary and command `ezmark`, crate `ezmark`, repo `<github-user>/ezmark`. Collision check on 2026-10-27: crates.io free; PyPI free; Homebrew free; winget free; GitHub repos with the name are unrelated (a jQuery plugin, watermark tools, attendance apps) except one dead markdown editor with zero stars last pushed 2020-02-05; the `ezmark` GitHub user handle is taken, so the repo lives under the author's account; npm has an abandoned `ezmark` 0.0.1 package, irrelevant since nothing is published to npm. Domains not checked.
2. Remote images: decided 2026-10-27, out of v1. Revisit in v1.1 with its own ADR.
3. Platform claims: decided 2026-10-27, the README uses "tested on" with the actual OS versions and hardware the author and external tester used. No floor is promised beyond what was tested.
