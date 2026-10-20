//! sitemap.xml generation.

use anyhow::Result;

use crate::config::Config;
use crate::content::Post;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Generate the sitemap covering every index page, post, tag and page.
pub fn generate(config: &Config, posts: &[Post], page_slugs: &[String]) -> Result<String> {
    let base = config.base_url();
    let mut urls: Vec<(String, Option<String>)> = vec![
        ("/".into(), None),
        ("/blog/".into(), None),
        ("/archive/".into(), None),
        ("/tags/".into(), None),
    ];
    for slug in page_slugs {
        urls.push((format!("/{slug}/"), None));
    }
    for post in posts {
        urls.push((
            post.url(),
            Some(
                post.updated
                    .unwrap_or(post.date)
                    .format("%Y-%m-%d")
                    .to_string(),
            ),
        ));
    }
    for (tag, _) in crate::content::tag_counts(posts) {
        urls.push((format!("/tags/{}/", crate::slug::slugify(&tag)), None));
    }
    // Keep pagination URLs in the map when there are multiple pages.
    let per_page = config.site.posts_per_page.max(1);
    let pages = posts.len().div_ceil(per_page);
    for page in 2..=pages {
        urls.push((format!("/blog/page/{page}/"), None));
    }

    let mut sitemap = String::new();
    sitemap.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    sitemap.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (path, lastmod) in urls {
        sitemap.push_str("  <url>\n");
        sitemap.push_str(&format!(
            "    <loc>{}</loc>\n",
            xml_escape(&format!("{base}{path}"))
        ));
        if let Some(lastmod) = lastmod {
            sitemap.push_str(&format!("    <lastmod>{lastmod}</lastmod>\n"));
        }
        sitemap.push_str("  </url>\n");
    }
    sitemap.push_str("</urlset>\n");
    Ok(sitemap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Post;
    use crate::frontmatter::{ReadingWidth, Theme};
    use chrono::NaiveDate;

    fn sample(slug: &str, date: &str, updated: Option<&str>, tags: &[&str]) -> Post {
        Post {
            slug: slug.into(),
            file: String::new(),
            title: "T".into(),
            description: "d".into(),
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            updated: updated.map(|u| NaiveDate::parse_from_str(u, "%Y-%m-%d").unwrap()),
            draft: false,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            featured: false,
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
            words: 10,
        }
    }

    #[test]
    fn sitemap_lists_posts_tags_and_pages() {
        let config: Config = toml::from_str(
            "[site]\nurl = \"https://example.com\"\ntitle = \"t\"\ndescription = \"d\"\n[author]\nname = \"a\"\n",
        )
        .unwrap();
        let posts = vec![sample(
            "post-one",
            "2026-10-01",
            Some("2026-10-05"),
            &["rust", "web"],
        )];
        let sitemap = generate(&config, &posts, &["about".into(), "projects".into()]).unwrap();
        assert!(sitemap.contains("<loc>https://example.com/posts/post-one/</loc>"));
        assert!(sitemap.contains("<lastmod>2026-10-05</lastmod>"));
        assert!(sitemap.contains("<loc>https://example.com/tags/rust/</loc>"));
        assert!(sitemap.contains("<loc>https://example.com/about/</loc>"));
        assert!(sitemap.contains("<loc>https://example.com/</loc>"));
    }
}
