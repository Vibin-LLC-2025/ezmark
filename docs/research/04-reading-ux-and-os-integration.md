# 04: Reading UX and OS integration for a markdown preview app

Researched 2026-10-26. Scope: what "optimized for human eyes" means in concrete numbers, which reader features matter for v1, and what it takes for "double-click a .md file and read it" to work on Windows 11 and macOS.

## Summary

1. The biggest readability win is limiting line length. GitHub's 980px layout at 16px gives roughly 110 characters per line, well past Butterick's 45–90 range. Cap prose at about 68ch and let tables, code and images run wider.
2. Body text: 17px system UI font, line-height 1.6, 1em paragraph spacing. Use GitHub's heading scale so READMEs look the way people expect.
3. Don't bundle fonts in v1. Use the system stacks (system-ui / ui-monospace) and offer a bundled "reading font" (iA Writer Quattro, OFL) later.
4. Themes in v1: follow system (the default), light and dark. Use GitHub Light/Dark syntax colors. Support `forced-colors` and `prefers-contrast` from day one; it is mostly CSS. Sepia comes later.
5. Navigation in v1: outline sidebar, heading anchors, find, zoom, back/forward between linked .md files, per-file scroll memory, keyboard scrolling.
6. In v1, auto-reload that keeps the scroll position is what makes or breaks the app. Every viewer we found advertises it. Watch the directory, not the file handle, because editors save by writing a temp file and renaming it.
7. Windows: an installer can register the app as a candidate handler, but it cannot legitimately make itself the default (UserChoice hash). The supported path is a first-run deep link to the app's Default Apps settings page. Windows 11 Notepad now renders markdown, which raises the bar the app has to clear.
8. macOS: CFBundleDocumentTypes plus an imported `net.daringfireball.markdown` UTI is enough for file association. A Quick Look extension (.appex, now mandatory since Sequoia dropped .qlgenerator) is the mac users' most expected extra, and the hardest piece to build.
9. Explorer preview handler: later at best. PowerToys already ships one for free.
10. Evidence on what users complain about is thin (mostly product pages, not complaint threads), so the v1 cut leans on what competing viewers advertise.

## Reading UX spec (defaults and why)

### Measure (line length)
- **Default:** prose column `max-width: 68ch`. The `ch` unit is the width of "0", which is wider than the average lowercase letter in proportional fonts, so 68ch works out to roughly 75–80 characters of real text. That sits inside Butterick's 45–90 range (practicaltypography.com, "line length").
- **User options:** Narrow (56ch), Default (68ch), Wide (84ch), Full (no cap). Firefox Reader View exposes the same kind of "content width" control, which shows readers expect it.
- **Why not GitHub's layout:** github-markdown-css documents a 980px width with 45px padding, so the content box is about 890px. At 16px with an average glyph width near 0.5em, that is about 110 characters per line. GitHub's width is the familiar look, but it is too long for comfortable reading, and fixing it is exactly the improvement this app is pitching.
- **Breakout elements:** tables, code blocks, wide images and Mermaid diagrams may extend past the prose column up to `min(100%, 120ch)` and stay centered, similar to Tufte CSS's `fullwidth` figure class. This keeps prose narrow without squeezing data.
- **Window padding:** 32px on each side at desktop widths, 16px below 600px. GitHub uses 45px desktop and 15px mobile. Ours is a little smaller because the window itself already has chrome around it.

### Size and leading
- **Body size: 17px** (1.0625rem on a 16px root). Butterick's web range is 15–25px. GitHub uses 16px. Tufte CSS uses 21px (15px root × 1.4rem). iA argues text should get bigger as reading distance grows, and a desktop viewer is read at arm's length, often on a laptop. 17px is a modest step up from browser defaults that still fits a decent number of words in a half-screen window. Zoom steps (see Navigation) cover everything else.
- **Body line-height: 1.6.** Butterick's general rule is 120–145%. Screens and long lines push toward the top of that range or above it, and WCAG 1.4.12 favors something looser: the content has to survive a user override to 1.5, so designing at 1.5 or above makes that free. GitHub uses 1.5. Tufte ends up near 1.43 but with a much narrower column. 1.6 works well with a 68ch measure.
- **Paragraph spacing:** 0 top, 1em bottom (GitHub's effective value).
- **Headings:** line-height 1.25, weight 600, margin-top 1.5em, margin-bottom 0.5em to 1em.
- **Lists:** 2em indent, 0.25em between items, nested lists tight.

### Heading scale
Keep GitHub's scale so READMEs look the way people expect. Most of the markdown people will open is READMEs, docs and AI-generated notes written with GitHub rendering in mind.

| Level | Size | Notes |
|---|---|---|
| h1 | 2em | bottom border 1px muted, padding-bottom 0.3em |
| h2 | 1.5em | bottom border 1px muted, padding-bottom 0.3em |
| h3 | 1.25em | |
| h4 | 1em | weight 600 carries the hierarchy |
| h5 | 0.875em | |
| h6 | 0.85em | muted color |

Optional "Editorial" style (later): no rules under headings, italic h2/h3 at weight 400, serif body. This follows Tufte CSS (h1 3.2rem, h2 2.2rem italic, h3 1.7rem italic).

### Font stacks (no bundling in v1)
- **Sans (default body):** `system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI Variable Text", "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif, "Apple Color Emoji", "Segoe UI Emoji"`. On macOS this gives San Francisco and on Windows Segoe UI. GitHub's own stack is the same minus `system-ui` and the Variable name. Keep the order deliberate: a W3C discussion warns that a font installed on the "wrong" OS can win if the stack order is careless.
- **Serif (optional reading style):** `ui-serif, "New York", Charter, "Bitstream Charter", Cambria, Georgia, serif`. `ui-serif` gives New York in WebKit. Windows has no comparable system serif for screen reading, so it falls back to Cambria or Georgia. This difference is the strongest argument for bundling a serif later.
- **Mono:** `ui-monospace, "SF Mono", SFMono-Regular, Menlo, "Cascadia Mono", Consolas, "Liberation Mono", monospace`. That is GitHub's stack plus Cascadia. (Assumption, not checked in this pass: Cascadia Mono ships with current Windows 11. If it is missing, Consolas is fine.)
- **Bundling, cost vs benefit:**
  - Benefit: the same rendering on both OSes, a recognizable "reading" identity, and on Windows a fix for the missing good screen serif.
  - Cost: package size (a variable woff2 per family runs from a few hundred KB to over 1 MB with full glyph coverage; check the actual release files), the OFL license text has to ship alongside, CJK and emoji fall back to system fonts and clash with a bundled Latin face, and a non-native face can look slightly off next to native UI.
  - Licensing is clean: Inter is OFL 1.1 ("Inter" is a Reserved Font Name, so don't rename a modified copy). iA Writer Mono/Duo/Quattro are OFL derivatives of IBM Plex (confirmed via FreshPorts and Font Squirrel, not the repo's own LICENSE file). JetBrains Mono is OFL 1.1.
  - Verdict: v1 uses system fonts only. Later, add an optional bundled "Reading" style with iA Writer Quattro (body) and JetBrains Mono (code), which addresses the Windows serif gap. Skip Inter: it is too close to the system fonts to justify the extra size.

### Code blocks
- Font size 0.875em (GitHub uses 85%), line-height 1.45, padding 1em, border-radius 6px, muted background, no border.
- Long lines scroll horizontally by default, with a "wrap lines" toggle (global setting, plus a per-block button later). Each code block is focusable with `tabindex="0"` so keyboard users can scroll it.
- On hover or focus, show a copy button and the language label in the top-right corner. This is v1: copying commands out of READMEs is a main reason people open them.
- Inline code: 0.875em, padding 0.2em 0.4em, radius 6px, `white-space: break-spaces` (GitHub).

### Tables
- `display: block; overflow-x: auto; width: max-content; max-width: 100%` (GitHub pattern), so wide tables scroll instead of breaking the layout. They may use the breakout width described above.
- Cell padding 6px 13px, 1px borders, zebra stripes on even rows, header weight 600.
- Respect GFM column alignment. Use `font-variant-numeric: tabular-nums` in cells.
- Later: sticky header row for long tables.

### Images and media
- `max-width: 100%; height: auto`. Resolve relative paths against the file's directory. Clicking an image opens it at full size in an overlay (later).
- Remote images: load them by default, as GitHub does, with a setting to block remote content for privacy. Never run scripts from markdown.

### Blockquotes and callouts
- Blockquote: left border 0.25em, padding 0 1em, muted text color (GitHub).
- GFM alerts in v1: `> [!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`, with GitHub's colors (light: note #0969da, important #8250df, warning #9a6700, tip #1a7f37, caution #cf222e; dark: #4493f8, #ab7df8, #d29922, #3fb950, #f85149), a left border, a title row and an icon. They now appear widely in READMEs.
- Obsidian-style callouts (`[!info]-` with folding): later.

### Themes
- **v1:** Follow system (default), Light, Dark. Use CSS custom properties defined in `@media (prefers-color-scheme)` blocks plus a `[data-theme]` override, the same structure github-markdown-css uses. Set `color-scheme: light dark` on the root so scrollbars and form controls follow the theme.
- **WebView2:** `CoreWebView2Profile.PreferredColorScheme` (Auto, Light, Dark; default Auto follows the OS) drives `prefers-color-scheme` for content and also WebView2's own dialogs and context menus. Set it from the app's theme setting rather than hard-coding colors.
- **WKWebView:** follows the app's or window's effective NSAppearance. WebKit does not darken pages automatically; the page has to opt in with `color-scheme`. To force light or dark inside the app, set the window or app appearance (`NSApp.appearance`) rather than switching CSS by hand, so `prefers-color-scheme` stays correct. One known WebKit bug (220367) mis-evaluated the media query for CSS variables in a fullscreen Quick Look WKWebView on Big Sur, so test the Quick Look path separately.
- **Colors:** body text contrast at least 7:1 (WCAG AAA 1.4.6). Don't use pure black on pure white. Something like #1f2328 on #ffffff works for light, and #e6e6e6 on #16181c for dark (illustrative; check them with a contrast tool).
- **Later:** Sepia (warm background around #f4ecd8 with dark brown text; illustrative values) and a Gray theme. Firefox Reader View offers Auto, Light, Dark, Sepia, Contrast and Gray, which is a reasonable target set.
- **High contrast:** respect `forced-colors: active` (Windows contrast themes, supported in Chromium/Edge, so WebView2 should inherit it; verify) and `prefers-contrast: more` (macOS Increase Contrast). Don't add a separate "high contrast theme" in v1. Make the CSS degrade cleanly with system colors and visible borders.
- **Syntax highlighting pairs:** GitHub Light with Light, GitHub Dark with Dark, GitHub Light/Dark High Contrast with forced-colors or prefers-contrast. For sepia later, Solarized Light or a custom warm palette. A highlighter that supports dual themes switched by CSS variables (Shiki-style) avoids re-rendering when the theme changes. The engine workstream owns the highlighter choice.

### Print
- Print CSS in v1: white background, black text whatever the theme; hide the sidebar and toolbar; `break-after: avoid` on headings; `break-inside: avoid` on pre, table, figure, blockquote and alerts; `orphans: 3; widows: 3`; print link URLs after external links (optional); repeat table headers across pages (`thead { display: table-header-group }`); force code to wrap in print.
- "Save as PDF" comes from the OS print dialog on both platforms, so v1 doesn't need a separate PDF export.

## Feature matrix

v1 = needed at launch. Later = worth doing once the core is solid. Never = conflicts with being a lightweight read-only viewer.

| Area | Feature | Verdict | Why |
|---|---|---|---|
| Render | GFM: tables, task lists, strikethrough, autolinks, footnotes, alerts | v1 | READMEs assume GitHub semantics |
| Render | Syntax highlighting, light/dark paired | v1 | Code-heavy docs are unreadable without it |
| Render | Front matter hidden or shown as a collapsed metadata block | v1 | Raw YAML at the top looks broken |
| Render | Mermaid, KaTeX/MathJax (lazy-loaded) | Later (early) | Common in AI and dev docs; engine workstream decides |
| Render | Relative images and links resolved against the file | v1 | Without this, nothing works |
| Typography | Prose measure cap with Narrow/Default/Wide/Full | v1 | The core "optimized for eyes" claim |
| Typography | Font choice (Sans / Serif / bundled Reading font) | Later | System sans is enough at launch |
| Theme | Follow system / Light / Dark | v1 | Expected baseline |
| Theme | forced-colors / prefers-contrast support | v1 | Mostly CSS; accessibility |
| Theme | Sepia, Gray, custom CSS themes | Later | Nice to have; custom CSS draws power users |
| Nav | Outline sidebar (toggle, highlights current section) | v1 | Long docs are hard to use without it; Marked, Typora and iA all have one |
| Nav | Heading anchor links (hover "#", copy link) | v1 | Cheap, and needed for in-document links |
| Nav | Find in page (Ctrl/Cmd+F, Enter/Shift+Enter, match count) | v1 | Expected in any reading tool |
| Nav | Zoom (Ctrl/Cmd +/-/0), remembered globally | v1 | Accessibility and distance reading |
| Nav | Per-file reading position memory | v1 | Cheap (path + scroll % in a small store) |
| Nav | Keyboard: Space/Shift+Space, j/k, g/G and Home/End, [ / ] for previous/next heading, ? for help | v1 | Keyboard-first readers; Marked has a similar set |
| Nav | Back/forward when following links to other .md files (Alt+Left/Right, Cmd+[ / ]) | v1 | Docs folders link to each other |
| Nav | Breadcrumbs or a folder file tree | Later | Moves toward being a "docs browser"; keep scope tight |
| Nav | Quick-open across folder (Ctrl+P) | Later | Same reason |
| Live | Auto-reload on external change, scroll position kept, debounced about 100ms | v1 | Advertised by every viewer we found (DBV, PrettyMark, Markpad, mdpreview, Markdown Live Viewer) |
| Live | Small "Updated" pulse in the status area (no modal) | v1 | Shows the reload happened without interrupting |
| Live | Deleted file: keep last render, show "File was deleted" banner; renamed: follow if detectable, else same banner | v1 | Atomic-save editors make this common |
| Live | Scroll to the first changed block (Marked's "scroll to first edit") | Later | Useful for side-by-side editing |
| Chrome | Native title bar showing file name (and dirty path on hover) | v1 | Native feel, accessible, free window behaviors |
| Chrome | Remember window size/position (per display) | v1 | Users complain loudly when this is missing |
| Chrome | One window per document (macOS-native); focus existing window if the same file is opened again | v1 | Matches macOS Preview; simple |
| Chrome | Tabs | Later | Requested by Windows users (PrettyMark ships it), but not needed at launch |
| Chrome | Frameless/custom title bar | Later | Not v1: costs accessibility and native behavior for little gain |
| Chrome | Toolbar: none by default; status line with outline toggle, theme, zoom % | v1 | Minimal-chrome philosophy (iA, Marked, macOS Preview) |
| Actions | Open in editor (Ctrl/Cmd+E): configured editor, else OS "edit" verb, else default text editor | v1 | Viewer-to-editor handoff is the most common next step |
| Actions | Reveal in Explorer/Finder, copy file path | v1 | Cheap |
| Actions | Recent files (File menu, macOS Dock menu, Windows Start/jump list via shell) | v1 for menu; jump list later | |
| Export | Print with print CSS; PDF through the OS dialog | v1 | Almost free with a webview |
| Export | Copy selection as rich text | v1 (native webview behavior) | Free |
| Export | "Copy as HTML", export standalone HTML, direct PDF export with options | Later | Useful, but a viewer is not a publisher |
| Export | DOCX/EPUB export, Pandoc integration | Never | That is Marked 2's niche and pulls toward a toolchain |
| Platform | File association, Open With, drag-drop, CLI, single instance | v1 | The core "double-click and read" promise |
| Platform | macOS Quick Look extension | Later (first post-v1 item on mac) | High value, separate signed target; see below |
| Platform | Windows Explorer preview handler | Later at best, possibly never | PowerToys already provides it |
| Platform | macOS Services menu ("Open in MDView") | Later | Low demand |
| Scope | Editing, split view, plugins, sync | Never | It is a viewer |

### What the reference apps get right
- **Marked 2:** The chrome is a thin status bar. The TOC opens from a status-bar icon or Cmd+T, j/k move through it, Return opens a heading, and "f" opens it with a filter field focused. Numbered scroll bookmarks survive refreshes, "scroll to first edit" follows changes, and h or ? shows every shortcut. Lesson: keyboard-first navigation built into a minimal surface.
- **iA Writer:** One typeface and one size on purpose, because "defining typeface and text size is not a matter of taste". Size later scaled with window width (responsive typography), so zooming keeps proportions without reflowing the text. Lesson: ship strong defaults, not a settings panel.
- **Typora** (general knowledge, not verified this pass): almost no visible chrome, the document fills the window, and the outline and file tree sit in a sidebar that can be hidden. Lesson: content first, with navigation one shortcut away.
- **macOS Preview** (general knowledge, not verified this pass): native title bar with the document name and proxy icon, a sidebar toggle, one window per document with native tabs, and windows restored where they were left. Lesson: lean on native window behavior instead of rebuilding it.

## Windows 11 integration

### Context: the baseline moved
Notepad on Windows 11 now has "lightweight formatting" with Markdown-style input and files, including a toggle between formatted and raw views. It was announced on the Windows Insider blog on 2025-05-30 (Notepad 11.2504.50.0) and has since been reported as generally available, with markdown formatting on by default. So the pitch is no longer "Windows shows raw markdown." The app has to beat Notepad on typography, GFM fidelity (tables, code highlighting, alerts, images), navigation and live reload. Pass this to the competitors workstream.

### File association (installer)
- Register a ProgID such as `MDView.Markdown` with `DefaultIcon`, `FriendlyTypeName`, and `shell\open\command = "C:\...\mdview.exe" "%1"`.
- Under each extension (`.md`, `.markdown`, `.mdown`, `.mkd`, `.mkdn`, `.mdwn`), add the ProgID to `OpenWithProgids`. This puts the app in "Open with" without taking over the default.
- Declare Default Programs capabilities: `Software\MDView\Capabilities` with `ApplicationName`, `ApplicationDescription`, `FileAssociations` (extension to ProgID), then a value under `Software\RegisteredApplications` pointing to that Capabilities path. Microsoft's Default Programs docs require this explicit per-extension registration for the app to be offered as a default. The app-specific ProgID protects its data if another app takes over the extension later.
- Prefer a per-user install (HKCU) so no elevation is needed and it works on locked-down machines. MSI, NSIS and MSIX can all do this. MSIX declares associations in the manifest (`uap:FileTypeAssociation`) instead.
- **`.mdx`:** don't claim it. MDX is JSX plus markdown and can't be rendered faithfully. At most add `OpenWithProgids` (Open with only) and render it as markdown with imports/JSX shown as code, plus a notice. Never register `.txt`.

### Becoming the default (hardest Windows item)
- Windows protects the per-user choice under `UserChoice` with a hash. Writing ProgId/Hash directly doesn't work, and the UserChoice Protection Driver (UCPD.sys) actively blocks tampering. Community tools like SetUserFTA reverse-engineer the hash and reportedly broke after a Windows update. An installer that pokes UserChoice will generate bug reports and possibly AV flags. Don't do it.
- **Supported path:** after install, the first time a user double-clicks a `.md` file, Windows shows "Select an app" with the app listed (because of the registration above). Also, on first run, offer a "Make MDView the default for markdown" button that launches `ms-settings:defaultapps?registeredAppUser=<escaped RegisteredApplications name>` (use `registeredAppMachine` for per-machine installs, `registeredAUMID` for MSIX). This deep link opens the app's own Default Apps page on Windows 11 21H2/22H2 with the 2023-04 cumulative update, and on 23H2 or later. On older builds, fall back to plain `ms-settings:defaultapps`.
- Show the button only while the app is not the default. `IApplicationAssociationRegistration::QueryCurrentDefault` is still supported for checking.

### winget
- winget runs the app's own installer, so associations come from the installer, not winget. The winget installer manifest has `FileExtensions` and `Commands` metadata fields (from general knowledge of the winget manifest schema, not re-verified in this pass). Fill them in for discoverability. PrettyMark (a WebView2 viewer) already ships through winget, so this is the normal route.

### CLI (`mdview README.md`)
- Add the install folder (or a `bin` subfolder holding only the launcher) to the user PATH from the installer. MSIX can use an App Execution Alias instead.
- A GUI-subsystem exe launched from a terminal returns to the prompt immediately, which is the right behavior for a viewer. Resolve relative paths against the caller's working directory before forwarding to a running instance.
- Later: `mdview -` to read stdin (useful for `some-tool | mdview -`), and `--wait` for scripts.

### Single instance and windows
- Use a named mutex plus a named pipe (or the framework's single-instance plugin; stack workstream). The second launch forwards argv and cwd to the first and exits. The first instance opens a new window, or focuses the existing window if that file is already open.
- Multi-select "Open" in Explorer launches one process per file or passes several paths depending on the verb (`%1` vs `%*` / DDE-less `MultiSelectModel`). Single-instance forwarding handles both.

### Drag and drop
- Dropping onto the window: handle it in the host or webview and open the file in the current window, or a new one if the current document has history. Dropping onto the exe or a shortcut works for free (the path arrives in argv).

### Jump lists and recent files
- Once the app is registered for the file type, the shell can populate a "Recent" category automatically for files opened through it, and the app can add to it with `SHAddToRecentDocs`. Mark as later. A File > Recent menu is v1.

### Explorer preview pane (IPreviewHandler): later at best
- How it works: a COM object implementing `IPreviewHandler` (+ `IInitializeWithStream` or File), registered under the ProgID as `shellex\{8895b1c6-b41f-4c1c-a562-0d564250836f}` = handler CLSID, and listed under `...\CurrentVersion\PreviewHandlers`. It runs out of process in the `prevhost.exe` surrogate (AppID `{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}`). Managed handlers should get their own AppID with `DllSurrogate` so they don't share the host process. MSIX can declare one with `desktop2:DesktopPreviewHandler`.
- Hosting a WebView2 inside it is doable (PowerToys does this for Markdown), but it adds COM registration, a second rendering host, bitness and integrity-level issues, and override conflicts: per Microsoft, enabling the PowerToys handlers overrides other installed preview handlers.
- PowerToys already gives any Windows user a Markdown preview pane. Verdict: not v1. Reconsider only if the renderer is a self-contained native library that drops in cheaply.

### Live reload on Windows
- `ReadDirectoryChangesW` on the parent directory, filtered to the target name, catches the temp-file-and-rename saves many editors use. Debounce about 100ms, then re-read. Fall back to polling (mtime and size every 1–2s) on network shares and some sync folders (OneDrive placeholders), where change notifications are unreliable.

## macOS integration

### File association
- In `Info.plist`, add `CFBundleDocumentTypes` with one entry: `CFBundleTypeName` "Markdown Document", `CFBundleTypeRole` **Viewer**, `LSItemContentTypes` = [`net.daringfireball.markdown`], and `LSHandlerRank` **Alternate** (or **Default**; Typora uses Default with role Editor). As a viewer, Alternate is the polite choice: the app shows up in Open With without fighting editors. Users who want it as the default use Get Info > Open with > Change All.
- Add `UTImportedTypeDeclarations` for `net.daringfireball.markdown` (conforms to `public.plain-text`, extensions `md`, `markdown`, `mdown`, `mkd`, `mkdn`). There is no system-declared markdown UTI, so each app imports it. Some files show up with other UTIs (other apps' exported types, or dynamic `dyn.*` types), so test with files created by Obsidian, VS Code and Xcode. QLMarkdown explicitly handles several dynamic UTIs for this reason.
- Declaring the document type is also what makes Dock-icon drag-drop and Finder "Open With" work. Nothing else is needed.
- Default app on install: nothing to do. Optionally, a settings button calls the macOS 12+ NSWorkspace "set default application" API (not verified in this pass whether it shows a user confirmation for this content type). Xcode often ends up as the `.md` default after it is installed, so an in-app "Make default" button helps.

### Opening files, single instance, multiple windows
- macOS is single-instance per app by default. Launch Services routes Finder double-clicks, `open README.md`, `open -a MDView README.md` and Dock drops to the running app as open-document events (`application(_:open:)` / `openURLs`). The framework has to surface these, and must also handle files that arrive before the first window exists (cold launch from Finder). A missed cold-launch event is a classic bug in cross-platform frameworks; stack workstream should check.
- One window per document, with Window menu and native tabs (`NSWindow` tabbing gives "Merge All Windows" for free if allowed). Reopen previous windows on launch according to the system "Close windows when quitting" setting.

### CLI shim
- Ship a shim that runs `open -a "MDView" -- "$@"` (with paths made absolute, plus a `-W` option for wait). Install it through the Homebrew cask `binary` stanza (lands in `/opt/homebrew/bin` on Apple silicon) and an in-app "Install command line tool" menu item, the same pattern as VS Code's "Install 'code' command in PATH". Writing to `/usr/local/bin` needs admin rights on a clean system, so the menu item has to prompt for authorization or offer `~/.local/bin`.

### Quick Look extension (hardest macOS item)
- Why mac markdown apps ship one: pressing space in Finder (and the Finder preview column) is the main mac "glance at a file" flow, and without an extension, markdown shows as plain text. Marked, QLMarkdown and others ship Quick Look extensions.
- Since macOS 15 Sequoia, legacy `.qlgenerator` plug-ins no longer load. It has to be an app extension (`.appex` in `Contents/PlugIns`), either view-based (`QLPreviewingController`, often hosting a WKWebView) or data-based (`QLPreviewProvider` returning a `QLPreviewReply`, which can be HTML).
- Practical constraints seen in QLMarkdown:
  - The host app must be launched once before the extension is discovered.
  - The extension is sandboxed. To show local images, QLMarkdown needed a broad read-access exception and also inlines images as data.
  - It ships a Content Security Policy that blocks untrusted JS; Mermaid runs in strict mode and MathJax loads only when needed.
  - Only one extension handles a given type, and other installed previewers can win silently.
  - Re-signing after install made Quick Look silently fall back to plain text in one developer's debugging write-up.
  - `qlmanage -m` doesn't list third-party appex handlers, so debugging is painful.
- Framework note: Electron and Tauri don't produce an appex natively. It is a separate Swift target, built, signed and notarized alongside the app. It can reuse the same HTML/CSS renderer if the renderer can run as JS inside WKWebView or as a native library.
- Verdict: the first item after v1 on macOS, not a v1 blocker. Build v1 so the renderer can be reused there.

### Sandboxing and distribution
- A sandboxed (Mac App Store) build can read the file the user opened, but not sibling files, without extra grants. Relative images, links to other `.md` files and folder navigation would then need security-scoped bookmarks or a "grant access to this folder" prompt. Recommend direct distribution (notarized DMG plus a Homebrew cask) for v1, unsandboxed or with a narrow temporary exception, and revisit the App Store later. Distribution workstream should confirm.

### Services menu
- An NSServices entry such as "Open Markdown in MDView" for selected Finder files, or "Preview selection as Markdown" for selected text. Later; low demand.

### Live reload on macOS
- FSEvents (or a dispatch source on the parent directory) catches atomic-rename saves; debounce and re-read. iCloud Drive files that haven't downloaded yet need a "Downloading..." state rather than an error.

## Accessibility notes
- **Semantics:** render real HTML semantics (h1–h6 in order, lists, `table` with `th scope`, `figure`/`img alt` from markdown alt text, `nav` landmark for the outline with `aria-current` on the active heading, `main` for content). WebView2 exposes this to UI Automation (Narrator, NVDA, JAWS) and WKWebView to VoiceOver, so a correct DOM is most of the work.
- **Keyboard:** everything reachable without a mouse. Add a skip-to-content link when the outline is visible. Code blocks and wide tables get `tabindex="0"` and an accessible label so they can be scrolled with arrow keys (WCAG 2.1.1). Keep the focus ring visible (don't remove outlines). Single-key shortcuts (j/k, g/G) must not fire while focus is in the find box or any input, and need a way to turn them off (WCAG 2.1.4 Character Key Shortcuts).
- **Task lists:** render checkboxes as `disabled` inputs with labels so screen readers say "checked" or "not checked".
- **Reduced motion:** with `prefers-reduced-motion: reduce`, no smooth scrolling for outline jumps, find, or g/G, and no animated reload pulse (use a static indicator).
- **Font scaling:** Ctrl/Cmd zoom from 50% to 300%, layout reflowing without horizontal scroll down to 320 CSS px (WCAG 1.4.10; data tables and code may scroll). The outline sidebar collapses to an overlay below about 720px. Check separately whether WebView2 follows the Windows "Text size" accessibility setting. If not, read it and apply it as the initial zoom.
- **Text spacing:** use no fixed heights or `overflow: hidden` on text containers so user overrides (line-height 1.5, paragraph spacing 2×, letter spacing 0.12em, word spacing 0.16em) never clip content (WCAG 1.4.12).
- **Contrast:** body text at least 7:1, UI at least 4.5:1, borders and callout edges at least 3:1 (WCAG 1.4.11). Use `forced-colors: active` with system colors (`Canvas`, `CanvasText`, `LinkText`, `Highlight`) and don't fight the user palette. Callout colors disappear in forced colors, so the alert title text and icon must carry the meaning. Respond to `prefers-contrast: more` with stronger borders and muted-text colors.
- **Color is not the only cue:** link underlines on by default in body text. Alert types are named in text ("Warning"), not only colored.
- **Window title:** the document name, so assistive tech and the taskbar or Window menu announce it.

## Notes on evidence quality
- Search did not turn up user complaint threads (Reddit and similar). The "users complain when missing" judgment uses which features competing viewers put first on their product pages (auto-reload with scroll preservation, dark mode, tabs, fast startup, small memory) plus well-known platform conventions. Treat the v1 cut as a reasoned proposal, not survey data.
- Unverified in this pass (flagged inline): Cascadia Mono shipping with Windows 11, winget `FileExtensions`/`Commands` fields, the macOS 12+ NSWorkspace default-app API behavior, WebView2 honoring forced-colors and Windows text-size settings, Inter/iA font file sizes, the illustrative sepia and dark color values, Explorer `MultiSelectModel` behavior, whether `open -a` accepts a `--` separator, and the Typora and macOS Preview descriptions.

## Sources (all accessed 2026-10-26)

Typography and layout
- Butterick, Practical Typography, "Line length": https://practicaltypography.com/line-length.html
- Butterick, Practical Typography, "Summary of key rules": https://practicaltypography.com/summary-of-key-rules.html
- github-markdown-css README (980px width, 45px/15px padding): https://github.com/sindresorhus/github-markdown-css
- github-markdown-css stylesheet (font stacks, heading scale, code, tables, alerts, color tokens): https://raw.githubusercontent.com/sindresorhus/github-markdown-css/main/github-markdown.css
- Tufte CSS (page): https://edwardtufte.github.io/tufte-css/
- Tufte CSS (stylesheet values): https://raw.githubusercontent.com/edwardtufte/tufte-css/gh-pages/tufte.css
- iA, "Responsive Typography: The Basics": https://ia.net/topics/responsive-typography-the-basics
- iA, "Bringing responsiveness to the app world" (iA Writer for Mac font size decisions): https://ia.net/topics/bringing-responsiveness-the-app-world
- Modern Font Stacks: https://modernfontstacks.com/
- W3C CSSWG archive, system-ui font discussion (stack ordering caveat): https://lists.w3.org/Archives/Public/public-css-archive/2019Sep/0089.html
- Mozilla Support, Firefox Reader View (width, spacing, fonts, Auto/Light/Dark/Sepia/Contrast/Gray themes): https://support.mozilla.org/kb/firefox-reader-view-clutter-free-web-pages
- Inter typeface repository (OFL 1.1, reserved font name): https://github.com/rsms/inter
- iA Writer fonts packaging (OFL, IBM Plex derived; repo link iaolo/iA-Fonts): https://www.freshports.org/x11-fonts/ia-writer-fonts/
- iA Writer Duospace on Font Squirrel (OFL): https://www.fontsquirrel.com/fonts/ia-writer-duospace
- JetBrains Mono repository (OFL 1.1): https://github.com/JetBrains/JetBrainsMono

Reference apps
- Marked 2, document navigation tips (TOC, j/k, bookmarks): https://brettterpstra.com/2017/08/08/marked-2-tips-table-of-contents/
- Marked 2, long-form reading: https://brettterpstra.com/2013/11/01/marked-2-tips-long-form-reading-with-marked/
- Marked help, interface features: https://markedapp.com/help/Marked_Basics/Interface_Features.html
- Marked Quick Look: https://markedapp.com/help/Quick_Look
- DBV Markdown Reader (auto-reload keeping scroll position, Tauri): https://www.producthunt.com/products/dbv-markdown-reader
- PrettyMark (WebView2, live reload, tabs, winget): https://wingetly.io/apps/eagle1/pretty-mark
- mdpreview (auto-reload preserving scroll): https://github.com/fboender/mdpreview
- Markdown Live Viewer (scroll preserved on update): https://github.com/kevindra/markdown-live-viewer
- Typora bundle document types (Editor role, Default rank): https://doesitarm.com/app/typora

Themes and webviews
- ICoreWebView2Profile (PreferredColorScheme): https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2profile
- Use Your Loaf, "Supporting Dark Mode in WKWebView": https://useyourloaf.com/blog/supporting-dark-mode-in-wkwebview/
- Indiestack, "Supporting Dark Mode: In-App Web Content": https://indiestack.com/2018/10/
- WebKit Bugzilla 220367 (prefers-color-scheme in Quick Look WKWebView): https://bugs.webkit.org/show_bug.cgi?id=220367
- Chrome DevTools, emulate CSS media features (forced-colors, prefers-reduced-motion): https://developer.chrome.com/docs/devtools/rendering/emulate-css
- Kilian Valkhof, "prefers-contrast: forced is a mistake": https://kilianvalkhof.com/2021/css-html/prefers-contrast-forced-is-a-mistake/

Windows
- Microsoft Learn, Default Programs (Capabilities, RegisteredApplications, ProgIDs): https://learn.microsoft.com/en-us/windows/win32/shell/default-programs
- Microsoft Learn, IApplicationAssociationRegistration (only QueryCurrentDefault supported since Windows 8): https://learn.microsoft.com/en-us/windows/desktop/api/shobjidl_core/nn-shobjidl_core-iapplicationassociationregistration
- Microsoft Learn, Launch the Default Apps settings page (registeredAppUser/Machine/AUMID, OS floor): https://learn.microsoft.com/en-us/windows/apps/develop/launch/launch-default-apps-settings
- Microsoft Learn, Default apps platform guidance: https://learn.microsoft.com/en-us/windows/apps/develop/windows-integration/default-apps-platform
- gHacks, Windows 11 default apps protection: https://www.ghacks.net/?p=173104
- Atera, changing file associations in Windows 10/11 (UserChoice hash, UCPD, SetUserFTA): https://www.atera.com/blog/how-to-change-file-associations-in-windows-10-and-11/
- Microsoft Learn, Preview handlers overview: https://learn.microsoft.com/en-ie/Windows/Win32/shell/preview-handlers
- Microsoft Learn, How to register a preview handler: https://learn.microsoft.com/en-ie/Windows/Win32/shell/how-to-register-a-preview-handler
- Microsoft Learn, IPreviewHandler interface: https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ipreviewhandler
- Microsoft Learn, PowerToys File Explorer add-ons (Markdown preview, override caveat): https://learn.microsoft.com/windows/powertoys/file-explorer
- Windows Insider Blog, Text formatting in Notepad (2025-05-30): https://blogs.windows.com/windows-insider/2025/05/30/text-formatting-in-notepad-begin-rolling-out-to-windows-insiders/
- Winaero, Notepad markdown support generally available: https://winaero.com/microsoft-makes-notepad-markdown-support-generally-available
- BleepingComputer, hands-on with Notepad markdown: https://www.bleepingcomputer.com/news/microsoft/hands-on-with-windows-11-notepads-new-markdown-support

macOS
- Apple, Registering the File Types Your App Supports (CFBundleDocumentTypes, LSItemContentTypes, LSHandlerRank): https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/DocumentInteraction_TopicsForIOS/Articles/RegisteringtheFileTypesYourAppSupports.html
- Wikipedia, Uniform Type Identifier (net.daringfireball.markdown): https://en.wikipedia.org/wiki/Uniform_Type_Identifier
- Christian Tietze, "Sorting Out Overlapping File Types" (markdown UTIs): https://forum.christiantietze.de/posts/2020/05/sorting-out-file-types/
- CommonMark forum, UTI of a CommonMark document: https://talk.commonmark.org/t/uti-of-a-commonmark-document/2406
- Apple, QLPreviewProvider: https://developer.apple.com/documentation/quicklookui/qlpreviewprovider
- Apple, Quick Look framework: https://developer.apple.com/documentation/quicklook
- Eclectic Light, "How Sequoia has changed QuickLook and its thumbnails": https://eclecticlight.co/2024/10/31/how-sequoia-has-changed-quicklook-and-its-thumbnails/
- Eclectic Light, "Explainer: QuickLook" (2026-05-16): https://eclecticlight.co/2026/05/16/explainer-quicklook/
- Michael Tsai, "Sequoia No Longer Supports QuickLook Generator Plug-ins": https://mjtsai.com/blog/2024/11/05/sequoia-no-longer-supports-quicklook-generator-plug-ins
- QLMarkdown README (sandbox, CSP, host launch, UTIs): https://raw.githubusercontent.com/sbarex/QLMarkdown/main/README.md
- QLMarkdown WKWebView commit: https://github.com/sbarex/QLMarkdown/commit/2c61b3462f2ceb9e1e26d1d78c4c45b5d461d048
- DEV, debugging a markdown Quick Look extension (signing gotcha): https://dev.to/nguyendinhdoan/debugging-a-markdown-quick-look-extension-signing-file-changes-and-wkwebview-2429
- MacStories, changing default apps for file types: https://club.macstories.net/posts/changing-default-apps-for-specific-file-types-on-macos

Accessibility
- Deque, WCAG 1.4.12 Text Spacing: https://dequeuniversity.com/resources/wcag2.1/1.4.12-text-spacing
- Deque, Text styles, resize, reflow and zoom checklist: https://dequeuniversity.com/checklists/web/text
- Silktide, WCAG 1.4.12 guide: https://silktide.com/accessibility-guide/the-wcag-standard/1-4/distinguishable/1-4-12-text-spacing/
