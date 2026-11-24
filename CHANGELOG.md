# Changelog

All notable changes to ezmark are recorded here by hand
([Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format, semantic
versioning). Release automation is v1.1 work.

## [Unreleased]

### Security, 2026-11-24

- **url 2.5.8 to 2.5.9 and cookie 0.18.2 to 0.18.3** (lockfile only), for
  RUSTSEC-2026-0592 (stack overflow in `Url::origin()` on nested `blob:`
  URLs) and RUSTSEC-2026-0594 (private cookie nonces can repeat after
  `fork()`). url arrives through ammonia and Tauri, cookie through Tauri.
  Upgraded rather than ignored in `deny.toml`; `cargo deny` failed on both,
  which blocked every pull request.

### Added (M1: engine, first part, and live reload brought forward from M3)

Started 2026-11-14. Live reload was planned for M3; it was built now, with
the engine work it depends on (content keys for blocks).

- **Syntax highlighting, inside the CSP.** syntect runs in Rust before the
  sanitizer and emits classes, never inline styles, so `style-src 'self'`
  stays as strict as it was. Each run of text gets at most one flat span with
  one of 14 fixed classes (`hl-keyword`, `hl-string`, ...), and the sanitizer
  keeps a class on a `<span>` only if it is one of those. `ui/highlight.css`
  carries GitHub's light and dark colours as custom properties, so a theme
  change needs no re-render. The pure-Rust regex engine is used, so a reader
  still needs only Rust.
- **Front matter.** A leading `---` block is shown as a closed "Front matter"
  block with a table of its top-level keys, instead of being dropped.
- **Parser conformance.** The CommonMark 0.31.2 spec (652 examples) and the
  GFM spec (670 examples that are not marked disabled) are checked in under
  `tests/spec/` and run against the parser with spec settings on every test
  run. All pass. One GFM example has no expected HTML in the spec (the tag
  filter, which here is the sanitizer's job); it is counted as the spec's own
  runner counts it.
- **Pipeline snapshots.** The same 1,322 inputs go through the full pipeline
  and the output is saved under `tests/pipeline/`. Any change in output fails
  the test until the snapshot is regenerated (`EZMARK_BLESS=1`) and the diff
  reviewed. `tests/pipeline/README.md` lists every kind of deliberate
  difference from the spec.
- **Live reload.** The open file's folder is watched; saves are debounced
  (75 ms of quiet, at most 500 ms); the file is read again and rendered only
  if its bytes changed. The view replaces only the blocks that changed and
  keeps the reader's place by content, under the plan's uniqueness rule. A
  short "Updated" note shows after a reload, and a message stays up while the
  file is moved or deleted.
- **One render thread with a one-slot mailbox** replaces a thread per open,
  as plan section 9 specifies: waiting requests coalesce, and a reload never
  displaces an open.
- `cargo run -p mdcore --example html -- file.md` prints the engine's HTML,
  and `--example timing` the stage timings in `docs/measurements.md`.

#### Fixed after review, 2026-11-15

An independent review of this work before it was committed confirmed nine
defects. All are fixed, each with a test.

- Two opens handed to the render thread in the wrong order could leave the
  newer one unrendered and "Still rendering" on screen for good. The mailbox
  now keeps the later generation.
- A link definition directly above a paragraph was hashed into that
  paragraph's key, so two identical paragraphs had different keys that
  swapped when the definition moved, and the reader could be sent to the
  other copy. The key now covers the paragraph's own lines only.
- A file an editor still held open when its change was noticed was never
  read again. A failed read is now retried every 250 ms, up to 20 times.
- A rename that changed only the case of the file name stopped reloads on
  Windows.
- Renaming or deleting the file's folder went unnoticed, because the folder's
  watch goes with it. The watcher now also checks every 2 seconds that the
  file is where it was, and watches the folder afresh when it returns.
- A reload that finished after the file was deleted took the "moved or
  deleted" message down. Only a view of another file, or the file's return,
  does now.
- The position hold after a reload outlived its document and could scroll the
  next one. Every new view ends it.
- The pipeline snapshots depended on what happened to exist in the temp
  directory. They now run in a folder made for the run.
- The end-to-end test could not tell a suppressed unchanged save from one
  merged into the next save. It now waits for the unchanged check first.

#### Deviations from the plan, and what is not done

- **syntect's stock HTML writer was measured and replaced.** It nests a span
  per grammar scope and spells every scope part into the class: 27 bytes of
  HTML per byte of code (a 433 KB block became 11.5 MB). The flat writer
  produces about 4.4 bytes per byte, and its fixed class list is a tighter
  sanitizer rule than a class prefix would have been.
- **Highlighting is budgeted by bytes of code, not by file size.** Plan
  section 9 skips highlighting for files over 2 MB. Measured, the cost follows
  the amount of code (about 370 KB a second), so one document gets 256 KB of
  code highlighted wherever it sits, and a block with a line over 4,096 bytes
  is left plain.
- **Block keys travel as marker elements, not as `data-hash` attributes.** A
  top-level block can come out of raw HTML that the parser never saw as one
  element, so keys cannot be attached in Rust without a second HTML parser.
  An empty `<ezmark-block key>` before each block goes through the sanitizer
  with everything else, and the view attaches keys once the browser has
  parsed the document.
- **No vendored differ**, as section 17 allowed: unchanged blocks at the start
  and end of the document stay in the page, and everything between is
  replaced. Edits are local, so this covers the usual case in a few lines.
- **Not done yet from M1:** image dimensions from file headers, link
  resolution, the wider hostile corpus, and the adversarial performance
  fixtures as a recorded CI step (their timings are in
  `docs/measurements.md`, measured by hand).
- **Not done yet from M3:** the stop action for a stuck render, cooperative
  cancellation between stages, large-file mode, and the acceptance runs with
  saves from VS Code, Vim, and Obsidian on real machines. The save patterns
  those editors use are tested (write in place, truncate then write, rename
  over), but by a test, not by the editors.
- **The grammar set is syntect's default one**, which lacks TypeScript, TOML,
  Dockerfile, Kotlin, Swift, and PowerShell. TypeScript and JSX fall back to
  the JavaScript grammar. A larger set is a dependency decision (binary size,
  first-build time, licences) that has not been made.
- **`notify` is licensed CC0-1.0**, added to the allowed licences in
  `deny.toml`.

### Added (M0: spike and skeleton)

- Cargo workspace with `default-members = ["crates/app"]`, so
  `cargo run --release -- file.md` at the repo root runs the viewer. The app
  crate's default feature is `custom-protocol`, so release builds serve the
  embedded `ui/`.
- `mdcore`: comrak parsing with raw HTML kept, ammonia sanitizing, and the
  shared boundary validator for local references.
- `ezmark` shell: argv handling (relative, quoted, Unicode, missing file, no
  file), drag-drop, single-instance forwarding of argv and working directory,
  buffering of opens until the frontend is ready (including macOS
  `RunEvent::Opened`), a navigation guard, and the `ezasset` protocol for local
  images.
- Strict CSP and an ACL that covers every command.
- Security harness (`--features test-harness`): one sanitized hostile
  fixture, one enforcement fixture, an ACL probe through the real webview IPC
  path, asset-handler probes, and launch tests, all against a loopback listener
  with a positive control.
- CI on `windows-2025` and `macos-15`: format, clippy, tests and harness, a
  macOS cold open through Launch Services, real-app screenshots, and a
  production release build checked for harness remnants. Weekly clean build.
- ADRs 0001 to 0004, ARCHITECTURE.md, SECURITY.md, CONTRIBUTING.md,
  measurements.

### Changed after the 2026-11-13 audit

An adversarial audit of the M0 code on 2026-11-13 scored it 7.0 out of 10 and
listed 36 findings. This is what changed in answer. The plan's own text is
amended in its section 17, and the security contract in ADR 0003.

#### Security

- **A network lock for the webview (Windows).** New tests injected every kind
  of load and navigation past the sanitizer, and three reached the test
  listener on WebView2 157: a navigation the Rust guard denied had already
  sent its request, a `preconnect` hint opened a connection no CSP directive
  governs, and a frame opened one before `frame-src` refused it. None is
  reachable from a document, because the sanitizer removes the markup, but
  the layers are meant to hold independently. The webview now starts with a
  proxy that does not exist, no loopback exception, no host name resolution,
  and background requests off. macOS has none yet: WKWebView does not leak a
  denied navigation, but opens two connections for hints and a frame, and a
  dead proxy on its data store made no reliable difference.
- **ammonia 4.2.4 to 4.2.5** for RUSTSEC-2026-0533 (mutation XSS), found by
  the new `cargo deny` job.
- **The webview's menu and downloads.** The built-in context menu on a link
  could save or open a URL the document chose. It is suppressed on links, and
  every download is refused in Rust.
- **The CSP no longer allows inline styles or `data:` images.**
- **Links to `//host/x` and `\\host\x` are dropped**, and link targets are
  checked on the string a browser would parse (leading whitespace and embedded
  tabs removed).
- **A `.git` at or above the home directory, or at a filesystem root, no
  longer becomes the root.**
- **Document ids and names are prefixed with `user-content-`**, so they cannot
  shadow the app's own elements. Only a fixed list of class names survives.
- **Images are capped at 20 MB and four concurrent reads.**

#### Rendering

- **GitHub features survive the sanitizer**: task-list checkboxes, alert
  styling, footnote and heading links, code language classes, `align`, and
  `<picture>` with local sources. Each is pinned by a row in a test table.
- **`http` links are kept as links.** They used to become plain text.
- **An image that may not load says why**, in a line of text: remote, outside
  the folder ezmark may read, not found, or refused. It used to vanish.
- **A stray `%` in an image path works** (`<img src="100%.png">`).
- **A baseline stylesheet** in the system's light or dark scheme: tables,
  code, quotes, alerts, task lists, footnotes.

#### Robustness

- **A render that panics shows an error** naming the file. It used to show
  nothing.
- **A nesting guard.** A file nested more than 512 levels deep is shown as
  plain text with a note, instead of stalling the parser (20,000 nested
  elements in 120 KB took 2.8 s, growing with the square of depth).
- **"Still rendering name.md"** appears after 700 ms of waiting.
- **UTF-16 files open**, with or without a byte-order mark. A file that is
  not valid text says so.
- **A failed start shows a message box**, with instructions if WebView2 is
  missing. It used to exit silently.
- **`--help`, `--version`, and errors** for an unknown `--option` or a second
  file. A release build with no console shows them in a message box.
- **The page is sent once at startup, not twice**, and a view-changed event
  for a view already shown costs no fetch.
- **Image URLs are stable across re-renders** of the same document, ready for
  live reload.

#### Tests and CI

- Real-app tests went from 7 to 12: three enforcement fixtures in place of
  one remote image, navigation attempts, awkward files, and the command line.
  Unit tests went from 26 to 62, and UI tests from 1 to 10.
- The harness no longer waits a fixed time before asserting that nothing
  happened.
- A real drag-and-drop onto the window, automated on Windows.
- `cargo deny` in CI; a non-blocking Windows installer build; the link check
  on pull requests covers only links within the repo, and the weekly run
  checks external ones.
- Test scratch folders and webview profiles are removed after each test.

### M0 spike notes and deviations from the plan

Recorded as the plan requires, rather than absorbed silently.

- **2026-10-27: M0 started a week early.** Work began on 2026-10-27 instead of
  2026-11-03. Milestone target dates are unchanged until M0 closes.
- **2026-10-27: M0 is not closed.** Three obligations need hardware this
  machine does not have, and stay open until done: the clean Windows 11 VM
  rehearsal (the author's machine runs Windows 11 Home, which has no Hyper-V
  or Windows Sandbox), a clean macOS run on a real Mac, and a cold open by
  double-clicking in Finder. CI covers macOS on a `macos-15` runner and
  proves the cold open through `open -a`, which takes the same Launch
  Services path as Finder. The CI runner and the author's machine are proxies,
  not substitutes. Drag-drop onto the empty-state window is implemented but
  not yet checked. The per-obligation status is in `docs/m0-evidence.md`.
- **2026-10-27: clippy for the app crate runs on the Windows and macOS jobs,
  not the `ubuntu-latest` lint job.** The app crate's platform code only
  compiles, and so only lints, on its own OS; Linux would also need webkit2gtk
  for a platform ezmark does not support. The lint job keeps rustfmt, clippy
  for `mdcore`, the TypeScript check, and the link check.
- **2026-10-27: Build Tools finding.** On the author's machine, Build Tools
  2019 with only the MSVC v142 tools and the Windows 10 SDK 10.0.19041, and
  no "Desktop development with C++" workload, built ezmark. As of
  2026-10-27 a fresh install gets Build Tools 2022 or newer (the
  `windows-2025` runner image ships Visual Studio 2026), so the VM rehearsal still decides the
  component list and disk cost the README recommends.
- **2026-10-27: license files.** The license is MIT OR Apache-2.0 as planned,
  shipped as `LICENSE-MIT` and `LICENSE-APACHE` (the Rust convention) rather
  than one `LICENSE` file.
- **2026-10-27: skeleton directories arrive with their milestone.**
  `ui/vendor/` (idiomorph, M3), `tests/spec/` and `tests/pipeline/` (M1), and
  `tests/corpus/` and `tests/visual/` are created when first used, since git
  does not track empty directories.
- **2026-10-27: harness results are also written to a file.** The plan names a
  JSON channel on stdout. An app launched through `open` on macOS has no
  useful stdout, so the harness also writes its result to a file.
- **2026-10-27: carried into M1: relative link hrefs.** The sanitizer keeps
  relative `href` values as plain relative paths, while plan section 7 says
  relative paths survive only after the rewrite step. This is safe in M0
  because the frontend intercepts every click and the Rust navigation guard
  denies any other navigation, but M1's URL rewriting must settle the form
  links take before `resolve_link` exists.
- **2026-10-27: tauri-build does not clean generated permissions.**
  tauri-build reads every file in `permissions/autogenerated` and never
  removes stale ones, so a harness build followed by a production build would
  have shipped the harness ACL entries. `build.rs` deletes them when the
  feature is off, and the artifact check would catch a regression.
- **2026-10-27: generations are assigned when a request is accepted.** Plan
  section 9 says every render request carries its generation. The first
  skeleton assigned it when the render worker started, so two quick opens were
  ordered by thread scheduling rather than arrival, and the buffered startup
  open could be ordered after a request accepted once the frontend was ready.
  `request_open` and `frontend_ready` now assign it under the state lock, and
  `app.js` ignores a view older than the one it shows (covered by a new
  `node:test` run in the lint job, which needs no npm dependency). The window
  title is set on the main thread from the view current at that moment, so a
  slower worker's title update can no longer land after a newer one's.
- **2026-11-13: M0 closes on CI evidence.** The three obligations that need
  hardware the author does not have (a clean Windows 11 VM, a clean Mac, a
  Finder double-click) move to the External test milestone, where two other
  people do exactly those runs. Recorded in plan section 17.
- **2026-11-13: the plan is amended, not rewritten.** Section 17 lists every
  place the audit changed it: `http` links, the lenient `%`, the tighter CSP,
  the network lock, asset URLs keyed to folder and root rather than
  generation, the nesting guard, the early "still rendering" notice, and
  three M3 designs to be tried in a simpler form first (one page instead of
  chunks, a hash reconcile instead of a vendored differ, a killable worker
  instead of orphaned threads).
- **2026-11-13: one more dependency, `rfd`**, for native message boxes when
  there is no window: a failed start, and `--help` from a build with no
  console. It adds two crates to the lock file.
- **2026-11-13: the release profile only strips symbols.** Link-time
  optimisation was measured and left off: 11% smaller for a 67% longer first
  build (`docs/measurements.md`).
- **2026-11-13: first run on a real Mac.** On a MacBook Air (M3, 8 GB,
  macOS 26.6.2) the README command built in 55 seconds and opened the demo
  document, a file dragged onto the empty window opened, and the bundled app
  opened a file cold from Launch Services and from a drop on its icon in
  Finder. The `finder_open` test failed there: Finder refused the scripted
  `open ... using` with `The application "(null)" does not have permission
  to open` the file, although the same test passes on the `macos-15` runner.
  The cause is not established, and the test is not changed here. The check
  against a listener on another machine is still open. Details in
  `docs/m0-evidence.md` and `docs/measurements.md`.
