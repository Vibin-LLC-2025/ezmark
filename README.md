# ezmark

A fast, read-only markdown viewer for Windows 11 and macOS. Open a `.md`
file and read it. It does not edit, and it never will.

[![CI](https://github.com/Vibin-LLC-2025/ezmark/actions/workflows/ci.yml/badge.svg)](https://github.com/Vibin-LLC-2025/ezmark/actions/workflows/ci.yml)

> **Status: milestone M0 (spike and skeleton), closing.** The run story, the
> security harness, and CI on both platforms work, and a document renders the
> way it does on GitHub. Syntax highlighting, the tuned typography and themes,
> following links, and live reload arrive in M1 to M3, so this is not a daily
> viewer yet. See the [plan](docs/CLONE-AND-RUN-PLAN.md) and the
> [changelog](CHANGELOG.md).

![ezmark on Windows 11](docs/media/m0-windows.png)

On macOS: [screenshot](docs/media/m0-macos.png). What has been proven, and
what is still open, is in [docs/m0-evidence.md](docs/m0-evidence.md).

## Why

Developers read READMEs, specs, and AI-generated markdown locally all day. The
tools for it either edit, run scripts from the document, lose your place when
the file changes, or render differently from GitHub. ezmark aims to be:

- strictly read-only
- faithful to GitHub's rendering: alerts, footnotes, task lists, anchors
- safe with untrusted files: sanitized HTML, a strict CSP, no network
- steady while you read: live reload that keeps your place (M3)
- correct about relative links and images, however the file was opened
- honest when it cannot show something: an image that may not load, a file
  that is not text, a render that is taking long, each says so in words
- small enough to read in an afternoon

## Run it

### Prerequisites

**Windows 11**

- Rust via [rustup](https://rustup.rs/), with the default MSVC target.
- Visual Studio Build Tools (standalone, not the IDE) with the MSVC C++
  build tools and a Windows SDK. On the author's machine, Build Tools 2019
  with only "MSVC v142 x64/x86 build tools" and "Windows 10 SDK 10.0.19041"
  was enough. The recommended component list and its disk cost come from a
  clean-VM rehearsal that is still to do. A restart may be needed before
  `cargo` finds the linker.
- WebView2, which Windows 11 includes.

**macOS**

- Rust via [rustup](https://rustup.rs/).
- Xcode Command Line Tools: `xcode-select --install`.
- Tested on: macOS 15 on Apple Silicon, in CI, and macOS 26.6 on an M3
  MacBook Air, by hand. No other version is claimed yet.

**Both:** git. Nothing else.

### One command

The first build compiles about 250 crates. It took 109 seconds on a 12-core
desktop, 55 seconds on an M3 MacBook Air, and under 3 minutes on GitHub's
Windows and macOS runners; expect several minutes on an older laptop. Later
builds take seconds.

```sh
git clone https://github.com/Vibin-LLC-2025/ezmark.git
cd ezmark
cargo run --release -- path/to/file.md
```

Paths may be relative, quoted, or contain spaces and Unicode. Run it without a
file to get a window you can drop a file onto. `--help` and `--version` do
what you expect. Files in UTF-8 or UTF-16 both open, which matters on Windows,
where Windows PowerShell writes UTF-16 for every `>` redirect.

### Optional: build an app bundle

Not needed to read files. This produces a macOS `.app` (ad hoc signed) and a
per-user Windows installer, and its file associations are tested at release
(M5), not yet:

```sh
cargo install tauri-cli --version 2.12.4 --locked
cargo tauri build
```

## Security

ezmark treats every document as hostile. Raw HTML is sanitized in Rust, a
strict Content Security Policy allows images only from ezmark's own asset
scheme, every link click is intercepted, and file access is confined to the
document's folder or its git repository (never your whole home folder). The
full contract is [ADR 0003](docs/adr/0003-security-contract.md).

"No network" is stated at the level of the evidence. The CSP blocks remote
loads, the app code makes no network calls, and no network plugins are
enabled. On Windows the webview also runs behind a network lock: its only
route out is a proxy that does not exist, because testing showed WebView2
making connections of its own that no content policy governs (a refused
navigation still sends its request, and a preconnect hint connects). macOS
has no such lock yet (see the limitations below). The CI
security harness injects every kind of load and navigation past the sanitizer
and requires that none reaches a listener.

What ezmark cannot promise is the behaviour of the system webview itself. A
process-level measurement on Windows found that, with the lock, the webview's
network stack makes no connection at all, but that on a machine signed in with
a Microsoft account the WebView2 runtime still contacts Microsoft about that
account through Windows' own sign-in library. It does so whether or not a
file is open, and nothing an app can set turns it off. The numbers and their
blind spots are in [docs/measurements.md](docs/measurements.md).

Report vulnerabilities privately: see [SECURITY.md](SECURITY.md).

## Known limitations

- **Smart App Control** on Windows 11, when enabled, blocks unsigned
  executables, including ones you build yourself. ezmark does not ask you to
  change security settings; if Smart App Control is on, it will not run.
- **No Linux** in v1.
- **No Mermaid diagrams or math** in v1. Their fences show as code.
- **Remote images never load** in v1. A line of text takes their place and
  shows the URL. The same goes for an image outside the folder ezmark may
  read, which includes `../` images in a repository downloaded as a ZIP,
  because without a `.git` folder the document's own folder is the limit.
- **Links do nothing yet.** Clicking one is cancelled; following links to
  other files and opening web links in your browser are M2.
- **Not yet**: syntax highlighting (M1), the tuned typography, themes beyond
  the system light or dark default, outline, find (M2), and live reload (M3).
- **Deeply nested files show as plain text.** Nesting more than 512 levels
  deep (no real document does) costs the parser time that grows with the
  square of the depth, so such a file is shown unrendered, with a note.
- **A slow render cannot be stopped yet**, only superseded by opening another
  file. The window says "Still rendering" while it waits. A stop action is M3.
- **The network lock is Windows-only.** On macOS, markup injected past the
  sanitizer (a preconnect hint, a frame) made WKWebView open two connections
  to a local test listener. A document cannot produce that markup, so this is
  a missing second layer, not a hole; see the security section.

## Scope

**Never:** editing of any kind, split view, plugins, sync, telemetry, and
DOCX, EPUB, or Pandoc export.

**Later (v1.1 or after):** Mermaid and math, opt-in remote images, print, open
in editor, reveal in Explorer or Finder, copy buttons on code blocks, tabs,
bundled fonts, a macOS Quick Look extension, an Explorer preview pane, Linux,
and signed installers with auto-update.

## Why not MDHero or Tinta?

Both are good, and if you want a working viewer before ezmark reaches v1, try
them first. As of
2026-10-26:

- [MDHero](https://github.com/vaibhav-kakde-in/mdhero) (MIT, Tauri) is the
  closest match: viewer-first, cross-platform, live reload. Its issue queue
  is moving toward editing (undo, pasting images), footnotes do not render,
  and there is no Intel Mac build.
- [Tinta](https://github.com/oipoistar/tinta) (MIT, native C++) is a tiny,
  fast Windows-only viewer with an edit mode.

ezmark's bet is the combination: strictly read-only, GitHub-faithful,
hostile-file-safe with tests to show it, and a codebase small enough to audit.

## Development

[CONTRIBUTING.md](CONTRIBUTING.md) has the commands, including the security
harness. [ARCHITECTURE.md](ARCHITECTURE.md) is the codemap, the decisions are
in [docs/adr](docs/adr), and measured numbers are in
[docs/measurements.md](docs/measurements.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
