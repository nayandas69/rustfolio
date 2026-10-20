//! Content and built-output validation.
//!
//! * [`validate_posts`] runs pre-build structural checks (duplicate slugs).
//! * [`verify_site`] runs post-build checks over the generated directory:
//!   required artifacts, internal link integrity, asset references,
//!   RSS and sitemap correctness.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};
use walkdir::WalkDir;

use crate::config::Config;
use crate::content::Post;
use crate::errors::SiteError;

/// Structural checks over the loaded content.
pub fn validate_posts(posts: &[Post]) -> Result<()> {
    let mut seen: BTreeMap<&str, &str> = BTreeMap::new();
    for post in posts {
        if let Some(first) = seen.get(post.slug.as_str()) {
            return Err(SiteError::DuplicateSlug {
                slug: post.slug.clone(),
                first: (*first).to_string(),
                second: post.file.clone(),
            }
            .into());
        }
        seen.insert(post.slug.as_str(), post.file.as_str());
    }
    Ok(())
}

/// Resolve an internal URL to the file that should exist in the build.
fn resolve_url(url: &str) -> String {
    let path = url
        .split(['?', '#'])
        .next()
        .unwrap_or("/")
        .trim_start_matches('/');
    let path = percent_decode(path);
    if path.is_empty() || path.ends_with('/') {
        format!("{path}index.html")
    } else if Path::new(&path).extension().is_some() {
        path
    } else {
        // Clean URLs: /blog matches blog/index.html.
        format!("{path}/index.html")
    }
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Ok(byte) = u8::from_str_radix(hex.unwrap_or("zz"), 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Extract `href="…"` and `src="…"` values from generated HTML.
fn extract_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for attr in ["href=\"", "src=\""] {
        let mut rest = html;
        while let Some(pos) = rest.find(attr) {
            rest = &rest[pos + attr.len()..];
            if let Some(end) = rest.find('"') {
                urls.push(rest[..end].to_string());
                rest = &rest[end..];
            } else {
                break;
            }
        }
    }
    urls
}

fn is_internal(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//") && !url.starts_with("mailto:")
}

/// Verify a fully built site.
pub fn verify_site(dir: &Path, config: &Config, posts: &[Post]) -> Result<()> {
    let mut problems: Vec<String> = Vec::new();

    // 1. Required artifacts exist.
    let required = [
        "index.html",
        "404.html",
        "500.html",
        "error.html",
        "failed.html",
        "rss.xml",
        "sitemap.xml",
        "robots.txt",
        "search.json",
    ];
    for artifact in required {
        if !dir.join(artifact).exists() {
            problems.push(SiteError::MissingArtifact(artifact.to_string()).to_string());
        }
    }
    for post in posts {
        let page = resolve_url(&post.url());
        if !dir.join(&page).exists() {
            problems.push(SiteError::MissingArtifact(page).to_string());
        }
    }

    // 2. RSS channel has one item per published post.
    if let Ok(rss) = std::fs::read_to_string(dir.join("rss.xml")) {
        let items = rss.matches("<item>").count();
        if items != posts.len() {
            problems.push(format!(
                "rss.xml has {items} items but there are {} published posts",
                posts.len()
            ));
        }
    }

    // 3. Sitemap contains every post and the homepage.
    if let Ok(sitemap) = std::fs::read_to_string(dir.join("sitemap.xml")) {
        let base = config.base_url();
        if !sitemap.contains(&format!("<loc>{base}/</loc>")) {
            problems.push("sitemap.xml is missing the homepage entry".to_string());
        }
        for post in posts {
            if !sitemap.contains(&format!("<loc>{base}{}</loc>", post.url())) {
                problems.push(format!("sitemap.xml is missing `{}`", post.url()));
            }
        }
    }

    // 4. Every internal link and asset reference in generated HTML resolves.
    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .expect("walkdir yields paths inside root")
            .to_string_lossy()
            .to_string();
        if !rel.ends_with(".html") {
            continue;
        }
        let Ok(html) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for url in extract_urls(&html) {
            if !is_internal(&url) {
                continue;
            }
            let resolved = resolve_url(&url);
            if !dir.join(&resolved).exists() {
                problems.push(
                    SiteError::BrokenLink {
                        file: rel.clone(),
                        target: url.clone(),
                        message: format!("no file at `{resolved}` in the build output"),
                    }
                    .to_string(),
                );
            }
        }
    }

    if !problems.is_empty() {
        bail!(
            "built site failed verification ({} problem{}):\n  - {}",
            problems.len(),
            if problems.len() == 1 { "" } else { "s" },
            problems.join("\n  - ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_slugs_are_rejected() {
        let mk = |file: &str| crate::content::Post {
            slug: "same".into(),
            file: file.into(),
            title: "T".into(),
            description: "d".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            updated: None,
            draft: false,
            tags: vec![],
            featured: false,
            toc_enabled: true,
            theme: Default::default(),
            accent: None,
            accent_css_url: None,
            wide: false,
            reading_width: Default::default(),
            trusted_html: false,
            canonical: None,
            social_image: None,
            html: String::new(),
            toc: Vec::new(),
            reading_time: 1,
            words: 5,
        };
        let posts = vec![
            mk("content/posts/same.md"),
            mk("content/pages/../posts/same.md"),
        ];
        let err = validate_posts(&posts).unwrap_err();
        assert!(err.to_string().contains("duplicate slug `same`"));
    }

    #[test]
    fn resolve_url_maps_clean_urls() {
        assert_eq!(resolve_url("/"), "index.html");
        assert_eq!(resolve_url("/blog/"), "blog/index.html");
        assert_eq!(resolve_url("/blog/page/2/?x=1"), "blog/page/2/index.html");
        assert_eq!(resolve_url("/assets/main.css"), "assets/main.css");
    }

    #[test]
    fn extract_urls_finds_attributes() {
        let html = r#"<a href="/x/">a</a><img src="/y.png"><link href="https://e.x/">"#;
        let urls = extract_urls(html);
        assert!(urls.contains(&"/x/".to_string()));
        assert!(urls.contains(&"/y.png".to_string()));
        assert_eq!(urls.len(), 3);
    }

    #[test]
    fn verify_reports_broken_links() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.html"), "<a href=\"/broken/\">x</a>").unwrap();
        std::fs::create_dir_all(dir.path().join("broken")).unwrap();
        std::fs::write(dir.path().join("rss.xml"), "").unwrap();
        std::fs::write(
            dir.path().join("sitemap.xml"),
            "<urlset><url><loc>https://example.com/</loc></url></urlset>",
        )
        .unwrap();
        std::fs::write(dir.path().join("robots.txt"), "").unwrap();
        std::fs::write(dir.path().join("search.json"), "[]").unwrap();
        std::fs::write(dir.path().join("404.html"), "").unwrap();

        let config: crate::config::Config = toml::from_str(
            "[site]\nurl = \"https://example.com\"\ntitle = \"t\"\ndescription = \"d\"\n[author]\nname = \"a\"\n",
        )
        .unwrap();
        let err = verify_site(dir.path(), &config, &[]).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("broken"), "{msg}");
    }
}
