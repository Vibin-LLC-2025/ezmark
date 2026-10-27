# 01: Competitors and complaints for a cross-platform read-only markdown viewer

Researched and written 2026-10-26. All URLs accessed 2026-10-26. GitHub stars and last-push dates pulled from the GitHub API on 2026-10-26.

## Summary

1. The exact use case ("double-click a .md, see a clean read-only render, fast, Windows + macOS, free, open source") is already mostly served by **MDHero** (MIT, Tauri, ~8 MB, Windows/macOS/Linux, viewer-first, file association, live reload, 301 stars, last push 2026-10-24). It is good, not perfect.
2. 2026 has seen a flood of small viewers (at least 18 "markdown viewer" stories on Hacker News in 2026; the search was capped at 20 results, most under 10 points), largely driven by people reading AI-agent output. The space is crowded, low-traction, and fragmented by platform.
3. Windows 11 Notepad now renders basic markdown by default, covering the casual Windows double-click case. macOS still has no built-in rendered preview (Quick Look shows raw text, per user reports such as [hamatti.org](https://notes.hamatti.org/Technology/macOS-preview-rendering-for-Markdown-files); no Apple documentation checked).
4. Strongest complaint clusters by hard evidence: lost scroll position on reload, OS-integrated previewers (Quick Look plugins, PowerToys) breaking after OS updates, "I just want to read, not edit", rendering mismatch vs GitHub, abandonment, and security (code execution via preview).
5. "Electron bloat" is real as a sentiment, but most evidence comes from builders' launch posts, not user complaints. Paywalls are a minor theme (paid tools are cheap one-time purchases).
6. The unoccupied square is narrow: strictly read-only, signed builds on both platforms including Intel Macs, GitHub-faithful rendering, safe-by-default HTML, scroll-preserving live reload, correct relative links, tiny auditable codebase. No single tool checks all of these; MDHero checks most.
7. Coverage caveat: Reddit searches returned zero usable threads; App Store / Microsoft Store reviews were not reachable; "Inlet" could not be found.

## Per-tool table

Maintenance status uses the GitHub last-push date where a public repo exists. "Fewer than 3 found" means I did not find three sourced complaints and did not invent one.

| Tool | What it is | Platform | License / price | Maintenance (as of 2026-10-26) | Top complaints (sourced) |
|---|---|---|---|---|---|
| Typora | WYSIWYG markdown editor | Win, mac, Linux | Proprietary, $14.99 one-time, 3 devices | Active (closed source) | 1. Cannot be made read-only: "may i read-only the markdown article?" open since 2017-05-18, 47 comments ([typora-issues #710](https://github.com/typora/typora-issues/issues/710)). 2. File size cap (~2 MB) and performance ([#1961](https://github.com/typora/typora-issues/issues/1961), [#68](https://github.com/typora/typora-issues/issues/68), [dev.to write-up](https://dev.to/l1u7/fixing-the-file-is-too-large-to-render-in-typora-by-increasing-the-file-size-limit-hnc)). 3. Became paid after a free beta ([dev.to](https://dev.to/evanlin/til-typora-10-and-now-paid-with-useful-resources-5bnf), [G2 reviews](https://www.g2.com/products/typora/reviews)). Also security: CVE-2023-2317 DOM XSS to RCE via a malicious .md ([STAR Labs](https://starlabs.sg/advisories/23/23-2317/)). |
| MarkText | Open-source WYSIWYG editor (Electron) | Win, mac, Linux | MIT, free | Revived: last push 2026-10-27 after a roughly 2022 to 2025 lull | 1. "Is this project still being worked on?" ([#3624](https://github.com/marktext/marktext/issues/3624)); earlier call for maintainers ([#1290](https://github.com/marktext/marktext/issues/1290)). 2. "low Startup Performance" ([#3862](https://github.com/marktext/marktext/issues/3862)). 3. Performance on large docs ([#511](https://github.com/marktext/marktext/issues/511)). Editor, not reader. |
| Obsidian (as reader) | PKM app over a folder "vault" (Electron) | Win, mac, Linux, mobile | Proprietary, free for personal use | Active | 1. Cannot open a single .md outside a vault or be the .md handler; multi-year request thread, users call it a dealbreaker and use symlink workarounds ([forum thread 314](https://forum.obsidian.md/t/have-obsidian-be-the-handler-of-md-files-add-ability-to-use-obsidian-as-a-markdown-editor-on-files-outside-vault-file-association/314?page=8)). 2. Vault system "quite limiting", no easy CLI open ([HN, Marky thread](https://hn.algolia.com/api/v1/items/47795468)). 3. Fewer than 3 found with sources (RAM complaints only secondhand). |
| VS Code built-in preview | Preview pane inside a code editor | Win, mac, Linux | MIT core, free | Active | 1. Scroll position lost or jumping: [#63690](https://github.com/microsoft/vscode/issues/63690), [#22995](https://github.com/microsoft/vscode/issues/22995), still open in 2026: [#317033](https://github.com/microsoft/vscode/issues/317033) (23 comments). 2. No native GitHub alerts (`> [!NOTE]`), open since 2024-04-05, 30 comments ([#209652](https://github.com/microsoft/vscode/issues/209652)). 3. Images outside the workspace blocked by design ([#64685](https://github.com/Microsoft/vscode/issues/64685)). Also "scrolls very poorly on my Mac" ([HN](https://hn.algolia.com/api/v1/items/47795468)). |
| Markdown Preview Enhanced | VS Code extension, feature-rich preview | Win, mac, Linux (in VS Code) | Custom license (GitHub reports "Other"), free | Active (last push 2026-10-09), 1,416 open issues | 1. Four code-execution CVEs in 2026 from crafted markdown or repo config: CVE-2026-54566, -54525, -50733, -49493 ([OpenCVE 54566](https://app.opencve.io/cve/CVE-2026-54566), [OpenCVE 54525](https://app.opencve.io/cve/CVE-2026-54525), [SentinelOne 50733](https://www.sentinelone.com/vulnerability-database/cve-2026-50733/), [SentinelOne 49493](https://www.sentinelone.com/vulnerability-database/cve-2026-49493/)), plus CVE-2025-65716 ([OX Security](https://ox.security/blog/cve-2025-65716-markdown-preview-enhanced-vscode-vulnerability)). 2. High CPU ([#228](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/228)); slow refresh on long docs ([#1870](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/1870)). 3. Scroll sync fails ([#1730](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/1730), [#667](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/667)). |
| iA Writer | Minimalist writing app | mac, iOS, Win, Android | Proprietary, about $29.99 Win / $49.99 mac, one-time | Active | Fewer than 3 found. Price is the main friction ([rywalker.com](https://rywalker.com/research/ia-writer)); reviewer worry about feature creep ([nubiapage review](https://nubiapage.com/ko/ia-writer-review-2026/)). A writing tool, not a viewer. |
| Marked 2 / Marked 3 | Preview-only companion app that watches a file | mac only | Proprietary, $13.99 to $14.99 (Marked 2); Marked 3 subscription or unlock | Marked 2 in maintenance; Marked 3 current | 1. Mac only ([App Store](https://apps.apple.com/us/app/marked-2-markdown-preview/id890031187)). 2. GitHub callouts not rendered in Marked 2, "not planned", gated to Marked 3 ([support thread](https://support.markedapp.com/questions/D106)). 3. Marked QL rendering wrong in Finder column view on Sequoia ([support](https://support.markedapp.com/questions/D1eg)). |
| MacDown | Open-source mac editor with preview | mac only | MIT, free | Dormant: last push 2023-07-10, 578 open issues; Homebrew cask deprecated | 1. Abandoned, Intel-only, needs Rosetta ([Homebrew](https://formulae.brew.sh/cask/macdown), [MacRumors](https://forums.macrumors.com/threads/alternative-to-macdown.2411390/)). 2. Preview scrolls to top on each keystroke / sync drift ([#21](https://github.com/MacDownApp/macdown/issues/21)). 3. Fewer than 3 found. A fork (MacDown 3000) exists. |
| Zettlr | Academic markdown editor (Electron) | Win, mac, Linux | GPL-3.0, free | Active (last push 2026-10-25) | 1 and 2. Forum threads on typing lag ([forum 89](https://forum.zettlr.com/d/89-typing-lag), [forum 43](https://forum.zettlr.com/d/43-lag-in-typing)); thread bodies would not load on 2026-10-26, so details are from search snippets only. 3. Startup crash from stale config ([forum](https://forum.zettlr.com/d/139-zettlr-crashing-on-startup-missing-docs)). Editor, not viewer. |
| Glow | Terminal markdown renderer | Win, mac, Linux (terminal) | MIT, free | Active (last push 2026-10-05), 27.8k stars | 1. No images, open since 2020 ([#211](https://github.com/charmbracelet/glow/issues/211)). 2. Hard line breaks ([#212](https://github.com/charmbracelet/glow/issues/212)). 3. Table links wrapped and unclickable, fixed in Glamour v0.10 ([Charm blog](https://charm.land/blog/glamour-tables/)). Not a double-click GUI. |
| grip | Local server rendering via GitHub's API | Any (Python + browser) | MIT, free | Stale: last push 2024-07-10, 125 open issues | 1. Hourly GitHub API rate limit, needs a token ([README mirror](https://sources.debian.org/src/grip/4.6.1-3/README.md)). 2. Needs network; offline mode unfinished (same source). 3. Fewer than 3 found. |
| QLMarkdown | macOS Quick Look extension | mac only | GPL-3.0, free | Active (last push 2026-10-04), 3.7k stars | 1. Broke on macOS 15 ([#148](https://github.com/sbarex/QLMarkdown/issues/148), 31 comments). 2. Broke again on macOS 27 ([#207](https://github.com/sbarex/QLMarkdown/issues/207)). 3. Deprecated on Homebrew ([#165](https://github.com/sbarex/QLMarkdown/issues/165)); scrolling not working ([#102](https://github.com/sbarex/QLMarkdown/issues/102)). |
| PowerToys (Preview Pane add-on, Peek) | Explorer preview handler and spacebar previewer | Windows | MIT, free | Active | 1. Preview pane failures ("Catastrophic failure" [#23384](https://github.com/microsoft/PowerToys/issues/23384), 54 comments; [#1796](https://github.com/microsoft/PowerToys/issues/1796); [#9829](https://github.com/microsoft/PowerToys/issues/9829)). 2. Images not shown or broken ([#1837](https://github.com/microsoft/PowerToys/issues/1837), open since 2020; [#31747](https://github.com/microsoft/PowerToys/issues/31747)). 3. Transient by design; a user asked for a persistent read-only "Markdown Reader" because editors are "heavy for just reading" ([#45267](https://github.com/microsoft/PowerToys/issues/45267), 2026-02-01). |
| MarkdownPad 2 | Legacy Windows editor | Windows | Freemium | Abandoned | Preview crashes on Windows 8/10/11 due to the dead Awesomium engine; workaround inconsistent ([AlternativeTo](https://www.alternativeto.net/software/markdownpad/about/), [ctrl.blog](https://www.ctrl.blog/entry/review-markdown-editors-for-windows/)). Fewer than 3 distinct found. |
| Windows 11 Notepad | Built-in editor with a markdown formatting view | Windows 11 | Free, bundled | Active | Basic only: headings, bold/italic, lists, links per coverage ([AlternativeTo news](https://alternativeto.net/news/2025/5/notepad-for-windows-11-adds-text-formatting-markdown-support-and-new-toolbar), [Winaero](https://winaero.com/microsoft-makes-notepad-markdown-support-generally-available)). No complaints mined. Important as a free "good enough" baseline. |
| Markdown Viewer (simov, browser extension) | Renders .md in Chrome/Firefox | Any browser | MIT, free | Active-ish (last push 2025-12-29) | 1. file:// access setup confusing, raw text shown ([Firefox reviews](https://ca.add-ons.mozilla.org/en-US/firefox/addon/markdown-viewer-chrome/reviews/?score=1)). 2. Double-click with browser closed shows raw markdown ([#288](https://github.com/simov/markdown-viewer/issues/288)). 3. Developer attributes failures to user OS/browser config (same reviews page). |
| **MDHero** | Viewer-first app with optional editor (Tauri, Svelte) | Win (x64, ARM64), mac (Apple Silicon only), Linux | MIT, free | Very active: v0.2.10 on 2026-09-13, last push 2026-10-24, 301 stars, 38 open issues | 1. Relative links failed when a file was opened directly (fixed 2026-09-08) ([#96](https://github.com/vaibhav-kakde-in/mdhero/issues/96)). 2. No Intel Mac build ([#151](https://github.com/vaibhav-kakde-in/mdhero/issues/151)); unsigned Windows installer triggers SmartScreen ([mdhero.app](https://mdhero.app/)). 3. Rendering and customisation gaps: footnotes ([#187](https://github.com/vaibhav-kakde-in/mdhero/issues/187)), fonts not changeable on macOS ([#144](https://github.com/vaibhav-kakde-in/mdhero/issues/144)), "Auto Refresh" request ([#177](https://github.com/vaibhav-kakde-in/mdhero/issues/177)). Scope drifting toward editing (undo [#182](https://github.com/vaibhav-kakde-in/mdhero/issues/182), paste images [#186](https://github.com/vaibhav-kakde-in/mdhero/issues/186), tab groups [#155](https://github.com/vaibhav-kakde-in/mdhero/issues/155)). |
| Tinta | Native C++ Direct2D viewer, ~1 MB exe, ~200 ms start | Windows only | MIT, free | Very active: last push 2026-10-26, 250 stars | 1. Cannot change font family/size ([AlternativeTo review](https://alternativeto.net/software/tinta/about/), [#155](https://github.com/oipoistar/tinta/issues/155)). 2. Synced scrolling broken in large docs ([#267](https://github.com/oipoistar/tinta/issues/267)). 3. Glyph rendering bug ([#264](https://github.com/oipoistar/tinta/issues/264)). Has an edit mode. |
| Marky | Tauri viewer for agentic coding, CLI-launched | mac, Linux (no Windows) | Apache-2.0, free | Slowing: last push 2026-04-27, 280 stars | No Windows; "native" claim disputed for a webview app; requests for text size and layout control ([HN thread](https://hn.algolia.com/api/v1/items/47795468)). |
| MacMD Viewer | Native SwiftUI viewer with Quick Look | mac only | $19.99 one-time (also Setapp) | Active | Paid and mac only ([Setapp](https://setapp.com/apps/macmd-viewer)). The vendor runs SEO comparison pages about every competitor; used here only for discovery. |
| Other 2026 minimal viewers | MDviewer (JackYoung27), mrkd, tims-markdown-reader, MDView, mdviewer (rajatarya), WackDown, dbv, permark, Meva, .md Viewer, alecames/MarkdownViewer | Mostly single-platform | Mostly MIT, free | All tiny (0 to 24 stars) | Little user feedback exists. Show HN posts got 2 to 9 points and 0 to 1 comments ([HN search](https://hn.algolia.com/api/v1/search?query=markdown%20viewer&tags=story&numericFilters=created_at_i>1735689600)). |
| Inlet | Named in the brief | n/a | n/a | n/a | Not findable by search on 2026-10-26. |

## Complaint clusters, ranked

Ranked by strength and volume of independent evidence I could cite (issue comment counts, number of separate tools affected), not by raw mention counts, since Reddit and store reviews could not be mined.

### 1. Scroll position lost on reload or live update (strong, many tools)
- VS Code: [#63690](https://github.com/microsoft/vscode/issues/63690) (21 comments), [#22995](https://github.com/microsoft/vscode/issues/22995), [#12865](https://github.com/microsoft/vscode/issues/12865), and still open in 2026 [#317033](https://github.com/microsoft/vscode/issues/317033) (23 comments).
- MacDown: preview jumps to the top on each keystroke ([#21](https://github.com/MacDownApp/macdown/issues/21)).
- GNOME Builder: preview scrolls to top on refresh ([#243](https://gitlab.gnome.org/GNOME/gnome-builder/-/issues/243)). Geany: preview resets scroll on any modification ([mailing list](https://lists.geany.org/hyperkitty/list/github-comments@lists.geany.org/thread/QV7DUMH7NHW4OSRE72BOGB3SOIVH4ZPL)).
- Cursor: "Markdown file scrolls back up unexpectedly" ([forum](https://forum.cursor.com/t/markdown-file-scrolls-back-up-unexpectedly/168821)).
- Scroll sync failures: MPE [#1730](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/1730), Tinta [#267](https://github.com/oipoistar/tinta/issues/267).
- Verdict: confirmed. A reader that live-reloads must keep its place.

### 2. OS-integrated previewers break or are flaky (strong, both platforms)
- QLMarkdown broke on macOS 15 ([#148](https://github.com/sbarex/QLMarkdown/issues/148), 31 comments), on macOS 27 ([#207](https://github.com/sbarex/QLMarkdown/issues/207)), and on Monterey ([#122](https://github.com/sbarex/QLMarkdown/issues/122)); Homebrew deprecation ([#165](https://github.com/sbarex/QLMarkdown/issues/165)).
- Other Quick Look plugins broke on Sequoia ([Brett Terpstra forum](https://forum.brettterpstra.com/t/mmd-quicklook-not-working-in-macos-sequoia-15-5/2600), [Eastgate forum](https://forum.eastgate.com/t/quicklook-preview-in-sequoia/8566), [Marked support](https://support.markedapp.com/questions/D1eg)); Obsidian's Quick Look blocked keyboard input on macOS 27 ([forum](https://forum.obsidian.md/t/obsidians-quick-look-on-macos-27-no-keystroke-possible-after-quick-looking-a-markdown-file/119162)).
- PowerToys Markdown preview pane: "Catastrophic failure" ([#23384](https://github.com/microsoft/PowerToys/issues/23384), 54 comments), not displaying ([#1796](https://github.com/microsoft/PowerToys/issues/1796), 37), not working ([#9829](https://github.com/microsoft/PowerToys/issues/9829), 32); handler conflicts and focus quirks ([Directory Opus forum](https://resource.dopus.com/t/markdown-md-viewers-microsoft-and-others/35051)).
- Verdict: confirmed. This is a warning as much as an opportunity: shipping OS integration means owning breakage after every OS release.

### 3. "I just want to read it" (editors, IDEs, vaults, transient previews) (strong signal, moderate volume)
- HN, 2026-01-10: "a massive lack of Markdown VIEWERS"; the poster wants to double-click and read, not open an editor and hit preview ([HN comment](https://hn.algolia.com/api/v1/items/46561851)).
- Typora read-only request open since 2017, 47 comments ([#710](https://github.com/typora/typora-issues/issues/710)).
- Obsidian cannot open files outside a vault ([forum thread 314](https://forum.obsidian.md/t/have-obsidian-be-the-handler-of-md-files-add-ability-to-use-obsidian-as-a-markdown-editor-on-files-outside-vault-file-association/314?page=8)).
- PowerToys request for a persistent read-only reader: Peek is transient, editors are "heavy" ([#45267](https://github.com/microsoft/PowerToys/issues/45267)).
- Show HN MDviewer author: previews are "inside editors, browser-based, or tied to heavier tools" ([HN](https://hn.algolia.com/api/v1/items/47315049)).
- Verdict: confirmed, and it is the core demand. But see the gap analysis: many tools now answer it.

### 4. Rendering mismatch vs GitHub (moderate)
- VS Code lacks GitHub alerts natively ([#209652](https://github.com/microsoft/vscode/issues/209652), 30 comments, open since 2024-04-05).
- Marked 2 callouts not supported ([support](https://support.markedapp.com/questions/D106)); Obsidian shows GitHub alerts wrong in README previews ([forum](https://forum.obsidian.md/t/github-style-callouts-alerts-in-plugin-readme-previews-not-displayed-properly-in-the-community-plugin-catalogue/91792)).
- MDHero footnotes do not render ([#187](https://github.com/vaibhav-kakde-in/mdhero/issues/187)); Glow hard line breaks ([#212](https://github.com/charmbracelet/glow/issues/212)).
- grip exists purely to get GitHub-exact output, at the cost of API rate limits ([README](https://sources.debian.org/src/grip/4.6.1-3/README.md)).
- Verdict: confirmed. GFM parity (tables, task lists, alerts, footnotes, heading anchors) is a checklist competitors still miss.

### 5. Abandonment / unmaintained (moderate, mostly older tools)
- MacDown: last push 2023-07-10, Intel-only, Homebrew cask deprecated ([Homebrew](https://formulae.brew.sh/cask/macdown)).
- MarkText: "Is this project still being worked on?" ([#3624](https://github.com/marktext/marktext/issues/3624)); revived in 2026 per a [fork README](https://github.com/Tkaixiang/marktext).
- grip: last push 2024-07-10. MarkdownPad: dead rendering engine ([ctrl.blog](https://www.ctrl.blog/entry/review-markdown-editors-for-windows/)). Marked 2: maintenance mode after Marked 3 ([markedapp llms.txt](https://markedapp.com/llms.txt)).
- New-wave risk: Marky last pushed 2026-04-27, about ten days after a 75-point Show HN on 2026-04-16.
- Verdict: confirmed. "Small, done, and maintained" is credible only if scope is tiny.

### 6. Security: preview executes code (moderate volume, high severity)
- MPE: four code-execution CVEs in 2026 (see table) plus CVE-2025-65716 ([OX Security](https://ox.security/blog/cve-2025-65716-markdown-preview-enhanced-vscode-vulnerability)).
- Typora: CVE-2023-2317 DOM XSS to RCE from opening a crafted .md ([STAR Labs](https://starlabs.sg/advisories/23/23-2317/)); CVE-2023-39703 ([Tenable](https://tenable.com/cve/CVE-2023-39703)).
- Verdict: confirmed as a real risk, though users rarely voice it as a complaint. It is a differentiator to state, not a pain users raise.

### 7. Broken relative links / images (moderate)
- VS Code blocks images outside the workspace by design ([#64685](https://github.com/Microsoft/vscode/issues/64685)).
- PowerToys preview: images not shown ([#1837](https://github.com/microsoft/PowerToys/issues/1837)), image links broken ([#31747](https://github.com/microsoft/PowerToys/issues/31747)).
- MDHero relative .md links failed for directly opened files on Windows ([#96](https://github.com/vaibhav-kakde-in/mdhero/issues/96)); MarkText relative image directories ([#549](https://github.com/marktext/marktext/issues/549)).
- Verdict: confirmed.

### 8. Bloat / Electron / slow startup (weaker user evidence than expected)
- User-side: MarkText "low Startup Performance" ([#3862](https://github.com/marktext/marktext/issues/3862)); MPE high CPU ([#228](https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/228)); editors "heavy for just reading" ([PowerToys #45267](https://github.com/microsoft/PowerToys/issues/45267)); Typora file size cap ([#1961](https://github.com/typora/typora-issues/issues/1961)).
- Builder-side: nearly every 2026 launch advertises "no Electron" (for example [MDviewer Show HN](https://hn.algolia.com/api/v1/items/47315049), [Tinta](https://github.com/oipoistar/tinta)), and HN commenters push back that a webview app is not "native" ([Marky thread](https://hn.algolia.com/api/v1/items/47795468)).
- Verdict: partially confirmed. Builders lean on it more than users complain about it.

### 9. Mac-first tools with poor or no Windows support (moderate)
- Marked, MacDown, MacMD Viewer, QLMarkdown, Marky, MDviewer, mrkd, MDView are mac-only (see table). PowerToys #45267 frames Windows as lacking a lightweight native reader. Conversely, Tinta is Windows-only. Only MDHero, Typora, MarkText, Zettlr, and VS Code span both.
- Verdict: confirmed as fragmentation, but MDHero already spans both.

### 10. Paywalls / licensing (weak)
- Typora went paid ([dev.to](https://dev.to/evanlin/til-typora-10-and-now-paid-with-useful-resources-5bnf)); Marked callouts gated behind Marked 3 ([support](https://support.markedapp.com/questions/D106)); iA Writer and MacMD Viewer are paid. Prices are low one-time fees; little anger found.
- Verdict: minor.

### Themes from the brief that were weak or not confirmed
- **No live reload on external change:** partial. Typora had hot-reload regressions ([#4794](https://github.com/typora/typora-issues/issues/4794)) and save conflicts ([#5209](https://github.com/typora/typora-issues/issues/5209)); MDHero has an "Auto Refresh" request ([#177](https://github.com/vaibhav-kakde-in/mdhero/issues/177)). Most modern viewers ship live reload, so it is table stakes rather than a gap.
- **Ugly default typography / no dark mode:** weak. Requests are about customisation (fonts in Tinta [#155](https://github.com/oipoistar/tinta/issues/155) and MDHero [#144](https://github.com/vaibhav-kakde-in/mdhero/issues/144); text size in the Marky thread), not ugliness. Dark mode is ubiquitous now.
- **File association / "open with":** confirmed for Obsidian (thread 314) and browser extensions ([simov #288](https://github.com/simov/markdown-viewer/issues/288)); already solved by MDHero, Tinta, and alecames/MarkdownViewer.

## Gap analysis

**Does a tool already do "double-click a .md, see a beautiful read-only render, fast" on both Windows and macOS? Mostly yes: MDHero.** It is free, MIT, around 8 MB (Tauri), opens as a viewer, registers as the default handler, live-reloads, has dark mode, Mermaid, KaTeX, TOC, and a macOS Quick Look extension (mdhero.app claim; an older competitor comparison page disputes it), and it shipped five releases between 2026-06-08 and 2026-09-13 with a push on 2026-10-24 ([mdhero.app](https://mdhero.app/), [repo](https://github.com/vaibhav-kakde-in/mdhero)). On Windows alone, Tinta is arguably better on speed (native C++, ~1 MB, ~200 ms start). On Windows, Notepad now covers the casual "just show me headings and bold" case for free. The broad hypothesis "there is no good cross-platform markdown viewer" is **not supported** by the evidence as of 2026-10-26.

**Verified weaknesses of the leading competitor**, which define the remaining room:
- Not strictly read-only: View/Split/Edit modes, and the open-issue queue pulls it further toward an editor (undo, paste images, tab groups, Save As).
- Platform holes: no Intel Mac build ([#151](https://github.com/vaibhav-kakde-in/mdhero/issues/151)); unsigned Windows installer with a SmartScreen warning ([mdhero.app](https://mdhero.app/)).
- Rendering gaps: footnotes ([#187](https://github.com/vaibhav-kakde-in/mdhero/issues/187)), a recent relative-link bug ([#96](https://github.com/vaibhav-kakde-in/mdhero/issues/96)), fonts on macOS ([#144](https://github.com/vaibhav-kakde-in/mdhero/issues/144)).
- Feature breadth (AI lookup, Marp, Vim mode, tabs) works against "small, auditable, extremely clean code."
- Webview-based, so it does not win the "native" argument HN commenters care about.

**The unoccupied square** is narrow: a deliberately read-only viewer that (a) runs on Windows and macOS including Intel Macs, (b) ships signed and notarized builds, (c) renders GitHub-faithfully (alerts, footnotes, task lists, anchors), (d) sanitizes HTML and runs no scripts by default, (e) preserves scroll on live reload, (f) resolves relative links and images correctly however the file was opened, and (g) has a codebase small enough to read in an afternoon. Each item is done somewhere; no single tool does all, and none markets "read-only and safe" as the point.

**Honest assessment:** as a product, the gap is small and the market is saturated with low-traction entrants (at least 18 HN launches in 2026, search capped at 20 results, most under 10 points). As a portfolio piece, a tightly scoped, well-engineered viewer can still stand out on quality rather than novelty, if it is positioned explicitly against MDHero and Tinta and its README states which of the items above it does better. The evidence-backed differentiators with the most leverage are scroll-preserving reload, GitHub parity, and security by default. OS preview integration (Quick Look, Explorer preview pane) is much requested but is also the most fragile maintenance burden (cluster 2).

## Coverage gaps and caveats
- **Reddit:** standard and extended searches targeting r/markdown, r/ObsidianMD, r/macapps, r/software returned no usable threads. Not mined.
- **App stores / AlternativeTo:** only Tinta's and MarkdownPad's AlternativeTo pages and Firefox add-on reviews were read. Mac App Store and Microsoft Store reviews were not reachable via search.
- **Hacker News:** news.ycombinator.com returned HTTP 429; threads were read through the Algolia HN API.
- **Vendor bias:** macmdviewer.com publishes comparison pages about most competitors; used only to discover tools, not as complaint evidence.
- **Secondhand data:** Typora slowness, Obsidian RAM, Notepad's exact markdown coverage, Typora pricing, and the MarkText revival date rely on third-party pages.
- **Inlet:** not found.

## Sources (all accessed 2026-10-26)

Issue trackers and repositories
- https://github.com/vaibhav-kakde-in/mdhero
- https://github.com/vaibhav-kakde-in/mdhero/issues/96
- https://github.com/vaibhav-kakde-in/mdhero/issues/144
- https://github.com/vaibhav-kakde-in/mdhero/issues/151
- https://github.com/vaibhav-kakde-in/mdhero/issues/155
- https://github.com/vaibhav-kakde-in/mdhero/issues/177
- https://github.com/vaibhav-kakde-in/mdhero/issues/182
- https://github.com/vaibhav-kakde-in/mdhero/issues/186
- https://github.com/vaibhav-kakde-in/mdhero/issues/187
- https://github.com/oipoistar/tinta
- https://github.com/oipoistar/tinta/issues/155
- https://github.com/oipoistar/tinta/issues/264
- https://github.com/oipoistar/tinta/issues/267
- https://github.com/GRVYDEV/marky
- https://github.com/typora/typora-issues/issues/68
- https://github.com/typora/typora-issues/issues/710
- https://github.com/typora/typora-issues/issues/1961
- https://github.com/typora/typora-issues/issues/4794
- https://github.com/typora/typora-issues/issues/5209
- https://github.com/marktext/marktext/issues/511
- https://github.com/marktext/marktext/issues/549
- https://github.com/marktext/marktext/issues/1290
- https://github.com/marktext/marktext/issues/3624
- https://github.com/marktext/marktext/issues/3862
- https://github.com/Tkaixiang/marktext
- https://github.com/microsoft/vscode/issues/12865
- https://github.com/microsoft/vscode/issues/22995
- https://github.com/microsoft/vscode/issues/63690
- https://github.com/microsoft/vscode/issues/317033
- https://github.com/microsoft/vscode/issues/209652
- https://github.com/Microsoft/vscode/issues/64685
- https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/228
- https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/667
- https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/1730
- https://github.com/shd101wyy/vscode-markdown-preview-enhanced/issues/1870
- https://github.com/charmbracelet/glow/issues/211
- https://github.com/charmbracelet/glow/issues/212
- https://github.com/sbarex/QLMarkdown/issues/102
- https://github.com/sbarex/QLMarkdown/issues/122
- https://github.com/sbarex/QLMarkdown/issues/148
- https://github.com/sbarex/QLMarkdown/issues/165
- https://github.com/sbarex/QLMarkdown/issues/207
- https://github.com/microsoft/PowerToys/issues/1796
- https://github.com/microsoft/PowerToys/issues/1837
- https://github.com/microsoft/PowerToys/issues/9829
- https://github.com/microsoft/PowerToys/issues/23384
- https://github.com/microsoft/PowerToys/issues/31747
- https://github.com/microsoft/PowerToys/issues/45267
- https://github.com/MacDownApp/macdown/issues/21
- https://github.com/simov/markdown-viewer/issues/288
- https://gitlab.gnome.org/GNOME/gnome-builder/-/issues/243
- https://lists.geany.org/hyperkitty/list/github-comments@lists.geany.org/thread/QV7DUMH7NHW4OSRE72BOGB3SOIVH4ZPL
- https://sources.debian.org/src/grip/4.6.1-3/README.md

Hacker News (via the Algolia API)
- https://hn.algolia.com/api/v1/items/46561851
- https://hn.algolia.com/api/v1/items/47795468
- https://hn.algolia.com/api/v1/items/47315049
- https://hn.algolia.com/api/v1/items/48200226
- https://hn.algolia.com/api/v1/items/46963865
- https://hn.algolia.com/api/v1/items/49514729
- https://hn.algolia.com/api/v1/search?query=markdown%20viewer&tags=story&numericFilters=created_at_i>1735689600

Forums, reviews, product pages
- https://forum.obsidian.md/t/have-obsidian-be-the-handler-of-md-files-add-ability-to-use-obsidian-as-a-markdown-editor-on-files-outside-vault-file-association/314?page=8
- https://forum.obsidian.md/t/github-style-callouts-alerts-in-plugin-readme-previews-not-displayed-properly-in-the-community-plugin-catalogue/91792
- https://forum.obsidian.md/t/obsidians-quick-look-on-macos-27-no-keystroke-possible-after-quick-looking-a-markdown-file/119162
- https://forum.cursor.com/t/markdown-file-scrolls-back-up-unexpectedly/168821
- https://forum.zettlr.com/d/89-typing-lag
- https://forum.zettlr.com/d/43-lag-in-typing
- https://forum.zettlr.com/d/139-zettlr-crashing-on-startup-missing-docs
- https://forum.brettterpstra.com/t/mmd-quicklook-not-working-in-macos-sequoia-15-5/2600
- https://forum.eastgate.com/t/quicklook-preview-in-sequoia/8566
- https://forums.macrumors.com/threads/alternative-to-macdown.2411390/
- https://resource.dopus.com/t/markdown-md-viewers-microsoft-and-others/35051
- https://support.markedapp.com/questions/D106
- https://support.markedapp.com/questions/D1eg
- https://markedapp.com/llms.txt
- https://apps.apple.com/us/app/marked-2-markdown-preview/id890031187
- https://ca.add-ons.mozilla.org/en-US/firefox/addon/markdown-viewer-chrome/reviews/?score=1
- https://alternativeto.net/software/tinta/about/
- https://www.alternativeto.net/software/markdownpad/about/
- https://alternativeto.net/news/2025/5/notepad-for-windows-11-adds-text-formatting-markdown-support-and-new-toolbar
- https://winaero.com/microsoft-makes-notepad-markdown-support-generally-available
- https://www.ctrl.blog/entry/review-markdown-editors-for-windows/
- https://dev.to/evanlin/til-typora-10-and-now-paid-with-useful-resources-5bnf
- https://dev.to/l1u7/fixing-the-file-is-too-large-to-render-in-typora-by-increasing-the-file-size-limit-hnc
- https://www.g2.com/products/typora/reviews
- https://rywalker.com/research/ia-writer
- https://nubiapage.com/ko/ia-writer-review-2026/
- https://charm.land/blog/glamour-tables/
- https://formulae.brew.sh/cask/macdown
- https://mdhero.app/
- https://mdhero.app/windows/
- https://setapp.com/apps/macmd-viewer
- https://macmdviewer.com/blog/best-markdown-viewer (vendor page, discovery only)
- https://www.producthunt.com/products/dbv-markdown-reader
- https://notes.hamatti.org/Technology/macOS-preview-rendering-for-Markdown-files

Security advisories
- https://app.opencve.io/cve/CVE-2026-54566
- https://app.opencve.io/cve/CVE-2026-54525
- https://www.sentinelone.com/vulnerability-database/cve-2026-50733/
- https://www.sentinelone.com/vulnerability-database/cve-2026-49493/
- https://ox.security/blog/cve-2025-65716-markdown-preview-enhanced-vscode-vulnerability
- https://starlabs.sg/advisories/23/23-2317/
- https://tenable.com/cve/CVE-2023-39703
