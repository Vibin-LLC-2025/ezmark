# Feasibility report: a lightweight cross-platform markdown viewer

Written 2026-10-26. Synthesized from five research workstreams run by opus agents at medium effort (see `research/01` through `research/05` for evidence and source URLs). This is an assessment, not a build plan; nothing has been scaffolded.

## 1. Verdict

**Go, as a portfolio project. No-go as a product bet.**

The premise "there is no good cross-platform markdown viewer" is not supported by the evidence. MDHero (MIT, Tauri, about 8 MB, Windows + macOS + Linux, viewer-first, file association, live reload, 301 stars, last push 2026-10-24) already covers most of the use case, Tinta covers Windows with a 1 MB native exe, and Windows 11 Notepad now renders basic markdown out of the box. At least 18 "markdown viewer" launches hit Hacker News in 2026, most with under 10 points. The market is crowded and low-traction.

What is still open is narrow but real, and it maps directly onto what the complaint research found. No single tool does all of the following:

1. Strictly read-only (MDHero's issue queue is pulling it toward an editor).
2. Signed and notarized builds on both platforms, including Intel Macs (MDHero: unsigned Windows, no Intel build).
3. GitHub-faithful rendering: alerts, footnotes, task lists, heading anchors (VS Code still lacks native alerts; MDHero lacks footnotes).
4. Scroll position preserved across live reload (the single most-cited complaint across VS Code, MacDown, GNOME Builder, Geany, Cursor).
5. Raw HTML sanitized, no scripts, strict CSP (Markdown Preview Enhanced had four code-execution CVEs in 2026; Typora had an RCE via crafted .md).
6. Relative links and images that resolve correctly however the file was opened.
7. A codebase small enough to read in an afternoon.

That list is the product. It is also exactly the kind of thing a hiring manager can verify by reading the repo, which is why the portfolio case holds even though the market case is weak. The README should compare itself honestly to MDHero and Tinta and say which of the seven it does better.

## 2. Recommended stack

**Tauri 2 (Rust core) + comrak + ammonia + syntect, with a dependency-free HTML/CSS/TypeScript view layer. No frontend framework.**

| Layer | Choice | Why |
|---|---|---|
| Shell | Tauri 2.12.x | Single-digit MB Windows installer, about 57 MB macOS bundle vs 323 MB for Electron. Official plugins for window state, single instance, file associations, updater. Rust core is a strong portfolio signal. |
| Parser | comrak 0.59 | Port of cmark-gfm, the parser GitHub's pipeline is built on. Passes 652/652 CommonMark and 670/670 GFM spec tests. Every needed extension in one crate, plus link and image URL rewriter hooks. pulldown-cmark is about 1.9x faster but parse time is not the bottleneck. |
| Sanitizer | ammonia 4.2 | html5ever-based allowlist in Rust. Needs an extended allowlist (checkbox inputs, picture/source, heading ids, a fixed class set). |
| Highlighting | syntect, class-based output | Done in Rust at render time, so no JS highlighter on the startup path and no flash. |
| DOM update | idiomorph (about 3.7 kB) | Full re-parse on change, then morph the DOM in place. This is what preserves scroll position, open details blocks, and rendered diagrams. |
| File watch | notify 8 + notify-debouncer-full | Watch the parent directory, not the file, because editors save via temp file plus rename. Debounce about 75 ms, hash content before re-render. |
| Math, diagrams | KaTeX and Mermaid, lazy-loaded only when a file uses them | Mermaid is about 170 kB gzip for the entry alone and must never be on the startup path. |

**Why not the alternatives.** Electron removes the WebKit-vs-Chromium question but fails "lightweight" on disk and weakens the pitch. Wails v3.0.0 shipped on 2026-10-26 with two security advisories its own notes call partly fixed. Pure Rust GUI (iced, egui) means rebuilding text selection, find-in-page, tables, and print that a webview gives for free. Native per platform doubles the UI codebase for a solo developer. A local server plus browser tab has no app identity and a weak portfolio signal.

**Honest framing of Tauri's numbers.** Its measurable win over Electron is disk size, not RAM or startup. A 2026 macOS benchmark put Tauri at 311 ms cold start and 109 MB idle vs Electron at 273 ms and 128 MB. Do not claim "10x faster" in the README; claim small, auditable, and secure.

## 3. The decisive technical risk and its mitigation

**Rendering parity between WebView2 (Windows, evergreen Chromium) and WKWebView (macOS, WebKit frozen to the OS version).**

The evidence shows this is real but bounded for a static document viewer. Nothing found blocks document rendering. Specifics:

- Tauri's default macOS floor is 10.13, which maps to Safari 11. Raise `minimumSystemVersion` to 13.0 (Safari 16.1 baseline) and write CSS against that baseline.
- Relative images need a scoped custom asset protocol; the URL form differs per platform and is hidden behind Tauri's `convertFileSrc`. Solve once in Rust, test on both.
- Printing from an iframe silently fails on macOS (Tauri #13451, upstream WebKit, open). Use top-level `window.print()`, which works on both.
- Font rendering differs between CoreText and DirectWrite. This is true in Electron too. Accept it as native look.
- WKWebView is capped at 60 fps on ProMotion displays. Not a functional break for a reader.
- Run screenshot comparison tests on both OS runners in CI from the first week.

## 4. v1 scope

Ships in v1:

- GFM rendering: tables, task lists, strikethrough, autolinks, footnotes, GitHub alerts, heading anchors with GitHub's slug rules, emoji shortcodes, front matter collapsed into a metadata block, sanitized raw HTML (details, kbd, picture, centered images).
- Syntax highlighting with GitHub Light and Dark themes paired to the UI theme.
- KaTeX and Mermaid, lazy-loaded only when present. The engine research puts these in v1 and the UX research puts them in "later, early". Tiebreak: v1, because AI-generated docs and READMEs use Mermaid often enough that a viewer without it reads as broken, and lazy loading keeps them off the startup path. If v1 scope slips, these are the first two items to cut.
- Reading typography: prose column capped at 68ch (about 75–80 characters), 17px system font, line-height 1.6, GitHub's heading scale. Tables, code, and images may break out to 120ch. Narrow / Default / Wide / Full width options.
- Themes: follow system (default), light, dark. Support `forced-colors` and `prefers-contrast` from day one.
- Navigation: outline sidebar, find in page, zoom, per-file scroll position memory, keyboard scrolling (Space, j/k, g/G), back/forward when following links between .md files.
- Live reload that keeps scroll position, a small "updated" indicator, a banner if the file is deleted or renamed.
- Native title bar, window size and position remembered, one window per document, focus the existing window if the same file is opened again.
- Actions: open in editor, reveal in Explorer/Finder, copy button on code blocks, print with print CSS (PDF via the OS dialog).
- OS: file association and Open With on both platforms, drag and drop, a CLI launcher, single instance.
- Security: strict CSP, no scripts, all link clicks intercepted, `file://` and UNC paths always blocked, path resolution confined to the file's folder tree or repo root.
- Signed and notarized releases for macOS (universal), signed NSIS installer for Windows.

Explicitly not in v1:

- Editing, split view, plugins, sync. Never.
- Tabs, sepia theme, bundled reading fonts, file tree, quick-open. Later.
- macOS Quick Look extension. First post-v1 item on macOS. It is a separate signed Swift app extension, sandboxed, and the thing that breaks most often after OS updates (QLMarkdown broke on macOS 15 and again on macOS 27).
- Windows Explorer preview handler. PowerToys already provides one, and it is a frequent source of "Catastrophic failure" reports. Later at best.
- DOCX, EPUB, Pandoc. Never.
- Telemetry. Never. "No network calls except the optional update check" is a selling point that is verifiable in an open repo.

## 5. Differentiation, derived from the complaints

| Complaint cluster (ranked by evidence) | What this app does about it |
|---|---|
| Scroll position lost on reload | Full re-parse plus idiomorph DOM morph; stable heading ids as anchors. Make this a headline README claim with a GIF. |
| OS previewers break after OS updates | Do not ship Quick Look or a preview handler in v1. Be a plain app that keeps working. |
| "I just want to read it" | Strictly read-only. No edit mode, ever. State it in the first README line. |
| Rendering does not match GitHub | comrak, the cmark-gfm port, plus a nightly parity test against GitHub's own markdown API over a README corpus. |
| Abandonment | Tiny scope, so "done and maintained" is credible. Automate releases so maintenance is cheap. |
| Preview executes code | Sanitizer plus CSP plus navigation control. Ship a SECURITY.md and an XSS fixture suite in CI. |
| Broken relative links and images | Resolve against the file's folder or repo root, scoped asset protocol, tested on both platforms with files opened by double-click, CLI, and drag-drop. |

## 6. Distribution: cost and reality

| Path | Yearly cost | What you get |
|---|---|---|
| Minimum acceptable | about $99 | Apple Developer Program for notarization. Windows via free Microsoft Store MSIX (zero SmartScreen warning) or free SignPath Foundation signing (publisher shows as "SignPath Foundation"). |
| Polished | about $219 | Above plus Azure Artifact Signing at about $9.99/month, so the Windows publisher name is yours. Individuals qualify only in the US or Canada. Elsewhere, Certum's open-source cloud certificate at EUR 49. |

Facts that change the plan:

- An unsigned macOS app on Sequoia shows "Move to Trash / Done" with no Open button. The README note "right-click, Open" has been wrong since 2024. Budget the $99.
- An unsigned Windows installer is not an option: Smart App Control blocks it outright.
- Paying for Windows signing does not remove the SmartScreen warning. Microsoft says reputation takes "several weeks and hundreds of clean installs." Sign every release with one identity from the first beta, publish the Store MSIX, and explain the prompt in the README.
- Homebrew's official cask repo needs 225 stars for a self-submission. Use a personal tap.
- Apple's first notarization can sit "In Progress" for 24 to 48 hours. Do it in week 1, not launch week.
- GitHub Actions, Pages, Dependabot, CodeQL, attestations, winget, and the Microsoft Store individual account are all free for a public repo.

## 7. Repo hygiene that matters to a hiring manager

In priority order, from the distribution research: README with hero GIF and per-platform install; clean small-commit history with Conventional Commits; tests plus CI badge; lint and format enforced in CI; a deliberately chosen license (MIT, or "MIT OR Apache-2.0" as Tauri uses); signed releases with a CHANGELOG; ARCHITECTURE.md; ADRs for the stack, the engine, and the no-telemetry decision; SECURITY.md with private vulnerability reporting; CONTRIBUTING and issue templates.

Three ADRs are worth writing before the first line of code, because they are the decisions a reviewer will ask about: Tauri over Electron, comrak over pulldown-cmark, and no telemetry.

## 8. Timeline

Solo, part time at about 10 to 15 hours a week, from the distribution research:

| Weeks | Work |
|---|---|
| 1 | Repo skeleton, license, CI on both OSes, ADRs. Start Apple enrollment and Windows signing applications now; lead times are unpredictable. |
| 2 to 6 | Core viewer: parse, sanitize, render, watch, morph, typography, themes, navigation. |
| 7 to 8 | Signing and notarization in CI, release-please, attestations. First notarization early. |
| 9 to 10 | Updater, Homebrew tap, winget manifest, Store MSIX. |
| 11 to 12 | Docs, GIFs, landing page, beta testers installing from the real download URL. |
| 13 to 16 | Buffer for signing and antivirus surprises. v1.0. Show HN. |

Distribution work alone is 40 to 60 hours, much of it waiting on third parties.

## 9. Top risks

1. **Market saturation.** Mitigation: position as a portfolio piece with a precise comparison to MDHero and Tinta; do not expect traction.
2. **WebKit drift on macOS.** Mitigation: macOS 13 floor, CSS baseline, screenshot tests on both runners.
3. **Windows first-run trust.** Mitigation: one signing identity from the first beta, Store MSIX, README explanation.
4. **Scope creep toward an editor.** This is what happened to MDHero. Mitigation: write "never" items into the README and close those issues with a link to it.
5. **Tauri 3 migration.** 3.0.0-alpha.5 exists as of 2026-10-14. Mitigation: keep the Tauri surface small (plugins and config, little custom glue).
6. **Pathological inputs.** comrak had a quadratic-parse CVE (fixed in 0.17). Mitigation: parse off the UI thread with cancellation, fuzz the pipeline in CI, large-file mode above about 2 MB.

## 10. Open decisions for you

1. **Remote images by default.** The engine research recommends blocking them (privacy, VS Code "Strict" model). The UX research recommends loading them (GitHub model; blocking breaks every README with shields.io badges). Recommendation: load `https` images by default, always block `http`, `file://`, and UNC, and offer a one-click privacy mode. This is a judgment call on your product values.
2. **Intel Mac support.** A universal build costs only CI minutes and is a stated gap in MDHero. Recommendation: yes.
3. **App name.** Not researched. Check winget, Homebrew, crates.io, and the Microsoft Store for collisions before committing.
4. **Windows signing path.** Depends on whether you are a US or Canadian resident (Azure Artifact Signing eligibility).

## 11. Evidence quality notes

- Reddit searches returned no usable threads, and Mac App Store and Microsoft Store reviews were not reachable. Complaint ranking rests on GitHub issues, forums, and Hacker News.
- Tauri and Electron performance numbers come from a single 2026 third-party benchmark and from CI runners. Treat them as directional.
- Azure Artifact Signing's pricing page showed placeholders on 2026-10-26; the $9.99 figure comes from Microsoft Learn dated 2026-08-29.
- "Inlet", named in the original brief, could not be found.
