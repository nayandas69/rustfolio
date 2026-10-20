//! Build-time search index generation.
//!
//! A single compact JSON document indexes titles, descriptions, tags and
//! the first slice of each article's text. It is fetched lazily by the
//! client-side search on the blog index — no backend, no database.

use anyhow::Result;
use serde::Serialize;

use crate::content::Post;

/// Maximum characters of article text kept in the index, per entry.
const TEXT_LIMIT: usize = 4000;

#[derive(Serialize, serde::Deserialize)]
pub struct SearchEntry {
    pub slug: String,
    pub url: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub date: String,
    pub text: String,
}

/// Build the JSON search index for `posts`.
pub fn build_index(posts: &[Post]) -> Result<String> {
    let entries: Vec<SearchEntry> = posts
        .iter()
        .map(|post| SearchEntry {
            slug: post.slug.clone(),
            url: post.url(),
            title: post.title.clone(),
            description: post.description.clone(),
            tags: post.tags.clone(),
            date: post.date_iso(),
            text: truncate_chars(&post_words(post), TEXT_LIMIT),
        })
        .collect();
    Ok(serde_json::to_string(&entries)?)
}

fn post_words(post: &Post) -> String {
    // `post.html` is rendered; fall back to description when the body is
    // empty. Reading text is not stored separately, so reuse the HTML.
    let raw = strip_tags(&post.html);
    if raw.is_empty() {
        post.description.clone()
    } else {
        raw
    }
}

/// Extremely small tag stripper for search text (tags, attributes, tags).
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Truncate on a character boundary.
fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    text.chars().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Post;
    use chrono::NaiveDate;

    fn sample(slug: &str, body_html: &str) -> Post {
        Post {
            slug: slug.into(),
            file: String::new(),
            title: "Rust Ownership".into(),
            description: "How ownership works".into(),
            date: NaiveDate::parse_from_str("2026-10-20", "%Y-%m-%d").unwrap(),
            updated: None,
            draft: false,
            tags: vec!["rust".into()],
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
            html: body_html.into(),
            toc: Vec::new(),
            reading_time: 4,
            words: 800,
        }
    }

    #[test]
    fn index_serializes_round_trip() {
        let posts = vec![
            sample("ownership", "<p>Borrowing and <em>moves</em>.</p>"),
            sample("draft-excluded", "<p>never seen</p>"),
        ];
        let json = build_index(&posts).unwrap();
        let parsed: Vec<SearchEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].url, "/posts/ownership/");
        assert_eq!(parsed[0].text, "Borrowing and moves.");
        assert_eq!(parsed[0].tags, vec!["rust"]);
    }

    #[test]
    fn long_text_is_truncated() {
        let long_body = format!("<p>{}</p>", "word ".repeat(5000));
        let json = build_index(&[sample("long", &long_body)]).unwrap();
        let parsed: Vec<SearchEntry> = serde_json::from_str(&json).unwrap();
        assert!(parsed[0].text.chars().count() <= 4000);
    }
}
