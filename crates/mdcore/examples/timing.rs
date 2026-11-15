//! Stage timings on generated fixtures, for docs/measurements.md.
//!
//! ```text
//! cargo run --release -p mdcore --example timing
//! ```

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

fn context() -> mdcore::RenderContext {
    let nowhere = PathBuf::from("ezmark-timing-nowhere");
    mdcore::RenderContext {
        root: nowhere.clone(),
        doc_dir: nowhere,
        asset_url: Arc::new(|raw: &str| raw.to_owned()),
    }
}

fn rust_lines(lines: usize) -> String {
    let line = "    let total: u64 = items.iter().map(|item| item.size * 2).sum(); // sum\n";
    line.repeat(lines)
}

fn mixed(bytes: usize) -> String {
    let unit = "## Section\n\nSome *emphasis*, `code`, and a [link](https://example.com).\n\n\
                - one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n\
                ```rust\nfn main() { println!(\"hi\"); }\n```\n\n";
    unit.repeat(bytes / unit.len() + 1)
}

fn main() {
    let fixtures: Vec<(&str, String)> = vec![
        ("mixed markdown, 50 KB", mixed(50 * 1024)),
        ("mixed markdown, 1 MB", mixed(1024 * 1024)),
        ("mixed markdown, 10 MB", mixed(10 * 1024 * 1024)),
        (
            "one 3,500-line Rust block (within the highlight budget)",
            format!("```rust\n{}```\n", rust_lines(3_500)),
        ),
        (
            "one 10,000-line Rust block",
            format!("```rust\n{}```\n", rust_lines(10_000)),
        ),
        (
            "20,000 nested quotes",
            format!("{}deep\n", ">".repeat(20_000)),
        ),
        ("50,000 unclosed <div>", "<div>\n".repeat(50_000)),
        ("100,000 unclosed emphasis markers", "*a ".repeat(100_000)),
        ("50,000 unclosed links", "[a](".repeat(50_000)),
    ];
    let ctx = context();
    // The first render loads the grammars; time that on its own.
    let start = Instant::now();
    mdcore::render("```rust\nfn main() {}\n```\n", &ctx);
    println!(
        "{:>9.1} ms  first highlighted render (loads the grammars)",
        start.elapsed().as_secs_f64() * 1000.0
    );
    for (name, source) in &fixtures {
        let start = Instant::now();
        let rendered = mdcore::render(source, &ctx);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "{elapsed:>9.1} ms  {name}: {} KB in, {} KB out{}",
            source.len() / 1024,
            rendered.html.len() / 1024,
            if rendered.fallback.is_some() {
                ", shown as plain text"
            } else {
                ""
            },
        );
    }
}
