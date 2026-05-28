use xmloxide::NodeId;
use super::feature::{FeatureTree, hit_content_attribute, is_visible_node};
use super::end_signals::has_end_signal;

const MIN_CONTENT_LEN: usize = 300;

fn traverse_user_card(doc: &xmloxide::Document, node: NodeId,
    has_a: &mut bool, has_i: &mut bool, has_name: &mut bool,
    has_img: &mut bool, has_zixun: &mut bool, text: &mut String) {
    if let Some(tag) = doc.node_name(node) {
        let tag = tag.to_lowercase();
        if tag == "a" { *has_a = true; }
        if tag == "i" { *has_i = true; }
        if tag == "img" { *has_img = true; }
    }
    if matches!(doc.node(node).kind, xmloxide::tree::NodeKind::Text { .. }) {
        if let Some(content) = doc.node_text(node) {
            let trimmed = content.trim().to_string();
            text.push_str(&trimmed);
            if trimmed.contains("律师") || trimmed.contains("医师")
                || trimmed.contains("咨询助手") || trimmed.contains("情感咨询")
            {
                *has_name = true;
            } else if trimmed.contains("咨询") || trimmed.contains("提问") {
                *has_zixun = true;
            }
        }
    }
    for child in doc.children(node) {
        traverse_user_card(doc, child, has_a, has_i, has_name, has_img, has_zixun, text);
    }
}

fn hit_user_attribute(doc: &xmloxide::Document, node: NodeId) -> bool {
    let tag = doc.node_name(node).unwrap_or("").to_lowercase();
    let allowed = ["a", "address", "div", "link", "p", "span", "strong"];
    if !allowed.contains(&tag.as_str()) {
        return false;
    }
    if let Some(class_val) = doc.attribute(node, "class") {
        let lc = class_val.to_ascii_lowercase();
        if lc.contains("author-name") || lc.contains("authorname")
            || lc.contains("author name") || lc.contains("authorcard")
            || lc.contains("zuozhe") || lc.contains("bianji")
            || lc.contains("xiaobian") || lc.contains("posted-by")
            || lc.contains("submitted-by")
        {
            return true;
        }
    }
    if let Some(id_val) = doc.attribute(node, "id") {
        let lc = id_val.to_ascii_lowercase();
        if lc == "author" || lc == "writer" || lc == "username" { return true; }
    }
    false
}

#[allow(dead_code)]
fn is_user_card(id: NodeId, doc: &xmloxide::Document) -> bool {
    let mut has_a = false;
    let mut has_i = false;
    let mut has_name = false;
    let mut has_img = false;
    let mut has_zixun = false;
    let mut text = String::new();
    traverse_user_card(doc, id, &mut has_a, &mut has_i,
        &mut has_name, &mut has_img, &mut has_zixun, &mut text);

    if text.len() < 200 && has_a && has_name && has_img && has_zixun {
        return true;
    }
    if hit_user_attribute(doc, id) {
        return true;
    }
    false
}

/// C++ GetNextContentNode: find next sibling with text content, unwrapping single-child wrappers.
#[allow(dead_code)]
pub fn get_next_content_node(doc: &xmloxide::Document, id: NodeId) -> Option<NodeId> {
    let mut sibling = doc.next_sibling(id);
    let mut cur = id;
    loop {
        if sibling.is_none() {
            // Go up: cur = cur->parent; sibling = cur->next
            if let Some(parent) = doc.parent(cur) {
                cur = parent;
                sibling = doc.next_sibling(cur);
            } else {
                break;
            }
        } else if !doc.is_element(sibling.unwrap()) {
            // Skip non-element siblings
            cur = sibling.unwrap();
            sibling = doc.next_sibling(cur);
        } else {
            let sib = sibling.unwrap();
            let content = doc.text_content(sib);
            if content.trim().is_empty() {
                cur = sib;
                sibling = doc.next_sibling(sib);
                continue;
            }
            // Unwrap single-child wrappers
            let mut result = sib;
            loop {
                let children: Vec<NodeId> = doc.children(result)
                    .filter(|&c| doc.is_element(c))
                    .collect();
                if children.len() == 1 {
                    result = children[0];
                } else {
                    break;
                }
            }
            return Some(result);
        }
    }
    None
}

#[allow(dead_code)]
fn is_content_node(
    id: NodeId,
    features: &FeatureTree,
    doc: &xmloxide::Document,
    body_exclude_len: usize,
    _match_node: bool,
) -> bool {
    let Some(feat) = features.get(id) else {
        return false;
    };
    let tag = doc.node_name(id).unwrap_or("").to_lowercase();
    // C++: exclude_a_text_len * 1.0 / (body_exclude_a_text_len + 1)
    let denom = (body_exclude_len + 1) as f64;
    let ratio = feat.exclude_a_text_len as f64 / denom;
    // C++ IsContentNode: text_len > 64 && HitContentAttribute && ratio > 0.35
    if feat.text_len > 64
        && hit_content_attribute(id, doc)
        && (feat.exclude_a_text_len as f64 / denom) > 0.35
    {
        return true;
    }
    // C++ negative: has_recomment_title && tag_a_nc > 3 && click_image_count > 3
    if feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 {
        return false;
    }
    // C++ negative: match_node && tag_a_nc > 30 && click_image_count > 3
    if _match_node && feat.tag_a_nc > 30 && feat.click_image_count > 3 {
        return false;
    }
    // C++ negative: (ul/ol) && tag_a_nc > 100
    if (tag == "ul" || tag == "ol") && feat.tag_a_nc > 100 {
        return false;
    }
    // C++ negative: tag_a_nc >= 101 && max_exclude_a_text_len < 20
    if feat.tag_a_nc >= 101 && feat.max_exclude_a_text_len < 20.0 {
        return false;
    }
    // C++ positive: match_node && ratio > 0.6
    if _match_node && ratio > 0.6 {
        return true;
    }
    false
}

/// Iterate all elements in node_list, find the best content node by ratio
#[allow(dead_code)]
pub fn locate_content_node_with_feature<'a>(
    features: &FeatureTree,
    doc: &xmloxide::Document,
    body_exclude_len: usize,
) -> Option<NodeId> {
    let body_node = FeatureTree::find_body(doc);
    let body_id = body_node.unwrap_or_else(|| doc.root());
    let all_elements: Vec<NodeId> = doc.descendants(body_id)
        .filter(|&id| doc.is_element(id) && id != body_id)
        .collect();

    // Method 1: ratio > 0.6, pick smallest ratio above threshold
    let mut best: Option<(NodeId, f64, usize)> = None;
    for &node in &all_elements {
        let Some(feat) = features.get(node) else {
            continue;
        };
        if feat.exclude_a_text_len < 128 || feat.is_discard_node {
            continue;
        }
        let ratio = feat.exclude_a_text_len as f64 / (body_exclude_len as f64).max(1.0);
        if ratio > 0.6 {
            let is_better = match best {
                Some((_, best_ratio, _)) => ratio < best_ratio,
                None => true,
            };
            if is_better {
                best = Some((node, ratio, feat.exclude_a_text_len));
            }
        }
    }

    // Method 2: content attribute match
    if best.is_none() {
        for &node in &all_elements {
            let Some(feat) = features.get(node) else {
                continue;
            };
            if feat.exclude_a_text_len > 64 && hit_content_attribute(node, doc) {
                best = Some((node, 0.0, feat.exclude_a_text_len));
                break;
            }
        }
    }

    // Method 3: self-ratio > 0.8 && exclude > 800
    if best.is_none() {
        let mut max_exclude = 0usize;
        for &node in &all_elements {
            let Some(feat) = features.get(node) else {
                continue;
            };
            if feat.exclude_a_text_len < 800 {
                continue;
            }
            let self_ratio = feat.exclude_a_text_len as f64 / (feat.text_len as f64).max(1.0);
            if self_ratio > 0.8 && feat.exclude_a_text_len > max_exclude {
                let parent_filtered = doc.parent(node).map_or(false, |p| {
                    features.get(p).map_or(false, |f| f.is_discard_node)
                });
                if !parent_filtered {
                    best = Some((node, self_ratio, feat.exclude_a_text_len));
                    max_exclude = feat.exclude_a_text_len;
                }
            }
        }
    }

    best.filter(|&(id, _, _)| {
        doc.text_content(id).trim().len() >= MIN_CONTENT_LEN
    }).map(|(id, _, _)| id)
}

/// DFS locate content node starting from body, after title/time
#[allow(dead_code)]
pub fn locate_content_node(
    parent: NodeId,
    doc: &xmloxide::Document,
    features: &FeatureTree,
    title_node: Option<NodeId>,
    body_exclude_len: usize,
    match_node: &mut bool,
) -> Option<NodeId> {
    for child in doc.children(parent) {
        if !doc.is_element(child) {
            continue;
        }

        // Check if this is the title node - sets match_node
        if let Some(tn) = title_node {
            if child == tn {
                *match_node = true;
                continue;
            }
        }

        // Before match_node: skip all elements (C++ behavior)
        if !*match_node {
            continue;
        }

        let tag = doc.node_name(child).unwrap_or("").to_lowercase();
        // C++: skip script/style/link/table/noscript/a/footer and hidden elements
        if matches!(
            tag.as_str(),
            "script" | "style" | "link" | "table" | "noscript" | "a" | "footer"
                | "form" | "select" | "option" | "video"
        ) {
            continue;
        }
        if !is_visible_node(child, doc) {
            continue;
        }

        // Check if this node is content
        if is_content_node(child, features, doc, body_exclude_len, *match_node) {
            // C++: try GetNextContentNode to unwrap single-child wrappers
            let inner = get_next_content_node(doc, child);
            if let Some(inner_id) = inner {
                if is_content_node(inner_id, features, doc, body_exclude_len, true) {
                    if let Some(p) = doc.parent(inner_id) {
                        return Some(p);
                    }
                }
            }
            return Some(child);
        }

        // Recurse into children
        if let Some(result) = locate_content_node(child, doc, features,
            title_node, body_exclude_len, match_node) {
            return Some(result);
        }
    }
    None
}

/// Find content by ratio method (similar to C++ LocateContentNodeWithFeature first pass)
fn find_content_by_ratio(
    doc: &xmloxide::Document,
    features: &FeatureTree,
    title_node: NodeId,
) -> Option<NodeId> {
    let body_exclude_len = features.body_exclude_a_text_len;
    if body_exclude_len == 0 {
        return None;
    }

    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    let mut found_title = false;
    let mut best: Option<(NodeId, f64, usize)> = None;

    for node in doc.descendants(body_id) {
        if !doc.is_element(node) {
            continue;
        }
        let Some(tag) = doc.node_name(node) else {
            continue;
        };

        if !found_title {
            if node == title_node {
                found_title = true;
            }
            continue;
        }

        let tag_lower = tag.to_lowercase();
        if matches!(
            tag_lower.as_str(),
            "script" | "style" | "noscript" | "nav" | "aside" | "header" | "footer"
                | "iframe" | "form" | "select" | "option"
        ) {
            continue;
        }

        let Some(feat) = features.get(node) else {
            continue;
        };

        // C++ filters from LocateContentNodeWithFeature
        if feat.is_discard_node {
            continue;
        }
        // C++: has_recomment_title && tag_a_nc > 3 && click_image_count > 3
        if feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 {
            continue;
        }
        // C++: tag_a_nc > 30 && click_image_count > 3
        if feat.tag_a_nc > 30 && feat.click_image_count > 3 {
            continue;
        }
        // C++: text_len < 128
        if feat.text_len < 128 {
            continue;
        }
        // C++: tag_a_nc > 200
        if feat.tag_a_nc > 200 {
            continue;
        }
        // C++: ParentIsFilterNode (ancestor chain discard check)
        if features.has_discard_ancestor(doc, node) {
            continue;
        }

        let exclude_len = feat.exclude_a_text_len;
        let ratio = exclude_len as f64 / body_exclude_len as f64;

        if ratio > 0.6 {
            let is_better = match best {
                Some((_, best_ratio, _)) => ratio < best_ratio,
                None => true,
            };
            if is_better {
                best = Some((node, ratio, exclude_len));
            }
        }

        if ratio > 0.35 && hit_content_attribute(node, doc) {
            let is_better = match best {
                Some((_, best_ratio, _)) => ratio < best_ratio,
                None => true,
            };
            if is_better {
                best = Some((node, ratio, exclude_len));
            }
        }
    }

    // Method 2: content attribute match (C++ fallback)
    if best.is_none() {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) {
                continue;
            }
            let Some(feat) = features.get(node) else {
                continue;
            };
            if feat.exclude_a_text_len > 64 && hit_content_attribute(node, doc) {
                best = Some((node, 0.0, feat.exclude_a_text_len));
                break;
            }
        }
    }

    // Fallback: self-ratio with ParentIsFilterNode check (C++ method 3)
    if best.is_none() {
        let mut max_exclude = 0usize;
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) {
                continue;
            }
            let Some(feat) = features.get(node) else {
                continue;
            };
            if feat.exclude_a_text_len < 800 {
                continue;
            }
            let self_ratio = feat.exclude_a_text_len as f64 / (feat.text_len as f64).max(1.0);
            if self_ratio > 0.8 && feat.exclude_a_text_len > max_exclude {
                // C++: ParentIsFilterNode check
                if !features.has_discard_ancestor(doc, node) {
                    best = Some((node, self_ratio, feat.exclude_a_text_len));
                    max_exclude = feat.exclude_a_text_len;
                }
            }
        }
    }

    best.filter(|&(id, _, _)| {
        doc.text_content(id).trim().len() >= MIN_CONTENT_LEN
    }).map(|(id, _, _)| id)
}

fn has_substantial_content(doc: &xmloxide::Document, id: NodeId) -> bool {
    let text = doc.text_content(id);
    let text = text.trim();
    if text.len() < MIN_CONTENT_LEN {
        return false;
    }
    let mut p_count = 0;
    for child in doc.descendants(id) {
        if doc.is_element(child) {
            if let Some(tag) = doc.node_name(child) {
                let t = tag.to_lowercase();
                if matches!(t.as_str(), "p" | "div" | "section" | "article" | "blockquote" | "li") {
                    p_count += 1;
                }
            }
        }
    }
    if p_count < 3 && text.len() < 600 {
        return false;
    }
    true
}

fn find_best_content_sibling(
    after_id: NodeId,
    doc: &xmloxide::Document,
    features: &FeatureTree,
) -> Option<NodeId> {
    struct Candidate {
        id: NodeId,
        score: f64,
        #[allow(dead_code)]
        exclude_a_text_len: usize,
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    let mut current = doc.next_sibling(after_id);

    while let Some(sib) = current {
        if !doc.is_element(sib) {
            current = doc.next_sibling(sib);
            continue;
        }

        let tag = doc.node_name(sib).unwrap_or("").to_lowercase();
        if matches!(
            tag.as_str(),
            "script" | "style" | "noscript" | "nav" | "aside" | "header" | "footer" | "iframe"
        ) {
            current = doc.next_sibling(sib);
            continue;
        }

        let text = doc.text_content(sib);
        let trimmed = text.trim();
        if trimmed.len() < 200 && has_end_signal(trimmed) {
            current = doc.next_sibling(sib);
            continue;
        }

        if !matches!(
            tag.as_str(),
            "div" | "article" | "section" | "main" | "p" | "blockquote"
                | "ul" | "ol" | "table" | "figure" | "form" | "td"
        ) {
            current = doc.next_sibling(sib);
            continue;
        }

        let feat_opt = features.get(sib);
        let score = feat_opt.map(|f| content_score(f)).unwrap_or(0.0);
        let exclude_len = feat_opt.map_or(0, |f| f.exclude_a_text_len);

        if exclude_len >= MIN_CONTENT_LEN {
            candidates.push(Candidate {
                id: sib,
                score,
                exclude_a_text_len: exclude_len,
            });
        }

        current = doc.next_sibling(sib);
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
    });

    let best = &candidates[0];
    if best.score < 100.0 {
        return None;
    }

    Some(best.id)
}

fn content_score(feat: &super::feature::NodeFeature) -> f64 {
    let mut score = feat.exclude_a_text_len as f64;
    if feat.tag_a_nc > 0 && feat.text_len > 0 {
        let link_density = feat.tag_a_nc as f64 / feat.text_len as f64;
        if link_density > 0.5 {
            score *= 0.1;
        } else if link_density > 0.2 {
            score *= 0.5;
        }
    }
    if feat.is_discard_node {
        score *= 0.01;
    }
    if feat.has_recomment_title {
        score *= 0.05;
    }
    score
}

pub fn find_content_by_anchor(
    doc: &xmloxide::Document,
    features: &FeatureTree,
    title_node: NodeId,
) -> Option<NodeId> {
    // 0. Try ratio-based content finding (C++ LocateContentNodeWithFeature style)
    if let Some(content) = find_content_by_ratio(doc, features, title_node) {
        return Some(content);
    }

    // 1. Check siblings of title node's parent
    if let Some(parent) = doc.parent(title_node) {
        if let Some(content) = find_best_content_sibling(parent, doc, features) {
            return Some(content);
        }
    }

    // 2. Check siblings of title node itself
    if let Some(content) = find_best_content_sibling(title_node, doc, features) {
        return Some(content);
    }

    // 3. Try h2/h3 anchored approach
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());
    for h_tag in &["h2", "h3"] {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) {
                continue;
            }
            if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case(h_tag)) {
                let text = doc.text_content(node);
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.len() > 5 && trimmed.len() < 100 && !has_end_signal(trimmed) {
                    if let Some(content) = find_best_content_sibling(node, doc, features) {
                        if has_substantial_content(doc, content) {
                            return Some(content);
                        }
                    }
                }
            }
        }
    }

    // 4. Try expanding from any h1
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) {
            continue;
        }
        if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("h1")) {
            let text = doc.text_content(node);
            if text.trim().is_empty() {
                continue;
            }
            if let Some(content) = find_best_content_sibling(node, doc, features) {
                if has_substantial_content(doc, content) {
                    return Some(content);
                }
            }
        }
    }

    None
}
