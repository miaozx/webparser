use crate::dom::{Document, Selection};
use super::feature::FeatureTree;
use super::end_signals::has_end_signal;

const MIN_CONTENT_LEN: usize = 300;

fn hit_content_attribute(el: &Selection) -> bool {
    let content_keywords = [
        "content", "article", "post", "entry", "story", "main",
        "art_content", "post_content", "post-content", "article-content",
        "article_content", "article-body", "article_body",
        "entry-content", "story-content", "storycontent",
        "main-content", "main_content", "mainText", "maintext",
        "bodyContent", "bodycontent", "fulltext", "fullText",
        "articleText", "articletext", "articleText",
        "post-body", "postbody", "post-bodycopy",
        "single-content", "single-post",
        "page-content", "section-content",
        "content-body", "contentBody", "content__body",
        "theme-content", "blog-content",
        "articlecont", "artibody", "art_postcontent",
    ];
    if let Some(class) = el.attr("class") {
        let lower = class.to_ascii_lowercase();
        for kw in &content_keywords {
            if lower.contains(kw) {
                return true;
            }
        }
    }
    if let Some(id) = el.attr("id") {
        let lower = id.to_ascii_lowercase();
        for kw in &content_keywords {
            if lower.contains(kw) {
                return true;
            }
        }
    }
    false
}

/// C++ IsUserCard: check for author/sidebar card elements
fn is_user_card(sel: &Selection) -> bool {
    // C++ HitUserAttribute: class/id patterns for author/sidebar
    if let Some(class) = sel.attr("class") {
        let lower = class.to_ascii_lowercase();
        if lower.contains("author-name") || lower.contains("AuthorName")
            || lower.contains("authorName") || lower.contains("author name")
            || lower.contains("AuthorCard") || lower.contains("zuozhe")
            || lower.contains("bianji") || lower.contains("xiaobian")
            || lower.contains("posted-by") || lower.contains("submitted-by")
        {
            return true;
        }
    }
    if let Some(id) = sel.attr("id") {
        let lower = id.to_ascii_lowercase();
        if lower == "author" || lower == "writer" || lower == "username" {
            return true;
        }
    }
    false
}

fn is_content_node(
    sel: &Selection,
    feat: &super::feature::NodeFeature,
    body_exclude_len: usize,
    _match_node: bool,
    tag: &str,
) -> bool {
    if feat.exclude_a_text_len < 64 {
        return false;
    }

    // C++ IsListNode: filter out link-list nodes
    if is_list_node(feat, tag) {
        return false;
    }

    let ratio = feat.exclude_a_text_len as f64 / (body_exclude_len as f64).max(1.0);

    // C++ IsContentNode: ratio > 0.35 + HitContentAttribute
    if ratio > 0.35 && hit_content_attribute(sel) {
        return true;
    }

    // C++: (ul/ol) && tag_a_nc > 100 → list with all links
    if (tag == "ul" || tag == "ol") && feat.tag_a_nc > 100 {
        return false;
    }

    // C++ negative checks
    if feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 {
        return false;
    }
    if _match_node && feat.tag_a_nc > 30 && feat.click_image_count > 3 {
        return false;
    }
    if feat.has_recomment_title && feat.tag_a_nc > 3 {
        return false;
    }
    if feat.tag_a_nc > 30 {
        return false;
    }
    if feat.tag_a_nc >= 101 && feat.max_exclude_a_text_len < 20.0 {
        return false;
    }

    // C++ IsContentNode: match_node && ratio > 0.6
    if _match_node && ratio > 0.6 {
        return true;
    }

    false
}

/// C++ IsListNode
fn is_list_node(feat: &super::feature::NodeFeature, tag: &str) -> bool {
    // C++: text_nc >= 3 && exclude_a_text_nc == 0 → all text is inside links
    if feat.text_nc >= 3 && feat.exclude_a_text_nc == 0 {
        return true;
    }
    // C++: text_nc >= 5 && exclude_a_text_len < 20 → mostly links
    if feat.text_nc >= 5 && feat.exclude_a_text_len < 20 {
        return true;
    }
    // C++: tag_a_nc >= 10 && max_exclude_a_text_len < 10
    if feat.tag_a_nc >= 10 && feat.max_exclude_a_text_len < 10.0 {
        return true;
    }
    // C++: tag_a_nc >= 100 && max_exclude_a_text_len < 20
    if feat.tag_a_nc >= 100 && feat.max_exclude_a_text_len < 20.0 {
        return true;
    }
    false
}

/// C++ GetNextContentNode: unwrap single-child wrappers
fn get_next_content_node<'a>(node: &Selection<'a>) -> Option<Selection<'a>> {
    let mut cur = node.clone();
    loop {
        let children: Vec<_> = cur.nodes().first()?.children()
            .iter()
            .filter(|c| c.is_element())
            .copied()
            .collect();
        if children.len() == 1 {
            cur = Selection::from(children[0]);
        } else {
            break;
        }
    }
    Some(cur)
}

/// C++ LocateContentNode: DFS from body, find content after title/time
fn locate_content_node<'a>(
    parent: &Selection<'a>,
    features: &FeatureTree,
    title_node: Option<&Selection<'a>>,
    body_exclude_len: usize,
    match_node: &mut bool,
    node_list: &[dom_query::NodeRef<'a>],
    start_idx: &mut usize,
) -> Option<Selection<'a>> {
    let Some(parent_ref) = parent.nodes().first().copied() else {
        return None;
    };

    let children: Vec<_> = parent_ref.children()
        .iter()
        .filter(|c| c.is_element())
        .copied()
        .collect();

    for child in &children {
        let sel = Selection::from(*child);

        // Check if this is the title node - sets match_node
        if let Some(tn) = title_node {
            if let Some(tref) = tn.nodes().first().copied() {
                if child.id == tref.id {
                    *match_node = true;
                    // After matching title, continue to next sibling (C++: child = child->next; continue)
                    continue;
                }
            }
        }

        // Before match_node: skip all elements (C++ behavior for text nodes before title)
        if !*match_node {
            continue;
        }

        // C++: skip script/style/link/table/noscript/a/footer and hidden elements
        let tag = child.node_name().unwrap_or_default().to_lowercase();
        if matches!(
            tag.as_str(),
            "script" | "style" | "link" | "table" | "noscript" | "a" | "footer"
                | "form" | "select" | "option" | "video"
        ) {
            continue;
        }

        // C++ IsVisibleNode
        if !super::feature::is_visible_node(&sel) {
            continue;
        }

        // C++ IsUserCard check (handled by is_user_card above)
        // (removed duplicate inline check)

        // Check if this node is content
        if let Some(feat) = features.get(&sel) {
            if feat.exclude_a_text_len >= 64 && *match_node {
                let ratio = feat.exclude_a_text_len as f64 / (body_exclude_len as f64).max(1.0);
                if ratio > 0.35 || tag == "div" || tag == "section" || tag == "article" {
                    eprintln!("DEBUG ANC check_node: tag={} exclude={} text={} ratio={:.3} match={} content_attr={} is_list={}",
                        tag, feat.exclude_a_text_len, feat.text_len, ratio, *match_node,
                        hit_content_attribute(&sel),
                        is_list_node(feat, &tag));
                }
            }
            if is_content_node(&sel, feat, body_exclude_len, *match_node, &tag) {
                eprintln!("DEBUG ANC locate_content_node MATCH: tag={} exclude_len={} text_len={}",
                    tag, feat.exclude_a_text_len, feat.text_len);
                // C++: try GetNextContentNode to unwrap single-child wrappers
                let inner = get_next_content_node(&sel);
                if let Some(inner_sel) = inner {
                    if let Some(inner_feat) = features.get(&inner_sel) {
                        if is_content_node(&inner_sel, inner_feat, body_exclude_len, true, &tag) {
                            // inner's parent is the content node
                            if let Some(p) = inner_sel.nodes().first()?.parent() {
                                return Some(Selection::from(p));
                            }
                        }
                    }
                }
                return Some(sel);
            }
        }

        // Recurse into children (C++: LocateContentNode(child, ...))
        if let Some(result) = locate_content_node(
            &sel, features, title_node, body_exclude_len,
            match_node, node_list, start_idx,
        ) {
            return Some(result);
        }
    }

    None
}

/// C++ LocateContentNodeWithFeature: fallback - iterate all nodes by ratio
fn locate_content_node_with_feature<'a>(
    features: &FeatureTree,
    node_list: &[dom_query::NodeRef<'a>],
    body_exclude_len: usize,
) -> Option<Selection<'a>> {
    let mut best: Option<(Selection<'a>, f64, usize)> = None;

    // Method 1: ratio > 0.6, pick smallest ratio above threshold
    for node in node_list {
        let sel = Selection::from(*node);
        let Some(feat) = features.get(&sel) else {
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
                best = Some((sel.clone(), ratio, feat.exclude_a_text_len));
            }
        }
    }

    // Method 2: content attribute match
    if best.is_none() {
        for node in node_list {
            let sel = Selection::from(*node);
            let Some(feat) = features.get(&sel) else {
                continue;
            };
            if feat.exclude_a_text_len > 64 && hit_content_attribute(&sel) {
                best = Some((sel.clone(), 0.0, feat.exclude_a_text_len));
                break;
            }
        }
    }

    // Method 3: self-ratio > 0.8 && exclude > 800
    if best.is_none() {
        let mut max_exclude = 0usize;
        for node in node_list {
            let sel = Selection::from(*node);
            let Some(feat) = features.get(&sel) else {
                continue;
            };
            if feat.exclude_a_text_len < 800 {
                continue;
            }
            let self_ratio = feat.exclude_a_text_len as f64 / (feat.text_len as f64).max(1.0);
            if self_ratio > 0.8 && feat.exclude_a_text_len > max_exclude {
                // Check parent not filtered
                let parent_filtered = node.parent().map_or(false, |p| {
                    features.get(&Selection::from(p)).map_or(false, |f| f.is_discard_node)
                });
                if !parent_filtered {
                    best = Some((sel.clone(), self_ratio, feat.exclude_a_text_len));
                    max_exclude = feat.exclude_a_text_len;
                }
            }
        }
    }

    best.filter(|(sel, _, len)| {
        sel.text().trim().len() >= MIN_CONTENT_LEN
    }).map(|(sel, _, _)| sel)
}

/// C++ IsFilterNode: IsListNode + HitAttributeFilter (simplified)
fn is_filter_node(node: &dom_query::NodeRef, features: &FeatureTree) -> bool {
    // Check if node is discarded by feature analysis
    let sel = Selection::from(*node);
    if let Some(feat) = features.get(&sel) {
        if feat.is_discard_node {
            return true;
        }
    }
    false
}

fn has_substantial_content(el: &Selection) -> bool {
    let text = el.text();
    let text = text.trim();
    if text.len() < MIN_CONTENT_LEN {
        return false;
    }
    let p_count = el.select("p, div, section, article, blockquote, li").length();
    if p_count < 3 && text.len() < 600 {
        return false;
    }
    true
}

fn find_best_content_sibling<'a>(
    after_node: &Selection<'a>,
    features: &FeatureTree,
) -> Option<Selection<'a>> {
    let Some(start_ref) = after_node.nodes().first().copied() else {
        return None;
    };

    struct Candidate<'a> {
        sel: Selection<'a>,
        score: f64,
        exclude_a_text_len: usize,
    }

    let mut candidates: Vec<Candidate<'a>> = Vec::new();

    let mut current = start_ref.next_sibling();
    while let Some(sib) = current {
        if !sib.is_element() {
            current = sib.next_sibling();
            continue;
        }

        let tag = sib.node_name().unwrap_or_default().to_lowercase();

        if matches!(
            tag.as_str(),
            "script" | "style" | "noscript" | "nav" | "aside" | "header" | "footer" | "iframe"
        ) {
            current = sib.next_sibling();
            continue;
        }

        let sel = Selection::from(sib);

        let text = sel.text();
        let trimmed = text.trim();
        if trimmed.len() < 200 && has_end_signal(trimmed) {
            current = sib.next_sibling();
            continue;
        }

        if !matches!(
            tag.as_str(),
            "div" | "article" | "section" | "main" | "p" | "blockquote"
                | "ul" | "ol" | "table" | "figure" | "form" | "td"
        ) {
            current = sib.next_sibling();
            continue;
        }

        let score = features.content_score(&sel);
        let exclude_len = features
            .get(&sel)
            .map_or(0, |f| f.exclude_a_text_len);

        if exclude_len >= MIN_CONTENT_LEN {
            candidates.push(Candidate {
                sel,
                score,
                exclude_a_text_len: exclude_len,
            });
        }

        current = sib.next_sibling();
    }

    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let best = &candidates[0];
    if best.score < 100.0 {
        return None;
    }

    Some(best.sel.clone())
}

pub fn find_content_by_ratio<'a>(
    doc: &'a Document,
    features: &FeatureTree,
    title_node: &Selection<'a>,
) -> Option<Selection<'a>> {
    let body_exclude_len = features.body_exclude_a_text_len;
    if body_exclude_len == 0 {
        return None;
    }

    let body = doc.body().unwrap_or_else(|| doc.root());
    let title_ref = title_node.nodes().first().copied();
    let mut found_title = false;

    let mut best: Option<(Selection<'a>, f64, usize)> = None;

    for node in body.descendants() {
        if !node.is_element() {
            continue;
        }
        let Some(tag) = node.node_name() else {
            continue;
        };

        if !found_title {
            if let Some(t) = title_ref {
                if node.id == t.id {
                    found_title = true;
                }
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

        let sel = Selection::from(node);
        let Some(feat) = features.get(&sel) else {
            continue;
        };
        if feat.is_discard_node || feat.exclude_a_text_len < 64 {
            continue;
        }
        if feat.tag_a_nc > 30 {
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
                best = Some((sel.clone(), ratio, exclude_len));
            }
        }

        if ratio > 0.35 && hit_content_attribute(&sel) {
            let is_better = match best {
                Some((_, best_ratio, _)) => ratio < best_ratio,
                None => true,
            };
            if is_better {
                best = Some((sel.clone(), ratio, exclude_len));
            }
        }
    }

    // Fallback: self-ratio for nodes containing title
    if best.is_none() {
        for node in body.descendants() {
            if !node.is_element() {
                continue;
            }
            let sel = Selection::from(node);
            let Some(feat) = features.get(&sel) else {
                continue;
            };
            if feat.exclude_a_text_len < 800 {
                continue;
            }
            let self_ratio = feat.exclude_a_text_len as f64 / (feat.text_len as f64).max(1.0);
            if self_ratio > 0.8 {
                if let Some(t) = title_ref {
                    if node.descendants().iter().any(|n| n.id == t.id) {
                        let parent_filtered = std::iter::successors(node.parent(), |p| p.parent())
                            .any(|p| {
                                features.get(&Selection::from(p)).map_or(false, |f| f.is_discard_node)
                            });
                        if !parent_filtered {
                            best = Some((sel.clone(), self_ratio, feat.exclude_a_text_len));
                            break;
                        }
                    }
                }
            }
        }
    }

    best.filter(|(sel, _, _)| {
        sel.text().trim().len() >= MIN_CONTENT_LEN
    }).map(|(sel, _, _)| sel)
}

pub fn find_content_by_anchor<'a>(
    doc: &'a Document,
    features: &FeatureTree,
    title_node: &Selection<'a>,
) -> Option<Selection<'a>> {
    // 0. Try ratio-based content finding (C++ LocateContentNodeWithFeature style)
    if let Some(content) = find_content_by_ratio(doc, features, title_node) {
        return Some(content);
    }

    // 1. Check siblings of title node's parent
    if let Some(parent) = title_node.parent().nodes().first().copied() {
        let parent_sel = Selection::from(parent);
        if let Some(content) = find_best_content_sibling(&parent_sel, features) {
            let text = content.text().trim().len();
                return Some(content);
        }
    }

    // 2. Check siblings of title node itself
    if let Some(content) = find_best_content_sibling(title_node, features) {
        let text = content.text().trim().len();
        return Some(content);
    }

    // 3. Try h2/h3 anchored approach
    for tag in &["h2", "h3"] {
        for node in doc.select(tag).nodes() {
            let sel = Selection::from(*node);
            let text = sel.text();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.len() > 5 && trimmed.len() < 100 && !has_end_signal(trimmed) {
                if let Some(content) = find_best_content_sibling(&sel, features) {
                    if has_substantial_content(&content) {
                        return Some(content);
                    }
                }
            }
        }
    }

    // 4. Try expanding from any h1
    for node in doc.select("h1").nodes() {
        let sel = Selection::from(*node);
        let text = sel.text();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(content) = find_best_content_sibling(&sel, features) {
            if has_substantial_content(&content) {
                return Some(content);
            }
        }
    }

    None
}
