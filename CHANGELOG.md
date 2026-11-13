# Changelog

All notable changes to ezmark are recorded here by hand
([Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format, semantic
versioning). Release automation is v1.1 work.

## [Unreleased]

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
  and background requests off.
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
