# 02: Application stack feasibility for a cross-platform markdown preview tool

Researched: 2026-10-26. Scope: Windows 10/11 and macOS only (Linux out of scope). Read-only viewer, not an editor.

## Summary and ranked recommendation

1. **Tauri 2 (Rust core + system webview).** Best fit: tiny installer, a Rust core that parses markdown (comrak or pulldown-cmark) and makes for a strong portfolio repo, and official plugins and config for every desktop chore this app needs (window-state and single-instance plugins, `bundle.fileAssociations`).
2. **Electron 45.** The parity-safe fallback: identical Chromium on both OSes removes the whole WebKit-vs-Chromium question, but a roughly 100 to 320 MB bundle fails the "lightweight" requirement and the portfolio signal is weaker.
3. **Wails v3 (Go + system webview).** Same architecture and parity profile as Tauri with a simpler language. However, v3.0.0 shipped on 2026-10-26 (the day of this research), with two security advisories its own release notes call only partly fixed, so it ranks below Tauri on maturity.
4. Pure Rust GUI (iced 0.14): smallest and most "native" option, but markdown fidelity (tables, syntax highlighting, text selection, find-in-page, print) means rebuilding what a webview gives for free.
5. Native per platform, Flutter, and the browser + local server model trail for codebase-size, maturity, or UX reasons (see risks).

**Honest framing:** Tauri's measurable win over Electron is disk size (57 MB vs 323 MB on macOS in a 2026 benchmark; single-digit MB installers on Windows), not RAM or startup, where 2026 measurements are close (Tauri 311 ms / 109 MB vs Electron 273 ms / 128 MB). **Biggest risk for Tauri:** macOS WebKit is frozen to the user's OS version while Windows always gets current Chromium. This is mitigated by raising `minimumSystemVersion`, pinning a CSS baseline, and running screenshot tests on both OSes in CI.

## Current versions (as of 2026-10-26)

| Project | Current stable | Date | Notes |
|---|---|---|---|
| Tauri (core crate) | 2.12.5 (CLI 2.12.4) | 2026-10-21 | 3.0.0-alpha.5 published 2026-10-14, so a major migration is on the horizon |
| Electron | 45.0.0 | 2026-10-20 | 44.x and 43.x also supported |
| Wails | 3.0.0 | 2026-10-26 | First non-prerelease v3; v2.17.0 (2026-10-18) still gets critical fixes; requires Go 1.26+ |
| iced | 0.14.0 | 2025-12-07/08 | Markdown widget with images, quotes, task lists; text selection slated for 0.15 |
| Blitz (Dioxus native HTML renderer) | 0.3.0-alpha.4 | 2026-05-17 | README: "pre-alpha", maintainers do not recommend building apps on it yet |
| flutter_markdown | discontinued (2025) | n/a | Continued as community-maintained `flutter_markdown_plus` (Foresight Mobile) |

## Comparison table

Legend: **M** = measured by a cited source; **R** = reasoned estimate, not measured. Numbers are for a small app, not this specific app.

| Dimension | Tauri 2 | Electron 45 | Wails v3 | Native x2 (SwiftUI/AppKit + WinUI 3/WPF, shared Rust core) | Flutter desktop | Pure Rust GUI (iced / egui) | Local server + default browser |
|---|---|---|---|---|---|---|---|
| Installer / bundle size | macOS bundle 57 MB (M, betterstack 2026, includes app content); Windows installer about 2.5 MB for a hello world (M, levminer 2022) | macOS 323 MB (M, betterstack 2026); hello world installer 40 to 115 MB (anecdotal) | Similar to Tauri, single-digit to low tens of MB (R; only vendor claims found) | Smallest per platform: a few MB (R); WinUI 3 self-contained is "significantly" larger per Microsoft | about 10 to 20 MB (R, unsourced feature table) | about 5 to 15 MB static binary (R) | Smallest binary (R), but zero app shell |
| Cold start | 311 ms median (M, macOS, 2026) | 273 ms median (M, macOS, 2026) | Similar to Tauri (R) | Fastest (R) | Fast, engine init cost (R) | Fast, GPU init cost (R) | Server instant; browser tab open dominates (R) |
| Idle memory | 109 MB including webview helpers (main process about 36 MB) (M, 2026) | 128 MB (M, 2026) | Similar to Tauri (R) | Lowest (R) | Moderate (R) | Lowest to moderate (R) | Shares the user's browser (R) |
| Runtime dependency | WebView2 Evergreen (preinstalled on all Windows 11, "vast majority" of Windows 10; Tauri installer bootstraps it); WKWebView ships with macOS | None (bundled Chromium) | Same as Tauri | Windows App SDK runtime (bundle or self-contained) | None | None | A browser; grip also needs the GitHub API online |
| Rendering engine parity | Chromium (Win) vs WebKit frozen to the OS version (Mac): **the core risk** | Identical Chromium both sides (text rasterization still OS-specific) | Same risk as Tauri | Two separate renderers: you own parity entirely | Identical Skia/Impeller both sides | Identical renderer both sides | Whatever browser the user has |
| Two-OS GitHub Actions CI | Low to moderate: official `tauri-action`, documented macOS signing/notarization env vars | Low to moderate: electron-builder/Forge, same Apple signing work | Moderate: newer tooling, smaller CI ecosystem (R) | High: Xcode + MSBuild pipelines, two test suites | Moderate | Low: `cargo build` per target, own packaging (e.g. cargo-bundle / cargo-packager) | Very low |
| Maturity / maintenance | High: 2.x stable since 2024, frequent patch releases | Very high: 8-week major cadence | v3 released 2026-10-26; v2 mature | Platform SDKs very mature; WinUI 3 has a mixed reputation | Mature framework, markdown package changed hands | iced/egui active but pre-1.0 APIs break every release | grip mature but narrow |
| Codebase size / cleanliness | Small: Rust crate for parse + file watch, one HTML/CSS/TS view | Small but carries Node/JS toolchain weight | Small: Go + HTML view | Largest: two UIs, FFI bridge | Medium: Dart widget tree | Small to medium, but you write renderer pieces (tables, code highlighting) | Smallest |
| Learning curve | Rust + web | JS only | Go + web | Swift, C#/C++, Rust FFI | Dart | Rust + immediate/Elm-style UI | Trivial |
| Portfolio signal | Strong: modern Rust, perf numbers in README, "lightweight" claim backed by data | Neutral to negative for a tool pitched as "lightweight" | Good, slightly less recognized than Tauri | Very strong craft signal, but doubles scope | Neutral for desktop | Strong for systems roles, risk of looking unfinished | Weak: reads as a script, not an app |

## WebView2 vs WKWebView rendering parity (the decisive question)

### What a markdown viewer actually exercises

A read-only viewer uses a narrow slice of the web platform: block text, headings, lists, tables, code blocks with syntax highlighting, inline images (often relative to the .md file), links, `prefers-color-scheme`, scrolling, find-in-page, and possibly print / export to PDF. It does not need WebRTC, the Popover API, or heavy JS frameworks. Most of the "webview pain" reports below come from complex apps, so their severity is lower for this app. Each item below is scoped to what this viewer would hit.

### Evidence, by feature

| Feature | Evidence | Severity for this app |
|---|---|---|
| **Engine version drift (macOS)** | [Tauri webview versions](https://v2.tauri.app/reference/webview-versions/): on macOS WebKit "is considered a core component and is therefore updated with the regular OS updates", and unsupported macOS versions stop receiving WebKit updates. Windows: "WebView2 can update itself, you are guaranteed a relatively recent chromium build". Tauri's default `bundle.macOS.minimumSystemVersion` is **10.13** ([config reference](https://v2.tauri.app/reference/config/)). Per the Tauri versions table, macOS 10.13 shipped Safari 11.0 to 11.1.2 (WebKit 604 to 605); macOS 13.0 shipped Safari 16.1 (WebKit 614); macOS 14.0 shipped Safari 17.0 (WebKit 616). (Apple also shipped standalone Safari updates to older macOS, so the real floor depends on the user; not sourced in detail here.) | **High** (root cause of most other parity issues). Mitigation: raise `minimumSystemVersion` (e.g. to 13.0, giving a Safari 16.1 baseline) and write CSS against that baseline (check caniuse). |
| **General cross-webview inconsistency** | [Hacker News thread, item 44118023 (2025)](https://news.ycombinator.com/item?id=44118023): user yojo moved from Tauri to Electron "because of frustration with rendering differences between the web views employed on different platforms"; cvburgess switched from Tauri 2.0 to Electron after a month citing "UI inconsistencies", Safari lagging on the Popover API, and a scattered build/codesign/CD story; Sytten: "a PITA to deal with user bugs in a specific macos version with a 8y out of date webview". CommonGuy credits Windows with getting its WebView2 implementation right (the only WebView2-specific comment, and a positive one). | Medium. The reports concern rich interactive apps; a static document view is far less exposed. |
| **Print / export to PDF** | [Tauri issue #13451](https://github.com/tauri-apps/tauri/issues/13451) (open, labeled `platform: macOS`, `status: upstream`): `iframe.contentWindow.print()` silently does nothing on macOS, while top-level `window.print()` works; Windows works in both cases. Maintainer (2026-03-12): the only real fix found relies on a private WebKit API, unacceptable for the App Store. No official Tauri print plugin (plugins-workspace #293 not actively worked on). Stirling-PDF worked around it by linking PDFKit from Rust (PR #5920, 2026-03-16). WebView2 exposes [`PrintToPdfAsync`](https://learn.microsoft.com/zh-tw/dotnet/api/microsoft.web.webview2.core.corewebview2.printtopdfasync) natively; Tauri exposes no JS print-to-PDF API on either platform (WKWebView's native PDF APIs not sourced in this research). | Low to medium. Use top-level `window.print()` (works on both). A native "Export PDF" button needs per-platform Rust code. |
| **Local file / image access** | WKWebView only reads `file://` content within the directory granted by `loadFileURL(_:allowingReadAccessTo:)` ([Apple DTS](https://developer.apple.com/forums/thread/768022)). Tauri sidesteps this with its asset protocol, but the URL form differs per platform: `https://asset.localhost/...` on Windows vs `asset://...` on macOS, hidden behind `convertFileSrc`. The asset protocol is off by default (`enable: false`, `scope: []`) and must be scoped; CSP must allow the asset origin. See the [asset protocol docs](https://v2.tauri.app/security/asset-protocol/); community reports of 403s when the scope does not match the resolved path ([discussion #11498](https://github.com/orgs/tauri-apps/discussions/11498)). | Medium. Relative images in a markdown file are core functionality. Solve once in Rust: rewrite image `src` to `convertFileSrc` URLs, or serve them through a custom URI scheme handler scoped to the open document's folder. |
| **Font rendering** | No reproducible Tauri or wry issue found for macOS vs Windows text differences ([wry issue search](https://github.com/tauri-apps/wry/issues?q=font+rendering) turned up only a Linux/Wayland blur bug, #1727). The difference is engine and OS level: CoreText on macOS vs DirectWrite on Windows, and `system-ui` resolves to San Francisco vs Segoe UI. `-webkit-font-smoothing` only works on macOS ([MDN](https://developer.mozilla.org/en/docs/Web/CSS/-webkit-font-smoothing)); see also [WebKit bug 14921](https://bugs.webkit.org/show_bug.cgi?id=14921) on rasterizer differences. **This applies to Electron too**: Chromium also rasterizes text via the platform font stack, so Electron does not give pixel-identical text either. | Low. Expected and acceptable (native look). Use a deliberate font stack; optionally bundle one webfont for code blocks for consistent metrics. |
| **Scrolling / frame rate** | WKWebView is capped at 60 fps on macOS ProMotion displays; Tauri apps cannot reach 120 Hz ([Apple forum thread 796512](https://developer.apple.com/forums/thread/796512), [WebKit bug 294338](https://bugs.webkit.org/show_bug.cgi?id=294338), open feature request). Safari's 120 Hz flag does not apply to embedded WKWebView. An [Apple forum thread](https://developer.apple.com/forums/thread/772937) reports scrolling behaviour changed on macOS 14. No Tauri issue found on rubber-band/overscroll for document scrolling. | Low. A viewer scrolls static content; 60 fps vs 120 fps is noticeable to some Mac users but not a functional break. |
| **CSS features** | Feature gaps are entirely a function of the macOS floor (see drift row). Typical markdown CSS (tables, custom properties, `prefers-color-scheme`) is long supported in WebKit; newer selectors such as `:has` need a recent Safari (verify exact versions on caniuse). | Low, once the floor is raised. |
| **Startup flash** | [Microsoft WebView2 performance guidance](https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance): creating WebView2 and first navigation is a common cold-start bottleneck; recommends lightweight splash UI. White-flash avoidance requires a matching background color, which can add up to about 200 ms perceived delay ([Phoenix desktop PR #593](https://github.com/phcode-dev/phoenix-desktop/pull/593)). | Low to medium. Keep the window hidden until first paint and set a background color matching the theme. |

### Bottom line on parity

For a markdown viewer, the parity risk is real but bounded and mostly front-loaded: choose a sane macOS floor, keep CSS to a well-supported subset, handle images in Rust, and add a screenshot comparison test on both OS runners. None of the evidence shows a blocker for static document rendering. The one confirmed hard gap (iframe print) is avoidable by design. Electron removes engine drift but not font differences, at roughly 5x to 100x the disk cost.

## Known Tauri pain points and their 2.x status (2026-10-26)

| Pain point | Status in Tauri 2.12.x |
|---|---|
| Asset protocol (local images) | Works; off by default; needs explicit `scope` and CSP entry; platform URL difference hidden by `convertFileSrc`. Solved but requires care. |
| CSP | `security.csp` "will be injected on all HTML files on the built application" (plus `devCsp`). Solved; a strict CSP is a portfolio plus. |
| File drag and drop | Tauri replaces WebView2's drop handler; official docs: disabling `dragDropEnabled` "is required to use HTML5 drag and drop on the frontend on Windows". For a viewer, Tauri's own `DragDropEvent` (gives native file paths) is actually the better API, so leave it enabled. Solved. |
| Window state | Official `window-state` plugin (Rust 1.90+). Solved. |
| Double-click a .md file | `bundle.fileAssociations` registers the type. macOS delivers files via `RunEvent::Opened` (cold start needs buffering in managed state until the frontend is ready); Windows delivers via argv, and the official `single-instance` plugin forwards a second launch's args and cwd to the running instance. Works, but two code paths. |
| macOS signing in CI | Documented: `APPLE_CERTIFICATE` (base64 .p12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, and App Store Connect API key vars for notarization, passed to `tauri-action`. Requires a paid Apple Developer Program membership. Same cost for every stack. Solved but not free. |
| Windows signing | Not researched in depth here; applies equally to all stacks (SmartScreen reputation). |
| Print | Top-level print works; iframe print broken on macOS (#13451, upstream, open). Not solved. |
| WebView2 missing | Default `webviewInstallMode` is `downloadBootstrapper` (silent); `offlineInstaller` and `fixedRuntime` exist for offline use. Removed-runtime edge case documented in issue #12030. Solved. |
| Major-version churn | 3.0.0-alpha.5 exists (2026-10-14). Expect a 2.x to 3.x migration within the project's lifetime. |

## Per-candidate risks for this specific app

### Tauri 2
- **Lead risk: macOS WebKit frozen to the OS version while Windows is evergreen Chromium.** Default floor 10.13 is far too old; must be raised and tested.
- Relative image loading needs deliberate design (asset scope or custom protocol), and is the most likely source of "works on Windows, broken on Mac" bugs.
- Two file-open code paths (macOS `RunEvent::Opened`, Windows argv + single-instance).
- No first-class print-to-PDF API; native code needed per OS if that feature matters.
- Tauri 3 is in alpha; plan for a migration.
- Rust compile times make CI slower (mitigate with `Swatinem/rust-cache`).

### Electron 45
- Fails "lightweight" on disk (hundreds of MB bundle), undermining the core pitch and the portfolio story.
- 8-week major cadence: dependency churn and security-update obligations for a hobby project.
- Memory and startup are not dramatically worse than Tauri per 2026 measurements, so the case against it is mainly size and signal.

### Wails v3
- v3.0.0 released 2026-10-26; release notes flag GHSA-2p5f-2gx8-mh8m as only partly fixed and note iframe access to bound methods is unrestricted (low impact for a viewer with no third-party frames, but a review flag).
- Same webview parity risks as Tauri.
- Smaller ecosystem; fewer plugin-grade solutions for file associations, window state, and updater compared with Tauri.

### Native per platform (SwiftUI/AppKit + WinUI 3 or WPF, shared Rust core)
- Doubles the UI codebase and test matrix; contradicts "simple and extremely clean" for a solo developer.
- Markdown display on each side would still most likely use a webview (WKWebView / WebView2) for fidelity, reintroducing the parity problem with more code around it.
- WinUI 3 deployment (Windows App SDK runtime, self-contained size) adds friction; WPF is mature but dated as a portfolio signal.

### Flutter desktop
- The canonical markdown package was discontinued by Google (2025) and now lives in a community fork, a maintenance-risk signal for the core feature.
- Widget-based markdown rendering lacks browser-grade text selection, find, and table layout polish; non-native text feel on macOS.
- Dart is a weaker portfolio signal for desktop/systems roles.

### Pure Rust GUI (iced, egui, gpui, Blitz)
- iced 0.14's markdown widget lacks text selection (planned for 0.15); egui_commonmark supports a GitHub-flavored subset; this research could not confirm an official standalone gpui crate release (only third-party repackagings such as gpui-box); Blitz is pre-alpha by its own README.
- You would rebuild find-in-page, selection, syntax-highlighted code blocks, and print yourself, which is exactly the work a webview removes.
- Accessibility (screen readers) is incomplete in most of these toolkits.
- Upside: identical rendering on both OSes and the best raw performance numbers. A credible "phase 2" or experiment, not the safest v1.

### Local HTTP server + default browser (grip model)
- No app identity: no dock/taskbar icon or window of its own, browser chrome around content, tabs accumulate.
- Double-click UX is achievable (register a tiny CLI as the .md handler that starts or reuses a server and opens a tab) but is a worse product, plus a lingering background process and a localhost port.
- grip specifically renders through the GitHub markdown API (network dependency, rate limits); a local renderer avoids that.
- Weak portfolio signal relative to the brief.

## Sources (all accessed 2026-10-26)

Versions and releases
- Tauri releases: https://github.com/tauri-apps/tauri/releases
- Electron releases: https://releases.electronjs.org/
- Electron lifecycle (secondary): https://endoflife.ai/article-electron-42-eol
- Wails releases: https://github.com/wailsapp/wails/releases
- Wails v3.0.0 release notes: https://github.com/wailsapp/wails/releases/tag/v3.0.0
- iced 0.14.0 release: https://github.com/iced-rs/iced/releases/tag/0.14.0
- iced markdown selection discussion: https://discourse.iced.rs/t/markdown-widgets-text-should-be-selectable/1107
- egui_commonmark: https://github.com/lampsitter/egui_commonmark
- blitz-html on lib.rs: https://lib.rs/crates/blitz-html
- Blitz README: https://cdn.jsdelivr.net/gh/dioxuslabs/blitz@main/README.md
- flutter_markdown_plus: https://pub.dev/documentation/flutter_markdown_plus/latest/
- flutter_markdown handover: https://foresightmobile.com/blog/flutter-markdown-plus-google-handover

Benchmarks
- Better Stack, Tauri vs Electron vs Deno vs Electrobun (updated 2026-07-20): https://betterstack.com/community/guides/scaling-nodejs/tauri-vs-electron-vs-deno-vs-electrobun/
- levminer, Tauri vs Electron (2022-08-22, Windows 11): https://www.levminer.com/blog/tauri-vs-electron
- Electron installer size anecdotes: https://medium.com/gowombat/how-to-reduce-the-size-of-an-electron-app-installer-a2bc88a37732
- Microsoft Windows App SDK 1.6 Native AOT: https://windowsreport.com/microsoft-unveils-windows-app-sdk-1-6-with-native-aot-support-improved-controls-and-more/
- WinUI 3 unpackaged deployment: https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/unpackage-winui-app

Parity evidence
- Tauri webview versions: https://v2.tauri.app/reference/webview-versions/
- Hacker News thread (Tauri vs Electron switching reports): https://news.ycombinator.com/item?id=44118023
- Tauri issue #13451, iframe print on macOS: https://github.com/tauri-apps/tauri/issues/13451
- Tauri asset protocol docs: https://v2.tauri.app/security/asset-protocol/
- Tauri discussion #11498 (asset protocol images): https://github.com/orgs/tauri-apps/discussions/11498
- Apple forum, loadFileURL scope: https://developer.apple.com/forums/thread/768022
- Apple forum, WKWebView 60 fps cap (Tauri report): https://developer.apple.com/forums/thread/796512
- WebKit bug 294338, WKWebView 120 Hz: https://bugs.webkit.org/show_bug.cgi?id=294338
- Apple forum, NSScrollView / WKWebView scrolling on macOS 14: https://developer.apple.com/forums/thread/772937
- wry font rendering issue search: https://github.com/tauri-apps/wry/issues?q=font+rendering
- MDN, -webkit-font-smoothing: https://developer.mozilla.org/en/docs/Web/CSS/-webkit-font-smoothing
- WebKit bug 14921 (rendering library differences): https://bugs.webkit.org/show_bug.cgi?id=14921
- Mac vs Windows font differences: https://mspk.substack.com/p/why-some-fonts-look-different-on
- WebView2 PrintToPdfAsync: https://learn.microsoft.com/zh-tw/dotnet/api/microsoft.web.webview2.core.corewebview2.printtopdfasync

Tauri pain points
- Tauri config reference (minimumSystemVersion, dragDropEnabled, fileAssociations, webviewInstallMode, csp): https://v2.tauri.app/reference/config/
- Single-instance plugin: https://v2.tauri.app/plugin/single-instance/
- Window-state plugin: https://v2.tauri.app/plugin/window-state/
- File associations guide: https://v2.tauri.app/learn/mobile-file-associations/
- macOS code signing: https://v2.tauri.app/distribute/sign/macos/
- Tauri issue #12030 (missing WebView2 runtime): https://github.com/tauri-apps/tauri/issues/12030
- WebView2 distribution: https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution
- WebView2 performance guidance: https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance
- Phoenix desktop PR #593 (background color delay): https://github.com/phcode-dev/phoenix-desktop/pull/593

Browser model
- grip: https://libraries.io/homebrew/grip
