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
