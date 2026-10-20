//! Structured domain errors.
//!
//! `anyhow::Error` is used at the application boundary for free-form
//! context chaining; the [`SiteError`] variants carry the structured
//! information (source file, line number, target) that tests assert on.

use thiserror::Error;

/// Convenient result alias used throughout the crate.
pub type Result<T> = std::result::Result<T, anyhow::Error>;

#[derive(Debug, Error)]
pub enum SiteError {
    #[error("invalid frontmatter in `{file}`: {message}")]
    Frontmatter { file: String, message: String },

    #[error("duplicate slug `{slug}` (from `{first}` and `{second}`)")]
    DuplicateSlug {
        slug: String,
        first: String,
        second: String,
    },

    #[error("unsupported value in `{file}`: {message}")]
    Unsupported { file: String, message: String },

    #[error("in `{file}`, line {line}: unsafe link `{target}` ({message})")]
    UnsafeLink {
        file: String,
        line: usize,
        target: String,
        message: String,
    },

    #[error("broken reference in `{file}`: `{target}` ({message})")]
    BrokenLink {
        file: String,
        target: String,
        message: String,
    },

    #[error("missing output artifact: `{0}` — the build is incomplete")]
    MissingArtifact(String),

    #[error("scss error in `{file}`: {message}")]
    Scss { file: String, message: String },
}

impl SiteError {
    pub fn frontmatter(file: &str, message: impl Into<String>) -> Self {
        Self::Frontmatter {
            file: file.to_string(),
            message: message.into(),
        }
    }

    pub fn unsupported(file: &str, message: impl Into<String>) -> Self {
        Self::Unsupported {
            file: file.to_string(),
            message: message.into(),
        }
    }
}
