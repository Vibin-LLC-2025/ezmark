# ADR 0004: A reader needs only Rust

- Status: accepted
- Date: 2026-10-27
- Source: [clone-and-run plan](../CLONE-AND-RUN-PLAN.md) section 4

## Context

The one-command promise (`cargo run --release -- file.md`) fails on someone
else's machine at every extra prerequisite. A JavaScript build step would add
Node, a package manager, a lockfile install, and network fetches to the
reader's path.

## Decision

- The view layer is plain HTML, CSS, and ES-module JavaScript in `ui/`,
  embedded into the binary at compile time (`frontendDist` points at the
  folder; there is no `devUrl` and no `beforeDevCommand`).
- JavaScript reaches Rust through `app.withGlobalTauri`, so no npm package is
  needed at runtime.
- Syntax highlighting happens in Rust (syntect, from M1), so there is no
  JavaScript highlighter.
- The only third-party JavaScript will be idiomorph (from M3), vendored as one
  file with its provenance recorded in `ui/vendor/VENDOR.md`.
- JavaScript carries JSDoc types and stays small (target under 600 lines). A
  `package.json` and lockfile pin TypeScript for contributors' type-check,
  which CI also runs. Readers never need Node.
- Nothing beyond normal Cargo dependency downloads is fetched at build time.
- Mermaid and KaTeX are deferred to v1.1 as a scope decision. Their v1.1
  approach gets its own ADR, and build-time fetching is ruled out for it.

## Consequences

- Prerequisites stay at Rust, the platform C toolchain (Build Tools or Xcode
  Command Line Tools), and git.
- Contributors get type errors locally before a push, through a pinned tool.
- No bundler means no tree-shaking or minification; at this size that costs
  nothing measurable.
- Scope requests that need a JS dependency (Mermaid in v1, plugins) are
  declined with a link to this ADR.
