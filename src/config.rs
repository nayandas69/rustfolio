//! Site-wide configuration, loaded from `rustfolio.toml`.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Config {
    pub site: SiteConfig,
    pub author: AuthorConfig,
    pub social: SocialConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SiteConfig {
    /// Absolute production URL, no trailing slash (used in RSS/sitemap/OG).
    pub url: String,
    pub title: String,
    pub description: String,
    pub locale: String,
    pub posts_per_page: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AuthorConfig {
    pub name: String,
    pub bio: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields, default)]
pub struct SocialConfig {
    pub github: Option<String>,
    pub twitter: Option<String>,
    pub linkedin: Option<String>,
    pub mastodon: Option<String>,
    pub rss: Option<String>,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            title: String::new(),
            description: String::new(),
            locale: "en".to_string(),
            posts_per_page: 8,
        }
    }
}

impl Config {
    /// Site base URL without a trailing slash.
    pub fn base_url(&self) -> &str {
        self.site.url.trim_end_matches('/')
    }

    fn sanity_check(&self) -> Result<()> {
        if !self.site.url.starts_with("https://") {
            bail!(
                "site.url must be an absolute https:// URL (currently `{}`)",
                self.site.url
            );
        }
        if self.site.title.trim().is_empty() {
            bail!("site.title must not be empty");
        }
        if self.author.name.trim().is_empty() {
            bail!("author.name must not be empty");
        }
        if self.site.posts_per_page == 0 {
            bail!("site.posts_per_page must be at least 1");
        }
        Ok(())
    }
}

/// Load and validate `rustfolio.toml` from `path`.
pub fn load(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading configuration `{}`", path.display()))?;
    let config: Config = toml::from_str(&raw)
        .with_context(|| format!("parsing configuration `{}`", path.display()))?;
    config.sanity_check()?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_config() {
        let config = load(Path::new("rustfolio.toml")).expect("repo config should parse");
        assert_eq!(config.site.title, "rustfolio");
        assert!(!config.site.url.is_empty());
    }

    #[test]
    fn rejects_relative_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rustfolio.toml");
        std::fs::write(
            &path,
            "[site]\nurl = \"http://example.com\"\ntitle = \"t\"\n[author]\nname = \"a\"\n",
        )
        .unwrap();
        assert!(load(&path).is_err());
    }
}
