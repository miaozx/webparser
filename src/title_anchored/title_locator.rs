use xmloxide::NodeId;
use super::feature::{FeatureTree, is_nav_header_by_node};

pub fn parse_head_title(doc: &xmloxide::Document) -> Option<String> {
    // C++: xpath //*/title, TraverseText(s_node, &text, true, false, false)
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    let root = doc.root_element().unwrap_or(body_id);
    for node in doc.descendants(root) {
        if !doc.is_element(node) {
            continue;
        }
        if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("title")) {
            let text = doc.text_content(node);
            let trimmed = text.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    // Fallback: og:title meta
    for node in doc.descendants(root) {
        if !doc.is_element(node) {
            continue;
        }
        if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("meta")) {
            if let Some(prop) = doc.attribute(node, "property") {
                if prop.eq_ignore_ascii_case("og:title")
                    || prop.eq_ignore_ascii_case("twitter:title")
                {
                    if let Some(content) = doc.attribute(node, "content") {
                        let t = content.trim().to_string();
                        if !t.is_empty() {
                            return Some(t);
                        }
                    }
                }
            }
        }
    }
    None
}

// C++ ParseTitleFromH1: head_title.find(content) != npos
fn parse_title_from_h1(doc: &xmloxide::Document, head_title: &str) -> Option<NodeId> {
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    for tag_name in &["h1", "h2"] {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) {
                continue;
            }
            if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case(tag_name)) {
                let text = doc.text_content(node);
                let text = text.trim().to_string();
                if text.is_empty() {
                    continue;
                }
                // C++: head_title.find(content) != npos (unidirectional)
                if head_title.find(&text).is_some() {
                    return Some(node);
                }
            }
        }
    }
    None
}

// C++ ParseTitleFromH1 with match_head_title=false: any h1/h2 with content > 0
pub fn parse_title_from_h1_fallback(doc: &xmloxide::Document) -> Option<NodeId> {
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    for tag_name in &["h1", "h2"] {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) {
                continue;
            }
            if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case(tag_name)) {
                let text = doc.text_content(node);
                let trimmed = text.trim();
                // C++: content.length() > 0 (not > 15)
                if !trimmed.is_empty() {
                    return Some(node);
                }
            }
        }
    }
    None
}

fn traverse_title(
    parent: NodeId,
    doc: &xmloxide::Document,
    head_title: &str,
    features: Option<&FeatureTree>,
    body: NodeId,
) -> Option<NodeId> {
    for child in doc.children(parent) {
        if !doc.is_element(child) {
            continue;
        }
        let tag = doc.node_name(child).unwrap_or("").to_lowercase();
        if matches!(tag.as_str(), "title" | "script" | "style" | "link") {
            continue;
        }
        let body_id = body;
        if features.map_or(false, |f| is_nav_header_by_node(child, doc, body_id, Some(f))) {
            continue;
        }
        // Check all text nodes in this subtree for title match
        for text_node in doc.descendants(child) {
            if !matches!(doc.node(text_node).kind, xmloxide::tree::NodeKind::Text { .. }) {
                continue;
            }
            let t_text = doc.node_text(text_node).unwrap_or("").trim().to_string();
            if t_text.len() > 15 {
                if head_title.find(&t_text).is_some() || t_text.find(head_title).is_some() {
                    if let Some(parent_el) = doc.parent(text_node) {
                        return Some(parent_el);
                    }
                }
            }
        }
        if let Some(result) = traverse_title(child, doc, head_title, features, body) {
            return Some(result);
        }
    }
    None
}

fn traverse_title_other(
    parent: NodeId,
    doc: &xmloxide::Document,
    head_title: &str,
) -> Option<NodeId> {
    for child in doc.children(parent) {
        if !doc.is_element(child) {
            continue;
        }
        let tag = doc.node_name(child).unwrap_or("").to_lowercase();
        if matches!(tag.as_str(), "script" | "style" | "link") {
            continue;
        }
        for text_node in doc.descendants(child) {
            if !matches!(doc.node(text_node).kind, xmloxide::tree::NodeKind::Text { .. }) {
                continue;
            }
            let t_text = doc.node_text(text_node).unwrap_or("").trim().to_string();
            let parent_tag = doc.parent(text_node)
                .and_then(|p| doc.node_name(p))
                .unwrap_or("")
                .to_lowercase();

            if t_text.len() > 15 && parent_tag == "h1" {
                if let Some(parent_el) = doc.parent(text_node) {
                    return Some(parent_el);
                }
            }
            if t_text.len() > 15 && !head_title.is_empty() {
                if head_title.find(&t_text).is_some() || t_text.find(head_title).is_some() {
                    if let Some(parent_el) = doc.parent(text_node) {
                        return Some(parent_el);
                    }
                }
            }
        }
        if let Some(result) = traverse_title_other(child, doc, head_title) {
            return Some(result);
        }
    }
    None
}

pub fn locate_title_node(
    doc: &xmloxide::Document,
    head_title: &str,
    features: Option<&FeatureTree>,
) -> Option<NodeId> {
    if head_title.is_empty() {
        return None;
    }

    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());

    // Step 1: ParseTitleFromH1
    if let Some(title) = parse_title_from_h1(doc, head_title) {
        return Some(title);
    }

    // Step 1b: ParseTitleFromH1 with match_head_title=false
    if let Some(title) = parse_title_from_h1_fallback(doc) {
        return Some(title);
    }

    // Step 2: TraverseTitle
    if let Some(title) = traverse_title(body_id, doc, head_title, features, body_id) {
        return Some(title);
    }

    // Step 3: TraverseTitleOther
    if let Some(title) = traverse_title_other(body_id, doc, head_title) {
        return Some(title);
    }

    None
}

/// C++ ParseTitleWithXpath: xpath "//*[@class='title'] | //*[@id='title']"
pub fn parse_title_with_xpath(doc: &xmloxide::Document) -> Option<NodeId> {
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) { continue; }
        if let Some(class_val) = doc.attribute(node, "class") {
            if class_val == "title" {
                let text = doc.text_content(node);
                if text.trim().len() > 15 {
                    return Some(node);
                }
            }
        }
        if let Some(id_val) = doc.attribute(node, "id") {
            if id_val == "title" {
                let text = doc.text_content(node);
                if text.trim().len() > 15 {
                    return Some(node);
                }
            }
        }
    }
    None
}
