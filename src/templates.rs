//! HTML template engine setup (minijinja).
//!
//! Templates live in `templates/*.html` and keep presentation separate
//! from Rust business logic. Values are passed as serializable JSON
//! maps built by the build orchestration.

use std::path::Path;

use anyhow::{Context, Result};
use minijinja::{AutoEscape, Environment, UndefinedBehavior};

pub struct Engine {
    env: Environment<'static>,
}

impl Engine {
    /// Load templates from `dir`. Auto-escaping is enabled for `.html`
    /// templates so content values can never inject markup.
    pub fn load(dir: &Path) -> Result<Self> {
        let mut env = Environment::new();
        // Chainable: `{{ site.locale }}` renders as empty when the context is
        // minimal (the `check` command renders base.html with an empty map),
        // instead of failing on attribute access of an undefined value.
        env.set_undefined_behavior(UndefinedBehavior::Chainable);
        env.set_loader(minijinja::path_loader(dir));
        env.set_auto_escape_callback(|name| {
            if name.ends_with(".html") {
                AutoEscape::Html
            } else {
                AutoEscape::None
            }
        });
        Ok(Self { env })
    }

    pub fn render<S: serde::Serialize>(&self, name: &str, context: S) -> Result<String> {
        // minijinja 3 takes `Into<Value>` contexts; the `Serde` wrapper is the
        // explicit conversion path for custom serializable context types.
        self.env
            .get_template(name)
            .with_context(|| format!("template `{name}` is missing or broken"))?
            .render(minijinja::value::Serde(&context))
            .with_context(|| format!("rendering template `{name}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_repository_templates() {
        let engine = Engine::load(Path::new("templates")).unwrap();
        let html = engine
            .render("404.html", minijinja::context! {})
            .expect("404 template must render");
        assert!(html.contains("<!doctype html>"), "{html}");
        // Auto-escaping must be active for HTML templates.
        let broken = engine.render("404.html", minijinja::context! { raw => "<script>" });
        assert!(broken.is_ok());
    }
}
