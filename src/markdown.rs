//! Markdown processing utilities.
//!
//! This module provides utilities for generating clean Markdown output,
//! including escaping special characters and converting HTML tables to GFM format.

/// Characters that have special meaning in Markdown and need escaping.
const MARKDOWN_SPECIAL_CHARS: &[char] = &['\\', '*', '_', '[', ']', '<', '>'];

/// Escape Markdown special characters in text content.
///
/// This function prevents accidental Markdown interpretation of content that
/// contains literal asterisks, underscores, brackets, etc.
///
/// # Arguments
///
/// * `text` - The text content to escape
/// * `in_code_block` - If true, skip escaping (code blocks preserve literal content)
///
/// # Characters Escaped
///
/// - `\` → `\\` (backslash)
/// - `*` → `\*` (asterisk - prevents italic/bold)
/// - `_` → `\_` (underscore - prevents italic/bold)
/// - `[` → `\[` (bracket - prevents links)
/// - `]` → `\]` (bracket - prevents links)
/// - `<` → `\<` (angle bracket - prevents HTML)
/// - `>` → `\>` (angle bracket - prevents blockquotes)
///
/// # Examples
///
/// ```
/// use rs_trafilatura::markdown::escape_markdown;
///
/// // Asterisks are escaped to prevent italic
/// assert_eq!(escape_markdown("*not italic*", false), r"\*not italic\*");
///
/// // Underscores are escaped to prevent italic
/// assert_eq!(escape_markdown("my_variable_name", false), r"my\_variable\_name");
///
/// // Code blocks are not escaped
/// assert_eq!(escape_markdown("*text*", true), "*text*");
/// ```
#[must_use]
pub fn escape_markdown(text: &str, in_code_block: bool) -> String {
    if in_code_block || text.is_empty() {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len() + text.len() / 4);

    for ch in text.chars() {
        if MARKDOWN_SPECIAL_CHARS.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
    }

    result
}

/// Post-process Markdown output to escape special characters in text content.
///
/// This function walks through Markdown content and escapes special characters
/// that appear outside of:
/// - Code blocks (fenced ``` or indented)
/// - Inline code (backticks)
/// - Already-escaped sequences
///
/// # Arguments
///
/// * `markdown` - The raw Markdown output from html-cleaning
///
/// # Returns
///
/// Markdown with properly escaped special characters in text content.
///
/// # Deprecation
///
/// Since quick_html2md v0.2, position-aware escaping is handled natively
/// by the converter when `escape_special_chars(true)` is set. This function
/// is no longer called internally but is kept for backwards compatibility.
#[must_use]
#[deprecated(since = "0.1.2", note = "Use quick_html2md's built-in escape_special_chars option instead")]
pub fn post_process_markdown(markdown: &str) -> String {
    if markdown.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(markdown.len() + markdown.len() / 8);
    let mut chars = markdown.chars().peekable();
    let mut in_fenced_code = false;
    let mut in_inline_code = false;
    let mut line_start = true;

    while let Some(ch) = chars.next() {
        // Track fenced code blocks (```)
        if line_start && ch == '`' {
            let mut backtick_count = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                backtick_count += 1;
            }

            if backtick_count >= 3 {
                in_fenced_code = !in_fenced_code;
                for _ in 0..backtick_count {
                    result.push('`');
                }
                continue;
            } else {
                // Not a fence, handle as inline code
                for _ in 0..backtick_count {
                    result.push('`');
                }
                in_inline_code = !in_inline_code;
                continue;
            }
        }

        // Track inline code
        if ch == '`' && !in_fenced_code {
            in_inline_code = !in_inline_code;
            result.push(ch);
            line_start = false;
            continue;
        }

        // Track line starts
        if ch == '\n' {
            result.push(ch);
            line_start = true;
            continue;
        }

        // If in code block or inline code, don't escape
        if in_fenced_code || in_inline_code {
            result.push(ch);
            line_start = false;
            continue;
        }

        // Skip already-escaped characters
        if ch == '\\' {
            result.push(ch);
            if let Some(&next) = chars.peek() {
                if MARKDOWN_SPECIAL_CHARS.contains(&next) {
                    if let Some(next_ch) = chars.next() {
                        result.push(next_ch);
                    }
                }
            }
            line_start = false;
            continue;
        }

        // Don't escape markdown formatting that should be preserved
        // (bold, italic, links, headings)
        // We only escape special chars that appear as literal text

        // Check for patterns we should preserve:
        // - **bold** and *italic* (matched pairs)
        // - [link](url) format
        // - # headings at line start
        // - - or * list items at line start

        // Preserve heading markers at line start
        if line_start && ch == '#' {
            result.push(ch);
            line_start = false;
            continue;
        }

        // Preserve blockquote markers at line start (> text, > > nested)
        if line_start && ch == '>' {
            result.push(ch);
            line_start = false;
            continue;
        }
        // Also preserve > after "> " (nested blockquotes)
        if ch == '>' && result.ends_with("> ") {
            result.push(ch);
            line_start = false;
            continue;
        }

        // Preserve list markers at line start
        if line_start && (ch == '-' || ch == '*' || ch == '+') {
            if chars.peek() == Some(&' ') {
                result.push(ch);
                line_start = false;
                continue;
            }
        }

        // For asterisks and underscores, we need context-aware escaping
        // Don't escape if it's part of markdown formatting (matched pairs)
        // **bold**, *italic*, __strong__, _emphasis_
        if ch == '*' || ch == '_' {
            // Look ahead to detect patterns
            let mut peek_chars = chars.clone();
            let next1 = peek_chars.next();
            let next2 = peek_chars.next();

            // Check for **bold** or __strong__ (double char pattern)
            let is_double = next1 == Some(ch);

            // Check for *italic* or _emphasis_ (single char, surrounded by non-chars)
            // Look back at what's in result
            let prev = result.chars().last();
            let prev_is_space = prev.map_or(true, |c| c.is_whitespace());
            let prev_is_word = prev.map_or(false, |c| c.is_alphanumeric());

            // Look at what comes after the potential marker
            let after_marker = if is_double { next2 } else { next1 };
            let next_is_word = after_marker.map_or(false, |c| c.is_alphanumeric());
            let next_is_space = after_marker.map_or(true, |c| c.is_whitespace() || c == ch);

            if is_double {
                // ** or __ - likely bold/strong opening or closing
                // Push both characters
                result.push(ch);
                result.push(ch);
                // Consume the second char
                chars.next();
            } else if (prev_is_space || prev_is_word) && next_is_word {
                // Looks like *open* (italic/em) - preserve
                result.push(ch);
            } else if prev_is_word && (next_is_space || next1 == Some(ch)) {
                // Looks like *close* (italic/em) - preserve
                result.push(ch);
            } else {
                // Likely literal asterisk/underscore, escape
                result.push('\\');
                result.push(ch);
            }
            line_start = false;
            continue;
        }

        // Preserve link brackets [ and ] when part of [text](url) pattern
        if ch == '[' {
            // Look ahead for ](url) pattern - this is likely a markdown link
            let remaining: String = chars.clone().collect();
            if remaining.contains("](") {
                result.push(ch);
                line_start = false;
                continue;
            }
        }
        if ch == ']' {
            if chars.peek() == Some(&'(') {
                result.push(ch);
                line_start = false;
                continue;
            }
        }

        // Preserve < and > in HTML-like contexts (e.g., <https://...>)
        // but escape in plain text
        if ch == '<' {
            let next = chars.peek();
            if next == Some(&'h') || next == Some(&'/') {
                // Likely <https://...> or closing tag remnant — preserve
                result.push(ch);
                line_start = false;
                continue;
            }
        }

        // Escape other special characters
        if MARKDOWN_SPECIAL_CHARS.contains(&ch) {
            result.push('\\');
        }
        result.push(ch);
        line_start = ch.is_whitespace();
    }

    result
}

/// Extract and clean tables from HTML for markdown output.
///
/// Tables are cleaned by removing style attributes and removing
/// tabs and newlines within table tags, then replaced with
/// placeholders for later restoration.
///
/// # Arguments
///
/// * `html` - The HTML content containing tables
///
/// # Returns
///
/// `(processed_html, table_list)` where `processed_html` has tables
/// replaced with placeholders, and `table_list` contains the cleaned
/// table HTML strings.
#[must_use]
pub fn extract_and_clean_tables(html: &str) -> (String, Vec<String>) {
    use regex::Regex;

    let mut tables: Vec<String> = Vec::new();
    let mut result = String::new();
    let mut remaining = html;
    let mut idx = 0;

    let style_re = Regex::new(r#"\s+style\s*=\s*"[^"]*""#).unwrap();
    let style_re2 = Regex::new(r#"\s+style\s*=\s*'[^']*'"#).unwrap();
    let link_re = Regex::new(r#"<a\b[^>]*>"#).unwrap();
    let link_close_re = Regex::new(r#"</a>"#).unwrap();

    while let Some(start) = remaining.find("<table") {
        let tag_start = start;

        // Find end of opening <table ...> tag
        let after_tag = match remaining[tag_start..].find('>') {
            Some(p) => tag_start + p + 1,
            None => {
                result.push_str(remaining);
                break;
            }
        };

        if after_tag >= remaining.len() {
            result.push_str(remaining);
            break;
        }

        // Find matching </table> with nesting support
        // Use byte-level comparisons to avoid UTF-8 boundary panics
        let mut depth = 1u32;
        let mut search_pos = after_tag;
        let bytes = remaining.as_bytes();
        let table_end = loop {
            if search_pos >= remaining.len() {
                break remaining.len();
            }

            if bytes[search_pos..].starts_with(b"</table>") {
                depth -= 1;
                if depth == 0 {
                    break search_pos + 8;
                }
                search_pos += 8;
            } else if bytes[search_pos..].starts_with(b"<table") {
                depth += 1;
                search_pos += 6;
            } else {
                search_pos += 1;
            }
        };

        // Push content before the table
        result.push_str(&remaining[..tag_start]);

        // Extract, clean, and store the table
        let table_html = &remaining[tag_start..table_end];
        let cleaned = style_re.replace_all(table_html, "");
        let cleaned = style_re2.replace_all(&cleaned, "");
        let cleaned = link_re.replace_all(&cleaned, "");
        let cleaned = link_close_re.replace_all(&cleaned, "");
        let cleaned = cleaned.replace('\t', "").replace('\n', "").replace('\r', "");

        let placeholder = format!("TBLPLCHLDR{idx:04}");
        idx += 1;
        tables.push(cleaned.to_string());

        // Insert placeholder wrapped in <p> tag to prevent quick_html2md from stripping it
        result.push_str("<p>");
        result.push_str(&placeholder);
        result.push_str("</p>\n");

        remaining = &remaining[table_end..];
    }

    result.push_str(remaining);

    (result, tables)
}

/// Restore table placeholders in markdown with cleaned table HTML.
///
/// # Arguments
///
/// * `markdown` - Markdown content with table placeholders
/// * `tables` - List of cleaned table HTML strings to restore
#[must_use]
pub fn restore_tables_in_markdown(markdown: &str, tables: &[String]) -> String {
    let mut result = markdown.to_string();
    for (i, table_html) in tables.iter().enumerate() {
        let placeholder = format!("TBLPLCHLDR{i:04}");
        result = result.replace(&placeholder, table_html);
    }
    result
}

/// Wrap orphan `<li>` elements (not inside `<ul>`/`<ol>`) in `<ul>` tags
/// so quick_html2md can convert them to markdown lists.
#[must_use]
/// Append a character at byte position `i` in `html`, handling multi-byte UTF-8.
/// Returns the number of bytes consumed (1 for ASCII, 2-4 for multi-byte).
#[inline]
fn push_char_at(result: &mut String, html: &str, i: usize) -> usize {
    if html.as_bytes()[i] < 128 {
        result.push(html.as_bytes()[i] as char);
        1
    } else {
        let ch = html[i..].chars().next().unwrap();
        result.push(ch);
        ch.len_utf8()
    }
}

pub fn wrap_orphan_lists(html: &str) -> String {
    let mut result = String::new();
    let bytes = html.as_bytes();
    let mut i = 0;
    let mut ul_depth = 0;
    let mut table_depth = 0;
    let mut in_group = false;
    let mut in_li = false;
    let mut text_after_li = false;

    while i < bytes.len() {
        // Handle tag starts
        if bytes[i] == b'<' {
            // </ul> or </ol>
            if i + 5 < bytes.len()
                && bytes[i + 1] == b'/'
                && ((bytes[i + 2] | 32) == b'u' || (bytes[i + 2] | 32) == b'o')
                && (bytes[i + 3] | 32) == b'l'
                && bytes[i + 4] == b'>'
            {
                close_group(&mut result, &mut in_group);
                in_li = false;
                text_after_li = false;
                if ul_depth > 0 {
                    ul_depth -= 1;
                }
                result.push_str(&html[i..i + 5]);
                i += 5;
                continue;
            }
            // <ul> or <ol> (opening)
            if i + 3 < bytes.len()
                && bytes[i + 1] != b'/'
                && ((bytes[i + 1] | 32) == b'u' || (bytes[i + 1] | 32) == b'o')
                && (bytes[i + 2] | 32) == b'l'
                && !bytes[i + 3].is_ascii_alphabetic()
            {
                let end = find_gt(bytes, i + 1);
                if end > 0 {
                    close_group(&mut result, &mut in_group);
                    in_li = false;
                    text_after_li = false;
                    ul_depth += 1;
                    result.push_str(&html[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
            // <li (opening) - case-insensitive
            if i + 3 < bytes.len()
                && bytes[i + 1] != b'/'
                && (bytes[i + 1] | 32) == b'l'
                && (bytes[i + 2] | 32) == b'i'
                && !bytes[i + 3].is_ascii_alphabetic()
            {
                let end = find_gt(bytes, i + 1);
                if end > 0 {
                    if ul_depth == 0 && table_depth == 0 && !in_group {
                        result.push_str("<ul>");
                        in_group = true;
                    }
                    in_li = true;
                    text_after_li = false;
                    result.push_str(&html[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
            // </li>
            if i + 5 < bytes.len()
                && bytes[i + 1] == b'/'
                && (bytes[i + 2] | 32) == b'l'
                && (bytes[i + 3] | 32) == b'i'
                && bytes[i + 4] == b'>'
            {
                result.push_str("</li>");
                i += 5;
                in_li = false;
                text_after_li = true;
                continue;
            }
            // Any tag (open or close) at depth 0 outside <li> closes orphan group
            if ul_depth == 0 && table_depth == 0 && in_group && !in_li {
                close_group(&mut result, &mut in_group);
            }
            // </table>
            if i + 8 < bytes.len()
                && bytes[i + 1] == b'/'
                && (bytes[i + 2] | 32) == b't'
                && (bytes[i + 3] | 32) == b'a'
                && (bytes[i + 4] | 32) == b'b'
                && (bytes[i + 5] | 32) == b'l'
                && (bytes[i + 6] | 32) == b'e'
                && bytes[i + 7] == b'>'
            {
                if table_depth > 0 {
                    table_depth -= 1;
                }
                result.push_str("</table>");
                i += 8;
                continue;
            }
            // <table (opening)
            if i + 6 < bytes.len()
                && bytes[i + 1] != b'/'
                && (bytes[i + 1] | 32) == b't'
                && (bytes[i + 2] | 32) == b'a'
                && (bytes[i + 3] | 32) == b'b'
                && (bytes[i + 4] | 32) == b'l'
                && (bytes[i + 5] | 32) == b'e'
                && !bytes[i + 6].is_ascii_alphabetic()
            {
                let end = find_gt(bytes, i + 1);
                if end > 0 {
                    table_depth += 1;
                    result.push_str(&html[i..=end]);
                    i = end + 1;
                    continue;
                }
            }
            // Copy the tag character
            text_after_li = false;
            let adv = push_char_at(&mut result, html, i);
            i += adv;
            continue;
        }
        // Non-whitespace text when not inside <li> and not inside table/ul closes orphan group
        if in_group && !in_li && table_depth == 0 && text_after_li && !bytes[i].is_ascii_whitespace() {
            close_group(&mut result, &mut in_group);
        }
        if !bytes[i].is_ascii_whitespace() {
            text_after_li = false;
        }
        let adv = push_char_at(&mut result, html, i);
        i += adv;
    }

    close_group(&mut result, &mut in_group);
    result
}

fn close_group(result: &mut String, in_group: &mut bool) {
    if *in_group {
        result.push_str("</ul>");
        *in_group = false;
    }
}

fn find_gt(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    while j < bytes.len() && bytes[j] != b'>' {
        j += 1;
    }
    if j < bytes.len() { j } else { 0 }
}

/// Convert an HTML table to GitHub Flavored Markdown format.
///
/// # Arguments
///
/// * `table_html` - The HTML table content
///
/// # Returns
///
/// GFM table string with proper formatting.
///
/// # Example Output
///
/// ```text
/// | Header A | Header B |
/// |----------|----------|
/// | Cell 1   | Cell 2   |
/// ```
#[must_use]
pub fn html_table_to_markdown(table_html: &str) -> String {
    use dom_query::Document;

    let doc = Document::from(table_html);
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut has_header = false;
    let mut alignments: Vec<Alignment> = Vec::new();

    // Extract header row
    let thead = doc.select("thead tr");
    if thead.length() > 0 {
        has_header = true;
        for tr in thead.iter() {
            let mut row = Vec::new();
            for th in tr.select("th").iter() {
                let text = th.text().trim().to_string();
                let align = th.attr("align")
                    .map(|a| Alignment::from_str(&a))
                    .unwrap_or(Alignment::None);
                alignments.push(align);
                row.push(text);
            }
            if !row.is_empty() {
                rows.push(row);
            }
        }
    }

    // Extract body rows
    let tbody_rows = doc.select("tbody tr, table > tr");
    for tr in tbody_rows.iter() {
        let mut row = Vec::new();
        let cells = tr.select("td, th");
        for (i, cell) in cells.iter().enumerate() {
            let text = cell.text().trim().to_string();

            // Capture alignment from first row if no header
            if !has_header && rows.is_empty() {
                let align = cell.attr("align")
                    .map(|a| Alignment::from_str(&a))
                    .unwrap_or(Alignment::None);
                alignments.push(align);
            } else if i < alignments.len() && alignments[i] == Alignment::None {
                // Update alignment if not set
                if let Some(align_str) = cell.attr("align") {
                    alignments[i] = Alignment::from_str(&align_str);
                }
            }

            row.push(text);
        }
        if !row.is_empty() {
            rows.push(row);
        }
    }

    if rows.is_empty() {
        return String::new();
    }

    // Calculate column widths
    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<usize> = vec![3; col_count]; // Minimum width for ---

    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            if i < col_widths.len() {
                col_widths[i] = col_widths[i].max(cell.len());
            }
        }
    }

    // Ensure alignments vector is the right size
    while alignments.len() < col_count {
        alignments.push(Alignment::None);
    }

    // Build output
    let mut output = String::new();

    for (row_idx, row) in rows.iter().enumerate() {
        // Build row
        output.push('|');
        for (col_idx, cell) in row.iter().enumerate() {
            let width = col_widths.get(col_idx).copied().unwrap_or(3);
            output.push(' ');
            output.push_str(&pad_cell(cell, width, alignments.get(col_idx).copied().unwrap_or(Alignment::None)));
            output.push_str(" |");
        }
        // Pad missing cells
        for col_idx in row.len()..col_count {
            let width = col_widths.get(col_idx).copied().unwrap_or(3);
            output.push(' ');
            output.push_str(&" ".repeat(width));
            output.push_str(" |");
        }
        output.push('\n');

        // Add separator after header (first row if has_header, or we treat first row as header)
        if row_idx == 0 {
            output.push('|');
            for col_idx in 0..col_count {
                let width = col_widths.get(col_idx).copied().unwrap_or(3);
                let align = alignments.get(col_idx).copied().unwrap_or(Alignment::None);
                output.push_str(&format_separator(width, align));
                output.push('|');
            }
            output.push('\n');
        }
    }

    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alignment {
    None,
    Left,
    Center,
    Right,
}

impl Alignment {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "left" => Self::Left,
            "center" => Self::Center,
            "right" => Self::Right,
            _ => Self::None,
        }
    }
}

fn format_separator(width: usize, align: Alignment) -> String {
    let dashes = width.max(3);
    match align {
        Alignment::Left => format!(":{}:", "-".repeat(dashes - 1)),
        Alignment::Center => format!(":{}:", "-".repeat(dashes.saturating_sub(2))),
        Alignment::Right => format!("{}:", "-".repeat(dashes - 1)),
        Alignment::None => format!(" {} ", "-".repeat(dashes)),
    }
}

fn pad_cell(text: &str, width: usize, align: Alignment) -> String {
    let text_len = text.chars().count();
    if text_len >= width {
        return text.to_string();
    }

    let padding = width - text_len;
    match align {
        Alignment::Right => format!("{}{}", " ".repeat(padding), text),
        Alignment::Center => {
            let left = padding / 2;
            let right = padding - left;
            format!("{}{}{}", " ".repeat(left), text, " ".repeat(right))
        }
        _ => format!("{}{}", text, " ".repeat(padding)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // escape_markdown tests
    // ============================================================================

    #[test]
    fn test_escape_asterisks() {
        assert_eq!(escape_markdown("*text*", false), r"\*text\*");
        assert_eq!(escape_markdown("**bold**", false), r"\*\*bold\*\*");
    }

    #[test]
    fn test_escape_underscores() {
        assert_eq!(escape_markdown("_text_", false), r"\_text\_");
        assert_eq!(escape_markdown("my_var_name", false), r"my\_var\_name");
    }

    #[test]
    fn test_escape_brackets() {
        assert_eq!(escape_markdown("[not a link]", false), r"\[not a link\]");
    }

    #[test]
    fn test_escape_backslash() {
        assert_eq!(escape_markdown(r"path\to\file", false), r"path\\to\\file");
    }

    #[test]
    fn test_escape_angle_brackets() {
        assert_eq!(escape_markdown("<html>", false), r"\<html\>");
    }

    #[test]
    fn test_no_escape_in_code_block() {
        assert_eq!(escape_markdown("*text*", true), "*text*");
        assert_eq!(escape_markdown("_var_", true), "_var_");
    }

    #[test]
    fn test_escape_empty_string() {
        assert_eq!(escape_markdown("", false), "");
    }

    #[test]
    fn test_escape_no_special_chars() {
        assert_eq!(escape_markdown("plain text", false), "plain text");
    }

    #[test]
    fn test_escape_mixed_content() {
        assert_eq!(
            escape_markdown("Use *asterisks* and _underscores_", false),
            r"Use \*asterisks\* and \_underscores\_"
        );
    }

    // ============================================================================
    // post_process_markdown tests
    // ============================================================================

    #[test]
    fn test_post_process_preserves_formatting() {
        // Bold and italic should be preserved when they look like formatting
        let input = "This is **bold** and *italic* text.";
        let result = post_process_markdown(input);
        eprintln!("Input:  {input}");
        eprintln!("Result: {result}");
        assert!(result.contains("**bold**"), "Expected **bold** but got: {result}");
        assert!(result.contains("*italic*"), "Expected *italic* but got: {result}");
    }

    #[test]
    fn test_post_process_preserves_code_blocks() {
        let input = "```\n*not escaped*\n```";
        let result = post_process_markdown(input);
        assert!(result.contains("*not escaped*"));
        assert!(!result.contains(r"\*"));
    }

    #[test]
    fn test_post_process_preserves_inline_code() {
        let input = "Use `*asterisks*` in code.";
        let result = post_process_markdown(input);
        assert!(result.contains("`*asterisks*`"));
    }

    #[test]
    fn test_post_process_preserves_headings() {
        let input = "# Heading\n## Subheading";
        let result = post_process_markdown(input);
        assert_eq!(result, input);
    }

    #[test]
    fn test_post_process_preserves_lists() {
        let input = "- Item 1\n* Item 2\n+ Item 3";
        let result = post_process_markdown(input);
        assert!(result.contains("- Item 1"));
        assert!(result.contains("* Item 2"));
    }

    // ============================================================================
    // html_table_to_markdown tests
    // ============================================================================

    #[test]
    fn test_simple_table() {
        let html = r#"<table>
            <tr><th>A</th><th>B</th></tr>
            <tr><td>1</td><td>2</td></tr>
        </table>"#;
        let result = html_table_to_markdown(html);
        assert!(result.contains("| A"));
        assert!(result.contains("| B"));
        assert!(result.contains("---"));
        assert!(result.contains("| 1"));
        assert!(result.contains("| 2"));
    }

    #[test]
    fn test_table_with_thead() {
        let html = r#"<table>
            <thead><tr><th>Header A</th><th>Header B</th></tr></thead>
            <tbody><tr><td>Cell 1</td><td>Cell 2</td></tr></tbody>
        </table>"#;
        let result = html_table_to_markdown(html);
        assert!(result.contains("Header A"));
        assert!(result.contains("Header B"));
        assert!(result.contains("Cell 1"));
        assert!(result.contains("Cell 2"));
    }

    #[test]
    fn test_table_alignment_left() {
        let html = r#"<table>
            <tr><th align="left">Left</th></tr>
            <tr><td>Data</td></tr>
        </table>"#;
        let result = html_table_to_markdown(html);
        assert!(result.contains(":--"));
    }

    #[test]
    fn test_table_alignment_center() {
        let html = r#"<table>
            <tr><th align="center">Center</th></tr>
            <tr><td>Data</td></tr>
        </table>"#;
        let result = html_table_to_markdown(html);
        assert!(result.contains(":") && result.contains("-"));
    }

    #[test]
    fn test_table_alignment_right() {
        let html = r#"<table>
            <tr><th align="right">Right</th></tr>
            <tr><td>Data</td></tr>
        </table>"#;
        let result = html_table_to_markdown(html);
        assert!(result.contains("--:"));
    }

    #[test]
    fn test_extract_and_clean_tables_no_tables() {
        let html = "<body>Are dress requirements in place of work important?</body>";
        let (processed, tables) = extract_and_clean_tables(html);
        eprintln!("processed: {processed:?}");
        assert!(tables.is_empty(), "no tables should be extracted");
        assert_eq!(processed, html, "no-table HTML should pass through unchanged");
    }

    #[test]
    fn test_empty_table() {
        let html = "<table></table>";
        let result = html_table_to_markdown(html);
        assert!(result.is_empty());
    }

    #[test]
    fn test_table_uneven_rows() {
        let html = r#"<table>
            <tr><th>A</th><th>B</th><th>C</th></tr>
            <tr><td>1</td><td>2</td></tr>
        </table>"#;
        let result = html_table_to_markdown(html);
        // Should handle uneven rows without panicking
        assert!(result.contains("| A"));
    }

    #[test]
    fn test_extract_and_clean_tables_basic() {
        let html = "<p>Before table</p>\n<table>\n<tr><th>Name</th><th>Age</th></tr>\n<tr><td>Alice</td><td>30</td></tr>\n</table>\n<p>After table</p>";
        eprintln!("Input HTML: {html:?}");

        let (processed, tables) = extract_and_clean_tables(html);
        eprintln!("Processed: {processed:?}");
        eprintln!("Tables count: {}", tables.len());
        for (i, t) in tables.iter().enumerate() {
            eprintln!("Table {i}: {t:?}");
        }

        assert_eq!(tables.len(), 1, "Should have extracted 1 table");
        assert!(processed.contains("TBLPLCHLDR0000"), "Processed should contain placeholder");
        assert!(!processed.contains("<table>"), "Processed should not contain table tag");
        assert!(!tables[0].contains('\t'), "Table should not contain tabs");
        assert!(!tables[0].contains('\n'), "Table should not contain newlines");
        assert!(!tables[0].contains("style"), "Table should not contain style attr");

        // Test restoration
        let markdown = format!("before\n{}\nafter", "TBLPLCHLDR0000");
        let restored = restore_tables_in_markdown(&markdown, &tables);
        eprintln!("Restored: {restored:?}");
        assert!(restored.contains("<table>"), "Restored should contain table tag");
    }

    #[test]
    fn test_extract_and_clean_tables_no_style() {
        let html = "<p>text</p><table style=\"width:100%;border:1px\"><tr style=\"color:red\"><th style=\"font-weight:bold\">H</th></tr></table><p>text</p>";
        let (processed, tables) = extract_and_clean_tables(html);
        eprintln!("Table cleaned: {:?}", tables[0]);
        assert!(!tables[0].contains("style"), "Style should be removed");
    }

    #[test]
    fn test_extract_and_clean_tables_tabs_newlines() {
        let html = "<p>text</p>\n<table>\n\t<tr>\n\t\t<td>A</td>\n\t</tr>\n</table>\n<p>text</p>";
        let (processed, tables) = extract_and_clean_tables(html);
        eprintln!("Table cleaned: {:?}", tables[0]);
        assert!(!tables[0].contains('\t'), "Tabs should be removed");
        assert!(!tables[0].contains('\n'), "Newlines should be removed");
        assert_eq!(tables[0], "<table><tr><td>A</td></tr></table>");
    }

    #[test]
    fn test_wrap_orphan_lists_basic() {
        let html = "<p>text</p><li>A</li><li>B</li><p>text</p>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert_eq!(result, "<p>text</p><ul><li>A</li><li>B</li></ul><p>text</p>");
    }

    #[test]
    fn test_wrap_orphan_lists_nested() {
        let html = "<body><li>item1</li>\n<li>item2</li></body>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert!(result.contains("<ul>"));
        assert!(result.contains("</ul>"));
        assert!(result.contains("<li>item1</li>"));
        assert!(result.contains("<li>item2</li>"));
    }

    #[test]
    fn test_wrap_orphan_lists_skips_nested() {
        let html = "<ul><li>A</li><li>B</li></ul>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert_eq!(result, "<ul><li>A</li><li>B</li></ul>");
    }

    #[test]
    fn test_wrap_orphan_lists_preserves_body_text() {
        let html = "<body>Are dress requirements in place of work important?</body>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert!(result.contains("Are dress requirements"), "text should be preserved");
        assert_eq!(result, html);
    }

    #[test]
    fn test_wrap_orphan_lists_preserves_chinese() {
        let html = "<p>中文测试</p>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert_eq!(result, html);
    }

    #[test]
    fn test_wrap_orphan_lists_no_li() {
        let html = "<p>no list here</p>";
        let result = wrap_orphan_lists(html);
        assert_eq!(result, html);
    }

    #[test]
    fn test_wrap_orphan_lists_preserves_table() {
        let html = "<li>A</li><table><tr><td>cell</td></tr></table><li>B</li>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert!(result.starts_with("<ul><li>A</li></ul>"));
        assert!(result.ends_with("<ul><li>B</li></ul>"));
    }

    #[test]
    fn test_wrap_orphan_lists_real_world() {
        let html = "<body><li>90 Second Half ends.</li>\n<li>90 Attempt missed.</li></body>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert_eq!(result, "<body><ul><li>90 Second Half ends.</li>\n<li>90 Attempt missed.</li></ul></body>");
    }

    #[test]
    fn test_wrap_orphan_lists_inside_table() {
        let html = "<table><tr><td><li>A</li><li>B</li></td><td>2</td></tr></table>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        // <li> inside <table> should NOT be wrapped in <ul>
        assert_eq!(result, "<table><tr><td><li>A</li><li>B</li></td><td>2</td></tr></table>");
    }

    #[test]
    fn test_wrap_orphan_lists_inside_table_and_outside() {
        let html = "<li>outside1</li><table><tr><td><li>inside</li></td></tr></table><li>outside2</li>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert_eq!(result, "<ul><li>outside1</li></ul><table><tr><td><li>inside</li></td></tr></table><ul><li>outside2</li></ul>");
    }

    #[test]
    fn test_wrap_orphan_lists_mixed() {
        let html = "<ul><li>proper</li></ul><li>orphan1</li><li>orphan2</li><p>end</p>";
        let result = wrap_orphan_lists(html);
        eprintln!("result: {result:?}");
        assert!(result.contains("<ul><li>proper</li></ul>"));
        assert!(result.contains("<ul><li>orphan1</li><li>orphan2</li></ul>"));
    }

    #[test]
    fn test_quick_html2md_preserves_placeholder() {
        use quick_html2md::{html_to_markdown_with_options, MarkdownOptions};

        let test_cases = vec![
            ("<p>Before</p>\nTBL000\n<p>After</p>", "raw text"),
            ("<p>Before</p>\n<p>TBL000</p>\n<p>After</p>", "in p tag"),
            ("<p>Before</p>\n<div>TBL000</div>\n<p>After</p>", "in div tag"),
            ("<p>Before</p>\n<span>TBL000</span>\n<p>After</p>", "in span tag"),
            ("<p>Before</p>\n<!--TBL000-->\n<p>After</p>", "html comment"),
        ];

        let md_options = MarkdownOptions::new()
            .include_links(true)
            .include_images(true)
            .preserve_tables(false)
            .escape_special_chars(true);

        for (html, desc) in &test_cases {
            let markdown = html_to_markdown_with_options(html, &md_options);
            let contains = markdown.contains("TBL000");
            eprintln!("[{desc}] contains={contains}, output={markdown:?}");
        }
    }
}
