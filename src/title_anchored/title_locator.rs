use crate::dom::{Document, Selection};
use super::feature::FeatureTree;

pub fn parse_head_title(doc: &Document) -> Option<String> {
    // <title> tag (C++ ParseHeadTitle style: XPath //*/title)
    for node in doc.select("title").nodes() {
        let sel = Selection::from(*node);
        let t = sel.text().trim().to_string();
        if !t.is_empty() {
            return Some(clean_title_for_search(&t));
        }
    }
    // Fallback: og:title meta (common when dom_query misses <title>)
    for node in doc.select("meta[property='og:title'], meta[name='og:title'], meta[name='twitter:title']").nodes() {
        let sel = Selection::from(*node);
        if let Some(content) = sel.attr("content") {
            let t = content.trim().to_string();
            if !t.is_empty() {
                return Some(clean_title_for_search(&t));
            }
        }
    }
    None
}

fn clean_title_for_search(title: &str) -> String {
    let t = title.trim();
    if t.is_empty() {
        return String::new();
    }
    // Strip site name suffix after common separators
    let separators = [" - ", " — ", " – ", " | ", " :: ", " » ", " › ", "——", "––"];
    for sep in &separators {
        if let Some(pos) = t.find(sep) {
            let left = t[..pos].trim();
            let right = t[pos + sep.len()..].trim();
            if !left.is_empty() && !right.is_empty() {
                // Keep side with more Chinese chars (article title side)
                let cn_left = left.chars().filter(|c| *c >= '\u{4e00}' && *c <= '\u{9fff}').count();
                let cn_right = right.chars().filter(|c| *c >= '\u{4e00}' && *c <= '\u{9fff}').count();
                if cn_left >= cn_right {
                    return left.to_string();
                }
                return right.to_string();
            }
        }
    }
    // Hyphen suffix (site-name)
    if let Some(pos) = t.rfind('-') {
        let right = t[pos + 1..].trim();
        let left = t[..pos].trim();
        if !left.is_empty() && !right.is_empty() && right.len() <= 12 && left.len() > right.len() * 2 {
            return left.to_string();
        }
    }
    t.to_string()
}

/// C++ ParseTitleFromH1: find first h1/h2 whose text is contained in head_title
fn parse_title_from_h1<'a>(
    doc: &'a Document,
    head_title: &str,
) -> Option<Selection<'a>> {
    for tag in &["h1", "h2"] {
        for node in doc.select(tag).nodes() {
            let sel = Selection::from(*node);
            let text = sel.text().trim().to_string();
            if text.is_empty() {
                continue;
            }
            // C++: if (head_title.find(content) != npos) || (content.length() > 0 && !match_head_title)
            // match_head_title=true: head_title must contain the content
            if head_title.find(&text).is_some() || text.find(head_title).is_some() {
                return Some(sel);
            }
            // Also accept when head_title contains a cleaned version
            let cleaned = clean_title_for_search(head_title);
            if cleaned != head_title && (cleaned.find(&text).is_some() || text.find(&cleaned).is_some()) {
                return Some(sel);
            }
        }
    }
    None
}

/// C++ ParseTitleFromH1 with match_head_title=false: accept any h1/h2 with text > 15 chars
fn parse_title_from_h1_fallback<'a>(
    doc: &'a Document,
) -> Option<Selection<'a>> {
    for tag in &["h1", "h2"] {
        for node in doc.select(tag).nodes() {
            let sel = Selection::from(*node);
            let text = sel.text().trim().to_string();
            if text.len() > 15 {
                return Some(sel);
            }
        }
    }
    None
}

/// C++ TraverseTitle: DFS body, find text node matching head_title
fn traverse_title<'a>(
    parent: &Selection<'a>,
    head_title: &str,
    features: Option<&FeatureTree>,
    body: &Selection<'a>,
) -> Option<Selection<'a>> {
    let Some(parent_ref) = parent.nodes().first().copied() else {
        return None;
    };
    for child in parent_ref.children() {
        if child.is_element() {
            let sel = Selection::from(child);
            let tag = child.node_name().unwrap_or_default().to_lowercase();

            // Skip title, script, style, link (C++: tag_name == "title")
            if matches!(tag.as_str(), "title" | "script" | "style" | "link") {
                continue;
            }

            // Skip nav headers (C++: IsNavHeader)
            if is_nav_header(&sel, features, body) {
                continue;
            }

            // Check all text nodes in this subtree
            for text_node in child.descendants() {
                if !text_node.is_text() {
                    continue;
                }
                let t_sel = Selection::from(text_node);
                let t_text = t_sel.text().trim().to_string();
                if t_text.len() > 15 {
                    // C++: head_title.find(node_text) != npos || node_text.find(head_title) != npos
                    if head_title.find(&t_text).is_some() || t_text.find(head_title).is_some() {
                        // Return the parent element of this text node (C++: title_node = child->parent)
                        if let Some(parent_el) = text_node.parent() {
                            return Some(Selection::from(parent_el));
                        }
                    }
                }
            }

            // Recurse
            if let Some(result) = traverse_title(&sel, head_title, features, body) {
                return Some(result);
            }
        }
    }
    None
}

/// C++ TraverseTitleOther: fallback - find h1 with text > 15, or text containing head_title
fn traverse_title_other<'a>(
    parent: &Selection<'a>,
    head_title: &str,
) -> Option<Selection<'a>> {
    let Some(parent_ref) = parent.nodes().first().copied() else {
        return None;
    };
    for child in parent_ref.children() {
        if child.is_element() {
            let sel = Selection::from(child);
            let tag = child.node_name().unwrap_or_default().to_lowercase();

            if matches!(tag.as_str(), "script" | "style" | "link") {
                continue;
            }

            // Check all text nodes
            for text_node in child.descendants() {
                if !text_node.is_text() {
                    continue;
                }
                let t_sel = Selection::from(text_node);
                let t_text = t_sel.text().trim().to_string();
                let parent_tag = text_node.parent()
                    .and_then(|p| p.node_name())
                    .unwrap_or_default()
                    .to_lowercase();

                // C++: if node_text.length() > 15 && tag_name == "h1", accept
                if t_text.len() > 15 && parent_tag == "h1" {
                    if let Some(parent_el) = text_node.parent() {
                        return Some(Selection::from(parent_el));
                    }
                }

                // C++: if node_text.length() > 15 && (head_title contains text || text contains head_title)
                if t_text.len() > 15 && !head_title.is_empty() {
                    if head_title.find(&t_text).is_some() || t_text.find(head_title).is_some() {
                        if let Some(parent_el) = text_node.parent() {
                            return Some(Selection::from(parent_el));
                        }
                    }
                }
            }

            if let Some(result) = traverse_title_other(&sel, head_title) {
                return Some(result);
            }
        }
    }
    None
}

/// C++ IsNavHeader
fn is_nav_header(sel: &Selection, features: Option<&FeatureTree>, body: &Selection) -> bool {
    let tag = sel.nodes().first()
        .and_then(|n| n.node_name())
        .unwrap_or_default()
        .to_lowercase();

    // C++: if node.text_len / body.text_len > 0.80 → not nav (too much content)
    if let Some(feats) = features {
        if let Some(feat) = feats.get(sel) {
            if let Some(body_feat) = feats.get(body) {
                if body_feat.text_len > 0 && (feat.text_len as f64 / body_feat.text_len as f64) > 0.80 {
                    return false;
                }
            }
        }
    }

    // C++ tag/class/id pattern check
    if tag == "nav" {
        return true;
    }
    if let Some(class) = sel.attr("class") {
        let lower = class.to_ascii_lowercase();
        if lower == "nav"
            || lower == "menu_nav"
            || lower == "main_nav"
            || lower == "logo"
            || lower == "navbar-header"
        {
            return true;
        }
    }
    if let Some(id) = sel.attr("id") {
        if id.as_ref() == "MainMenu" {
            return true;
        }
    }
    false
}

pub fn locate_title_node<'a>(
    doc: &'a Document,
    head_title: &str,
    features: Option<&FeatureTree>,
) -> Option<Selection<'a>> {
    if head_title.is_empty() {
        return None;
    }

    let body = doc.body().unwrap_or_else(|| doc.root());
    let body_sel = Selection::from(body);

    // Step 1: C++ ParseTitleFromH1
    if let Some(title) = parse_title_from_h1(doc, head_title) {
        return Some(title);
    }

    // Step 1b: C++ ParseTitleFromH1 with match_head_title=false
    // Accept first h1/h2 with text > 15 chars even if no head_title match
    if let Some(title) = parse_title_from_h1_fallback(doc) {
        return Some(title);
    }

    // Step 2: C++ TraverseTitle
    if let Some(title) = traverse_title(&body_sel, head_title, features, &body_sel) {
        return Some(title);
    }

    // Step 3: C++ TraverseTitleOther
    if let Some(title) = traverse_title_other(&body_sel, head_title) {
        return Some(title);
    }

    None
}
