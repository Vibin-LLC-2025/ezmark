# Architecture

ezmark is two crates and a static folder. The main architectural statement is
the split between them: the markdown engine is a plain Rust library with no
GUI dependency, and the Tauri shell around it stays thin.

```
crates/mdcore   engine: decode, parse, highlight, rewrite URLs, sanitize   (no Tauri)
crates/app      shell: windows, open events, watcher, IPC, asset protocol
ui/             view: index.html, app.css, highlight.css, app.js, reload.js, placeholders.js
                (embedded at compile time)
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
- `highlight.rs`: syntect on fenced code, before the sanitizer. Flat spans
  with one class each from a fixed list of 14, never inline styles, because
  the CSP allows styles from the bundle only. Bounded per document.
- `frontmatter.rs`: a leading `---` block becomes a closed `<details>` with a
  table of its top-level keys. Not a YAML parser, and no dependency.
- `blocks.rs`: a content key for every top-level block (the hash of its
  source text), written as an empty `<ezmark-block>` marker before the block.
  Live reload finds the reader's place by these keys.
- `nesting.rs`: a cheap guard that sends absurdly deep nesting to a plain-text
  fallback instead of the parser, whose cost grows with the square of depth.
- `text.rs`: bytes to text. UTF-8 and UTF-16, with or without a byte-order
  mark, and a flag when something could not be decoded.
- `hex.rs`: the transport encoding for document-supplied text inside URLs.

Still to come in M1: image dimensions read from file headers, and link
resolution.

### `crates/app` (binary `ezmark`)

- `main.rs`: builder wiring, the main window, the navigation guard, the
  download refusal, the WebView2 network lock, drag-drop, and macOS
  `RunEvent::Opened`.
- `launch.rs`: argv parsing (`--help`, `--version`, one file), and telling the
  user something when there is no window and maybe no console. Relative paths
  resolve against the invoking process's working directory, including a
  forwarded second launch's.
- `documents.rs`: the Rust-owned state. Every open (argv, drag-drop, second
  instance, macOS open event) goes through `request_open`, which buffers until
  the frontend signals readiness. Each request gets its generation when it is
  accepted, in arrival order; a render is committed only if nothing newer was
  accepted since. A load that fails, or panics, ends in an error view. All
  rendering runs on one thread with a one-slot mailbox, so waiting requests
  coalesce. A reload renders only if the file's bytes differ from the ones on
  screen.
- `watch.rs`: watches the open file's folder (editors save by rename, which a
  watch on the file would not survive), debounces, and asks for a reload. A
  file that disappears is reported only after a grace period, because a save
  can look like that for a moment. Events do not cover everything: a renamed
  folder takes its watch along silently, and a file an editor holds open
  cannot be read when its event arrives. So the watcher also checks every 2
  seconds that the file is where it was, and looks again shortly after a read
  that failed. The timing is a state machine that takes the time as an
  argument, so its tests use no clock.
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
runtime npm dependency, and no third-party script. `app.js` shows what Rust
says to show, swaps blocked images for text before they reach the page, says
when a render is slow, and cancels every click and context menu on a link that
leaves the page. `placeholders.js` reads the blocked-image markers.
`reload.js` is live reload's half of the view: it reads the block markers out
of the document, replaces only the blocks that changed, and decides where the
reader lands. `highlight.css` holds the code colours for both themes.

### `tests/`

- `security/sanitized/`: hostile fixtures that go through the full pipeline.
- `security/enforcement/`: raw HTML injected past the sanitizer, so the layers
  behind it have to hold: `remote-img.html` and `resources.html` against the
  CSP alone, `speculative.html` against the network lock, `navigation.html`
  against the click handler, `form-action`, and the navigation guard.
- `security/assets/`: small binary fixtures.
- `ui/`: a `node:test` run of the real `ui/` files against fake Tauri and DOM
  globals (`npm test`, contributor-only). `reload.test.mjs` holds the plan's
  reading-position acceptance cases.
- `spec/`: the CommonMark 0.31.2 and GFM spec files, unmodified. The parser
  must reproduce every example with spec settings
  (`crates/mdcore/tests/conformance.rs`).
- `pipeline/`: what the full pipeline makes of those same examples, saved.
  A change in output fails the test until the snapshot is regenerated and the
  diff read.

M2 and M4 add `corpus/` and `visual/`.

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
mdcore::render(text, doc_dir, root, asset_url(scope)): nesting guard, parse,
  block markers, front matter, highlight, sanitize
        |
        v
commit if still the latest generation, set title, emit view-changed(generation),
  watch the file's folder
        |
        v
app.js: newer than shown? -> current_view -> template (inert) -> blocked
        images become text -> markers read and removed -> into the page
        -> ezmark:rendered
        |
        v
<img src="ezasset .../<scope>/<hex raw reference>">
        |
        v
asset::handle: current scope? validate again, image types only, nosniff
```

A render that has not produced a view within 700 ms shows "Still rendering
name.md" in the window. Opening another file supersedes it.

## When the file changes

```
the editor saves (in place, truncate and write, or write a temp file and rename)
        |
        v
watch.rs: an event naming the file -> quiet for 75 ms (at most 500 ms of
  continuous writing) -> is the file there?
        |                                   |
        | yes                               | no: wait 600 ms, look again,
        v                                   v     then file-status(missing)
documents::request_reload -> the render thread's one-slot mailbox
        |
        v
read the file; same bytes as on screen? -> nothing happens
        |
        | different
        v
generation += 1, render, commit, emit view-changed (asset URLs unchanged:
  they are keyed to the folder and root, not to the render)
        |
        v
app.js: same path as shown -> note the block at the top of the window and
  its offset -> replace only the blocks whose HTML changed -> find that
  block again by its key -> scroll so it sits where it sat
```

The place is found by content, under one rule (plan section 9): a key is
matched only if exactly one block in the old document and exactly one in the
new one carry it. If the reader's block does not qualify, the nearest block
above it that does is brought to the top of the window; if none does, the
scroll ratio is kept. The view can therefore land above where the reader was,
but never on a different copy of an identical block.

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
   from the bundle, and nothing in the app opens a network connection. On
   Windows the webview's network stack is pointed at a proxy that does not
   exist (the network lock).
7. Nothing from the `test-harness` feature appears in a production binary.
8. A document's ids, names, and classes cannot collide with the app's own:
   ids and names are prefixed, and only a fixed list of classes survives.
   The highlighter's classes are on that list, so highlighting needs no
   inline style and no relaxation of the CSP.
9. Block markers never reach the page. They are read and removed while the
   document is still in an inert template. A marker a document writes itself
   passes the same sanitizer and can only affect where that document's own
   reader lands after a reload.
