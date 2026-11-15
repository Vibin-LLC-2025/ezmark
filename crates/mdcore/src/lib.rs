//! ezmark's markdown engine.
//!
//! A plain library with no GUI dependency: everything that leaves it is
//! sanitized HTML, and every local reference a document makes goes through
//! the one shared validator in [`boundary`]. The Tauri shell in `crates/app`
//! is the only caller; a future Quick Look extension could be another.

mod blocks;
pub mod boundary;
mod frontmatter;
pub mod hex;
mod highlight;
mod nesting;
mod render;
mod text;

pub use blocks::{MARKER_ATTRIBUTE, MARKER_TAG, content_hash};
pub use boundary::{Fs, RealFs, Rejection, split_reference, validate_reference};
pub use highlight::{CLASSES as HIGHLIGHT_CLASSES, DOCUMENT_BUDGET, LINE_LIMIT};
pub use nesting::MAX_DEPTH;
pub use render::{
    AssetUrlFn, BLOCKED_IMAGE_SCHEME, Fallback, ID_PREFIX, PLAIN_TEXT_LIMIT, RenderContext,
    Rendered, render, sanitize,
};
pub use text::{Decoded, decode};
