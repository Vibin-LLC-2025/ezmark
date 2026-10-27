# Contributing

Thanks for looking. ezmark is a read-only viewer with a deliberately small
scope: before proposing a feature, check the "never" and "later" lists in the
[README](README.md#scope). Issues that ask for editing, plugins, or Mermaid in
v1 are closed with a link there.

## Setup

Readers need Rust and the platform C toolchain only (see the README).
Contributors may also want Node 22 or newer for the type-check below.

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
```

Clippy runs twice because harness code is compiled only with the feature.
The security harness opens real app windows for a few seconds each; it needs a
desktop session, not a headless one.

## The security harness

`crates/app/tests/harness.rs` spawns the `test-harness` build once per fixture.
To add a fixture:

- Sanitized corpus (`tests/security/sanitized/`): markdown that goes through
  the full pipeline. Use `{{LOOPBACK}}` for any URL that must never be
  fetched. Expected: no executable content, no CSP reports, no connections.
- Enforcement corpus (`tests/security/enforcement/`): raw HTML injected past
  the sanitizer. Expected: an enforced CSP report per payload, no connections.

`scripts/check-release-artifact.sh` checks that no harness marker reaches a
release binary. If you add a harness command, add its name to that script.

## Commits and pull requests

- [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`,
  `fix:`, `docs:`, `test:`, `ci:`, `chore:`), small and focused.
- Pull requests are squash-merged, even the author's own.
- CI must be green on both runners: format, clippy, tests, harness, and a
  release build.
- Any change to the security contract updates ADR 0003 and adds a fixture.

## Icons

`crates/app/icons/` is generated:

```sh
node scripts/make-icon.mjs icon-source.png
cargo install tauri-cli --version 2.12.4 --locked
cargo tauri icon icon-source.png -o crates/app/icons
```

Then delete the Android, iOS, and Store variants, which ezmark does not use.
