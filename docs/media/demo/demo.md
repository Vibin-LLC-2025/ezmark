# ezmark demo document

A read-only markdown viewer. This file is what the screenshots show, so it
exercises what the current build does: GitHub-flavoured rendering, sanitizing,
a relative image served through the asset protocol, and a remote image that is
never fetched.

![ezmark icon, loaded from a relative path](icon.png)
![A remote badge](https://img.shields.io/badge/remote-never%20loaded-blue)

## What works today

- [x] `cargo run --release -- file.md` from the repo root
- [x] Relative images through the `ezasset` protocol
- [x] Sanitized raw HTML, enforced CSP, ACL-checked IPC, a network lock
- [x] Alerts, task lists, footnotes, and heading links[^1]
- [x] Syntax highlighting, front matter, and live reload that keeps your place
- [ ] Typography and themes (M2)

| Piece | Where | Lines |
|---|---|:-:|
| Engine | `crates/mdcore` | small |
| Shell | `crates/app` | smaller |
| View | `ui/` | smallest |

```rust
fn main() {
    println!("hello from a fenced code block");
}
```

> [!NOTE]
> GitHub alerts render with their own colour.

> [!WARNING]
> A document cannot run script, load a remote resource, or leave its folder.

<details><summary>Raw HTML survives sanitizing when it is safe</summary>

This came from a `<details>` element in the source. Press <kbd>Space</kbd> on
the summary to toggle it.

</details>

<script>alert("this never runs")</script>

[^1]: Footnotes jump here and back. [Back to the top](#ezmark-demo-document).
