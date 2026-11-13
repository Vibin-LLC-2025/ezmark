# Architecture

ezmark is two crates and a static folder. The main architectural statement is
the split between them: the markdown engine is a plain Rust library with no
GUI dependency, and the Tauri shell around it stays thin.

```
crates/mdcore   engine: decode, parse, rewrite URLs, sanitize   (no Tauri)
crates/app      shell: windows, open events, IPC, asset protocol
ui/             view: index.html, app.css, app.js, placeholders.js   (embedded at compile time)
```

## Codemap

### `crates/mdcore`

- `boundary.rs`: the one shared validator for every local reference a
  document makes. Raw gate, percent-decode exactly once, inspect before any
  filesystem call, canonicalize, compare with the root. The `Fs` trait lets
  tests prove that rejected input never reaches the filesystem.
- `render.rs`: comrak with raw HTML kept, then ammonia with the allowlist and
  the per-attribute policy. Local images are rewritten to asset URLs through
  a callback the shell supplies; an image that may not load becomes an inert
  marker that says why; everything else that could fetch is dropped.
- `nesting.rs`: a cheap guard that sends absurdly deep nesting to a plain-text
  fallback instead of the parser, whose cost grows with the square of depth.
- `text.rs`: bytes to text. UTF-8 and UTF-16, with or without a byte-order
  mark, and a flag when something could not be decoded.
- `hex.rs`: the transport encoding for document-supplied text inside URLs.

M1 adds highlighting (syntect), front matter, and image dimensions here.

### `crates/app` (binary `ezmark`)

- `main.rs`: builder wiring, the main window, the navigation guard, the
  download refusal, the network lock, drag-drop, and macOS
  `RunEvent::Opened`.
- `launch.rs`: argv parsing (`--help`, `--version`, one file), and telling the
  user something when there is no window and maybe no console. Relative paths
  resolve against the invoking process's working directory, including a
  forwarded second launch's.
- `documents.rs`: the Rust-owned state. Every open (argv, drag-drop, second
  instance, macOS open event) goes through `request_open`, which buffers until
  the frontend signals readiness. Each request gets its generation when it is
  accepted, in arrival order; a render is committed only if nothing newer was
  accepted since. A load that fails, or panics, ends in an error view.
- `commands.rs`: the IPC surface, two commands, neither taking an argument.
- `asset.rs`: the `ezasset` protocol for local images, with a size cap and a
  limit on concurrent reads.
- `harness.rs`: test-harness plumbing, compiled only with
  `--features test-harness`.
- `build.rs`: enrols commands in the ACL and removes stale harness permission
  files from production builds.
- `harness/`: the injected harness script and its runtime capability. Kept out
  of `ui/` so they can never be embedded in a release build.
- `tests/harness.rs`: spawns the harness build per fixture.

### `ui/`

Plain ES-module JavaScript with JSDoc types, no framework, no bundler, no
runtime npm dependency. `app.js` shows what Rust says to show, swaps blocked
images for text before they reach the page, says when a render is slow, and
cancels every click and context menu on a link that leaves the page.
`placeholders.js` reads the blocked-image markers.

### `tests/`

- `security/sanitized/`: hostile fixtures that go through the full pipeline.
- `security/enforcement/`: raw HTML injected past the sanitizer, so the layers
  behind it have to hold: `remote-img.html` and `resources.html` against the
  CSP alone, `speculative.html` against the network lock, `navigation.html`
  against the click handler, `form-action`, and the navigation guard.
- `security/assets/`: small binary fixtures.
- `ui/`: a `node:test` run of the real `ui/` files against fake Tauri and DOM
  globals (`npm test`, contributor-only).

M1 adds `spec/` and `pipeline/`, M2 and M4 add `corpus/` and `visual/`.

## Opening a file

```
argv / drag-drop / second launch / macOS open event
        |
        v
documents::request_open ---- frontend not ready ----> buffered (latest wins)
        |                                                   |
        | ready                         frontend_ready starts it and returns
        | generation += 1               at once: generation += 1
        | emit loading                  reply { view, loading }
        v                                                   v
open_now(generation), on a worker thread, panics caught:
  read (50 MB cap), decode (UTF-8 or UTF-16), find root (.git or file's dir,
  never the home directory or above), asset scope id for (dir, root)
        |
        v
mdcore::render(text, doc_dir, root, asset_url(scope)): nesting guard, parse, sanitize
        |
        v
commit if still the latest generation, set title, emit view-changed(generation)
        |
        v
app.js: newer than shown? -> current_view -> template (inert) -> blocked
        images become text -> into the page -> ezmark:rendered
        |
        v
<img src="ezasset .../<scope>/<hex raw reference>">
        |
        v
asset::handle: current scope? validate again, image types only, nosniff
```

A render that has not produced a view within 700 ms shows "Still rendering
name.md" in the window. Opening another file supersedes it.

## Invariants

These are the security contract (plan sections 7 and 17,
[ADR 0003](docs/adr/0003-security-contract.md)) expressed as properties of the
code. Breaking one is a security bug.

1. Everything that leaves `mdcore` is sanitized.
2. Every local reference goes through `mdcore::validate_reference`, and its
   inspection step runs before any filesystem call.
3. The root is set only from Rust or the OS. No IPC command takes a path that
   can move it, and no command carries a trust flag.
4. Every IPC command is enrolled in the ACL and granted only to `main`.
5. The webview never navigates away from the app origin, opens no window, and
   downloads nothing.
6. The CSP allows images only from the asset scheme, scripts and styles only
   from the bundle, and nothing in the app opens a network connection. The
   webview's network stack is pointed at a proxy that does not exist (the
   network lock: complete on Windows, partial on macOS).
7. Nothing from the `test-harness` feature appears in a production binary.
8. A document's ids, names, and classes cannot collide with the app's own:
   ids and names are prefixed, and only a fixed list of classes survives.
