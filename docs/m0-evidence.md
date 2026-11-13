# M0 evidence

Every M0 obligation from the plan (section 10's M0 row and the section 6 run
story), with where it was proven. "CI" means
[run 40460451943](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40460451943)
on commit `7562c8b`, 2026-10-27, on runner images `win25-vs2026 20261021.278.1`
(`windows-2025`) and `macos15 20260907.0337.1` (`macos-15`, Apple Silicon).
"Local" means the author's Windows 11 machine described in
[measurements.md](measurements.md).

Status key: **proven**, **proven by proxy** (the mechanism is exercised, but
not under the exact conditions the plan names), **moved** (to the External
test milestone, plan section 17), **open**.

Rows marked "local, 2026-11-13" were re-proven on the author's Windows machine
after the changes that followed the 2026-11-13 audit (CHANGELOG). Their CI run
on both runners is the first run of the commit that adds this sentence.

## M0 row (plan section 10)

| Obligation | Status | Evidence |
|---|---|---|
| Repo, license, ADRs | proven | Public repo; `LICENSE-MIT` and `LICENSE-APACHE`; `docs/adr/0001` to `0004` |
| CI with format, clippy, test, release build on both OS runners | proven | CI: `lint`, `test` and `build` green on `windows-2025` and `macos-15`. App-crate clippy runs on the OS runners (CHANGELOG, 2026-10-27) |
| `cargo run --release -- file.md` from the root, both platforms | Windows proven; macOS proven by proxy | Local: the literal command on a quoted path with spaces and Unicode opened the document. CI: the same production `--release` build compiles on both runners, and the harness launches the app from argv on both |
| Clean Windows 11 VM rehearsal, disk use, first-build time | moved | No Hyper-V or Windows Sandbox on Windows 11 Home. Author-machine numbers stand in: 109 s first build, 361 MB `CARGO_HOME`, 1,295 MB `target/`. The External test milestone does this run on a clean machine with someone other than the author |
| Clean macOS run | moved | The `macos-15` runner is the proxy. The author's own Mac run is recorded here when done; the External test does the clean-machine run |
| One real screenshot per OS | proven | [m0-windows.png](media/m0-windows.png) (local, production release build) and [m0-macos.png](media/m0-macos.png) (CI, `screencapture` on the runner) |
| Asset protocol serves a relative image on both OSes | proven | `relative_image_is_served_through_the_asset_protocol` (local, 2026-11-13): three relative images load at 3x2, one of them named `100%.png`; an out-of-root image and a missing one each become a placeholder saying why; the handler refuses a crafted out-of-root reference and a previous asset scope. The out-of-root target is a valid PNG, so a decode failure cannot pass as a refusal; with the handler's root check removed, the test fails (local mutation check, 2026-10-27) |
| macOS `RunEvent::Opened` buffering, cold Finder open of the bundled app | proven by proxy | CI: `cargo tauri build --bundles app` then `open -a ezmark.app file.md` on a cold start. Result: origin `opened-event`, `buffered: true`, document rendered. `open` uses the same Launch Services open-documents path as a Finder double-click. A second test asks Finder itself to open the file (`finder_open_is_buffered_and_replayed`); it needs permission to control Finder, so in CI it reports without failing the job. A real double-click is moved to the External test |
| Sanitized hostile fixture, both OSes | proven | `sanitized_hostile_fixture_runs_clean` (local, 2026-11-13): no scripts, inline handlers, script URLs, or frames in the DOM; no attribute naming a URL except a link's own target; every image replaced by a placeholder; a raw `<input>` forced to a disabled checkbox; ids prefixed; zero CSP reports; zero connections |
| Enforcement fixture with an enforced CSP report naming the blocked URL and directive | proven | `enforcement_fixture_is_blocked_by_the_enforced_csp` requires a report whose `blockedURI` is exactly the fixture URL, with `img-src` and `enforce`. [Run 40461627894](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40461627894): WebView2 `{"blockedURI":"http://127.0.0.1:58927/enforcement-pixel.png","disposition":"enforce","effectiveDirective":"img-src","violatedDirective":"img-src"}`; WKWebView `{"blockedURI":"http://127.0.0.1:49187/enforcement-pixel.png","disposition":"enforce","effectiveDirective":"img-src","violatedDirective":"img-src"}` |
| Enforcement corpus beyond one image (added 2026-11-13) | proven on Windows; on macOS proven except one speculative connection (see [measurements.md](measurements.md)) | Three more fixtures injected past the sanitizer. `enforcement_corpus_reaches_no_listener`, with the network lock off: 18 kinds of resource load and 8 script-driven requests, 27 enforced CSP reports, zero connections. `speculative_connections_reach_no_listener`: preconnect and dns-prefetch hints and a frame, zero connections with the network lock; without it WebView2 157 connects for the hints and for the frame (measured, see [measurements.md](measurements.md)). `navigation_attempts_never_leave_the_app_page`: link clicks, middle clicks, context menus, a `target=_blank` link, `window.open`, a form submit, a meta refresh, and `location`; the click handler cancels the clicks, `form-action` reports the form, the Rust guard denies the two navigations, the page stays, zero connections |
| Blocked outbound attempt, listener positively verified | proven | Every loopback listener first accepts and counts its own connection; the assertion is zero connections beyond it |
| Denied IPC call through the real webview IPC path | proven | Every harness run calls `harness_denied_probe`, enrolled in the ACL but granted to no capability. Rejected on both OSes (macOS: "harness_denied_probe not allowed. Permissions associated with this command: allow-harness-denied-probe"), and its body never ran |
| Production artifact lacks the injection command, its ACL entry, and test assets | proven | CI `build` job: `check-release-artifact.sh absent` on both release binaries; the `test` job runs the same script with `present` on the harness build as the positive control |

## Run story (plan section 6)

| Requirement | Status | Evidence |
|---|---|---|
| `default-members = ["crates/app"]` | proven | `cargo run` at the root ran `target/release/ezmark.exe` |
| `default = ["custom-protocol"]` serves embedded assets in release | proven | The production release build renders `ui/` (Windows screenshot) |
| Config paths resolve when invoked from the root | proven | `cargo build` and `cargo tauri build` from the root, locally and in CI |
| Quoted paths, spaces, Unicode | proven | Local literal command; `relative_unicode_path_resolves_against_the_working_directory` on both runners |
| Relative paths against the invoking working directory | proven | Same test, plus the forwarded second launch |
| Missing file: clear error, not a blank window | proven | `missing_file_shows_a_clear_error`: "File not found: <path>" rendered |
| No argument: empty-state window | proven | `no_argument_shows_the_empty_state` |
| Drag-drop onto the empty-state window | proven on Windows; macOS open | `scripts/dragdrop-windows.ps1` performs a real OLE drag of a file with a space and a non-ASCII character in its name from a source window onto the app window, with real mouse input, and requires origin `drag-drop` and the document rendered. Passed 3 of 3 runs on 2026-11-13, and again on the final build; CI runs it on `windows-2025`. macOS: not yet checked |
| macOS open events buffered and replayed | proven by proxy | See the cold-open row above |
| Second launch forwarded with argv and cwd | proven | `second_launch_is_forwarded_to_the_running_instance` passes on both runners (the plugin covers macOS too) |
| Raw `cargo run` registers no file associations | proven by construction | No code path writes associations; only `cargo tauri build` bundles declare them |

## Added after the audit (2026-11-13)

Not M0 obligations, but claims the README now makes, with their evidence. All
local on Windows, 2026-11-13.

| Claim | Evidence |
|---|---|
| UTF-16 files open; a file that is not text says so; deep nesting falls back to plain text with a note | `awkward_files_open_and_say_what_was_done`, through the real app |
| `--help`, `--version`, and usage errors never open a window | `command_line_options_answer_without_opening_a_window`; by hand, a release build started with no console shows the version in a message box |
| A render that panics shows an error | `a_panicking_render_becomes_an_error_view` (unit) |
| A junction or symlink out of the root is refused | `a_link_out_of_the_root_is_refused` (unit, real filesystem, with a positive control that the link works) |
| The webview's "Save link as" fetched a URL the document chose, before the fix | By hand in the production build at `62d419d`: the menu on a link offered "Open link in new window", "Save link as", "Copy link"; choosing "Save link as" opened two TCP connections to a local listener. Now the menu is suppressed on links (`linkContextMenu` in the navigation test), downloads are refused in Rust, and the network lock leaves a request nowhere to go |

## What closes M0

M0 closes on the evidence above once CI is green on both runners for the
commit that adds the post-audit changes (plan section 17). Still worth doing
when a Mac is at hand, and recorded here when done:

1. `cargo run --release -- docs/media/demo/demo.md` on a real Mac.
2. Drag a file onto the empty window on macOS.
3. Build the app bundle and double-click a `.md` file in Finder with ezmark
   chosen in Open With (or run the `finder_open` test, which asks Finder to do
   the same).
4. Run the `speculative` test and note how many connections reach the
   listener; on the CI runner it was one, with the lock.
