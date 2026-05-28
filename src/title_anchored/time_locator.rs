use std::sync::LazyLock;
use regex::Regex;
use chrono::Utc;
use xmloxide::NodeId;
use super::feature::{FeatureTree, is_nav_header_by_node, is_visible_node};

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

fn parse_english_date(s: &str) -> String {
    use chrono::NaiveDate;
    let s = s.trim();
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
    if let Ok(d) = NaiveDate::parse_from_str(s, "%B %d, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%b %d, %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%B %d %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%b %d %Y") {
        return d.format("%Y-%m-%d").to_string();
    }
    s.to_string()
}

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

    if let Some(m) = RE_EN_DATE.find(text) {
        return Some(parse_english_date(m.as_str()));
    }

    if let Some(m) = RE_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str().replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

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

    if let Some(m) = RE_EN_DATE.find(text) {
        return Some(parse_english_date(m.as_str()));
    }

    if let Some(m) = RE_SHORT_DATE.find(text) {
        let now = Utc::now();
        let year = now.format("%Y");
        let s = m.as_str().replace('.', "-");
        return Some(format!("{}-{}", year, s.trim()));
    }

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

pub fn locate_time_near_title(
    doc: &xmloxide::Document,
    title_node: NodeId,
    title: &str,
) -> Option<NodeId> {
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());

    // 0. C++ TraversePublishTime style
    if let Some(result) = traverse_publish_time(doc, body_id, title_node, title, 192) {
        return Some(result);
    }

    // 1. Check <time> elements
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) {
            continue;
        }
        if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("time")) {
            if let Some(dt) = doc.attribute(node, "datetime") {
                if !dt.is_empty() && extract_publish_time(dt).is_some() {
                    return Some(node);
                }
            }
            let text = doc.text_content(node);
            if extract_publish_time(&text).is_some() {
                return Some(node);
            }
        }
    }

    // 2. Search siblings of title node
    let mut current = doc.next_sibling(title_node);
    for _ in 0..8 {
        if let Some(sib) = current {
            if doc.is_element(sib) {
                let text = doc.text_content(sib);
                if extract_publish_time(&text).is_some() {
                    return Some(sib);
                }
            }
            current = doc.next_sibling(sib);
        } else {
            break;
        }
    }

    // 3. Check parent's next sibling (article metadata block)
    if let Some(parent) = doc.parent(title_node) {
        let mut psib = doc.next_sibling(parent);
        for _ in 0..4 {
            if let Some(sib) = psib {
                let text = doc.text_content(sib);
                if extract_publish_time(&text).is_some() {
                    return Some(sib);
                }
                psib = doc.next_sibling(sib);
            } else {
                break;
            }
        }
    }

    // 4. Check elements with known date class/id patterns
    let date_patterns = [
        "date", "time", "pubdate", "publish-date", "post-date",
        "artdate", "ardate", "article-date", "entry-date",
    ];
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) {
            continue;
        }
        let class_opt = doc.attribute(node, "class");
        let id_opt = doc.attribute(node, "id");
        let combined = format!(
            "{} {}",
            class_opt.unwrap_or(""),
            id_opt.unwrap_or("")
        ).to_ascii_lowercase();
        for pat in &date_patterns {
            if combined.contains(pat) || combined.contains(&pat.to_ascii_lowercase()) {
                let text = doc.text_content(node);
                if extract_publish_time(&text).is_some() {
                    return Some(node);
                }
            }
        }
        // Check itemprop
        if let Some(itemprop) = doc.attribute(node, "itemprop") {
            if itemprop == "datePublished" || itemprop == "dateModified" {
                let text = doc.text_content(node);
                if extract_publish_time(&text).is_some() {
                    return Some(node);
                }
            }
        }
    }

    // 5. Check <meta> tags
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) {
            continue;
        }
        if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("meta")) {
            let name_opt = doc.attribute(node, "name").map(|s| s.to_ascii_lowercase());
            let prop_opt = doc.attribute(node, "property").map(|s| s.to_ascii_lowercase());
            let meta_match = name_opt.as_ref().map_or(false, |n| {
                n == "pubdate" || n == "publishdate" || n == "citation_publication_date"
            }) || prop_opt.as_ref().map_or(false, |p| {
                p == "article:published_time"
            });
            if meta_match {
                if let Some(content) = doc.attribute(node, "content") {
                    if extract_publish_time(content).is_some() {
                        return Some(node);
                    }
                }
            }
        }
    }

    None
}

fn traverse_publish_time(
    doc: &xmloxide::Document,
    root: NodeId,
    title_node: NodeId,
    title: &str,
    max_chars: usize,
) -> Option<NodeId> {
    let mut found_title = false;
    let mut traverse_text_len = 0usize;
    let raw_title = title.to_string();
    dfs_search_time(
        doc, root, title_node, &raw_title,
        &mut found_title, &mut traverse_text_len, max_chars)
}

fn dfs_search_time(
    doc: &xmloxide::Document,
    node: NodeId,
    title_ref: NodeId,
    raw_title: &str,
    found_title: &mut bool,
    traverse_text_len: &mut usize,
    max_chars: usize,
) -> Option<NodeId> {
    let effective_len = traverse_text_len.saturating_sub(raw_title.len());
    if effective_len > max_chars {
        return None;
    }

    let p_tag_name = doc.node_name(node).unwrap_or("").to_lowercase();

    for child in doc.children(node) {
        let effective_len = traverse_text_len.saturating_sub(raw_title.len());
        if effective_len > max_chars {
            break;
        }

        if doc.is_element(child) {
            if let Some(tag) = doc.node_name(child) {
                if tag.eq_ignore_ascii_case("title") {
                    continue;
                }
                if matches!(tag.as_ref(), "script" | "style" | "link") {
                    continue;
                }

                // IsNavHeader check
                let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
                if is_nav_header_by_node(child, doc, body_id, None) {
                    continue;
                }

                if tag.eq_ignore_ascii_case("h2") && child != title_ref {
                    continue;
                }

                if !is_visible_node(child, doc) {
                    continue;
                }

                if child == title_ref {
                    *found_title = true;
                    *traverse_text_len = 0;
                }

                if !*found_title {
                    for desc in doc.descendants(child) {
                        if matches!(doc.node(desc).kind, xmloxide::tree::NodeKind::Text { .. }) {
                            if let Some(t) = doc.node_text(desc) {
                                *traverse_text_len += t.trim().len();
                            }
                        }
                    }
                    if let Some(result) = dfs_search_time(doc, child, title_ref,
                        raw_title, found_title, traverse_text_len, max_chars) {
                        return Some(result);
                    }
                    continue;
                }

                if tag.eq_ignore_ascii_case("time") {
                    let text = doc.text_content(child);
                    if extract_publish_time(&text).is_some() {
                        return Some(child);
                    }
                    if let Some(dt) = doc.attribute(child, "datetime") {
                        if extract_publish_time(dt).is_some() {
                            return Some(child);
                        }
                    }
                }

                if let Some(result) = dfs_search_time(doc, child, title_ref,
                    raw_title, found_title, traverse_text_len, max_chars) {
                    return Some(result);
                }
            }
        } else if matches!(doc.node(child).kind, xmloxide::tree::NodeKind::Text { .. }) {
            let node_text = doc.node_text(child).unwrap_or("");
            let trimmed = node_text.trim();

            if trimmed.is_empty() {
                continue;
            }

            if p_tag_name == "h1" && trimmed == raw_title {
                *traverse_text_len = 0;
            }

            if !*found_title {
                *traverse_text_len += trimmed.len();
                continue;
            }

            let effective = traverse_text_len.saturating_sub(raw_title.len());
            if effective <= max_chars && extract_publish_time(trimmed).is_some() {
                if let Some(parent) = doc.parent(child) {
                    return Some(parent);
                }
            }

            if p_tag_name == "time" && extract_publish_time(trimmed).is_some() {
                if let Some(parent) = doc.parent(child) {
                    return Some(parent);
                }
            }

            *traverse_text_len += trimmed.len();
        }
    }

    None
}
