use std::sync::LazyLock;
use regex::Regex;
use chrono::Utc;
use crate::dom::{Document, Selection};

// Compiled regex patterns (lazy = compiled once)
static RE_DT_WITH_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{4}[\-\./][0-9]{1,2}[\-\./][0-9]{1,2}[\sT]?[0-9]{1,2}:?[0-9]{1,2}:?[0-9]{1,2})").unwrap()
});
static RE_DT_DATE_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{4}[\-\./][0-9]{1,2}[\-\./][0-9]{1,2})").unwrap()
});
static RE_CN_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{4}\s?年\s?[0-9]{1,2}\s?月\s?[0-9]{1,2})").unwrap()
});
static RE_RELATIVE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]+)\s?(天前|个月前|年前|小时前)").unwrap()
});
static RE_SHORT_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{1,2}[\-\.][0-9]{2}\s+[0-9]{2}:[0-9]{2})$").unwrap()
});
static RE_CN_SHORT_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([0-9]{1,2}月[0-9]{1,2}日(\s*[0-9]{1,2}:[0-9]{1,2})?)").unwrap()
});
static RE_EN_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(\d{1,2}\s+(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|aug(?:ust)?|sep(?:tember)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)(?:\.?)[,\s]+\d{4})"
    ).unwrap()
});

/// Parse English date strings like "15 Jan 2024" or "January 15, 2024"
/// and normalize to YYYY-MM-DD format.
fn parse_english_date(s: &str) -> String {
    use chrono::NaiveDate;
    let s = s.trim();
    // Try parsing as "15 January 2024" or "15 Jan 2024"
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d %B %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d %b %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d %B, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d %b, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    // Try parsing as "January 15, 2024" or "Jan 15, 2024"
    if let Ok(d) = NaiveDate::parse_from_str(s, "%B %d, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%b %d, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    // Try without comma
    if let Ok(d) = NaiveDate::parse_from_str(s, "%B %d %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%b %d %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    s.to_string()
}

/// Extract "digit area" — a window around the first digit span.
/// This avoids running expensive regex on the full text (C++ GetDigitAreaStr).
fn get_digit_area(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let first_digit = chars.iter().position(|c| c.is_ascii_digit());
    let Some(pos) = first_digit else {
        return s.to_string();
    };
    let start = pos.saturating_sub(30);
    let end = (pos + 60).min(chars.len());
    chars[start..end].iter().collect()
}

/// Copy of C++ BaseParser::ExtractPublishTime.
/// Returns `None` for short/invalid text, extracts and normalizes the date.
///
/// Follows the C++ priority order:
///   1. YYYY-MM-DD HH:MM:SS  (also handles / . T separators)
///   2. YYYY-MM-DD           (no time)
///   3. YYYY年MM月DD日         (Chinese)
///   4. X天前 / X个月前 / X年前 / X小时前  (relative, needs crawl_time)
///   5. MM-DD HH:MM          (short date, prefixes current year)
///   6. MM月DD日              (Chinese short date)
pub fn extract_publish_time(text: &str) -> Option<String> {
    if text.len() > 256 {
        return None;
    }
    let digit_count = text.chars().filter(|c| c.is_ascii_digit()).count();
    if digit_count < 3
        && !text.contains('年')
        && !text.contains('月')
        && !text.contains('天')
        && !text.contains("小时")
    {
        return None;
    }

    let da = get_digit_area(text);
    if da.len() > 256 {
        return None;
    }

    // 1. YYYY-MM-DD HH:MM:SS  (with optional T separator)
    if let Some(m) = RE_DT_WITH_TIME.find(&da) {
        return Some(m.as_str().replace('/', "-").replace('.', "-").replace('T', " ").trim().to_string());
    }

    // 2. YYYY-MM-DD only
    if let Some(m) = RE_DT_DATE_ONLY.find(&da) {
        return Some(m.as_str().replace('/', "-").replace('.', "-").trim().to_string());
    }

    // 3. Chinese: YYYY年MM月DD日
    if let Some(m) = RE_CN_DATE.find(&da) {
        let s = m.as_str()
            .replace("年", "-")
            .replace("月", "-")
            .replace("日", "")
            .replace(" ", "");
        return Some(s);
    }

    // 4. Relative time: X天前 etc. — requires crawl time
    //    The caller can use extract_publish_time_with_crawl for this.
    //    Here we fall through to absolute patterns.

    // 4b. English date: "15 Jan 2024" or "January 15, 2024"
    if let Some(m) = RE_EN_DATE.find(text) {
        return Some(parse_english_date(m.as_str()));
    }

    // 5. Short date: MM-DD HH:MM (prefix current year)
    if let Some(m) = RE_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str().replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

    // 6. Chinese short: MM月DD日
    if let Some(m) = RE_CN_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str()
            .replace("月", "-")
            .replace("日", " ")
            .replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

    None
}

/// Like extract_publish_time but also handles relative time expressions
/// by resolving them against the given unix-timestamp crawl_time.
pub fn extract_publish_time_with_crawl(text: &str, crawl_time: u64) -> Option<String> {
    if text.len() > 256 {
        return None;
    }
    let digit_count = text.chars().filter(|c| c.is_ascii_digit()).count();
    if digit_count < 3
        && !text.contains('年')
        && !text.contains('月')
        && !text.contains('天')
        && !text.contains("小时")
    {
        return None;
    }

    // 1-3: same as extract_publish_time (tries digit_area first)
    let da = get_digit_area(text);
    if da.len() <= 256 {
        if let Some(m) = RE_DT_WITH_TIME.find(&da) {
            return Some(m.as_str().replace('/', "-").replace('.', "-").replace('T', " ").trim().to_string());
        }
        if let Some(m) = RE_DT_DATE_ONLY.find(&da) {
            return Some(m.as_str().replace('/', "-").replace('.', "-").trim().to_string());
        }
        if let Some(m) = RE_CN_DATE.find(&da) {
            let s = m.as_str()
                .replace("年", "-")
                .replace("月", "-")
                .replace("日", "")
                .replace(" ", "");
            return Some(s);
        }
    }

    // 4. Relative time: resolves against crawl_time
    if let Some(caps) = RE_RELATIVE.captures(text) {
        let digit: u64 = caps[1].parse().unwrap_or(0);
        let seconds = match &caps[2] {
            "天前" => digit * 86400,
            "个月前" => digit * 30 * 86400,
            "年前" => digit * 365 * 86400,
            "小时前" => digit * 3600,
            _ => 0,
        };
        if seconds > 0 {
            let ts = crawl_time - seconds;
            use chrono::TimeZone;
            let dt = Utc.timestamp_opt(ts as i64, 0).single()?;
            return Some(dt.format("%Y-%m-%d %H:%M:%S").to_string());
        }
    }

    // 4b. English date: "15 Jan 2024" or "January 15, 2024"
    if let Some(m) = RE_EN_DATE.find(text) {
        return Some(parse_english_date(m.as_str()));
    }

    // 5. Short date: MM-DD HH:MM
    if let Some(m) = RE_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str().replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

    // 6. Chinese short: MM月DD日
    if let Some(m) = RE_CN_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str()
            .replace("月", "-")
            .replace("日", " ")
            .replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

    None
}

pub fn locate_time_near_title<'a>(
    doc: &'a Document,
    title_node: &Selection<'a>,
    title: &str,
) -> Option<Selection<'a>> {
    // 0. C++ TraversePublishTime: DFS from body, search within 192 chars after title
    let body = doc.body().unwrap_or_else(|| doc.root());
    let body_sel = Selection::from(body);
    if let Some(result) = traverse_publish_time(doc, &body_sel, title_node, title, 192) {
        return Some(result);
    }

    // 1. Check <time> elements
    for node in doc.select("time").nodes() {
        let sel = Selection::from(*node);
        if let Some(dt) = sel.attr("datetime") {
            if !dt.is_empty() && extract_publish_time(&dt).is_some() {
                return Some(sel);
            }
        }
        let text = sel.text();
        if extract_publish_time(&text).is_some() {
            return Some(sel);
        }
    }

    // 2. Search siblings and nearby nodes from title
    if let Some(title_ref) = title_node.nodes().first() {
        let mut current = title_ref.next_sibling();
        for _ in 0..8 {
            if let Some(sib) = current {
                if sib.is_element() {
                    let sel = Selection::from(sib);
                    let text = sel.text();
                    if extract_publish_time(&text).is_some() {
                        return Some(sel);
                    }
                }
                current = sib.next_sibling();
            } else {
                break;
            }
        }

        // 3. Check parent's next sibling (article metadata block)
        if let Some(parent) = title_ref.parent() {
            let parent_sel = Selection::from(parent);
            let mut psib = parent_sel.next_sibling();
            for _ in 0..4 {
                if psib.length() == 0 {
                    break;
                }
                let text = psib.text();
                if extract_publish_time(&text).is_some() {
                    return Some(psib);
                }
                psib = psib.next_sibling();
            }
        }
    }

    // 4. Check elements with known date class/id patterns
    let date_selectors = [
        ".date", ".time", ".pubdate", ".publish-date", ".post-date",
        ".artdate", ".ardate", ".article-date", ".entry-date",
        "[itemprop='datePublished']", "[itemprop='dateModified']",
    ];
    for sel_str in &date_selectors {
        let sel = doc.select(sel_str);
        if sel.length() > 0 {
            let text = sel.text();
            if extract_publish_time(&text).is_some() {
                return Some(sel);
            }
        }
    }

    // 5. Check <meta> tags
    for node in doc.select("meta[name='pubdate'], meta[name='publishdate'], meta[property='article:published_time'], meta[name='citation_publication_date']").nodes() {
        let sel = Selection::from(*node);
        if let Some(content) = sel.attr("content") {
            if extract_publish_time(&content).is_some() {
                return Some(sel);
            }
        }
    }

    None
}

/// C++ TraversePublishTime: DFS from node, search for publish time within max_chars of traversed text
fn traverse_publish_time<'a>(
    doc: &'a dom_query::Document,
    root: &Selection<'a>,
    title_node: &Selection<'a>,
    title: &str,
    max_chars: usize,
) -> Option<Selection<'a>> {
    let Some(root_ref) = root.nodes().first().copied() else {
        return None;
    };
    let Some(title_ref) = title_node.nodes().first().copied() else {
        return None;
    };

    let mut found_title = false;
    let mut traverse_text_len = 0usize;
    let raw_title = title.to_string();

    dfs_search_time(root_ref, doc, title_ref, &raw_title,
        &mut found_title, &mut traverse_text_len, max_chars)
}

fn dfs_search_time<'a>(
    node: dom_query::NodeRef<'a>,
    doc: &'a dom_query::Document,
    title_ref: dom_query::NodeRef<'a>,
    raw_title: &str,
    found_title: &mut bool,
    traverse_text_len: &mut usize,
    max_chars: usize,
) -> Option<Selection<'a>> {
    // C++: traverse_text->length() - title.length() > 192 → stop
    let effective_len = traverse_text_len.saturating_sub(raw_title.len());
    if effective_len > max_chars {
        return None;
    }

    // Get parent tag name for this level (C++: p_tag_name)
    let p_tag_name = node.node_name().unwrap_or_default().to_lowercase();

    // Check all children
    for child in node.children() {
        let effective_len = traverse_text_len.saturating_sub(raw_title.len());
        if effective_len > max_chars {
            break;
        }

        if child.is_element() {
            if let Some(tag) = child.node_name() {
                // C++: skip title tag
                if tag.eq_ignore_ascii_case("title") {
                    continue;
                }
                // C++: skip script/style/link
                if matches!(tag.as_ref(), "script" | "style" | "link") {
                    continue;
                }
                // C++: IsNavHeader check
                let child_sel = Selection::from(child);
                if super::feature::is_nav_header_by_node(&child) {
                    continue;
                }
                // C++: h2 && child != title_node → skip (don't recurse into other sections)
                if tag.eq_ignore_ascii_case("h2") && child.id != title_ref.id {
                    continue;
                }
                // C++: IsVisibleNode (style display:none)
                if !super::feature::is_visible_node(&child_sel) {
                    continue;
                }

                // Check if this IS the title node
                if child.id == title_ref.id {
                    *found_title = true;
                    *traverse_text_len = 0;
                }

                // If not found title yet, accumulate traverse_text and recurse
                if !*found_title {
                    // Accumulate text from this element
                    for desc in child.descendants() {
                        if desc.is_text() {
                            let t = desc.text();
                            *traverse_text_len += t.trim().len();
                        }
                    }
                    if let Some(result) = dfs_search_time(child, doc, title_ref, raw_title,
                        found_title, traverse_text_len, max_chars) {
                        return Some(result);
                    }
                    continue;
                }

                // After title: check <time> tag
                if tag.eq_ignore_ascii_case("time") {
                    let text = child_sel.text();
                    if extract_publish_time(&text).is_some() {
                        return Some(child_sel);
                    }
                    if let Some(dt) = child_sel.attr("datetime") {
                        if extract_publish_time(&dt).is_some() {
                            return Some(child_sel);
                        }
                    }
                }

                // Recurse into children
                if let Some(result) = dfs_search_time(child, doc, title_ref, raw_title,
                    found_title, traverse_text_len, max_chars) {
                    return Some(result);
                }
            }
        } else if child.is_text() {
            let node_text = child.text();
            let trimmed = node_text.trim();

            if trimmed.is_empty() {
                continue;
            }

            // C++: h1 && text == title → reset traverse_text
            if p_tag_name == "h1" && trimmed == raw_title {
                *traverse_text_len = 0;
            }

            // C++: If before title, accumulate text
            if !*found_title {
                *traverse_text_len += trimmed.len();
                continue;
            }

            // After title: check for publish time
            // C++: (traverse_text->length() - title.length() < 192) && ExtractPublishTime
            let effective = traverse_text_len.saturating_sub(raw_title.len());
            if effective <= max_chars && extract_publish_time(trimmed).is_some() {
                if let Some(parent) = child.parent() {
                    return Some(Selection::from(parent));
                }
            }

            // C++: also check <time> tag's text content
            if p_tag_name == "time" && extract_publish_time(trimmed).is_some() {
                if let Some(parent) = child.parent() {
                    return Some(Selection::from(parent));
                }
            }

            // Accumulate traversed text
            *traverse_text_len += trimmed.len();
        }
    }

    None
}
