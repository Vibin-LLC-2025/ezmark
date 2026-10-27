# M0 evidence

Every M0 obligation from the plan (section 10's M0 row and the section 6 run
story), with where it was proven. "CI" means
[run 40460451943](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40460451943)
on commit `7562c8b`, 2026-10-27, on runner images `win25-vs2026 20261021.278.1`
(`windows-2025`) and `macos15 20260907.0337.1` (`macos-15`, Apple Silicon).
"Local" means the author's Windows 11 machine described in
[measurements.md](measurements.md).

Status key: **proven**, **proven by proxy** (the mechanism is exercised, but
not under the exact conditions the plan names), **open**.

## M0 row (plan section 10)

| Obligation | Status | Evidence |
|---|---|---|
| Repo, license, ADRs | proven | Public repo; `LICENSE-MIT` and `LICENSE-APACHE`; `docs/adr/0001` to `0004` |
| CI with format, clippy, test, release build on both OS runners | proven | CI: `lint`, `test` and `build` green on `windows-2025` and `macos-15`. App-crate clippy runs on the OS runners (CHANGELOG, 2026-10-27) |
| `cargo run --release -- file.md` from the root, both platforms | Windows proven; macOS proven by proxy | Local: the literal command on a quoted path with spaces and Unicode opened the document. CI: the same production `--release` build compiles on both runners, and the harness launches the app from argv on both |
| Clean Windows 11 VM rehearsal, disk use, first-build time | **open** | No Hyper-V or Windows Sandbox on Windows 11 Home. Author-machine numbers stand in: 109 s first build, 361 MB `CARGO_HOME`, 1,295 MB `target/` |
| Clean macOS run | **open** | No Mac available. The `macos-15` runner is the proxy |
| One real screenshot per OS | proven | [m0-windows.png](media/m0-windows.png) (local, production release build) and [m0-macos.png](media/m0-macos.png) (CI, `screencapture` on the runner) |
| Asset protocol serves a relative image on both OSes | proven | `relative_image_is_served_through_the_asset_protocol` passes on both runners and locally: two relative images load at 3x2; an out-of-root image gets no `src`; the handler refuses a crafted out-of-root reference and a stale generation. Both screenshots show the relative image |
| macOS `RunEvent::Opened` buffering, cold Finder open of the bundled app | proven by proxy | CI: `cargo tauri build --bundles app` then `open -a ezmark.app file.md` on a cold start. Result: origin `opened-event`, `buffered: true`, document rendered. `open` uses the same Launch Services open-documents path as a Finder double-click; a real double-click is **open** |
| Sanitized hostile fixture, both OSes | proven | `sanitized_hostile_fixture_runs_clean`: no scripts, inline handlers, script URLs, frames, or loopback URLs in the DOM; zero CSP reports; zero connections |
| Enforcement fixture with an enforced CSP report naming the blocked URL and directive | proven | `enforcement_fixture_is_blocked_by_the_enforced_csp` requires a report whose `blockedURI` is exactly the fixture URL, with `img-src` and `enforce`. [Run 40461627894](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40461627894): WebView2 `{"blockedURI":"http://127.0.0.1:58927/enforcement-pixel.png","disposition":"enforce","effectiveDirective":"img-src","violatedDirective":"img-src"}`; WKWebView `{"blockedURI":"http://127.0.0.1:49187/enforcement-pixel.png","disposition":"enforce","effectiveDirective":"img-src","violatedDirective":"img-src"}` |
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
| Drag-drop onto the empty-state window | **open** | Implemented (`WindowEvent::DragDrop` into `request_open`); not yet checked by hand or automation |
| macOS open events buffered and replayed | proven by proxy | See the cold-open row above |
| Second launch forwarded with argv and cwd | proven | `second_launch_is_forwarded_to_the_running_instance` passes on both runners (the plugin covers macOS too) |
| Raw `cargo run` registers no file associations | proven by construction | No code path writes associations; only `cargo tauri build` bundles declare them |

## What closes M0

1. The clean Windows 11 VM rehearsal, with its numbers and component list
   written into the README and measurements.
2. A clean macOS run on a real Mac, including a Finder double-click cold open.
3. A drag-drop check on both OSes.
