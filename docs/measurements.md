# Measurements

Numbers the README quotes, with where and when they were measured. Each
release adds a section; nothing here is a pass or fail threshold.

## M0 spike, 2026-10-27

### Author's machine (Windows, not a clean VM)

Windows 11 Home 10.0.26300, Intel Core i7-12700F (12 cores, 20 threads),
32 GB RAM, NVMe SSD. Visual Studio Build Tools 2019 (16.11) was already
installed, with only two components: MSVC v142 x64/x86 build tools
(`Microsoft.VisualStudio.Component.VC.Tools.x86.x64`, 14.29) and the
Windows 10 SDK 10.0.19041. No "Desktop development with C++" workload. Rust
was not installed beforehand.

| What | Result |
|---|---|
| `rustup-init.exe`, default profile, stable 1.99.1 | 36.6 s; `~/.rustup` 1,330 MB (includes offline docs) |
| First `cargo build --release` (empty `CARGO_HOME` and target dir; 265 crates downloaded, 249 compiled) | 109 s |
| Disk after that build | `CARGO_HOME` 361 MB, `target/` 1,295 MB |
| Release binary `ezmark.exe` (production features, default release profile, unstripped) | 10,732,032 bytes |
| `cargo install tauri-cli --version 2.12.4 --locked` (optional second step) | 212 s |
| Build Tools 2019 install directory | 2,882 MB (Windows SDK lives separately) |

The first-build time depends heavily on core count. A clean Windows 11 VM
rehearsal, with a fresh Build Tools install and someone other than the author,
is still to do; it decides the numbers and the component list the README
recommends.

### CI runners

[Run 40460451943](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40460451943),
2026-10-27, the first run, so every cache was empty.

| What | `windows-2025` (image `win25-vs2026 20261021.278.1`) | `macos-15` (image `macos15 20260907.0337.1`, Apple Silicon) |
|---|---|---|
| Release build, production features, no cache | 2 min 50 s | 2 min 18 s |
| Release binary (unstripped) | 10,719,232 bytes | 12,083,648 bytes |
| `cargo install tauri-cli --version 2.12.4 --locked` | not run | 5 min 43 s |
| Whole `test` job (clippy twice, tests, harness, screenshot; macOS also bundles and cold-opens) | 5 min 50 s | 9 min 12 s |
| Whole `build` job | 3 min 41 s | 2 min 57 s |

With warm caches ([run 40461627894](https://github.com/Vibin-LLC-2025/ezmark/actions/runs/40461627894),
same images, tauri-cli restored from the cache), the slowest job, macOS
`test`, took 2 min 50 s, and the whole run about 3 minutes.

## After the audit, 2026-11-13

Same author's machine as above. WebView2 runtime 157.0.4341.57.

### Release binary

| Profile | Clean build | `ezmark.exe` |
|---|---|---|
| Cargo defaults | 100 s | 10,766,336 bytes |
| `strip = true` (chosen) | 97 s | 10,761,728 bytes |
| `strip`, `lto = "thin"` | 89 s | 11,020,288 bytes |
| `strip`, `lto = true`, `codegen-units = 1` | 167 s | 9,584,640 bytes |

Builds ran one after another while other work used the machine, so read the
times to within about 10%. Stripping changes almost nothing on Windows, where
symbols already live in a separate file; it is on because it is free and
matters on macOS. Full link-time optimisation saves 11% for a 67% longer first
build, and the first build is what a reader waits through, so it is off.

### Nesting depth and render time

Release build, parse then sanitize, before the nesting guard existed:

| Input | Size | Parse | Sanitize |
|---|---|---|---|
| 500 unclosed `<div>` | 3 KB | 0.2 ms | 1.6 ms |
| 2,000 unclosed `<div>` | 12 KB | 0.1 ms | 23 ms |
| 5,000 unclosed `<div>` | 30 KB | 0.4 ms | 127 ms |
| 20,000 unclosed `<div>` | 120 KB | 14 ms | 2,768 ms |
| 20,000 `>` on one line | 20 KB | 348 ms | 2,550 ms |
| 20,000 unclosed `<b>` | 60 KB | 10 ms | 26 ms |
| 50,000 sibling `<div>a</div>` | 650 KB | 4 ms | 113 ms |
| Ordinary mixed markdown | 1 MB | | 190 ms in total |
| Ordinary mixed markdown | 10 MB | | 1,960 ms in total |

The cost follows depth, not size, and grows with its square. The guard sends
anything deeper than 512 to a plain-text view.

### What reaches the network

Two kinds of measurement, answering different questions.

**What a page can make the webview do.** The harness injects markup past the
sanitizer and counts connections at a loopback listener
(`crates/app/tests/harness.rs`). With the network lock switched off:

| Injected | Reached the listener |
|---|---|
| 18 kinds of resource load, 8 kinds of script-driven request | Nothing. 27 enforced CSP reports |
| `<link rel="preconnect">` | A TCP connection, no request. No CSP directive applies |
| `<iframe src>` | A TCP connection, no request, although `frame-src` refused the load and reported it |
| `<meta http-equiv="refresh">`, `location = ...` | The full `GET`, although the Rust guard denied the navigation and the page stayed |

With the network lock on, as shipped: nothing reaches the listener in any of
these.

The same tests on WKWebView (`macos-15` runner, CI, 2026-11-13): the resource
loads and script-driven requests reach nothing, as on Windows. A denied
navigation sends no request. The preconnect hints and the frame open two
connections. macOS has no lock; a dead proxy on the webview's data store was
tried and gave one connection in one run and two in the next. Before the right-click menu was suppressed on links, choosing "Save
link as" in the production build opened two connections to a listener.

**What the process tree does on its own.** `scripts/netwatch-windows.ps1`
launches the production binary with a fresh webview profile and records every
socket owned by `ezmark.exe` and the `msedgewebview2.exe` processes under it
(sampled about every 1.3 ms, worst gap 33 ms), together with the webview's own
network log, which has no gaps.

| | Before the lock (commit `62d419d`) | With the lock |
|---|---|---|
| `ezmark.exe` | No socket in any run | No socket in any run |
| Webview network stack | Two requests to `config.edge.skype.com` at start; a `wpad` proxy lookup; a component-update call to `edge.microsoft.com` after 60 s; three connection attempts each to `[::1]:80` and `127.0.0.1:80` | Nothing: no connection attempt, lookup, or URL, loopback included (30 s on the demo document, 90 s idle) |
| Webview browser process, outside its network stack | Two connections to a Microsoft address on port 443, in 18 of 22 runs | The same two connections, in 1 of 2 runs |

That last row is the part ezmark cannot switch off. On a machine signed in to
Windows with a Microsoft account, the WebView2 runtime fetches that account's
profile through Windows' own sign-in library, not through the webview's
network stack, so neither the CSP nor the lock nor any browser argument tried
(fifteen of them) reaches it. The destination is inferred from the address
range and from the library's contents, not observed. It is unrelated to the
document: it happens with no file open. It also means the runtime writes the
account's name into the webview profile folder under
`%LOCALAPPDATA%\io.github.vibin-llc-2025.ezmark`.

What this cannot see: a connection from `ezmark.exe` shorter than the sampling
gap, traffic made on the app's behalf by system services outside the process
tree, anything on a timer longer than 90 seconds, and the process tree on macOS. An
elevated packet capture would close the first three.

### A short fuzz of the renderer

Not the scheduled fuzzing the plan puts after M3: a one-off, 150 seconds on
one core. The security fixtures, the demo document, and the README were
mutated at random (tag fragments, quotes, entities, markers, deletions, and
duplications spliced in) and each result rendered and checked. 565,968
documents: no panic; no element outside the allowlist; no attribute outside
it; every `src` an asset URL or a blocked marker; every `id` and `name`
prefixed; every `href` a fragment, an allowed scheme, or a relative path;
every `<input>` a disabled checkbox. The slowest render took 10.5 ms.

## A real Mac, 2026-11-13

MacBook Air (Mac15,13), Apple M3 (8 cores), 8 GB RAM, macOS 26.6.2, Xcode
installed. Commit `3e751d6`. Not a clean machine: rustup was present with an
older stable toolchain, but `CARGO_HOME` did not exist, so every crate was
downloaded and compiled. Other applications were open throughout.

| What | Result |
|---|---|
| `rustup toolchain install` for the pinned 1.99.1, minimal profile | 463 MB on disk |
| First `cargo build --release --locked` (266 crates downloaded, 249 compiled) | 55 s |
| Disk after that build | `CARGO_HOME` 214 MB, `target/` 887 MB |
| Release binary `ezmark` (production features, stripped) | 8,347,968 bytes |
| `cargo install tauri-cli --version 2.12.4 --locked` (optional second step) | 130 s |
| `cargo tauri build --debug --bundles app --features test-harness` | 37 s; `ezmark.app` 43 MB (debug) |
| Disk after all of the above and the two cold-open tests | `CARGO_HOME` 512 MB, `target/` 2.4 GB |

This is the first run on a macOS newer than the `macos-15` runner. What was
checked by hand, and the one test that failed (`finder_open`), is in
[m0-evidence.md](m0-evidence.md). No network measurement was made here.

## M1 engine and live reload, 2026-11-14

Same author's machine as above, release build.

### Release binary

| Build | `ezmark.exe` |
|---|---|
| After the audit (2026-11-13) | 10,761,728 bytes |
| With syntect (grammars compiled in) and notify | 13,318,656 bytes |

The 2.6 MB is almost all the grammar set. The clean first-build time was not
measured again; it is due before the README's number is next quoted.

### Render time, whole pipeline

`cargo run --release -p mdcore --example timing`. Every figure includes
parsing, block markers, highlighting, and sanitizing.

| Input | Size in | HTML out | Time |
|---|---|---|---|
| First highlighted render (loads the grammars) | | | 20 ms |
| Ordinary mixed markdown | 50 KB | 275 KB | 41 ms |
| Ordinary mixed markdown | 1 MB | 5.6 MB | 828 ms |
| Ordinary mixed markdown | 10 MB | 51 MB | 4,133 ms |
| One 3,500-line Rust block, inside the highlight budget | 252 KB | 1.1 MB | 685 ms |
| One 10,000-line Rust block, over the budget, shown plain | 722 KB | 722 KB | 4 ms |
| 20,000 nested quotes (plain-text fallback) | 19 KB | 78 KB | 5 ms |
| 50,000 unclosed `<div>` (plain-text fallback) | 292 KB | 585 KB | 37 ms |
| 100,000 unclosed emphasis markers | 292 KB | 293 KB | 30 ms |
| 50,000 unclosed links | 195 KB | 195 KB | 34 ms |

The "ordinary mixed" fixture is one short section repeated, with a heading, a
table, a list, and a small Rust block in each, so it has far more blocks per
kilobyte than real prose: the 1 MB file holds about 33,000 top-level blocks
and 6,500 code blocks.

What M1 cost: the same 1 MB fixture took 190 ms before highlighting and block
markers, and the 10 MB one 1,960 ms. Highlighting runs at about 370 KB of code
a second and is capped at 256 KB a document, so it adds at most about 0.7 s.
The rest is the marker before each block (52 bytes, and one more element for
the sanitizer to parse), which matters only at this block density.

### What the stock highlighter output would have cost

The same 433 KB Rust block through syntect's own class writer, before it was
replaced: 11.5 MB of HTML and 1,421 ms. Through the flat writer: 1.9 MB and
1,199 ms. The time is the grammar's regular expressions either way; the
saving is in what the sanitizer and the webview then have to parse.
