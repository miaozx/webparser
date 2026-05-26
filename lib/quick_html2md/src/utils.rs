//! Utility functions for HTML to Markdown conversion.

use dom_query::Selection;

use crate::options::MarkdownOptions;

/// Get the lowercase tag name of a selection.
pub(crate) fn get_tag_name(sel: &Selection) -> String {
    sel.nodes()
        .first()
        .and_then(dom_query::NodeRef::node_name)
        .unwrap_or_default()
        .to_lowercase()
}

/// Maximum consecutive blank lines to preserve.
const MAX_BLANK_LINES: usize = 2;

/// Normalize markdown output by collapsing blank lines and trimming.
pub(crate) fn normalize_output(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut blank_count = 0;

    for line in text.lines() {
        let is_blank = line.trim().is_empty();

        if is_blank {
            blank_count += 1;
            // Only add blank line if we haven't exceeded the max
            if blank_count <= MAX_BLANK_LINES {
                result.push('\n');
            }
        } else {
            // Add the line with trailing whitespace trimmed
            result.push_str(line.trim_end());
            result.push('\n');
            blank_count = 0;
        }
    }

    // Trim leading/trailing whitespace and ensure single trailing newline
    let trimmed = result.trim();
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}\n")
    }
}

/// Resolve a URL against a base URL.
/// Returns the resolved URL string, or the original URL if resolution fails.
pub(crate) fn resolve_url(url: &str, options: &MarkdownOptions) -> String {
    let base_url = match &options.base_url {
        Some(base) => base,
        None => return url.to_string(),
    };

    // If URL is already absolute (has scheme), return as-is
    if url.contains("://") || url.starts_with("//") || url.starts_with("data:") {
        return url.to_string();
    }

    // Try to resolve using the url crate if available
    #[cfg(feature = "url")]
    {
        if let Ok(base) = url::Url::parse(base_url) {
            if let Ok(resolved) = base.join(url) {
                return resolved.to_string();
            }
        }
    }

    // Fallback: simple string-based resolution
    resolve_url_simple(url, base_url)
}

/// Simple string-based URL resolution fallback.
fn resolve_url_simple(url: &str, base_url: &str) -> String {
    if url.starts_with('/') {
        // Absolute path - join with origin
        let base = base_url.trim_end_matches('/');
        // Extract origin from base URL (scheme + host)
        if let Some(scheme_end) = base.find("://") {
            let after_scheme = &base[scheme_end + 3..];
            if let Some(path_start) = after_scheme.find('/') {
                let origin = &base[..scheme_end + 3 + path_start];
                return format!("{origin}{url}");
            }
        }
        format!("{base}{url}")
    } else {
        // Relative path - join with base directory
        // If base ends with '/', it's a directory - use it directly
        // Otherwise, find the parent directory
        if base_url.ends_with('/') {
            format!("{base_url}{url}")
        } else {
            let base_dir = if let Some(last_slash) = base_url.rfind('/') {
                &base_url[..=last_slash]
            } else {
                base_url
            };
            format!("{base_dir}{url}")
        }
    }
}

/// Escape markdown special characters in text.
///
/// Uses position-aware escaping to avoid over-escaping characters that only
/// create markdown constructs in specific positions. Always escapes the core
/// set (`\`, `` ` ``, `*`, `_`, `[`, `]`, `<`) and only escapes positional
/// characters (`#`, `>`, `-`, `+`, `.`, `!`) where they could create headings,
/// blockquotes, lists, or image syntax.
/// The `at_line_start` parameter indicates whether this text appears at the
/// true start of a line in the output (not inside a heading, blockquote, etc.).
/// When `false`, positional escaping (`#`, `>`, `-`, `+`, ordered list `.`) is
/// skipped since the line already has a markdown prefix that prevents ambiguity.
pub(crate) fn escape_markdown_text(text: &str, at_line_start: bool) -> String {
    let mut result = String::with_capacity(text.len() + text.len() / 10);
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut at_line_start = at_line_start;
    // Tracks whether we've seen only digits (and whitespace) since line start.
    // Used to detect ordered list markers like "1. " or "10. ".
    let mut only_digits_on_line = false;

    for (i, c) in text.char_indices() {
        match c {
            '\n' => {
                result.push('\n');
                at_line_start = true;
                only_digits_on_line = false;
                continue;
            }
            // Always escape: core markdown chars
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' => {
                result.push('\\');
                result.push(c);
            }
            // Heading marker: only at line start
            '#' if at_line_start => {
                result.push('\\');
                result.push(c);
            }
            // Blockquote marker: only at line start
            '>' if at_line_start => {
                result.push('\\');
                result.push(c);
            }
            // Unordered list markers: only at line start followed by space
            '-' | '+' if at_line_start && i + 1 < len && bytes[i + 1] == b' ' => {
                result.push('\\');
                result.push(c);
            }
            // Digits at line start: track for ordered list detection
            '0'..='9' if at_line_start || only_digits_on_line => {
                only_digits_on_line = true;
                at_line_start = false;
                result.push(c);
                continue;
            }
            // Ordered list marker: digits then ". " at line start
            '.' if only_digits_on_line && i + 1 < len && bytes[i + 1] == b' ' => {
                result.push('\\');
                result.push(c);
            }
            // Image syntax: ! immediately before [
            '!' if i + 1 < len && bytes[i + 1] == b'[' => {
                result.push('\\');
                result.push(c);
            }
            _ => result.push(c),
        }
        if c != ' ' && c != '\t' {
            at_line_start = false;
            only_digits_on_line = false;
        }
    }

    result
}

/// Escape special characters in URLs for markdown.
/// Escapes parentheses which would break markdown link syntax.
pub(crate) fn escape_url(url: &str) -> String {
    url.replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_output_collapses_blank_lines() {
        let input = "line1\n\n\n\n\nline2";
        let output = normalize_output(input);
        assert_eq!(output, "line1\n\n\nline2\n");
    }

    #[test]
    fn test_normalize_output_trims_trailing_whitespace() {
        let input = "line1   \nline2\t\t\n";
        let output = normalize_output(input);
        assert_eq!(output, "line1\nline2\n");
    }

    #[test]
    fn test_normalize_output_empty() {
        let input = "";
        let output = normalize_output(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_normalize_output_whitespace_only() {
        let input = "   \n\n\t\t\n   ";
        let output = normalize_output(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_resolve_url_no_base() {
        let options = MarkdownOptions::new();
        assert_eq!(resolve_url("/path/to/file", &options), "/path/to/file");
    }

    #[test]
    fn test_resolve_url_absolute_passthrough() {
        let options = MarkdownOptions::new().base_url("https://example.com");
        assert_eq!(
            resolve_url("https://other.com/page", &options),
            "https://other.com/page"
        );
    }

    #[test]
    fn test_resolve_url_absolute_path() {
        let options = MarkdownOptions::new().base_url("https://example.com/some/page");
        let resolved = resolve_url("/images/logo.png", &options);
        assert!(resolved.contains("example.com"));
        assert!(resolved.contains("/images/logo.png"));
    }

    #[test]
    fn test_resolve_url_relative_path() {
        let options = MarkdownOptions::new().base_url("https://example.com/docs/page.html");
        let resolved = resolve_url("images/logo.png", &options);
        assert!(resolved.contains("example.com"));
        assert!(resolved.contains("images/logo.png"));
    }

    #[test]
    fn test_resolve_url_data_uri_passthrough() {
        let options = MarkdownOptions::new().base_url("https://example.com");
        let data_uri = "data:image/png;base64,ABC123";
        assert_eq!(resolve_url(data_uri, &options), data_uri);
    }
}
