//! Markdown → HTML rendering.
//!
//! One pass over `pulldown-cmark`'s event stream with targeted subtree
//! transforms:
//!
//! * heading IDs, anchor links and table-of-contents collection,
//! * syntax-highlighted code blocks (class-based, no inline styles),
//! * callouts from blockquotes (`> Note:` / `Tip:` / `Warning:` / `Caution:`),
//! * link-URL sanitisation with file/line diagnostics,
//! * relative image resolution plus lazy-loading attributes,
//! * raw HTML stripping unless the article opted into `trusted_html`.
//!
//! Raw HTML is *never* executed: at most it is emitted verbatim for
//! explicitly trusted articles authored by the site owner themselves.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;

use pulldown_cmark::{html::push_html, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

use crate::errors::SiteError;
use crate::highlight::Highlighter;
use crate::slug::slugify;

pub struct RenderOptions<'a> {
    /// Source file path, used in diagnostics.
    pub file: &'a str,
    /// Raw HTML passthrough for explicitly trusted articles.
    pub trusted_html: bool,
    pub highlighter: &'a Highlighter,
    /// Root directory that relative asset paths resolve against
    /// (the site's `static/` directory). Used to validate relative images.
    pub asset_root: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TocEntry {
    pub level: u8,
    pub id: String,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Rendered {
    pub html: String,
    pub toc: Vec<TocEntry>,
    /// Plain text (code included) used for the search index and reading time.
    pub text: String,
}

const CALLOUT_KINDS: [(&str, &str); 4] = [
    ("note", "Note"),
    ("tip", "Tip"),
    ("warning", "Warning"),
    ("caution", "Caution"),
];

/// Escape text for safe interpolation into HTML text nodes and attributes.
pub fn escape_html_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Allow http/https/mailto URLs, anchors, root-relative and relative paths.
/// Everything else (`javascript:`, `data:`, `vbscript:`, …) is rejected.
fn check_link_url(dest: &str) -> Result<(), &'static str> {
    let d = dest.trim();
    if d.is_empty() || d.starts_with('/') || d.starts_with('#') {
        return Ok(());
    }
    let authority_end = d.find(['/', '?', '#']).unwrap_or(d.len());
    match d[..authority_end].find(':') {
        Some(colon) if colon > 0 => {
            let scheme = d[..colon].to_ascii_lowercase();
            if matches!(scheme.as_str(), "http" | "https" | "mailto") {
                Ok(())
            } else {
                Err("blocked URL scheme")
            }
        }
        _ => Ok(()),
    }
}

fn heading_level(level: &HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Concatenate the textual content of an event subtree.
fn extract_text(events: &[Event]) -> String {
    let mut s = String::new();
    for ev in events {
        match ev {
            Event::Text(t) => s.push_str(t),
            Event::Code(t) => {
                s.push(' ');
                s.push_str(t);
                s.push(' ');
            }
            _ => {}
        }
    }
    s
}

/// Remove raw HTML events unless the article is trusted.
fn strip_unsafe_html(events: &mut Vec<Event>, trusted: bool) {
    if trusted {
        return;
    }
    events.retain(|ev| !matches!(ev, Event::Html(_) | Event::InlineHtml(_)));
}

/// Consume the event subtree that starts at `start` (the Start tag itself),
/// returning its inner events and the index of the matching End event.
fn take_inner<'a>(
    events: &'a [(Event<'a>, Range<usize>)],
    start: usize,
) -> (Vec<Event<'a>>, usize) {
    let mut depth = 1usize;
    let mut inner = Vec::new();
    let mut j = start + 1;
    while j < events.len() {
        match &events[j].0 {
            Event::Start(_) => depth += 1,
            Event::End(_) => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        inner.push(events[j].0.clone());
        j += 1;
    }
    (inner, j)
}

/// Detect and strip a callout prefix (`Note:` / `Tip:` / …) from the first
/// text run of a blockquote, returning the kind and the remaining first text.
fn take_callout(inner: &mut [Event]) -> Option<(&'static str, &'static str)> {
    for ev in inner {
        if let Event::Text(t) = ev {
            let head_end = t.find(':')?;
            let kind = t[..head_end].trim().to_ascii_lowercase();
            match CALLOUT_KINDS
                .iter()
                .find(|(slug, _)| *slug == kind)
                .copied()
            {
                Some((slug, label)) => {
                    let rest = t[head_end + 1..].trim_start().to_string();
                    *ev = Event::Text(rest.into());
                    return Some((label, slug));
                }
                None => return None,
            }
        }
    }
    None
}

/// Render a Markdown document to HTML.
pub fn render(body: &str, opts: &RenderOptions) -> anyhow::Result<Rendered> {
    let mut md_options = Options::empty();
    md_options.insert(
        Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TASKLISTS
            | Options::ENABLE_SMART_PUNCTUATION,
    );

    let events: Vec<(Event, Range<usize>)> = Parser::new_ext(body, md_options)
        .into_offset_iter()
        .collect();

    let mut out: Vec<Event> = Vec::new();
    let mut toc: Vec<TocEntry> = Vec::new();
    let mut text = String::new();
    let mut used_ids: HashMap<String, usize> = HashMap::new();

    let mut i = 0;
    while i < events.len() {
        let (ev, range) = &events[i];
        let line = body[..range.start].matches('\n').count() + 1;

        match ev {
            // ── Fenced / indented code blocks → highlighted figures ──
            Event::Start(Tag::CodeBlock(kind)) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                let (inner, end) = take_inner(&events, i);
                let code = extract_text(&inner);
                let highlighted = opts.highlighter.highlight(&code, &lang)?;
                text.push_str(&code);
                text.push('\n');
                let html = format!(
                    "<figure class=\"code-block\" data-lang=\"{}\">\
                     <figcaption class=\"code-header\"><span class=\"code-lang\">{}</span></figcaption>\
                     <pre><code class=\"language-{}\">{}</code></pre></figure>",
                    escape_html_text(&lang),
                    escape_html_text(&lang),
                    escape_html_text(&lang),
                    highlighted
                );
                out.push(Event::Html(html.into()));
                i = end;
            }

            // ── Headings → ids, anchor links, TOC entries ──
            Event::Start(Tag::Heading { level, .. }) => {
                let (mut inner, end) = take_inner(&events, i);
                let heading_text = extract_text(&inner);
                let base_id = slugify(&heading_text);
                let id = match used_ids.get(&base_id) {
                    Some(seen) => {
                        let n = seen + 1;
                        used_ids.insert(base_id.clone(), n);
                        format!("{base_id}-{n}")
                    }
                    None => {
                        used_ids.insert(base_id.clone(), 1);
                        base_id
                    }
                };
                let lvl = heading_level(level);
                text.push_str(&heading_text);
                text.push('\n');
                strip_unsafe_html(&mut inner, opts.trusted_html);
                let anchor = format!(
                    " <a class=\"heading-anchor\" href=\"#{id}\" aria-label=\"Permalink to this section\">#</a>"
                );
                let open = format!("<h{lvl} id=\"{}\">", escape_html_text(&id));
                let close = format!("{anchor}</h{lvl}>");
                out.push(Event::Html(open.into()));
                out.extend(inner);
                out.push(Event::Html(close.into()));
                if (2..=3).contains(&lvl) {
                    toc.push(TocEntry {
                        level: lvl,
                        id: id.clone(),
                        text: heading_text,
                    });
                }
                i = end;
            }

            // ── Links → sanitised anchors ──
            Event::Start(Tag::Link { dest_url, .. }) => {
                if let Err(reason) = check_link_url(dest_url) {
                    return Err(SiteError::UnsafeLink {
                        file: opts.file.to_string(),
                        line,
                        target: dest_url.to_string(),
                        message: reason.to_string(),
                    }
                    .into());
                }
                let (mut inner, end) = take_inner(&events, i);
                text.push_str(&extract_text(&inner));
                strip_unsafe_html(&mut inner, opts.trusted_html);
                let open = format!("<a href=\"{}\">", escape_html_text(dest_url.trim()));
                out.push(Event::Html(open.into()));
                out.extend(inner);
                out.push(Event::Html("</a>".into()));
                i = end;
            }

            // ── Images → lazy, async, relative-to-static resolution ──
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                if let Err(reason) = check_link_url(dest_url) {
                    return Err(SiteError::UnsafeLink {
                        file: opts.file.to_string(),
                        line,
                        target: dest_url.to_string(),
                        message: reason.to_string(),
                    }
                    .into());
                }
                let (inner, end) = take_inner(&events, i);
                let alt = extract_text(&inner);
                text.push_str(&alt);

                let mut src = dest_url.trim().to_string();
                let relative = !src.is_empty()
                    && !src.starts_with('/')
                    && !src.starts_with('#')
                    && !src.starts_with("http://")
                    && !src.starts_with("https://");
                if relative {
                    if let Some(root) = opts.asset_root {
                        if !root.join(&src).exists() {
                            return Err(SiteError::BrokenLink {
                                file: opts.file.to_string(),
                                target: src.clone(),
                                message: format!(
                                    "relative asset not found under `{}`",
                                    root.display()
                                ),
                            }
                            .into());
                        }
                    }
                    src = format!("/{src}");
                }
                let title_attr = if title.is_empty() {
                    String::new()
                } else {
                    format!(" title=\"{}\"", escape_html_text(title))
                };
                let html = format!(
                    "<img src=\"{}\" alt=\"{}\"{title_attr} loading=\"lazy\" decoding=\"async\">",
                    escape_html_text(&src),
                    escape_html_text(&alt)
                );
                out.push(Event::Html(html.into()));
                i = end;
            }

            // ── Blockquotes → callouts when labelled, quotes otherwise ──
            Event::Start(Tag::BlockQuote(_)) => {
                let (mut inner, end) = take_inner(&events, i);
                text.push_str(&extract_text(&inner));
                text.push('\n');
                match take_callout(&mut inner) {
                    Some((label, _slug)) => {
                        strip_unsafe_html(&mut inner, opts.trusted_html);
                        let mut body_html = String::new();
                        push_html(&mut body_html, inner.into_iter());
                        let html = format!(
                            "<aside class=\"callout callout-{}\" role=\"note\"><p class=\"callout-title\">{label}</p>{body_html}</aside>",
                            _slug
                        );
                        out.push(Event::Html(html.into()));
                    }
                    None => {
                        strip_unsafe_html(&mut inner, opts.trusted_html);
                        out.push(ev.clone());
                        out.extend(inner);
                        out.push(events[end].0.clone());
                    }
                }
                i = end;
            }

            // ── Raw HTML: only for explicitly trusted articles ──
            Event::Html(_) | Event::InlineHtml(_) => {
                if opts.trusted_html {
                    out.push(ev.clone());
                }
            }

            Event::Text(t) => {
                text.push_str(t);
                out.push(ev.clone());
            }

            _ => out.push(ev.clone()),
        }
        i += 1;
    }

    let mut html = String::new();
    push_html(&mut html, out.into_iter());
    Ok(Rendered {
        html,
        toc,
        text: text.trim().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_str(body: &str) -> Rendered {
        let hl = Highlighter::new();
        let opts = RenderOptions {
            file: "test.md",
            trusted_html: false,
            highlighter: &hl,
            asset_root: None,
        };
        render(body, &opts).expect("render should succeed")
    }

    #[test]
    fn renders_basic_markdown() {
        let r = render_str("Hello **world**");
        assert!(r.html.contains("<strong>world</strong>"));
        assert_eq!(r.text, "Hello world");
    }

    #[test]
    fn headings_get_ids_anchors_and_toc() {
        let r = render_str("# Title\n\n## Section One\n\n### Sub\n\n#### Deep");
        assert!(r.html.contains("<h2 id=\"section-one\">"));
        assert!(r.html.contains("href=\"#section-one\""));
        assert_eq!(r.toc.len(), 2, "only h2/h3 are collected");
        assert_eq!(r.toc[0].text, "Section One");
        assert_eq!(r.toc[1].level, 3);
    }

    #[test]
    fn duplicate_headings_get_unique_ids() {
        let r = render_str("## Alpha\n\n## Alpha");
        assert!(r.html.contains("id=\"alpha\""));
        assert!(r.html.contains("id=\"alpha-2\""));
    }

    #[test]
    fn unsafe_url_schemes_are_rejected_with_line_number() {
        let hl = Highlighter::new();
        let opts = RenderOptions {
            file: "post.md",
            trusted_html: false,
            highlighter: &hl,
            asset_root: None,
        };
        let body = "intro\n\n[click](javascript:alert(1))";
        let err = render(body, &opts).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("post.md"), "message: {msg}");
        assert!(msg.contains("line 3"), "message: {msg}");
        assert!(msg.contains("javascript"));
    }

    #[test]
    fn safe_url_schemes_are_allowed() {
        let r = render_str(
            "[a](https://example.com) [b](/posts/x/) [c](#x) [d](mailto:a@b.c) [e](./rel)",
        );
        assert_eq!(r.html.matches("<a href=").count(), 5);
    }

    #[test]
    fn raw_html_is_stripped_by_default_and_kept_when_trusted() {
        let hl = Highlighter::new();
        let strict = RenderOptions {
            file: "t.md",
            trusted_html: false,
            highlighter: &hl,
            asset_root: None,
        };
        let trusted = RenderOptions {
            file: "t.md",
            trusted_html: true,
            highlighter: &hl,
            asset_root: None,
        };

        let body = "<script>alert(1)</script>\n\nText <b>bold</b>";
        let strict_html = render(body, &strict).unwrap().html;
        assert!(!strict_html.contains("script"), "{strict_html}");
        assert!(!strict_html.contains("<b>bold</b>"), "{strict_html}");

        let trusted_html = render(body, &trusted).unwrap().html;
        assert!(trusted_html.contains("<b>bold</b>"));
    }

    #[test]
    fn code_blocks_are_highlighted_without_inline_styles() {
        let r = render_str("```rust\nfn main() {}\n```");
        assert!(r.html.contains("language-rust"));
        assert!(r.html.contains("code-block"));
        assert!(!r.html.contains("style="));
    }

    #[test]
    fn callouts_are_detected() {
        let r = render_str("> Note: this is important.\n\n> Warning: careful.\n\n> just a quote");
        assert!(r.html.contains("callout-note"), "{}", r.html);
        assert!(r.html.contains("callout-warning"));
        assert!(r.html.contains("this is important."));
        assert!(r.html.contains("<blockquote>"), "{}", r.html);
    }

    #[test]
    fn images_are_lazy_and_relative_paths_resolve_against_static_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("logo.svg"), "<svg/>").unwrap();
        let hl = Highlighter::new();
        let opts = RenderOptions {
            file: "t.md",
            trusted_html: false,
            highlighter: &hl,
            asset_root: Some(dir.path()),
        };

        let r = render("![Alt text](logo.svg)", &opts).unwrap();
        assert!(r.html.contains("src=\"/logo.svg\""), "{}", r.html);
        assert!(r.html.contains("loading=\"lazy\""));
        assert!(r.html.contains("alt=\"Alt text\""));

        let err = render("![Alt](missing.svg)", &opts).unwrap_err();
        assert!(format!("{err:#}").contains("missing.svg"));
    }

    #[test]
    fn tables_lists_and_footnotes_render() {
        let r =
            render_str("| a | b |\n|---|---|\n| 1 | 2 |\n\n- one\n- two\n\ntext[^1]\n\n[^1]: note");
        assert!(r.html.contains("<table>"));
        assert!(r.html.contains("<li>one</li>"));
        assert!(r.html.contains("footnote"), "{}", r.html);
    }
}
