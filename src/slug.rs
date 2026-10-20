//! Slug generation and small URL helpers.

/// Convert arbitrary text (usually a heading) into a URL-safe slug.
///
/// Non-alphanumeric characters collapse into single hyphens; leading and
/// trailing hyphens are trimmed. Non-ASCII characters are dropped, so
/// authors are encouraged to use descriptive ASCII headings.
pub fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    for ch in text.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

/// Percent-encode a value for use inside a query string (share links, feeds).
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(
            slugify("Building a Static Site Generator"),
            "building-a-static-site-generator"
        );
    }

    #[test]
    fn slugify_collapses_and_trims_hyphens() {
        assert_eq!(slugify("  Hello -- World!  "), "hello-world");
    }

    #[test]
    fn slugify_empty_is_untitled() {
        assert_eq!(slugify("??"), "untitled");
    }

    #[test]
    fn percent_encode_spaces_and_ampersands() {
        assert_eq!(percent_encode("a b&c"), "a%20b%26c");
    }
}
