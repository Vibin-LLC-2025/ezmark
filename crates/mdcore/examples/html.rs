//! Print the HTML the engine produces for a markdown file: what the viewer
//! would put in the page, block markers included.
//!
//! ```text
//! cargo run -p mdcore --example html -- README.md
//! ```

use std::path::Path;
use std::sync::Arc;

fn main() {
    let Some(file) = std::env::args_os().nth(1) else {
        eprintln!("usage: cargo run -p mdcore --example html -- <file.md>");
        std::process::exit(2);
    };
    let path = match std::fs::canonicalize(&file) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{}: {error}", Path::new(&file).display());
            std::process::exit(1);
        }
    };
    let bytes = std::fs::read(&path).expect("the file was just found");
    let folder = path.parent().expect("a file has a folder").to_path_buf();
    let ctx = mdcore::RenderContext {
        root: folder.clone(),
        doc_dir: folder,
        // The viewer's own scheme is not available here; show the reference.
        asset_url: Arc::new(|raw: &str| format!("ezasset-example:{raw}")),
    };
    print!(
        "{}",
        mdcore::render(&mdcore::decode(&bytes).text, &ctx).html
    );
}
