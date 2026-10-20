//! Integration tests: full builds against temporary content trees,
//! including the repository's real templates, SCSS and assets.

use std::path::{Path, PathBuf};

use rustfolio::build::{self, BuildOptions};

const CONFIG: &str = r#"
[site]
url = "https://example.com"
title = "Testfolio"
description = "A test build."
locale = "en"
posts_per_page = 2

[author]
name = "tester"
bio = "Writes tests."

[social]
github = "https://github.com/example"
"#;

const PUBLISHED: &str = r##"---
title: "The Published Post"
description: "A full-featured sample post."
date: 2026-10-20
tags: [rust, testing]
featured: true
toc: true
theme: paper
accent: "#E56A35"
---

## A heading

Body text with a [link](https://example.com) and a callout:

> Note: callouts work in integration tests too.

```rust
fn main() {
    println!("hello");
}
```
"##;

const OLDER: &str = r#"---
title: "The Older Post"
description: "Second post for pagination."
date: 2026-09-01
tags: [rust]
---

Older content.
"#;

const DRAFT: &str = r#"---
title: "Secret Draft"
description: "Must never ship."
date: 2026-10-01
draft: true
tags: [rust]
---

Draft content.
"#;

const ABOUT: &str =
    "---\ntitle: \"About\"\ndescription: \"About this site.\"\n---\n\nAbout page body.\n";
const PROJECTS: &str =
    "---\ntitle: \"Projects\"\ndescription: \"Things built.\"\n---\n\nProjects page body.\n";

fn copy_dir(src: &Path, dst: &Path) {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.unwrap();
        let rel = entry.path().strip_prefix(src).unwrap();
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
        } else {
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn write(path: &Path, content: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

fn sample_root() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    for dir in ["templates", "scss", "assets", "static"] {
        copy_dir(&manifest.join(dir), &root.join(dir));
    }
    write(&root.join("rustfolio.toml"), CONFIG);
    write(&root.join("content/posts/published-post.md"), PUBLISHED);
    write(&root.join("content/posts/older-post.md"), OLDER);
    write(&root.join("content/posts/secret-draft.md"), DRAFT);
    write(&root.join("content/pages/about.md"), ABOUT);
    write(&root.join("content/pages/projects.md"), PROJECTS);
    (tmp, root)
}

fn run_build(root: &Path, output: &str, include_drafts: bool) {
    build::run(&BuildOptions {
        root: root.to_path_buf(),
        output: root.join(output),
        include_drafts,
    })
    .expect("build should succeed");
}

fn read(root: &Path, rel: &str) -> String {
    std::fs::read_to_string(root.join(rel)).expect("file should exist")
}

fn assert_trees_equal(a: &Path, b: &Path, prefix: &Path) {
    let mut entries_a: Vec<PathBuf> = walkdir::WalkDir::new(a)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(a).unwrap().to_path_buf())
        .collect();
    entries_a.sort();
    let mut entries_b: Vec<PathBuf> = walkdir::WalkDir::new(b)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.path().strip_prefix(b).unwrap().to_path_buf())
        .collect();
    entries_b.sort();
    let names_a: Vec<String> = entries_a.iter().map(|p| p.display().to_string()).collect();
    let names_b: Vec<String> = entries_b.iter().map(|p| p.display().to_string()).collect();
    assert_eq!(
        names_a,
        names_b,
        "file lists differ under {}",
        prefix.display()
    );
    for (rel_a, rel_b) in entries_a.iter().zip(entries_b.iter()) {
        let content_a = std::fs::read(a.join(rel_a)).unwrap();
        let content_b = std::fs::read(b.join(rel_b)).unwrap();
        assert_eq!(
            content_a,
            content_b,
            "content differs for {}",
            rel_a.display()
        );
    }
}

#[test]
fn full_build_of_sample_site_is_complete_and_excludes_drafts() {
    let (_tmp, root) = sample_root();
    run_build(&root, "dist", false);
    let dist = root.join("dist");

    // Core pages exist.
    for artifact in [
        "index.html",
        "404.html",
        "blog/index.html",
        "archive/index.html",
        "tags/index.html",
        "about/index.html",
        "projects/index.html",
        "rss.xml",
        "sitemap.xml",
        "robots.txt",
        "search.json",
        "favicon.svg",
    ] {
        assert!(dist.join(artifact).is_file(), "missing {artifact}");
    }

    // With 2 published posts and per_page=2, only one blog page.
    assert!(!dist.join("blog/page/2/index.html").exists());

    // Draft is excluded everywhere.
    assert!(!dist.join("posts/secret-draft").exists());
    let index = read(&root, "dist/index.html");
    assert!(!index.contains("Secret Draft"));
    assert!(!index.contains("draft content"), "draft body must not leak");
    let search = read(&root, "dist/search.json");
    assert!(!search.contains("secret-draft"));

    // Article rendering: anchors, callout, highlighted code, accent class.
    let article = read(&root, "dist/posts/published-post/index.html");
    assert!(article.contains("id=\"a-heading\""), "{}", article);
    assert!(article.contains("callout-note"));
    assert!(article.contains("class=\"language-rust\""));
    assert!(article.contains("theme-paper"));
    assert!(
        article.contains("accent-published-post"),
        "accent css must be linked"
    );

    // RSS and sitemap correctness.
    let rss = read(&root, "dist/rss.xml");
    assert_eq!(rss.matches("<item>").count(), 2);
    assert!(rss.contains("https://example.com/posts/published-post/"));
    let sitemap = read(&root, "dist/sitemap.xml");
    assert!(sitemap.contains("https://example.com/posts/older-post/"));

    // Fingerprinted assets referenced by the HTML exist.
    let css_href = index
        .split("href=\"/assets/")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("stylesheet link")
        .to_string();
    assert!(
        dist.join("assets").join(&css_href).is_file(),
        "missing {css_href}"
    );
}

#[test]
fn build_output_is_deterministic() {
    let (_tmp, root) = sample_root();
    run_build(&root, "dist-a", false);
    run_build(&root, "dist-b", false);
    assert_trees_equal(&root.join("dist-a"), &root.join("dist-b"), &root);
}

#[test]
fn builds_with_drafts_included() {
    let (_tmp, root) = sample_root();
    run_build(&root, "dist", true);
    let dist = root.join("dist");
    assert!(dist.join("posts/secret-draft/index.html").is_file());
    let rss = read(&root, "dist/rss.xml");
    assert_eq!(rss.matches("<item>").count(), 3);
}

#[test]
fn invalid_frontmatter_fails_the_build_with_a_clear_error() {
    let (_tmp, root) = sample_root();
    write(
        &root.join("content/posts/broken.md"),
        "---\ntitle: Broken\ndescription: d\ndate: 2026-01-01\nnot_a_real_key: 1\n---\n\nx",
    );
    let err = build::run(&BuildOptions {
        root: root.clone(),
        output: root.join("dist"),
        include_drafts: false,
    })
    .unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("broken.md"), "message: {message}");
    assert!(message.contains("not_a_real_key"), "message: {message}");
    // The failed build must not publish a partial output.
    assert!(!root.join("dist").exists());
}

#[test]
fn broken_internal_link_fails_verification() {
    let (_tmp, root) = sample_root();
    write(
        &root.join("content/posts/links-bad.md"),
        "---\ntitle: Bad links\ndescription: d\ndate: 2026-10-02\ntags: [rust]\n---\n\nSee [this](/posts/does-not-exist/).\n",
    );
    let err = build::run(&BuildOptions {
        root: root.clone(),
        output: root.join("dist"),
        include_drafts: false,
    })
    .unwrap_err();
    let message = format!("{err:#}");
    assert!(message.contains("does-not-exist"), "message: {message}");
}
