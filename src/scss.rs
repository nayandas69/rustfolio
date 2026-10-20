//! SCSS compilation via grass — the exclusive styling pipeline.
//!
//! All authored styles compile through grass here; nothing is inlined in
//! HTML templates. Per-article accent overrides are generated as small SCSS
//! snippets and compiled through the same compiler, so they arrive as
//! CSS custom properties on a predefined class — never inline styles.

use std::path::Path;

use grass::{Options, OutputStyle};

use crate::errors::SiteError;

fn compile(
    source_name: &str,
    compile: impl FnOnce(&Options) -> Result<String, Box<grass::Error>>,
) -> anyhow::Result<String> {
    let options = Options::default().style(OutputStyle::Compressed);
    compile(&options).map_err(|e| {
        SiteError::Scss {
            file: source_name.to_string(),
            message: format!("{e}"),
        }
        .into()
    })
}

/// Compile an SCSS entrypoint (e.g. `scss/main.scss`) to minified CSS.
pub fn compile_file(path: &Path) -> anyhow::Result<String> {
    let display = path.display().to_string();
    compile(&display, |options| grass::from_path(path, options))
}

/// Compile an in-memory SCSS snippet to minified CSS.
fn compile_snippet(name: &str, source: &str) -> anyhow::Result<String> {
    compile(name, |options| grass::from_string(source, options))
}

/// Generate the per-article accent override stylesheet.
///
/// `accent` is a validated `#RRGGBB` value (enforced by frontmatter
/// validation). The emitted CSS sets a custom property on the article's
/// theme class.
pub fn compile_accent(slug: &str, accent: &str) -> anyhow::Result<String> {
    let source = format!(".theme-{slug} {{ --accent: {accent}; }}\n");
    compile_snippet(&format!("accent override for `{slug}`"), &source)
}

/// Compile every SCSS entrypoint under `dir` (files not starting with `_`).
///
/// Used by `rustfolio scss` in CI so any SCSS regression fails the pipeline.
pub fn compile_all(dir: &Path) -> anyhow::Result<Vec<String>> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            if name.ends_with(".scss") && !name.starts_with('_') {
                entries.push(entry.path().to_path_buf());
            }
        }
    }
    entries.sort();
    let mut compiled = Vec::new();
    for path in entries {
        compiled.push(compile_file(&path)?);
    }
    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_a_valid_snippet() {
        let css = compile_accent("my-post", "#E56A35").unwrap();
        assert!(css.contains("E56A35"), "got: {css}");
        assert!(css.contains(".theme-my-post"), "got: {css}");
        assert!(!css.contains("undefined"), "got: {css}");
    }

    #[test]
    fn invalid_scss_fails_loudly() {
        let err = compile_snippet("bad.scss", "a { color: ;; }");
        assert!(err.is_err());
    }

    #[test]
    fn main_stylesheet_compiles() {
        let css = compile_file(Path::new("scss/main.scss")).expect("main.scss must compile");
        assert!(!css.is_empty());
        assert!(
            !css.contains("undefined"),
            "SCSS must not leak errors: {css}"
        );
    }
}
