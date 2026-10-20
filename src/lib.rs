//! rustfolio — a Rust-powered static site generator for an engineering blog.
//!
//! The crate is exposed as a library so integration tests can drive full
//! builds against temporary content trees. `main.rs` is a thin CLI wrapper.

pub mod build;
pub mod config;
pub mod content;
pub mod errors;
pub mod feed;
pub mod frontmatter;
pub mod highlight;
pub mod markdown;
pub mod output;
pub mod scss;
pub mod search;
pub mod serve;
pub mod sitemap;
pub mod slug;
pub mod templates;
pub mod validate;
