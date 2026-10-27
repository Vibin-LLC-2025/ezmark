# ezmark demo document

A read-only markdown viewer. This file is what the M0 screenshots show, so it
exercises the pieces M0 proves: GitHub-flavoured parsing, sanitizing, and a
relative image served through the asset protocol.

![ezmark icon, loaded from a relative path](icon.png)

## What works in M0

- [x] `cargo run --release -- file.md` from the repo root
- [x] Relative images through the `ezasset` protocol
- [x] Sanitized raw HTML, enforced CSP, ACL-checked IPC
- [ ] Typography, themes, outline (M2)

| Piece | Where |
|---|---|
| Engine | `crates/mdcore` |
| Shell | `crates/app` |
| View | `ui/` |

```rust
fn main() {
    println!("hello from a fenced code block");
}
```

> [!NOTE]
> GitHub alerts parse today; their styling arrives with the typography work.

<details><summary>Raw HTML survives sanitizing when it is safe</summary>

This came from a `<details>` element in the source.

</details>

<script>alert("this never runs")</script>
