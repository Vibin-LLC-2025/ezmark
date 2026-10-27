//! ezmark's markdown engine.
//!
//! A plain library with no GUI dependency: everything that leaves it is
//! sanitized HTML, and every local reference a document makes goes through
//! the one shared validator in [`boundary`]. The Tauri shell in `crates/app`
//! is the only caller; a future Quick Look extension could be another.

pub mod boundary;
mod render;

pub use boundary::{Fs, RealFs, Rejection, split_reference, validate_reference};
pub use render::{AssetUrlFn, RenderContext, render, sanitize};
