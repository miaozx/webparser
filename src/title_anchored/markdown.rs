use xmloxide::NodeId;

use super::feature::{FeatureTree, is_visible_node, hit_content_attribute};
use super::end_signals::is_end_text;
use super::title_locator::{parse_head_title, locate_title_node};
use super::time_locator::locate_time_near_title;

/// Whether to include images in markdown output. Disabled by default.
const INCLUDE_IMAGES: bool = false;

const NEWLINE_TAGS: &[&str] = &[
    "p", "div", "section", "h1", "h2", "h3", "h4", "h5", "h6",
    "ul", "ol", "li", "br", "article", "dl", "dt", "dd",
    "img", "video", "table", "pre", "code", "blockquote",
];

pub struct TAExtractResult {
    pub title: String,
    pub publish_time: String,
    pub content_text: String,
    pub content_markdown: String,
}

pub fn extract_with_ta(html: &str) -> Option<TAExtractResult> {
    let doc = xmloxide::html5::parse_html5(html).ok()?;
    extract_from_doc(&doc)
}

pub fn extract_from_doc(doc: &xmloxide::Document) -> Option<TAExtractResult> {
    let features = FeatureTree::build(doc);
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());

    // Head title
    let head_title = parse_head_title(doc);

    // Title node: use full C++ logic; fallback to body
    let title_node = head_title.as_ref()
        .and_then(|ht| locate_title_node(doc, ht, Some(&features)))
        .or_else(|| Some(body_id));
    let title = title_node
        .map(|n| doc.text_content(n).trim().to_string())
        .unwrap_or_default();

    // Publish time (use full C++ logic: DFS 192 chars + time elements + date patterns + meta tags)
    let publish_time_string = title_node
        .and_then(|tn| locate_time_near_title(doc, tn, &title))
        .map(|tn| doc.text_content(tn).trim().to_string());

    // Content node: C++ ParseContent → LocateContentNode (title-anchored)
    // If content is too short (< 256 chars, C++ threshold), fall back to
    // C++ Parse → LocateContentNodeWithFeature (ratio-based fallback)
    // If still not found, return empty content (no body fallback)
    let fallback_node = title_node
        .and_then(|tn| {
            let cn = find_content_node_xml(doc, &features, tn, body_id)?;
            let preview = extract_content_markdown(doc, &features, cn, tn, body_id);
            let text = markdown_to_text(&preview);
            if text.len() >= 256 { Some(cn) } else { None }
        })
        .and_then(|cn| {
            // C++ ParseContent: reject if content node is a filter node
            if features.is_filter_node(cn, doc) { None } else { Some(cn) }
        })
        .and_then(|cn| {
            // C++ ParseContent: reject if content is image-link-heavy (sidebars etc.)
            let preview = extract_content_markdown(doc, &features, cn, title_node.unwrap_or(body_id), body_id);
            let total = preview.len();
            if total == 0 { return None; }
            let img_lines = preview.lines().filter(|l| l.trim().starts_with("![](")).count();
            let total_lines = preview.lines().count();
            if total_lines > 0 && (img_lines as f64 / total_lines as f64) > 0.5 {
                None
            } else {
                Some(cn)
            }
        })
        .or_else(|| {
            // C++ LocateContentNodeWithFeature fallback
            locate_content_node_with_feature_fallback(doc, &features, body_id)
        });
    let Some(fallback_node) = fallback_node else {
        return Some(TAExtractResult {
            title,
            publish_time: publish_time_string.unwrap_or_default(),
            content_text: String::new(),
            content_markdown: String::new(),
        });
    };
    let cn_tag = doc.node_name(fallback_node).unwrap_or("?");
    let cn_class = doc.attribute(fallback_node, "class").unwrap_or("");
    eprintln!("TA_FINAL_CN: tag={} class={:?} text_len={}",
        cn_tag, cn_class, doc.text_content(fallback_node).trim().len());

    // Extract markdown from content node
    let content_markdown = extract_content_markdown(doc, &features, fallback_node,
        title_node.unwrap_or(body_id), body_id);

    let content_text = markdown_to_text(&content_markdown);

    Some(TAExtractResult {
        title,
        publish_time: publish_time_string.unwrap_or_default(),
        content_text,
        content_markdown,
    })
}

fn parse_head_title_xml(doc: &xmloxide::Document) -> Option<String> {
    let root = doc.root_element().or_else(|| doc.children(doc.root()).next())?;
    for node in doc.descendants(root) {
        if doc.is_element(node) && doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("title")) {
            let t = doc.text_content(node).trim().to_string();
            if !t.is_empty() {
                return Some(t);
            }
        }
    }
    None
}

fn locate_title_node_xml(doc: &xmloxide::Document, head_title: &str,
    features: Option<&FeatureTree>) -> Option<NodeId> {
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());

    // Try h1/h2 matching head_title
    for tag in &["h1", "h2"] {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) { continue; }
            if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case(tag)) {
                let t = doc.text_content(node).trim().to_string();
                if !t.is_empty() && (head_title.contains(&t) || t.contains(head_title)) {
                    return Some(node);
                }
            }
        }
    }
    // fallback: first h1/h2 with text > 15
    for tag in &["h1", "h2"] {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) { continue; }
            if doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case(tag)) {
                let text = doc.text_content(node);
                let t = text.trim();
                if t.len() > 15 {
                    return Some(node);
                }
            }
        }
    }
    // last resort: use body itself as the "title node"
    Some(body_id)
}

fn locate_content_node_with_feature_fallback(
    doc: &xmloxide::Document,
    features: &FeatureTree,
    body_id: NodeId,
) -> Option<NodeId> {
    let body_exclude = features.body_exclude_a_text_len;
    if body_exclude == 0 { return None; }

    // Method 1: ratio > 0.6, pick smallest ratio (C++ LocateContentNodeWithFeature)
    let mut best: Option<(NodeId, f64)> = None;
    for node in doc.descendants(body_id) {
        if !doc.is_element(node) { continue; }
        let Some(feat) = features.get(node) else { continue; };
        if feat.is_discard_node || feat.text_len < 128 || feat.tag_a_nc > 200 { continue; }
        if feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 { continue; }
        if feat.tag_a_nc > 30 && feat.click_image_count > 3 { continue; }
        if features.has_discard_ancestor(doc, node) { continue; }
        let ratio = feat.exclude_a_text_len as f64 / (body_exclude as f64).max(1.0);
        if ratio > 0.6 {
            let better = match best {
                Some((_, br)) => ratio < br,
                None => true,
            };
            if better { best = Some((node, ratio)); }
        }
    }

    // Method 2: HitContentAttribute match
    if best.is_none() {
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) { continue; }
            let Some(feat) = features.get(node) else { continue; };
            if feat.exclude_a_text_len > 64 && hit_content_attribute(node, doc) {
                let t = doc.text_content(node).trim().len();
                if t >= 256 { best = Some((node, 0.0)); break; }
            }
        }
    }

    // Method 3: self-ratio > 0.8 && exclude > 800
    if best.is_none() {
        let mut max_ex = 0usize;
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) { continue; }
            let Some(feat) = features.get(node) else { continue; };
            if feat.exclude_a_text_len < 800 { continue; }
            let sr = feat.exclude_a_text_len as f64 / (feat.text_len as f64).max(1.0);
            if sr > 0.8 && feat.exclude_a_text_len > max_ex
                && !features.has_discard_ancestor(doc, node)
            {
                best = Some((node, sr));
                max_ex = feat.exclude_a_text_len;
            }
        }
    }

    // C++ also adds: if max_text_len > 126 from Method 1 as extra check
    // and Method 3 has 1000 node cap — both minor, omitted for now
    best.map(|(id, _)| id)
}

fn is_date_str(s: &str) -> bool {
    // Use the full C++ ExtractPublishTime logic via time_locator
    super::time_locator::extract_publish_time(s).is_some()
}

fn find_content_node_xml(doc: &xmloxide::Document, features: &FeatureTree,
    title_node: NodeId, body_id: NodeId) -> Option<NodeId> {
    // C++ LocateContentNode: iterate children of body, find title, then look for content
    let body_feat = features.get(body_id).cloned().unwrap_or_default();

    // DFS: for each child, check if it's the title, then check if it's content
    fn locate(
        doc: &xmloxide::Document,
        features: &FeatureTree,
        node: NodeId,
        title_node: NodeId,
        body_feat: &super::feature::NodeFeature,
        match_node: &mut bool,
    ) -> Option<NodeId> {
        for child in doc.children(node) {
            if !doc.is_element(child) { continue; }

            // Check if this IS the title node
            if child == title_node {
                *match_node = true;
                continue;  // Skip the title itself, check next sibling
            }

            // Before match: skip all elements (C++: (time_node || title_node) && !match_node)
            if !*match_node {
                // C++: still recurse into children to look for title deeper
                if let Some(result) = locate(doc, features, child, title_node, body_feat, match_node) {
                    return Some(result);
                }
                continue;
            }

            // After match: check if this is a content node
            let tag = doc.node_name(child).unwrap_or("").to_lowercase();

            // Skip filtered tags (C++: script/style/link/table/noscript/a/footer/invisible/filter)
            if matches!(tag.as_str(), "script" | "style" | "link" | "noscript" | "a" | "footer") {
                continue;
            }
            if !super::feature::is_visible_node(child, doc) { continue; }
            if features.is_filter_node(child, doc) { continue; }

            // C++ IsContentNode check
            if let Some(feat) = features.get(child) {
                // C++: text_len > 64 && HitContentAttribute && exclude/body_exclude > 0.35
                if feat.text_len > 64
                    && super::feature::hit_content_attribute(child, doc)
                    && body_feat.exclude_a_text_len > 0
                    && (feat.exclude_a_text_len as f64 / body_feat.exclude_a_text_len as f64) > 0.35
                {
                    // C++: try GetNextContentNode unwrapping, then return
                    let inner = super::content::get_next_content_node(doc, child);
                    if let Some(inner_id) = inner {
                        if let Some(inner_feat) = features.get(inner_id) {
                            if features.is_content_node(inner_id, doc, body_feat.exclude_a_text_len, true) {
                                if let Some(p) = doc.parent(inner_id) {
                                    return Some(p);
                                }
                            }
                        }
                    }
                    return Some(child);
                }
                // C++ negative checks
                if *match_node && feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 {
                    continue;
                }
                if *match_node && feat.tag_a_nc > 30 && feat.click_image_count > 3 {
                    continue;
                }
                if (tag == "ul" || tag == "ol") && feat.tag_a_nc > 100 {
                    continue;
                }
                if feat.tag_a_nc >= 101 && feat.max_exclude_a_text_len < 20.0 {
                    continue;
                }
                // C++: match_node && exclude/body_exclude > 0.6
                if *match_node && body_feat.exclude_a_text_len > 0
                    && (feat.exclude_a_text_len as f64 / body_feat.exclude_a_text_len as f64) > 0.6
                {
                    return Some(child);
                }
            }

            // Recurse into children (C++: LocateContentNode(child, ...))
            if let Some(result) = locate(doc, features, child, title_node, body_feat, match_node) {
                return Some(result);
            }
        }
        None
    }

    let mut match_node = false;
    let result = locate(doc, features, body_id, title_node, &body_feat, &mut match_node);

    // If no content node found via title-anchored approach, fall back to body itself
    // (C++ returns false and caller handles it)
    result
}

fn extract_content_markdown(doc: &xmloxide::Document, features: &FeatureTree,
    content_node: NodeId, title_node: NodeId, body_id: NodeId) -> String {
    // C++ GetContent flow

    // Determine if title is inside content node
    // When content_node == title_node, treat as not-in-content (title IS the content root,
    // so there's nothing to "match" — process everything from the start)
    let title_in_cont = content_node != title_node && is_contains_node(doc, content_node, title_node);

    // Time node / time area
    let time_node = find_time_in_content(doc, content_node, title_node, body_id);
    let time_in_cont = time_node.map_or(false, |tn| is_contains_node(doc, content_node, tn));
    let time_area = time_node.and_then(|tn| get_time_node_area(doc, tn));
    //

    // Traverse content
    let mut match_node = !(title_in_cont || time_in_cont);
    let mut cur_text = String::new();
    let mut para_list: Vec<String> = Vec::new();
    let mut in_code_tag = false;
    let mut is_end = false;

    traverse_content(
        doc, features, content_node, title_node, title_in_cont,
        time_area, time_in_cont, &mut match_node, &mut cur_text,
        &mut para_list, &mut in_code_tag, &mut is_end
    );

    // Flush remaining text
    if !cur_text.is_empty() {
        let tag = doc.node_name(content_node).unwrap_or("");
        if NEWLINE_TAGS.contains(&tag) {
            let pure = cur_text.trim().to_string();
            if !pure.is_empty() {
                if in_code_tag { para_list.push(pure); }
                else { para_list.push(format_paragraph(&pure)); }
            }
            cur_text.clear();
        }
        if !cur_text.is_empty() {
            para_list.push(cur_text);
        }
    }

    // Post-processing
    para_list = check_para_list(para_list);
    para_list = format_style(para_list);

    if para_list.is_empty() { return String::new(); }
    para_list.join("\n")
}

fn is_contains_node(doc: &xmloxide::Document, parent: NodeId, child: NodeId) -> bool {
    if parent == child { return true; }
    for c in doc.children(parent) {
        if is_contains_node(doc, c, child) { return true; }
    }
    false
}

fn find_time_in_content(doc: &xmloxide::Document, content_node: NodeId,
    _title_node: NodeId, _body_id: NodeId) -> Option<NodeId> {
    // Search for date patterns within content_node's text descendants,
    // limiting to first ~200 chars of traversed text (C++ style)
    let max_chars = 200usize;
    let mut traverse_len = 0usize;

    for node in doc.descendants(content_node) {
        if matches!(doc.node(node).kind, xmloxide::tree::NodeKind::Text { .. }) {
            if let Some(t) = doc.node_text(node) {
                let trimmed = t.trim();
                if trimmed.is_empty() { continue; }
                traverse_len += trimmed.len();
                if traverse_len > max_chars { break; }
                let is_date = is_date_str(trimmed);
                if is_date && traverse_len <= max_chars {
                    if let Some(parent) = doc.parent(node) {
                        return Some(parent);
                    }
                }
            }
        }
    }
    None
}

fn get_time_node_area(doc: &xmloxide::Document, time_node: NodeId) -> Option<NodeId> {
    let mut depth = 2;
    let text_len = doc.text_content(time_node).trim().len();
    let mut pnode = doc.parent(time_node);
    let mut cur = time_node;
    let mut result = time_node;
    while depth > 0 {
        if let Some(p) = pnode {
            let p_text_len = doc.text_content(p).trim().len();
            if (p_text_len - text_len) > 50 {
                result = cur;
                return Some(result);
            }
            cur = p;
            pnode = doc.parent(p);
            depth -= 1;
        } else {
            break;
        }
    }
    Some(result)
}

#[allow(clippy::too_many_arguments)]
fn traverse_content(
    doc: &xmloxide::Document,
    features: &FeatureTree,
    node: NodeId,
    title_node: NodeId,
    title_in_cont: bool,
    time_area: Option<NodeId>,
    time_in_cont: bool,
    match_node: &mut bool,
    cur_text: &mut String,
    para_list: &mut Vec<String>,
    in_code_tag: &mut bool,
    is_end: &mut bool,
) {
    if *is_end { return; }
    let p_tag_name = doc.node_name(node).unwrap_or("").to_lowercase();
    let mut li_index = 0usize;

    for child in doc.children(node) {
        if *is_end { break; }
        li_index += 1;
        let mut start = *match_node;

        // Time/title sync — search recursively in the subtree
        if time_in_cont || title_in_cont {
            if time_in_cont {
                if let Some(tn) = time_area {
                    if child == tn || is_contains_node(doc, child, tn) {
                        *match_node = true;
                        start = true;
                        let tc = doc.text_content(tn).trim().len();
                        if tc < 150 { continue; }
                    }
                }
            } else if title_in_cont {
                if child == title_node || is_contains_node(doc, child, title_node) {
                    *match_node = true;
                    start = true;
                    let tc = doc.text_content(title_node).trim().len();
                    if tc < 120 { continue; }
                }
            }
        } else {
            start = true;
        }

        let text_len_ = para_list.iter()
            .filter(|t| !t.starts_with("![](") && !t.starts_with("@video:"))
            .map(|t| t.len())
            .sum::<usize>();

        if matches!(doc.node(child).kind, xmloxide::tree::NodeKind::Text { .. }) {
            if start {
                let mut node_content = doc.node_text(child).unwrap_or("").to_string();
                if !*in_code_tag {
                    let trimmed = node_content.trim();
                    if trimmed.len() > 64 {
                        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
                        {
                            continue;
                        }
                    }
            // Check for end signals: exact match or common end patterns
            let is_end_signal = (is_end_text(trimmed) || trimmed == "推荐阅读"
                || trimmed.starts_with("相关文章") || trimmed.starts_with("推荐阅读"))
                && (text_len_ > 256 || cur_text.len() > 256);
            if is_end_signal {
                *is_end = true;
                    } else {
                        cur_text.push_str(&node_content);
                    }
                } else {
                    cur_text.push_str(&node_content);
                }
            }
        } else if doc.is_element(child) {
            // Check discard
            if let Some(feat) = features.get(child) {
                if feat.is_discard_node && feat.text_len > 0 { continue; }
            }
            let tag = doc.node_name(child).unwrap_or("").to_lowercase();

            // Skip certain tags
    if matches!(tag.as_str(), "script" | "style" | "link" | "noscript"
        | "form" | "select" | "option" | "video" | "svg")
    {
        continue;
    }

            // User card check
            if is_user_card(doc, child) { continue; }

            // Filter node check (list nodes, discard nodes)
            if features.is_filter_node(child, doc) {
                if let Some(feat) = features.get(child) {
                    if feat.text_len > 0 { continue; }
                }
            }

            // End class check: C++ rel_art_line + related/recommend section markers
            if let Some(class_val) = doc.attribute(child, "class") {
                let lc = class_val.to_ascii_lowercase();
                if lc.contains("rel_art_line") || lc.contains("relateread")
                    || lc.contains("relate") && (lc.contains("read") || lc.contains("news"))
                    || lc.contains("recommend")
                    || lc.contains("xglinks") || lc.contains("suggest")
                {
                    *is_end = true;
                    break;
                }
            }
            // Check if cur_text ends with end signal (for text that happens at paragraph level)
            let trimmed_text = cur_text.trim();
            if text_len_ > 256 && is_end_text(trimmed_text) {
                *is_end = true;
                break;
            }
            // Newline tag: flush paragraph, handle images/videos
            if NEWLINE_TAGS.contains(&tag.as_str()) {
                let raw = cur_text.trim().to_string();
                if !raw.is_empty() {
                    let is_end_signal = is_end_text(raw.trim()) && text_len_ > 256;
                    if *in_code_tag { para_list.push(raw); }
                    else { let fp = format_paragraph(&raw); if !fp.is_empty() { para_list.push(fp); } }
                    if is_end_signal {
                        cur_text.clear();
                        *is_end = true;
                        break;
                    }
                }

                // Handle img (disabled by default, re-enable via INCLUDE_IMAGES constant)
                if tag == "img" && INCLUDE_IMAGES {
                    handle_image(doc, child, para_list);
                }
                cur_text.clear();
            }

            // hljs-ln-numbers, pre-numbering, line-numbers-rows, gutter
            if let Some(class_val) = doc.attribute(child, "class") {
                let lc = class_val.to_ascii_lowercase();
                if matches!(lc.as_str(), "hljs-ln-numbers" | "pre-numbering"
                    | "line-numbers-rows" | "gutter")
                {
                    continue;
                }
            }

            // Table handling
            if matches!(tag.as_str(), "table" | "tr" | "td" | "tbody" | "th") {
                if tag == "table" && !has_text_child(doc, child) { continue; }
                if tag == "table" && !super::feature::has_child_table(child, doc) {
                    if !cur_text.is_empty() {
                        para_list.push(cur_text.clone());
                        cur_text.clear();
                    }
                    para_list.push(get_table_text(doc, child));
                    continue;
                }
            }

            // Code block
            if tag == "pre" {
                if !has_text_child(doc, child) { continue; }
                cur_text.push_str("```\n");
                *in_code_tag = true;
            }

            // Heading markers
            if tag == "h1" { cur_text.push_str("# "); }
            else if tag == "h2" { cur_text.push_str("## "); }
            else if tag == "h3" { cur_text.push_str("### "); }
            else if tag == "h4" { cur_text.push_str("#### "); }

            // List markers
            if tag == "li" {
                let depth = list_node_depth(doc, child);
                let indent = " ".repeat(depth * 2 + 1);
                if p_tag_name == "ul" {
                    cur_text.push_str(&format!("{}- ", indent));
                } else if p_tag_name == "ol" {
                    cur_text.push_str(&format!("{}{}. ", indent, li_index));
                }
            }

            // Bold
            if matches!(tag.as_str(), "b" | "strong") {
                let len = subtree_text_len(doc, features, child);
                if len > 0 { cur_text.push_str("**"); }
            }

            // MathJax skip
            if let Some(class_val) = doc.attribute(child, "class") {
                if class_val.contains("MathJax") { continue; }
            }

            // Recurse
            traverse_content(doc, features, child, title_node,
                title_in_cont, time_area, time_in_cont,
                match_node, cur_text, para_list, in_code_tag, is_end);

            // Close code block
            if tag == "pre" {
                cur_text.push_str("\n```");
                *in_code_tag = false;
            }

            // Close bold
            if matches!(tag.as_str(), "b" | "strong") {
                let len = subtree_text_len(doc, features, child);
                if len > 0 { cur_text.push_str("**"); }
            }

            // Flush after newline tag
            if NEWLINE_TAGS.contains(&tag.as_str()) {
                let pure = cur_text.trim().to_string();
                if !pure.is_empty() {
                    if *in_code_tag { para_list.push(pure); }
                    else { let fp = format_paragraph(&pure); if !fp.is_empty() { para_list.push(fp); } }
                }
                cur_text.clear();
            }
        }
    }
}

fn handle_image(doc: &xmloxide::Document, img_node: NodeId, para_list: &mut Vec<String>) {
    let src = doc.attribute(img_node, "data-original-src")
        .or_else(|| doc.attribute(img_node, "data-src"))
        .or_else(|| doc.attribute(img_node, "d-src"))
        .or_else(|| doc.attribute(img_node, "data-original"))
        .or_else(|| doc.attribute(img_node, "src"));
    if let Some(s) = src {
        if s.contains("data:") { return; }
        // Sohu AES decryption would go here if needed
        let absolute = resolve_url(&s);
        if !absolute.is_empty() {
            para_list.push(format!("![]({})", absolute));
        }
    }
}

fn resolve_url(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("//") {
        if s.starts_with("//") { format!("https:{}", s) } else { s.to_string() }
    } else {
        String::new()
    }
}

fn get_table_text(doc: &xmloxide::Document, node: NodeId) -> String {
    let mut rows: Vec<String> = Vec::new();
    collect_table_rows(doc, node, &mut rows, 0);
    if rows.is_empty() { return String::new(); }
    rows.join("\n")
}

fn collect_table_rows(doc: &xmloxide::Document, node: NodeId, rows: &mut Vec<String>, _depth: usize) {
    let tag = doc.node_name(node).unwrap_or("").to_lowercase();
    if tag == "tr" {
        let mut cells: Vec<String> = Vec::new();
        for child in doc.children(node) {
            if doc.is_element(child) {
                let ct = doc.node_name(child).unwrap_or("").to_lowercase();
                if ct == "td" || ct == "th" {
                    let mut cell_text = String::new();
                    collect_inline_text(doc, child, &mut cell_text);
                    cells.push(cell_text.trim().to_string());
                }
            }
        }
        rows.push(cells.join("\t"));
    }
    for child in doc.children(node) {
        collect_table_rows(doc, child, rows, _depth + 1);
    }
}

fn collect_inline_text(doc: &xmloxide::Document, node: NodeId, out: &mut String) {
    for child in doc.children(node) {
        if matches!(doc.node(child).kind, xmloxide::tree::NodeKind::Text { .. }) {
            if let Some(t) = doc.node_text(child) {
                out.push_str(t.trim());
            }
        } else if doc.is_element(child) {
            let tag = doc.node_name(child).unwrap_or("").to_lowercase();
            if !matches!(tag.as_str(), "script" | "style" | "link") {
                collect_inline_text(doc, child, out);
            }
        }
    }
}

fn has_text_child(doc: &xmloxide::Document, node: NodeId) -> bool {
    for child in doc.descendants(node) {
        if matches!(doc.node(child).kind, xmloxide::tree::NodeKind::Text { .. }) {
            if let Some(t) = doc.node_text(child) {
                if !t.trim().is_empty() { return true; }
            }
        }
    }
    false
}

fn is_user_card(doc: &xmloxide::Document, node: NodeId) -> bool {
    let tag = doc.node_name(node).unwrap_or("").to_lowercase();
    if let Some(class_val) = doc.attribute(node, "class") {
        let lc = class_val.to_ascii_lowercase();
        if lc.contains("author-name") || lc.contains("authorcard")
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

fn subtree_text_len(doc: &xmloxide::Document, features: &FeatureTree, node: NodeId) -> usize {
    if let Some(f) = features.get(node) { return f.text_len; }
    doc.text_content(node).trim().len()
}

fn list_node_depth(doc: &xmloxide::Document, node: NodeId) -> usize {
    let mut depth = 0usize;
    let mut cur = doc.parent(node);
    while let Some(p) = cur {
        if doc.is_element(p) {
            let tag = doc.node_name(p).unwrap_or("").to_lowercase();
            if tag == "ul" || tag == "ol" { depth += 1; }
        }
        cur = doc.parent(p);
    }
    depth
}

fn format_paragraph(text: &str) -> String {
    let mut s: String = text.chars()
        .map(|c| if c == '\r' || c == '\t' { ' ' } else { c })
        .collect();
    s = s.replace('\u{00A0}', " ");
    s = s.replace('\u{200b}', " ");
    s = s.replace('\u{200d}', " ");
    // Squeeze spaces
    let mut prev_space = false;
    let squeezed: String = s.chars().filter(|&c| {
        if c == ' ' {
            if prev_space { return false; }
            prev_space = true;
        } else {
            prev_space = false;
        }
        true
    }).collect();
    // Remove blank lines
    let lines: Vec<&str> = squeezed.lines()
        .filter(|l| !l.trim().is_empty())
        .collect();
    let joined = lines.join("\n");
    joined.trim().to_string()
}

fn format_style(para_list: Vec<String>) -> Vec<String> {
    para_list.into_iter().map(|p| {
        if p.starts_with("![](") || p.starts_with("@video:") {
            p
        } else {
            format!("{}  ", p)
        }
    }).collect()
}

/// Strip markdown formatting to get plain text length for comparison.
fn markdown_to_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for line in md.lines() {
        let mut l = line;
        // Strip heading markers
        if l.starts_with("# ") || l.starts_with("## ") || l.starts_with("### ") || l.starts_with("#### ") {
            let level = l.chars().take_while(|&c| c == '#').count();
            l = l[level..].trim_start();
            if l.starts_with(' ') { l = &l[1..]; }
        }
        // Strip list markers
        if l.starts_with("- ") || l.starts_with("* ") {
            l = &l[2..];
        }
        // Strip ordered list markers
        let ordered = l.chars().take_while(|c| c.is_ascii_digit()).count();
        if ordered > 0 && l[ordered..].starts_with(". ") {
            l = &l[ordered + 2..];
        }
        // Remove **bold**
        if !l.contains("**") {
            // Fast path
        }
        // Strip image links but keep URL as text
        if l.starts_with("![](") {
            if let Some(end) = l.find(')') {
                l = &l[4..end];
            }
        }
        let mut cleaned = l.to_string();
        // Remove ** markers
        cleaned = cleaned.replace("**", "");
        // Remove trailing spaces (markdown line break)
        cleaned = cleaned.trim_end().to_string();
        if !cleaned.is_empty() {
            if !out.is_empty() { out.push(' '); }
            out.push_str(&cleaned);
        }
    }
    out
}

fn check_para_list(para_list: Vec<String>) -> Vec<String> {
    let mut result = para_list;
    let mut i = 0;
    while i < result.len() {
        let p = result[i].clone();
        if matches!(p.as_str(), "#" | "##" | "###" | "####") {
            if i + 1 < result.len() {
                result[i + 1] = format!("{} {}", p, result[i + 1]);
                result.remove(i);
            } else {
                result.remove(i);
            }
        } else {
            i += 1;
        }
    }
    result
}
