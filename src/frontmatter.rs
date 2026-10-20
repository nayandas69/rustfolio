//! YAML frontmatter parsing and validation.
//!
//! Frontmatter is strict on purpose: unknown keys and invalid values are
//! hard errors, so typos in metadata never silently pass a build.
//!
//! The block is parsed by the small purpose-built parser below instead of a
//! general YAML crate. It understands exactly the documented schema subset —
//! flat `key: value` pairs, inline lists (`[a, b]`) and block lists
//! (`- item`) — which keeps error messages precise and drops the
//! unmaintained `serde_yaml`/`unsafe-libyaml` chain from the dependency
//! audit (RUSTSEC-2024-0449).

use chrono::NaiveDate;

use crate::errors::SiteError;

/// Per-article visual theme. Validated at parse time, rendered as a
/// pre-existing CSS class (never as an arbitrary inline style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Default,
    Dark,
    Paper,
    Minimal,
}

impl Theme {
    pub fn class_name(self) -> &'static str {
        match self {
            Self::Default => "theme-default",
            Self::Dark => "theme-dark",
            Self::Paper => "theme-paper",
            Self::Minimal => "theme-minimal",
        }
    }
}

/// Reading measure for the article body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadingWidth {
    #[default]
    Comfortable,
    Wide,
    Narrow,
}

impl ReadingWidth {
    pub fn class_name(self) -> &'static str {
        match self {
            Self::Comfortable => "width-comfortable",
            Self::Wide => "width-wide",
            Self::Narrow => "width-narrow",
        }
    }
}

/// One frontmatter value. The parser supports single-line strings and lists
/// of strings — everything the documented schema needs.
#[derive(Debug)]
enum Value {
    Str(String),
    List(Vec<String>),
    /// `key:` with no value and no list items.
    Null,
}

/// A parsed frontmatter block: ordered `key → value` entries.
#[derive(Debug)]
struct Fields {
    entries: Vec<(String, Value)>,
}

impl Fields {
    /// Parse the raw text between the `---` delimiters.
    fn parse(raw: &str, file: &str) -> Result<Self, SiteError> {
        let lines: Vec<&str> = raw.lines().collect();
        let mut entries = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];
            let line_no = i + 1;
            let trimmed = line.trim();
            i += 1;

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed == "-" || trimmed.starts_with("- ") {
                return Err(SiteError::frontmatter(
                    file,
                    format!("line {line_no}: list item outside a list"),
                ));
            }
            if line.starts_with([' ', '\t']) {
                return Err(SiteError::frontmatter(
                    file,
                    format!(
                        "line {line_no}: indentation is not supported (frontmatter must be flat)"
                    ),
                ));
            }

            let Some((key, value)) = trimmed.split_once(':') else {
                return Err(SiteError::frontmatter(
                    file,
                    format!("line {line_no}: expected `key: value`, got `{trimmed}`"),
                ));
            };
            // YAML requires the separator to end the line or be followed by
            // whitespace: `title:X` is a plain scalar, not a mapping key.
            if !value.is_empty() && !value.starts_with(' ') {
                return Err(SiteError::frontmatter(
                    file,
                    format!("line {line_no}: expected a space after the `:`"),
                ));
            }
            if !valid_key(key) {
                return Err(SiteError::frontmatter(
                    file,
                    format!(
                        "line {line_no}: invalid key `{key}` (use lowercase words, digits, `-` and `_`)"
                    ),
                ));
            }
            if entries.iter().any(|(k, _)| k == key) {
                return Err(SiteError::frontmatter(
                    file,
                    format!("line {line_no}: duplicate field `{key}`"),
                ));
            }

            let value_raw = value.trim();
            let value = if value_raw.is_empty() {
                // `key:` on its own: an indented block list, or nothing at all.
                let mut items = Vec::new();
                while i < lines.len() {
                    let next = lines[i].trim();
                    if next.is_empty() {
                        i += 1;
                    } else if next == "-" || next.starts_with("- ") {
                        let item = next.strip_prefix('-').unwrap_or(next).trim();
                        items.push(unquote(item, file, i + 1)?);
                        i += 1;
                    } else {
                        break;
                    }
                }
                if items.is_empty() {
                    Value::Null
                } else {
                    Value::List(items)
                }
            } else if let Some(rest) = value_raw.strip_prefix('[') {
                let inner = rest
                    .strip_suffix(']')
                    .ok_or_else(|| {
                        SiteError::frontmatter(
                            file,
                            format!("line {line_no}: unterminated list for `{key}`"),
                        )
                    })?
                    .trim();
                let mut items = Vec::new();
                if !inner.is_empty() {
                    for part in inner.split(',') {
                        items.push(unquote(part.trim(), file, line_no)?);
                    }
                }
                Value::List(items)
            } else if value_raw == "|" || value_raw == ">" {
                return Err(SiteError::frontmatter(
                    file,
                    format!("line {line_no}: block scalars are not supported in frontmatter"),
                ));
            } else {
                Value::Str(unquote(value_raw, file, line_no)?)
            };

            entries.push((key.to_string(), value));
        }

        Ok(Self { entries })
    }

    fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Reject keys outside `known`, naming the offender for actionable build
    /// errors (the strictness `deny_unknown_fields` used to provide).
    fn require_known(&self, file: &str, known: &[&str]) -> Result<(), SiteError> {
        for (key, _) in &self.entries {
            if !known.contains(&key.as_str()) {
                return Err(SiteError::frontmatter(
                    file,
                    format!("unknown field `{key}` (frontmatter is strict — check for typos)"),
                ));
            }
        }
        Ok(())
    }

    fn required_str(&self, key: &str, file: &str) -> Result<String, SiteError> {
        match self.get(key) {
            Some(Value::Str(s)) => Ok(s.clone()),
            Some(Value::List(_)) => Err(SiteError::frontmatter(
                file,
                format!("field `{key}` must be a single value, not a list"),
            )),
            Some(Value::Null) | None => Err(SiteError::frontmatter(
                file,
                format!("missing required field `{key}`"),
            )),
        }
    }

    fn optional_str(&self, key: &str, file: &str) -> Result<Option<String>, SiteError> {
        match self.get(key) {
            Some(Value::Str(s)) => Ok(Some(s.clone())),
            Some(Value::List(_)) => Err(SiteError::frontmatter(
                file,
                format!("field `{key}` must be a single value, not a list"),
            )),
            Some(Value::Null) | None => Ok(None),
        }
    }

    fn required_date(&self, key: &str, file: &str) -> Result<NaiveDate, SiteError> {
        parse_date(&self.required_str(key, file)?, key, file)
    }

    fn optional_date(&self, key: &str, file: &str) -> Result<Option<NaiveDate>, SiteError> {
        match self.optional_str(key, file)? {
            Some(raw) => parse_date(&raw, key, file).map(Some),
            None => Ok(None),
        }
    }

    fn boolean(&self, key: &str, file: &str, default: bool) -> Result<bool, SiteError> {
        match self.get(key) {
            Some(Value::Str(s)) if s == "true" => Ok(true),
            Some(Value::Str(s)) if s == "false" => Ok(false),
            Some(Value::Str(s)) => Err(SiteError::frontmatter(
                file,
                format!("field `{key}` must be `true` or `false`, got `{s}`"),
            )),
            Some(Value::List(_)) => Err(SiteError::frontmatter(
                file,
                format!("field `{key}` must be `true` or `false`, not a list"),
            )),
            Some(Value::Null) | None => Ok(default),
        }
    }

    fn str_list(&self, key: &str, file: &str) -> Result<Vec<String>, SiteError> {
        match self.get(key) {
            Some(Value::List(items)) => Ok(items.clone()),
            Some(Value::Str(s)) => Err(SiteError::frontmatter(
                file,
                format!(
                    "field `{key}` must be a list (inline `[…, …]` or one `- item` per line), got `{s}`"
                ),
            )),
            Some(Value::Null) | None => Ok(Vec::new()),
        }
    }

    /// A single-word value with a default when absent.
    fn keyword(&self, key: &str, file: &str, default: &str) -> Result<String, SiteError> {
        match self.get(key) {
            Some(Value::Str(s)) => Ok(s.clone()),
            Some(Value::List(_)) => Err(SiteError::frontmatter(
                file,
                format!("field `{key}` must be a single value, not a list"),
            )),
            Some(Value::Null) | None => Ok(default.to_string()),
        }
    }
}

/// Strip one pair of surrounding quotes. Values are otherwise taken
/// verbatim — the documented schema needs no escape sequences.
fn unquote(raw: &str, file: &str, line_no: usize) -> Result<String, SiteError> {
    let Some(quote) = raw.chars().next() else {
        return Ok(raw.to_string());
    };
    if quote != '"' && quote != '\'' {
        return Ok(raw.to_string());
    }
    let inner = &raw[quote.len_utf8()..];
    match inner.find(quote) {
        Some(end) if inner[end + quote.len_utf8()..].trim().is_empty() => {
            Ok(inner[..end].to_string())
        }
        _ => Err(SiteError::frontmatter(
            file,
            format!("line {line_no}: unterminated quoted string `{raw}`"),
        )),
    }
}

/// Frontmatter keys are lowercase identifiers with `-`/`_` separators.
fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

fn parse_date(raw: &str, key: &str, file: &str) -> Result<NaiveDate, SiteError> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|e| {
        SiteError::frontmatter(
            file,
            format!("field `{key}`: invalid date `{raw}` (expected YYYY-MM-DD): {e}"),
        )
    })
}

/// Parsed article frontmatter.
#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub title: String,
    pub description: String,
    pub date: NaiveDate,
    pub updated: Option<NaiveDate>,
    pub draft: bool,
    pub tags: Vec<String>,
    pub featured: bool,
    pub toc: bool,
    pub theme: Theme,
    /// Accent colour override, must be `#RRGGBB`.
    pub accent: Option<String>,
    pub wide: bool,
    pub reading_width: ReadingWidth,
    /// Explicitly opt in to raw HTML in this article. The default is to
    /// strip raw HTML from Markdown output.
    pub trusted_html: bool,
    pub canonical: Option<String>,
    pub social_image: Option<String>,
}

impl Frontmatter {
    /// Field names accepted in article frontmatter.
    const KNOWN: [&str; 15] = [
        "title",
        "description",
        "date",
        "updated",
        "draft",
        "tags",
        "featured",
        "toc",
        "theme",
        "accent",
        "wide",
        "reading_width",
        "trusted_html",
        "canonical",
        "social_image",
    ];

    fn from_fields(fields: &Fields, file: &str) -> Result<Self, SiteError> {
        fields.require_known(file, &Self::KNOWN)?;

        let theme = match fields.keyword("theme", file, "default")?.as_str() {
            "default" => Theme::Default,
            "dark" => Theme::Dark,
            "paper" => Theme::Paper,
            "minimal" => Theme::Minimal,
            other => {
                return Err(SiteError::frontmatter(
                    file,
                    format!(
                        "field `theme`: invalid value `{other}` (expected one of: default, dark, paper, minimal)"
                    ),
                ))
            }
        };
        let reading_width = match fields
            .keyword("reading_width", file, "comfortable")?
            .as_str()
        {
            "comfortable" => ReadingWidth::Comfortable,
            "wide" => ReadingWidth::Wide,
            "narrow" => ReadingWidth::Narrow,
            other => {
                return Err(SiteError::frontmatter(
                    file,
                    format!(
                        "field `reading_width`: invalid value `{other}` (expected one of: comfortable, wide, narrow)"
                    ),
                ))
            }
        };

        Ok(Self {
            title: fields.required_str("title", file)?,
            description: fields.required_str("description", file)?,
            date: fields.required_date("date", file)?,
            updated: fields.optional_date("updated", file)?,
            draft: fields.boolean("draft", file, false)?,
            tags: fields.str_list("tags", file)?,
            featured: fields.boolean("featured", file, false)?,
            toc: fields.boolean("toc", file, true)?,
            theme,
            accent: fields.optional_str("accent", file)?,
            wide: fields.boolean("wide", file, false)?,
            reading_width,
            trusted_html: fields.boolean("trusted_html", file, false)?,
            canonical: fields.optional_str("canonical", file)?,
            social_image: fields.optional_str("social_image", file)?,
        })
    }

    fn validate(&self, file: &str) -> Result<(), SiteError> {
        if self.title.trim().is_empty() {
            return Err(SiteError::frontmatter(file, "`title` must not be empty"));
        }
        if self.description.trim().is_empty() {
            return Err(SiteError::frontmatter(
                file,
                "`description` must not be empty",
            ));
        }
        for tag in &self.tags {
            if tag.trim().is_empty() {
                return Err(SiteError::frontmatter(
                    file,
                    "`tags` must not contain empty entries",
                ));
            }
            if crate::slug::slugify(tag) != *tag {
                return Err(SiteError::frontmatter(
                    file,
                    format!(
                        "tag `{tag}` must be lowercase kebab-case (e.g. `systems-programming`)"
                    ),
                ));
            }
        }
        for (field, value) in [("accent", &self.accent), ("canonical", &self.canonical)] {
            if let Some(value) = value {
                let ok = match field {
                    "accent" => {
                        let hex = value.strip_prefix('#').unwrap_or("");
                        hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())
                    }
                    _ => value.starts_with("https://") || value.starts_with('/'),
                };
                if !ok {
                    let expected = if field == "accent" {
                        "#RRGGBB"
                    } else {
                        "an absolute https:// URL or a root-relative path"
                    };
                    return Err(SiteError::unsupported(
                        file,
                        format!("`{field}` value `{value}` is invalid (expected {expected})"),
                    ));
                }
            }
        }
        if let (Some(updated), false) = (self.updated, self.draft) {
            if updated < self.date {
                return Err(SiteError::frontmatter(
                    file,
                    format!("`updated` ({updated}) is before `date` ({})", self.date),
                ));
            }
        }
        Ok(())
    }
}

/// Split a document into its raw YAML frontmatter and Markdown body.
fn split<'a>(source: &'a str, file: &str) -> Result<(&'a str, &'a str), SiteError> {
    let source = source.trim_start_matches('\u{feff}');
    let after_open = source.strip_prefix("---\n").ok_or_else(|| {
        SiteError::frontmatter(file, "document must begin with a `---` frontmatter block")
    })?;

    match after_open.find("\n---\n") {
        Some(i) => Ok((&after_open[..i], &after_open[i + 5..])),
        None => match after_open.find("\n---") {
            Some(i) if after_open[i + 4..].trim().is_empty() => Ok((&after_open[..i], "")),
            _ => Err(SiteError::frontmatter(
                file,
                "missing closing `---` delimiter",
            )),
        },
    }
}

/// Parse a Markdown document into its frontmatter and body.
///
/// The document must start with a `---` frontmatter block. Errors identify
/// the source file and line so build diagnostics are actionable.
pub fn parse(source: &str, file: &str) -> Result<(Frontmatter, String), SiteError> {
    let (raw, body) = split(source, file)?;
    let fields = Fields::parse(raw, file)?;
    let frontmatter = Frontmatter::from_fields(&fields, file)?;
    frontmatter.validate(file)?;
    Ok((frontmatter, body.to_string()))
}

/// Frontmatter for standalone pages (about, projects, …).
#[derive(Debug, Clone)]
pub struct PageFrontmatter {
    pub title: String,
    pub description: String,
}

impl PageFrontmatter {
    const KNOWN: [&str; 2] = ["title", "description"];

    fn from_fields(fields: &Fields, file: &str) -> Result<Self, SiteError> {
        fields.require_known(file, &Self::KNOWN)?;
        Ok(Self {
            title: fields.required_str("title", file)?,
            description: fields.required_str("description", file)?,
        })
    }
}

/// Parse a standalone page into its frontmatter and body.
pub fn parse_page(source: &str, file: &str) -> Result<(PageFrontmatter, String), SiteError> {
    let (raw, body) = split(source, file)?;
    let fields = Fields::parse(raw, file)?;
    let fm = PageFrontmatter::from_fields(&fields, file)?;
    if fm.title.trim().is_empty() {
        return Err(SiteError::frontmatter(file, "`title` must not be empty"));
    }
    Ok((fm, body.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "---\ntitle: \"T\"\ndescription: \"D\"\ndate: 2026-10-20\ntags: [rust, web]\nfeatured: true\ntoc: false\ntheme: paper\naccent: \"#E56A35\"\n---\n\nHello";

    #[test]
    fn parses_valid_frontmatter() {
        let (fm, body) = parse(VALID, "post.md").expect("valid frontmatter");
        assert_eq!(fm.title, "T");
        assert_eq!(fm.tags, vec!["rust", "web"]);
        assert_eq!(fm.theme, Theme::Paper);
        assert!(!fm.toc);
        assert_eq!(fm.accent.as_deref(), Some("#E56A35"));
        assert_eq!(body.trim(), "Hello");
    }

    #[test]
    fn block_lists_and_plain_scalars_parse() {
        let src = "---\ntitle: \"Notes: a colon inside\"\ndescription: plain words\ndate: 2026-03-04\ntags:\n  - rust\n  - web-development\nupdated:\n---\n";
        let (fm, _) = parse(src, "post.md").unwrap();
        assert_eq!(fm.title, "Notes: a colon inside");
        assert_eq!(fm.description, "plain words");
        assert_eq!(fm.tags, vec!["rust", "web-development"]);
        assert!(fm.updated.is_none());
    }

    #[test]
    fn quoted_items_in_inline_lists_parse() {
        let src = "---\ntitle: T\ndescription: D\ndate: 2026-01-02\ntags: [\"rust\", web]\n---\n";
        let (fm, _) = parse(src, "post.md").unwrap();
        assert_eq!(fm.tags, vec!["rust", "web"]);
    }

    #[test]
    fn defaults_are_sensible() {
        let (fm, _) = parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\n---\n",
            "post.md",
        )
        .unwrap();
        assert!(!fm.draft && !fm.featured && fm.toc);
        assert_eq!(fm.theme, Theme::Default);
        assert_eq!(fm.reading_width, ReadingWidth::Comfortable);
        assert!(!fm.trusted_html);
    }

    #[test]
    fn missing_required_fields_are_errors() {
        assert!(parse("---\ndescription: D\ndate: 2026-01-02\n---\n", "post.md").is_err());
        assert!(parse("---\ntitle: T\ndate: 2026-01-02\n---\n", "post.md").is_err());
        assert!(parse("---\ntitle: T\ndescription: D\n---\n", "post.md").is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\ntotally_made_up: 1\n---\n",
            "post.md",
        );
        assert!(err.is_err());
    }

    #[test]
    fn invalid_dates_and_themes_are_errors() {
        assert!(parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-02-30\n---\n",
            "post.md"
        )
        .is_err());
        assert!(parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\ntheme: neon\n---\n",
            "post.md"
        )
        .is_err());
        assert!(parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\naccent: red\n---\n",
            "post.md"
        )
        .is_err());
    }

    #[test]
    fn updated_before_date_is_rejected() {
        let src = "---\ntitle: T\ndescription: D\ndate: 2026-10-20\nupdated: 2026-01-01\n---\n";
        assert!(parse(src, "post.md").is_err());
    }

    #[test]
    fn drafts_may_have_inconsistent_dates() {
        let src = "---\ntitle: T\ndescription: D\ndate: 2026-10-20\nupdated: 2026-01-01\ndraft: true\n---\n";
        assert!(parse(src, "post.md").is_ok());
    }

    #[test]
    fn missing_frontmatter_block_is_an_error() {
        assert!(parse("just markdown", "post.md").is_err());
        assert!(parse(
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\n",
            "post.md"
        )
        .is_err());
    }

    #[test]
    fn duplicate_fields_are_rejected() {
        let src = "---\ntitle: T\ntitle: U\ndescription: D\ndate: 2026-01-02\n---\n";
        assert!(parse(src, "post.md").is_err());
    }

    #[test]
    fn invalid_booleans_are_rejected() {
        let src = "---\ntitle: T\ndescription: D\ndate: 2026-01-02\ndraft: yes\n---\n";
        assert!(parse(src, "post.md").is_err());
    }

    #[test]
    fn unsupported_yaml_is_rejected() {
        let sources = [
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\nbody: |\n  text\n---\n",
            "---\n  title: T\ndescription: D\ndate: 2026-01-02\n---\n",
            "---\ntitle: T\ndescription: D\ndate: 2026-01-02\n- stray\n---\n",
            "---\ntitle:\"no space after colon\"\ndescription: D\ndate: 2026-01-02\n---\n",
        ];
        for src in sources {
            assert!(parse(src, "post.md").is_err(), "should reject: {src:?}");
        }
    }

    #[test]
    fn pages_accept_only_title_and_description() {
        let (page, body) = parse_page(
            "---\ntitle: About\ndescription: \"About this site.\"\n---\n\nBody.\n",
            "about.md",
        )
        .unwrap();
        assert_eq!(page.title, "About");
        assert_eq!(body.trim(), "Body.");
        assert!(parse_page(
            "---\ntitle: About\ndescription: D\ndate: 2026-01-02\n---\n",
            "about.md"
        )
        .is_err());
    }
}
