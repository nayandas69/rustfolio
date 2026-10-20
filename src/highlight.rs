//! Syntax highlighting with syntect's class-based HTML generator.
//!
//! Class-based output keeps colors in SCSS (light and dark variants) instead
//! of per-token inline styles, and works with the site's design tokens.

use anyhow::Result;
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    syntax_set: SyntaxSet,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
        }
    }

    /// Highlight `code` as `lang`, returning class-based HTML.
    ///
    /// Unknown languages fall back to escaped plain text so a typo in a
    /// language tag degrades gracefully instead of failing the build.
    pub fn highlight(&self, code: &str, lang: &str) -> Result<String> {
        let syntax = match self.syntax_set.find_syntax_by_token(lang) {
            Some(s) => s,
            None => return Ok(crate::markdown::escape_html_text(code)),
        };
        let mut generator = ClassedHTMLGenerator::new_with_class_style(
            syntax,
            &self.syntax_set,
            ClassStyle::Spaced,
        );
        for line in LinesWithEndings::from(code) {
            generator.parse_html_for_line_which_includes_newline(line)?;
        }
        Ok(generator.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_with_classes_not_inline_styles() {
        let html = Highlighter::new()
            .highlight("fn main() {}", "rust")
            .unwrap();
        assert!(html.contains("source"), "got: {html}");
        assert!(!html.contains("style="));
    }

    #[test]
    fn unknown_language_falls_back_to_escaped_text() {
        let html = Highlighter::new().highlight("a < b", "notalang").unwrap();
        assert_eq!(html, "a &lt; b");
    }
}
