# Changelog

## 0.2.1 (2026-03-28)

### Fixed

- **Period escaping inside headings**: `escape_markdown_text()` now accepts an
  `at_line_start` parameter. Positional escaping (ordered list `1.`, heading `#`,
  blockquote `>`, list markers `-`/`+`) is only applied when text is at the true
  start of a line in the output buffer, not inside headings, emphasis, or other
  block elements. This fixes `### 1\. Section` → `### 1. Section`.

## 0.2.0 (2026-03-28)

### Fixed

- **Empty div inflation**: Whitespace-only text nodes inside structural HTML elements
  (`<div>`, `<section>`, `<nav>`, etc.) no longer inflate output by 3-40x. The converter
  now collapses these to at most a single space separator.

### Changed

- **Position-aware escaping**: `escape_special_chars` now uses smart, position-aware
  escaping instead of blanket character escaping. Core markdown characters
  (`\`, `` ` ``, `*`, `_`, `[`, `]`, `<`) are always escaped. Positional characters
  (`#`, `>`, `-`, `+`, `.`, `!`) are only escaped where they could create markdown
  constructs (e.g., `#` at line start, `-` at line start followed by space).
  Characters like `{`, `}`, `(`, `)`, `~` are no longer escaped.

### Internal

- Deduplicated `escape_markdown_text()` function (was duplicated in `converter.rs`
  and `elements/inline.rs`, now lives in `utils.rs`).

## 0.1.0 (2026-03-24)

Initial release.

- Full GFM support: headings, emphasis, strikethrough, lists, links, images, code blocks, tables, blockquotes
- CommonMark mode with strict compliance
- URL resolution for relative links and images
- Configurable options via builder pattern
