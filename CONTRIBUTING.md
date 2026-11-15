# Contributing

Thanks for looking. ezmark is a read-only viewer with a deliberately small
scope: before proposing a feature, check the "never" and "later" lists in the
[README](README.md#scope). Issues that ask for editing, plugins, or Mermaid in
v1 are closed with a link there.

## Setup

Readers need Rust and the platform C toolchain only (see the README).
Contributors may also want Node 22 or newer for the type-check and UI test below.

The Rust toolchain is pinned in `rust-toolchain.toml`; rustup installs it on
the first `cargo` command.

## Everyday commands

```sh
cargo run -- path/to/file.md                              # debug build, embedded UI
cargo test --workspace --features ezmark/test-harness     # unit tests and the security harness
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features ezmark/test-harness -- -D warnings
npm ci && npm run typecheck                               # contributor-only: JSDoc type-check of ui/
npm test                                                  # contributor-only: ui/ against fake Tauri and DOM globals
cargo deny check                                          # optional: dependency audit (cargo install cargo-deny)
EZMARK_BLESS=1 cargo test -p mdcore --test conformance    # accept a change in pipeline output, then read the diff (tests/pipeline/README.md)
cargo run -p mdcore --example html -- file.md             # print the HTML the engine makes of a file
cargo run --release -p mdcore --example timing            # stage timings for docs/measurements.md
```

Clippy runs twice because harness code is compiled only with the feature.
The security harness opens real app windows for a few seconds each; it needs a
desktop session, not a headless one.

Two checks drive the real desktop and so are scripts, not tests:
`scripts/dragdrop-windows.ps1` drags a real file onto the window, and
`scripts/screenshot-windows.ps1` and `scripts/screenshot-macos.sh` capture it.
CI runs them; run them by hand when you touch what they cover.
`scripts/netwatch-windows.ps1` records every connection the app's process
tree makes while it runs, for the numbers in `docs/measurements.md`.

## The security harness

`crates/app/tests/harness.rs` spawns the `test-harness` build once per fixture.
To add a fixture:

- Sanitized corpus (`tests/security/sanitized/`): markdown that goes through
  the full pipeline. Use `{{LOOPBACK}}` for any URL that must never be
  fetched (and `{{LOOPBACK_AUTHORITY}}` for its `host:port` alone). Expected:
  no executable content, no CSP reports, no connections.
- Enforcement corpus (`tests/security/enforcement/`): raw HTML injected past
  the sanitizer, so the layers behind it have to hold. Which file a payload
  belongs in depends on what is supposed to stop it:
  - `resources.html`: the CSP alone. This test runs with the network lock
    off, so a payload here must be one a CSP directive governs. Expected: no
    connections, and an enforced report for the loads the test names.
  - `speculative.html`: the network lock. For connections the webview opens
    on its own account, which no directive governs.
  - `navigation.html`: the click handler, `form-action`, and the navigation
    guard. The harness script clicks the elements with `probe-` ids.

Give each payload its own URL path, so a stray connection names its source.
The harness never waits a fixed time for something not to happen: it waits
for expected events by name (`EZMARK_HARNESS_EXPECT_BLOCKED`,
`EZMARK_HARNESS_EXPECT_DENIED_NAVIGATIONS`), then provokes one last CSP
report and waits for that. Keep it that way.

`scripts/check-release-artifact.sh` checks that no harness marker reaches a
release binary. If you add a harness command or environment switch, add its
name to that script.

## Commits and pull requests

- [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`,
  `fix:`, `docs:`, `test:`, `ci:`, `chore:`), small and focused.
- Pull requests are squash-merged, even the author's own.
- CI must be green on both runners: format, clippy, tests, harness, the
  dependency audit, and a release build. `main` requires it.
- Any change to the security contract updates ADR 0003 and adds a fixture.

## Icons

`crates/app/icons/` is generated:

```sh
node scripts/make-icon.mjs icon-source.png
cargo install tauri-cli --version 2.12.4 --locked
cargo tauri icon icon-source.png -o crates/app/icons
```

Then delete the Android, iOS, and Store variants, which ezmark does not use.
