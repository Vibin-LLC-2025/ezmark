# Architecture

ezmark is two crates and a static folder. The main architectural statement is
the split between them: the markdown engine is a plain Rust library with no
GUI dependency, and the Tauri shell around it stays thin.

```
crates/mdcore   engine: parse, rewrite URLs, sanitize   (no Tauri)
crates/app      shell: windows, open events, IPC, asset protocol
ui/             view: index.html, app.css, app.js       (embedded at compile time)
```

## Codemap

### `crates/mdcore`

- `boundary.rs`: the one shared validator for every local reference a
  document makes. Raw gate, percent-decode exactly once, inspect before any
  filesystem call, canonicalize, compare with the root. The `Fs` trait lets
  tests prove that rejected input never reaches the filesystem.
- `render.rs`: comrak with raw HTML kept, then ammonia with the URL policy.
  Local images are rewritten to asset URLs through a callback the shell
  supplies; everything else that could fetch is dropped.

M1 adds highlighting (syntect), front matter, image dimensions, the extended
allowlist, and remote-image placeholders here.

### `crates/app` (binary `ezmark`)

- `main.rs`: builder wiring, the main window, the navigation guard, drag-drop,
  and macOS `RunEvent::Opened`.
- `launch.rs`: argv parsing. Relative paths resolve against the invoking
  process's working directory, including a forwarded second launch's.
- `documents.rs`: the Rust-owned state. Every open (argv, drag-drop, second
  instance, macOS open event) goes through `request_open`, which buffers until
  the frontend signals readiness. Each open bumps a generation; a render is
  committed only if nothing newer started.
- `commands.rs`: the IPC surface, two commands in M0.
- `asset.rs`: the `ezasset` protocol for local images.
- `harness.rs`: test-harness plumbing, compiled only with
  `--features test-harness`.
- `build.rs`: enrols commands in the ACL and removes stale harness permission
  files from production builds.
- `harness/`: the injected harness script and its runtime capability. Kept out
  of `ui/` so they can never be embedded in a release build.
- `tests/harness.rs`: spawns the harness build per fixture.

### `ui/`

Plain ES-module JavaScript with JSDoc types, no framework, no bundler, no
runtime npm dependency. `app.js` shows what Rust says to show and intercepts
every link click.

### `tests/`

- `security/sanitized/`: hostile fixtures that go through the full pipeline.
- `security/enforcement/`: raw HTML injected past the sanitizer, so the CSP
  alone has to stop it.
- `security/assets/`: small binary fixtures.

M1 adds `spec/` and `pipeline/`, M2 and M4 add `corpus/` and `visual/`.

## Opening a file

```
argv / drag-drop / second launch / macOS open event
        |
        v
documents::request_open ---- frontend not ready ----> buffered (latest wins)
        |                                                   |
        | ready                         frontend_ready replays it
        v                                                   v
open_now: generation += 1, read (50 MB cap), find root (.git or file's dir)
        |
        v
mdcore::render(text, doc_dir, root, asset_url(generation))
        |
        v
commit if still the latest generation, set title, emit view-changed
        |
        v
app.js: current_view -> innerHTML (already sanitized) -> ezmark:rendered
        |
        v
<img src="ezasset .../<generation>/<hex raw reference>">
        |
        v
asset::handle: current generation? validate again, image types only, nosniff
```

## Invariants

These are the security contract (plan section 7, [ADR 0003](docs/adr/0003-security-contract.md))
expressed as properties of the code. Breaking one is a security bug.

1. Everything that leaves `mdcore` is sanitized.
2. Every local reference goes through `mdcore::validate_reference`, and its
   inspection step runs before any filesystem call.
3. The root is set only from Rust or the OS. No IPC command takes a path that
   can move it, and no command carries a trust flag.
4. Every IPC command is enrolled in the ACL and granted only to `main`.
5. The webview never navigates away from the app origin.
6. The CSP allows images only from the asset scheme and `data:`, and nothing
   in the app opens a network connection.
7. Nothing from the `test-harness` feature appears in a production binary.
