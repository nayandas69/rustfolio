//! Build orchestration: the single pass that turns `content/`, `scss/`,
//! `templates/`, `assets/` and `static/` into a complete static site.
//!
//! The build is deterministic (sorted iteration, content fingerprints, no
//! wall-clock timestamps in output) and atomic: pages render and verify in
//! a staging directory that is only swapped into place on full success.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use chrono::Datelike;
use serde_json::{json, Map, Value};

use crate::config::Config;
use crate::content::{self, Post};
use crate::highlight::Highlighter;
use crate::output::{self, Staging};
use crate::templates::Engine;
use crate::{feed, scss, search, sitemap, validate};

pub struct BuildOptions {
    /// Project root containing `rustfolio.toml`, `content/`, `scss/`, …
    pub root: PathBuf,
    pub output: PathBuf,
    pub include_drafts: bool,
}

/// Pages that must exist for the site navigation to be correct.
const REQUIRED_PAGES: [&str; 2] = ["about", "projects"];

pub fn run(opts: &BuildOptions) -> Result<()> {
    let started = std::time::Instant::now();
    let root = &opts.root;

    let config = crate::config::load(&root.join("rustfolio.toml"))?;
    let highlighter = Highlighter::new();
    let static_root = root.join("static");

    let mut posts = content::load_posts(
        &root.join("content/posts"),
        opts.include_drafts,
        &highlighter,
        &static_root,
    )?;
    validate::validate_posts(&posts)?;
    let pages = content::load_pages(&root.join("content/pages"), &highlighter, &static_root)?;
    for slug in REQUIRED_PAGES {
        if !pages.iter().any(|p| p.slug == slug) {
            bail!(
                "missing required page `content/pages/{slug}.md` — the navigation links to /{slug}/"
            );
        }
    }

    let engine = Engine::load(&root.join("templates"))?;

    // ── Styles and scripts: compiled, fingerprinted, long-cached ──
    let main_css = scss::compile_file(&root.join("scss/main.scss"))?;
    let css_name = format!("main-{}.css", output::fingerprint(main_css.as_bytes()));
    let site_js = std::fs::read_to_string(root.join("assets/js/site.js"))?;
    let search_js = std::fs::read_to_string(root.join("assets/js/search.js"))?;
    let js_name = format!("site-{}.js", output::fingerprint(site_js.as_bytes()));
    let search_js_name = format!("search-{}.js", output::fingerprint(search_js.as_bytes()));

    let staging = Staging::stage(&opts.output)?;
    let result = build_into(
        &staging,
        root,
        &config,
        &engine,
        &mut posts,
        &pages,
        &main_css,
        &css_name,
        &site_js,
        &js_name,
        &search_js,
        &search_js_name,
    );
    if let Err(err) = result {
        // Never leave a partial staging directory behind.
        let _ = std::fs::remove_dir_all(&staging.dir);
        return Err(err);
    }
    staging.publish()?;

    tracing::info!(
        "built {} posts, {} pages, {} tags → {} ({:.2}s)",
        posts.len(),
        pages.len(),
        content::tag_counts(&posts).len(),
        opts.output.display(),
        started.elapsed().as_secs_f32()
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn build_into(
    staging: &Staging,
    root: &Path,
    config: &Config,
    engine: &Engine,
    posts: &mut [Post],
    pages: &[content::Page],
    main_css: &str,
    css_name: &str,
    site_js: &str,
    js_name: &str,
    search_js: &str,
    search_js_name: &str,
) -> Result<()> {
    staging.write(&format!("assets/{css_name}"), main_css.as_bytes())?;
    staging.write(&format!("assets/{js_name}"), site_js.as_bytes())?;
    staging.write(&format!("assets/{search_js_name}"), search_js.as_bytes())?;

    // Per-post accent overrides: validated hex compiled through grass into
    // a custom-property class stylesheet — never an inline style attribute.
    for post in posts.iter_mut() {
        if let Some(accent) = post.accent.clone() {
            let css = scss::compile_accent(&post.slug, &accent)?;
            let name = format!(
                "accent-{}-{}.css",
                post.slug,
                output::fingerprint(css.as_bytes())
            );
            staging.write(&format!("assets/{name}"), css.as_bytes())?;
            post.accent_css_url = Some(format!("/assets/{name}"));
        }
    }

    copy_static(&root.join("static"), staging)?;

    let common = common_ctx(config, css_name, js_name, search_js_name);

    // ── Home ──
    let mut ctx = clone_ctx(&common, "home");
    if let Some(featured) = content::featured(posts) {
        ctx.insert("featured_post".into(), post_card(featured));
    }
    let excluded: Vec<&str> = content::featured(posts)
        .map(|f| vec![f.slug.as_str()])
        .unwrap_or_default();
    ctx.insert(
        "recent".into(),
        json!(content::recent(posts, 6, &excluded)
            .iter()
            .map(|p| post_card(p))
            .collect::<Vec<_>>()),
    );
    ctx.insert("topics".into(), json!(topics(posts, 8)));
    ctx.insert(
        "latest".into(),
        posts.first().map(post_card).unwrap_or(Value::Null),
    );
    render(engine, staging, "home.html", "index.html", &ctx)?;

    // ── Blog index with pagination ──
    let per_page = config.site.posts_per_page.max(1);
    let total_pages = posts.len().div_ceil(per_page).max(1);
    for page_num in 1..=total_pages {
        let start = (page_num - 1) * per_page;
        let end = (start + per_page).min(posts.len());
        let mut ctx = clone_ctx(&common, "blog");
        ctx.insert(
            "posts".into(),
            json!(posts[start..end].iter().map(post_card).collect::<Vec<_>>()),
        );
        ctx.insert("page".into(), json!(page_num));
        ctx.insert("total_pages".into(), json!(total_pages));
        ctx.insert("all_tags".into(), json!(topics(posts, usize::MAX)));
        ctx.insert(
            "prev_url".into(),
            json!(match page_num {
                2 => Some("/blog/".to_string()),
                n if n > 2 => Some(format!("/blog/page/{}/", n - 1)),
                _ => None,
            }),
        );
        ctx.insert(
            "next_url".into(),
            json!(if page_num < total_pages {
                Some(format!("/blog/page/{}/", page_num + 1))
            } else {
                None
            }),
        );
        let rel = if page_num == 1 {
            "blog/index.html".to_string()
        } else {
            format!("blog/page/{page_num}/index.html")
        };
        render(engine, staging, "blog.html", &rel, &ctx)?;
    }

    // ── Article pages ──
    for (index, post) in posts.iter().enumerate() {
        let mut ctx = clone_ctx(&common, "blog");
        ctx.insert("post".into(), article_json(post));
        ctx.insert(
            "show_toc".into(),
            json!(post.toc_enabled && !post.toc.is_empty()),
        );
        // Posts are newest-first: index-1 is newer, index+1 is older.
        ctx.insert(
            "prev".into(),
            index
                .checked_sub(1)
                .map(|i| post_card(&posts[i]))
                .unwrap_or(Value::Null),
        );
        ctx.insert(
            "next".into(),
            if index + 1 < posts.len() {
                post_card(&posts[index + 1])
            } else {
                Value::Null
            },
        );
        ctx.insert(
            "related".into(),
            json!(content::related(post, posts)
                .iter()
                .map(|p| post_card(p))
                .collect::<Vec<_>>()),
        );
        let canonical = post
            .canonical
            .clone()
            .unwrap_or_else(|| format!("{}{}", config.base_url(), post.url()));
        ctx.insert("canonical_url".into(), json!(canonical));
        ctx.insert("og_image".into(), json!(og_image(config, post)));
        ctx.insert(
            "share_url".into(),
            json!(format!("{}{}", config.base_url(), post.url())),
        );
        ctx.insert(
            "share_text".into(),
            json!(format!("{} — {}", post.title, config.site.title)),
        );
        render(
            engine,
            staging,
            "article.html",
            &format!("posts/{}/index.html", post.slug),
            &ctx,
        )?;
    }

    // ── Tags ──
    let counts = content::tag_counts(posts);
    let mut ctx = clone_ctx(&common, "tags");
    ctx.insert("tags".into(), json!(tag_list(&counts)));
    render(engine, staging, "tags.html", "tags/index.html", &ctx)?;

    for (tag, count) in &counts {
        let mut ctx = clone_ctx(&common, "tags");
        ctx.insert("tag".into(), json!(tag));
        ctx.insert("count".into(), json!(count));
        ctx.insert(
            "posts".into(),
            json!(content::posts_with_tag(posts, tag)
                .iter()
                .map(|p| post_card(p))
                .collect::<Vec<_>>()),
        );
        render(
            engine,
            staging,
            "tag.html",
            &format!("tags/{}/index.html", crate::slug::slugify(tag)),
            &ctx,
        )?;
    }

    // ── Archive, grouped by year ──
    let mut ctx = clone_ctx(&common, "archive");
    let mut years: Vec<Map<String, Value>> = Vec::new();
    for post in posts.iter() {
        let year = post.date.year().to_string();
        match years.last_mut() {
            Some(last) if last["year"] == json!(year) => {
                if let Value::Array(items) = last.get_mut("posts").expect("posts always set") {
                    items.push(post_card(post));
                }
            }
            _ => {
                let mut group = Map::new();
                group.insert("year".into(), json!(year));
                group.insert("posts".into(), json!([post_card(post)]));
                years.push(group);
            }
        }
    }
    ctx.insert("years".into(), json!(years));
    ctx.insert("total".into(), json!(posts.len()));
    render(engine, staging, "archive.html", "archive/index.html", &ctx)?;

    // ── Standalone pages (about, projects) ──
    for page in pages {
        let mut ctx = clone_ctx(&common, &page.slug);
        ctx.insert(
            "page".into(),
            json!({
                "title": page.title,
                "description": page.description,
                "html": page.html,
                "toc": page.toc,
                "url": page.url(),
            }),
        );
        render(
            engine,
            staging,
            "page.html",
            &format!("{}/index.html", page.slug),
            &ctx,
        )?;
    }

    // ── 404 and error pages ──
    // `404.html` is served by Vercel automatically for unknown URLs; the
    // other three are plain static pages reachable at /500, /error and
    // /failed (clean URLs), usable as fallbacks on any static host.
    let ctx = clone_ctx(&common, "");
    render(engine, staging, "404.html", "404.html", &ctx)?;
    render(engine, staging, "500.html", "500.html", &ctx)?;
    render(engine, staging, "error.html", "error.html", &ctx)?;
    render(engine, staging, "failed.html", "failed.html", &ctx)?;

    // ── Search index, feeds, robots ──
    staging.write("search.json", search::build_index(posts)?.as_bytes())?;
    staging.write("rss.xml", feed::generate(config, posts)?.as_bytes())?;
    staging.write(
        "sitemap.xml",
        sitemap::generate(
            config,
            posts,
            &pages.iter().map(|p| p.slug.clone()).collect::<Vec<_>>(),
        )?
        .as_bytes(),
    )?;
    staging.write(
        "robots.txt",
        format!(
            "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
            config.base_url()
        )
        .as_bytes(),
    )?;

    // ── Verify before publishing ──
    validate::verify_site(&staging.dir, config, posts)
        .with_context(|| "built site failed verification; output was not published")?;
    Ok(())
}

fn render(
    engine: &Engine,
    staging: &Staging,
    template: &str,
    rel: &str,
    ctx: &Map<String, Value>,
) -> Result<()> {
    let html = engine.render(template, Value::Object(ctx.clone()))?;
    staging.write(rel, html.as_bytes())
}

fn copy_static(static_dir: &Path, staging: &Staging) -> Result<()> {
    if !static_dir.is_dir() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(static_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(static_dir)
            .expect("walkdir yields paths inside root")
            .to_string_lossy()
            .to_string();
        let bytes =
            std::fs::read(entry.path()).with_context(|| format!("reading static asset `{rel}`"))?;
        staging.write(&rel, &bytes)?;
    }
    Ok(())
}

fn common_ctx(
    config: &Config,
    css_name: &str,
    js_name: &str,
    search_js_name: &str,
) -> Map<String, Value> {
    let mut social = Vec::new();
    let entries = [
        ("GitHub", &config.social.github),
        ("Twitter / X", &config.social.twitter),
        ("LinkedIn", &config.social.linkedin),
        ("Mastodon", &config.social.mastodon),
    ];
    for (name, url) in entries {
        if let Some(url) = url {
            if !url.trim().is_empty() {
                social.push(json!({"name": name, "url": url}));
            }
        }
    }
    social.push(json!({"name": "RSS", "url": "/rss.xml"}));

    let mut map = Map::new();
    map.insert(
        "site".into(),
        json!({
            "title": config.site.title,
            "description": config.site.description,
            "locale": config.site.locale,
            "base_url": config.base_url(),
            "wordmark": config.site.title.to_lowercase(),
        }),
    );
    map.insert(
        "author".into(),
        json!({"name": config.author.name, "bio": config.author.bio}),
    );
    map.insert("social".into(), json!(social));
    map.insert("css_name".into(), json!(css_name));
    map.insert("js_name".into(), json!(js_name));
    map.insert("search_js_name".into(), json!(search_js_name));
    map.insert("year".into(), json!(chrono::Utc::now().year()));
    map
}

fn clone_ctx(common: &Map<String, Value>, nav: &str) -> Map<String, Value> {
    let mut ctx = common.clone();
    ctx.insert("nav".into(), json!(nav));
    ctx
}

fn post_card(post: &Post) -> Value {
    json!({
        "slug": post.slug,
        "url": post.url(),
        "title": post.title,
        "description": post.description,
        "tags": post.tags,
        "date_display": post.date_display(),
        "date_iso": post.date_iso(),
        "updated_iso": post.updated_iso(),
        "reading_time": post.reading_time,
        "featured": post.featured,
        "words": post.words,
    })
}

fn article_json(post: &Post) -> Value {
    json!({
        "slug": post.slug,
        "url": post.url(),
        "title": post.title,
        "description": post.description,
        "tags": post.tags,
        "date_display": post.date_display(),
        "date_iso": post.date_iso(),
        "updated_iso": post.updated_iso(),
        "reading_time": post.reading_time,
        "words": post.words,
        "html": post.html,
        "toc": post.toc,
        "theme_class": post.theme.class_name(),
        "width_class": post.reading_width.class_name(),
        "wide": post.wide,
        "accent_css_url": post.accent_css_url,
    })
}

fn topics(posts: &[Post], limit: usize) -> Vec<Value> {
    let mut topics: Vec<(String, usize)> = content::tag_counts(posts);
    topics.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    topics
        .into_iter()
        .take(limit)
        .map(|(tag, count)| {
            json!({
                "tag": tag,
                "count": count,
                "url": format!("/tags/{}/", crate::slug::slugify(&tag)),
            })
        })
        .collect()
}

fn tag_list(counts: &[(String, usize)]) -> Vec<Value> {
    counts
        .iter()
        .map(|(tag, count)| {
            json!({
                "tag": tag,
                "count": count,
                "url": format!("/tags/{}/", crate::slug::slugify(tag)),
            })
        })
        .collect()
}

fn og_image(config: &Config, post: &Post) -> String {
    match &post.social_image {
        Some(image) => {
            if image.starts_with("https://") || image.starts_with("http://") {
                image.clone()
            } else {
                format!("{}{}", config.base_url(), image)
            }
        }
        None => format!("{}/og-default.svg", config.base_url()),
    }
}
