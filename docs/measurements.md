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

Filled in from the first green CI run.
