# ADR 0001: Tauri 2 with the system webview

- Status: accepted
- Date: 2026-10-27
- Evidence: [feasibility report](../FEASIBILITY-REPORT.md) section 2, [stack research](../research/02-stack-feasibility.md)

## Context

ezmark is a read-only markdown viewer for Windows 11 and macOS that anyone can
clone and run with one command. It needs browser-grade text rendering, text
selection, find in page, and print, on two operating systems, from a solo
developer's codebase small enough to read in an afternoon.

Candidates were Tauri 2, Electron, Wails v3, a pure-Rust GUI (iced, egui),
native UI per platform, and a local server with the default browser.

## Decision

Use Tauri 2 (2.12.x) with the system webview: WebView2 on Windows, WKWebView
on macOS. The engine is a plain Rust library (`crates/mdcore`); the Tauri
shell (`crates/app`) stays thin.

## Why

- A webview gives selection, find, tables, and print for free. Pure-Rust GUIs
  would rebuild all of that; native-per-platform doubles the UI code.
- Disk size: the measured win over Electron is size (a 2026 benchmark put a
  macOS bundle at 57 MB against 323 MB), not RAM or startup, where the two are
  close. The README claims small and auditable, never "10x faster".
- Official plugins and config cover the desktop chores (single instance,
  file associations, window state), keeping custom glue small.
- Wails v3.0.0 shipped on the day of the research with two advisories its own
  notes call partly fixed.
- A Rust core with a measured, tested pipeline is a stronger portfolio signal
  than a JS-only stack.

## Consequences

- Rendering parity is the main risk: WKWebView is frozen to the macOS
  version while WebView2 is evergreen. Mitigations: `minimumSystemVersion`
  13.0 as the build floor, CSS written against that baseline, and screenshot
  tests on both CI runners. The README says "tested on" with real versions
  rather than promising a floor.
- Two file-open paths: macOS delivers Finder and `open` requests as
  `RunEvent::Opened` (buffered until the frontend is ready); Windows delivers
  argv, and the single-instance plugin forwards second launches. M0 proved
  both.
- Local images need a custom protocol rather than `file://`; see ADR 0003.
- Tauri 3 is in alpha (3.0.0-alpha.5 on 2026-10-14). Keeping the Tauri surface
  small keeps that migration small.
- First compiles take minutes. The README states the measured number.
