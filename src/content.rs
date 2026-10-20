//! Content model: posts and pages loaded from the `content/` tree.
//!
//! Posts live in `content/posts/*.md` (filename = slug) and pages in
//! `content/pages/*.md`. Drafts are loaded for validation but excluded
//! from production builds unless explicitly included.

use std::path::Path;

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use walkdir::WalkDir;

use crate::frontmatter::{Frontmatter, ReadingWidth, Theme};
use crate::highlight::Highlighter;
use crate::markdown::{RenderOptions, Rendered, TocEntry};
use crate::slug::slugify;

const WORDS_PER_MINUTE: usize = 220;

#[derive(Debug, Clone)]
pub struct Post {
    pub slug: String,
    /// Source file path, for diagnostics.
    pub file: String,
    pub title: String,
    pub description: String,
    pub date: NaiveDate,
    pub updated: Option<NaiveDate>,
    pub draft: bool,
    pub tags: Vec<String>,
    pub featured: bool,
    pub toc_enabled: bool,
    pub theme: Theme,
    pub accent: Option<String>,
    /// URL of the per-post accent stylesheet, filled in during the build.
    pub accent_css_url: Option<String>,
    pub wide: bool,
    pub reading_width: ReadingWidth,
    pub trusted_html: bool,
    pub canonical: Option<String>,
    pub social_image: Option<String>,
    pub html: String,
    pub toc: Vec<TocEntry>,
    pub reading_time: u32,
    pub words: usize,
}

impl Post {
    pub fn url(&self) -> String {
        format!("/posts/{}/", self.slug)
    }

    pub fn date_display(&self) -> String {
        format!(
            "{} {}, {}",
            self.date.format("%B"),
            self.date.day(),
            self.date.year()
        )
    }

    pub fn date_iso(&self) -> String {
        self.date.format("%Y-%m-%d").to_string()
    }

    pub fn updated_iso(&self) -> Option<String> {
        self.updated.map(|d| d.format("%Y-%m-%d").to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Page {
    pub slug: String,
    pub file: String,
    pub title: String,
    pub description: String,
    pub html: String,
    pub toc: Vec<TocEntry>,
}

impl Page {
    pub fn url(&self) -> String {
        format!("/{}/", self.slug)
    }
}

pub fn reading_time(words: usize) -> u32 {
    ((words / WORDS_PER_MINUTE) + (words % WORDS_PER_MINUTE).min(1)) as u32
}

/// Load every post under `dir`. Drafts are always validated; they are
/// filtered out of the returned list unless `include_drafts` is set.
pub fn load_posts(
    dir: &Path,
    include_drafts: bool,
    highlighter: &Highlighter,
    static_root: &Path,
) -> Result<Vec<Post>> {
    let mut posts = Vec::new();
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        if entry.file_name().to_string_lossy().ends_with(".md") {
            posts.push(load_post(entry.path(), highlighter, static_root)?);
        }
    }
    posts.retain(|p| include_drafts || !p.draft);
    // Newest first; ties broken by title for deterministic output.
    posts.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));
    Ok(posts)
}

fn load_post(path: &Path, highlighter: &Highlighter, static_root: &Path) -> Result<Post> {
    let file = path.display().to_string();
    let source = std::fs::read_to_string(path)
        .map_err(|e| std::io::Error::new(e.kind(), format!("reading `{file}`: {e}")))?;
    let (fm, body) = crate::frontmatter::parse(&source, &file)?;

    let slug = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    if slug != slugify(&slug) {
        return Err(crate::errors::SiteError::unsupported(
            &file,
            format!("filename `{slug}.md` is not a valid slug; rename it to kebab-case (e.g. `my-post.md`)"),
        )
        .into());
    }

    let rendered = crate::markdown::render(
        &body,
        &RenderOptions {
            file: &file,
            trusted_html: fm.trusted_html,
            highlighter,
            asset_root: Some(static_root),
        },
    )?;
    let words = rendered.text.split_whitespace().count();
    let post = post_from(fm, slug, file, rendered, words);
    Ok(post)
}

fn post_from(
    fm: Frontmatter,
    slug: String,
    file: String,
    rendered: Rendered,
    words: usize,
) -> Post {
    Post {
        slug,
        file,
        title: fm.title,
        description: fm.description,
        date: fm.date,
        updated: fm.updated,
        draft: fm.draft,
        tags: fm.tags,
        featured: fm.featured,
        toc_enabled: fm.toc,
        theme: fm.theme,
        accent: fm.accent,
        accent_css_url: None,
        wide: fm.wide,
        reading_width: fm.reading_width,
        trusted_html: fm.trusted_html,
        canonical: fm.canonical,
        social_image: fm.social_image,
        html: rendered.html,
        toc: rendered.toc,
        reading_time: reading_time(words),
        words,
    }
}

/// Load standalone pages (about, projects, …) from `dir`.
pub fn load_pages(dir: &Path, highlighter: &Highlighter, static_root: &Path) -> Result<Vec<Page>> {
    let mut pages = Vec::new();
    for entry in WalkDir::new(dir)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() || !entry.file_name().to_string_lossy().ends_with(".md") {
            continue;
        }
        let path = entry.path();
        let file = path.display().to_string();
        let source = std::fs::read_to_string(path)
            .map_err(|e| std::io::Error::new(e.kind(), format!("reading `{file}`: {e}")))?;
        let (fm, body) = crate::frontmatter::parse_page(&source, &file)?;
        let slug = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if slug != slugify(&slug) {
            return Err(crate::errors::SiteError::unsupported(
                &file,
                format!("filename `{slug}.md` is not a valid slug; use kebab-case"),
            )
            .into());
        }
        let rendered = crate::markdown::render(
            &body,
            &RenderOptions {
                file: &file,
                trusted_html: false,
                highlighter,
                asset_root: Some(static_root),
            },
        )?;
        pages.push(Page {
            slug,
            file,
            title: fm.title,
            description: fm.description,
            html: rendered.html,
            toc: rendered.toc,
        });
    }
    pages.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(pages)
}

/// The latest featured post, if any.
pub fn featured(posts: &[Post]) -> Option<&Post> {
    posts.iter().find(|p| p.featured)
}

/// Up to `count` most recent posts, optionally excluding a set of slugs.
pub fn recent<'a>(posts: &'a [Post], count: usize, exclude: &[&str]) -> Vec<&'a Post> {
    posts
        .iter()
        .filter(|p| !exclude.contains(&p.slug.as_str()))
        .take(count)
        .collect()
}

/// Posts sharing tags with `post`, best match first.
pub fn related<'a>(post: &Post, posts: &'a [Post]) -> Vec<&'a Post> {
    let mut scored: Vec<(usize, &Post)> = posts
        .iter()
        .filter(|p| p.slug != post.slug)
        .map(|p| {
            let shared = p.tags.iter().filter(|t| post.tags.contains(t)).count();
            (shared, p)
        })
        .filter(|(shared, _)| *shared > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.date.cmp(&a.1.date)));
    scored.into_iter().map(|(_, p)| p).take(3).collect()
}

/// Tag usage counts, alphabetically for deterministic output.
pub fn tag_counts(posts: &[Post]) -> Vec<(String, usize)> {
    let mut counts = std::collections::BTreeMap::new();
    for post in posts {
        for tag in &post.tags {
            *counts.entry(tag.clone()).or_insert(0usize) += 1;
        }
    }
    counts.into_iter().collect()
}

/// Posts tagged with `tag`, newest first.
pub fn posts_with_tag<'a>(posts: &'a [Post], tag: &str) -> Vec<&'a Post> {
    posts
        .iter()
        .filter(|p| p.tags.iter().any(|t| t == tag))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_post(slug: &str, date: &str, tags: &[&str], featured: bool) -> Post {
        Post {
            slug: slug.to_string(),
            file: format!("content/posts/{slug}.md"),
            title: format!("Post {slug}"),
            description: "d".into(),
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            updated: None,
            draft: false,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            featured,
            toc_enabled: true,
            theme: Theme::default(),
            accent: None,
            accent_css_url: None,
            wide: false,
            reading_width: ReadingWidth::default(),
            trusted_html: false,
            canonical: None,
            social_image: None,
            html: String::new(),
            toc: Vec::new(),
            reading_time: 1,
            words: 100,
        }
    }

    #[test]
    fn reading_time_is_rounded_up() {
        assert_eq!(reading_time(0), 0);
        assert_eq!(reading_time(1), 1);
        assert_eq!(reading_time(220), 1);
        assert_eq!(reading_time(221), 2);
        assert_eq!(reading_time(660), 3);
    }

    #[test]
    fn related_posts_rank_by_shared_tags_then_date() {
        let target = sample_post("a", "2026-01-01", &["rust", "web"], false);
        let posts = vec![
            target.clone(),
            sample_post("b", "2026-02-01", &["rust"], false),
            sample_post("c", "2026-03-01", &["rust", "web"], false),
            sample_post("d", "2026-04-01", &["go"], false),
        ];
        let related = related(&target, &posts);
        assert_eq!(related.len(), 2);
        assert_eq!(related[0].slug, "c");
        assert_eq!(related[1].slug, "b");
    }

    #[test]
    fn tag_counts_are_deterministic() {
        let posts = vec![
            sample_post("a", "2026-01-01", &["rust", "web"], false),
            sample_post("b", "2026-02-01", &["rust"], false),
        ];
        assert_eq!(
            tag_counts(&posts),
            vec![("rust".to_string(), 2), ("web".to_string(), 1)]
        );
    }

    #[test]
    fn featured_and_recent_helpers() {
        // Helpers assume posts are already sorted newest-first (as loaded).
        let posts = vec![
            sample_post("c", "2026-03-01", &[], false),
            sample_post("b", "2026-02-01", &[], true),
            sample_post("a", "2026-01-01", &[], false),
        ];
        assert_eq!(featured(&posts).unwrap().slug, "b");
        assert_eq!(
            recent(&posts, 2, &["b"])
                .iter()
                .map(|p| p.slug.clone())
                .collect::<Vec<_>>(),
            vec!["c", "a"]
        );
    }

    #[test]
    fn posts_with_tag_filter() {
        let posts = vec![
            sample_post("a", "2026-01-01", &["rust"], false),
            sample_post("b", "2026-02-01", &["go"], false),
        ];
        assert_eq!(posts_with_tag(&posts, "rust").len(), 1);
    }
}
