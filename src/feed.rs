//! RSS 2.0 feed generation — hand-built XML with strict escaping.
//!
//! Built by the generator itself (deterministic output, no lastBuildDate
//! wall-clock timestamps).

use anyhow::Result;
use chrono::NaiveDate;

use crate::config::Config;
use crate::content::Post;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn rfc2822(date: NaiveDate) -> String {
    use chrono::TimeZone;
    let naive = date.and_hms_opt(0, 0, 0).expect("midnight is always valid");
    chrono::Utc.from_utc_datetime(&naive).to_rfc2822()
}

/// Generate the RSS 2.0 feed for `posts`.
pub fn generate(config: &Config, posts: &[Post]) -> Result<String> {
    let base = config.base_url();
    let mut feed = String::new();
    feed.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    feed.push_str("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n");
    feed.push_str("  <channel>\n");
    feed.push_str(&format!(
        "    <title>{}</title>\n",
        xml_escape(&config.site.title)
    ));
    feed.push_str(&format!("    <link>{}</link>\n", xml_escape(base)));
    feed.push_str(&format!(
        "    <description>{}</description>\n",
        xml_escape(&config.site.description)
    ));
    feed.push_str(&format!(
        "    <language>{}</language>\n",
        xml_escape(&config.site.locale)
    ));
    feed.push_str("    <generator>rustfolio</generator>\n");
    feed.push_str(&format!(
        "    <atom:link href=\"{}/rss.xml\" rel=\"self\" type=\"application/rss+xml\" />\n",
        xml_escape(base)
    ));

    for post in posts {
        feed.push_str("    <item>\n");
        feed.push_str(&format!(
            "      <title>{}</title>\n",
            xml_escape(&post.title)
        ));
        let url = format!("{}/posts/{}/", base, post.slug);
        feed.push_str(&format!("      <link>{}</link>\n", xml_escape(&url)));
        feed.push_str(&format!(
            "      <guid isPermaLink=\"true\">{}</guid>\n",
            xml_escape(&url)
        ));
        feed.push_str(&format!(
            "      <pubDate>{}</pubDate>\n",
            rfc2822(post.date)
        ));
        feed.push_str(&format!(
            "      <description>{}</description>\n",
            xml_escape(&post.description)
        ));
        for tag in &post.tags {
            feed.push_str(&format!("      <category>{}</category>\n", xml_escape(tag)));
        }
        feed.push_str("    </item>\n");
    }

    feed.push_str("  </channel>\n</rss>\n");
    Ok(feed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Post;
    use crate::frontmatter::{ReadingWidth, Theme};

    fn config() -> Config {
        toml::from_str(
            "[site]\nurl = \"https://example.com\"\ntitle = \"My Blog\"\ndescription = \"Desc\"\n[author]\nname = \"a\"\n",
        )
        .unwrap()
    }

    fn sample(slug: &str, title: &str, tags: Vec<String>) -> Post {
        Post {
            slug: slug.into(),
            file: String::new(),
            title: title.into(),
            description: "d & <d>".into(),
            date: NaiveDate::parse_from_str("2026-10-20", "%Y-%m-%d").unwrap(),
            updated: None,
            draft: false,
            tags,
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
            reading_time: 2,
            words: 300,
        }
    }

    #[test]
    fn generates_well_formed_rss_with_escaping() {
        let posts = vec![
            sample("a", "A <b> & c", vec!["rust".into()]),
            sample("b", "B", vec![]),
        ];
        let feed = generate(&config(), &posts).unwrap();
        assert!(feed.starts_with("<?xml"));
        assert_eq!(feed.matches("<item>").count(), 2);
        assert!(feed.contains("<guid isPermaLink=\"true\">https://example.com/posts/a/</guid>"));
        assert!(feed.contains("<title>A &lt;b&gt; &amp; c</title>"));
        assert!(feed.contains("<category>rust</category>"));
        assert!(feed.contains("Tue, 20 Oct 2026"));
    }
}
