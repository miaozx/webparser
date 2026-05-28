use xmloxide::NodeId;

use super::feature::{FeatureTree, hit_content_attribute};
use super::end_signals::is_end_text;
use super::title_locator::{parse_head_title, locate_title_node, parse_title_from_h1_fallback, parse_title_with_xpath};
use super::time_locator::locate_time_near_title;

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
    let doc = match xmloxide::html5::parse_html5(html) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("DEBUG_PARSE_ERR: {}", e);
            return None;
        }
    };
    extract_from_doc_with_options(&doc, false)
}

pub fn extract_from_doc(doc: &xmloxide::Document) -> Option<TAExtractResult> {
    extract_from_doc_with_options(doc, false)
}

pub fn extract_from_doc_with_options(doc: &xmloxide::Document, include_images: bool) -> Option<TAExtractResult> {
    let features = FeatureTree::build(doc);
    let body_id = FeatureTree::find_body(doc).unwrap_or_else(|| doc.root());

    // C++ Parse: try ParseWithNodeLocation (title-anchored) first.
    // head_title + title_node are NOT required (unlike current code that uses ?).
    let head_title = parse_head_title(doc);
    let title_node = head_title.as_ref()
        .and_then(|ht| locate_title_node(doc, ht, Some(&features)));

    // Try title-anchored content finding (C++ ParseWithNodeLocation → ParseContent)
    let (content_node, anchored, found_title) = if let (Some(_ht), Some(tn)) = (&head_title, title_node) {
        let title_text = doc.text_content(tn).trim().to_string();
        // C++: ParsePublishTime (optional)
        let _pub_time = locate_time_near_title(doc, tn, &title_text)
            .map(|tn| doc.text_content(tn).trim().to_string())
            .or_else(|| parse_publish_time_from_meta(doc, body_id));

        // C++ ParseContent
        match find_content_node_xml(doc, &features, tn, body_id) {
            Some(cn) if !features.is_filter_node(cn, doc) => {
                let content_md = extract_content_markdown(doc, &features, cn, tn, body_id, include_images);
                let text_len = markdown_to_text(&content_md).len();
                if text_len >= 256 {
                    (Some(cn), true, Some(tn))
                } else {
                    // content too short, fall through to fallback
                    (None, false, Some(tn))
                }
            }
            _ => (None, false, Some(tn))
        }
    } else {
        (None, false, None)
    };

    // C++ Parse fallback: LocateContentNodeWithFeature + GetContent + try ParseTitleFromH1/ParseTitleWithXpath
    let (content_node, found_title) = match content_node {
        Some(cn) => (cn, found_title),
        None => {
            // C++ fallback
            match locate_content_node_with_feature_fallback(doc, &features, body_id) {
                Some(cn) => {
                    let content_md = extract_content_markdown(doc, &features, cn, cn, body_id, include_images);
                    let text_len = markdown_to_text(&content_md).len();
                    if text_len < 256 {
                        return None;
                    }
                    // C++: try to find title in fallback mode
                    let title_node = found_title.or_else(|| {
                        // C++: ParseTitleFromH1("", title_node, &title, false)
                        parse_title_from_h1_fallback(doc)
                        // C++: ParseTitleWithXpath
                        .or_else(|| parse_title_with_xpath(doc))
                    });
                    (cn, title_node)
                }
                None => return None,
            }
        }
    };

    let title = found_title.map(|tn| doc.text_content(tn).trim().to_string()).unwrap_or_default();
    let publish_time_string = found_title.and_then(|tn| {
        locate_time_near_title(doc, tn, &title)
            .map(|t| doc.text_content(t).trim().to_string())
            .or_else(|| parse_publish_time_from_meta(doc, body_id))
    }).or_else(|| parse_publish_time_from_meta(doc, body_id));

    let content_markdown = if anchored {
        if let Some(tn) = found_title {
            extract_content_markdown(doc, &features, content_node, tn, body_id, include_images)
        } else {
            extract_content_markdown(doc, &features, content_node, content_node, body_id, include_images)
        }
    } else {
        extract_content_markdown(doc, &features, content_node, content_node, body_id, include_images)
    };

    let content_text = markdown_to_text(&content_markdown);
    if content_text.len() < 256 {
        return None;
    }

    Some(TAExtractResult {
        title,
        publish_time: publish_time_string.unwrap_or_default(),
        content_text,
        content_markdown,
    })
}

fn parse_publish_time_from_meta(doc: &xmloxide::Document, body_id: NodeId) -> Option<String> {
    for node in doc.descendants(body_id) {
        if doc.is_element(node) && doc.node_name(node).map_or(false, |n| n.eq_ignore_ascii_case("meta")) {
            if let Some(prop) = doc.attribute(node, "property") {
                if prop == "article:published_time" || prop == "article:modified_time" {
                    if let Some(c) = doc.attribute(node, "content") {
                        let t = c.trim().to_string();
                        if !t.is_empty() { return Some(t); }
                    }
                }
            }
            if let Some(name) = doc.attribute(node, "name") {
                let n = name.to_ascii_lowercase();
                if n == "pubdate" || n == "publishdate" || n == "citation_publication_date" {
                    if let Some(c) = doc.attribute(node, "content") {
                        let t = c.trim().to_string();
                        if !t.is_empty() { return Some(t); }
                    }
                }
            }
        }
    }
    None
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

    // Method 3: self-ratio > 0.8 && exclude > 800 (C++: max 1000 nodes)
    if best.is_none() {
        let mut max_ex = 0usize;
        let mut node_count = 0usize;
        for node in doc.descendants(body_id) {
            if !doc.is_element(node) { continue; }
            let Some(feat) = features.get(node) else { continue; };
            node_count += 1;
            if node_count > 1000 { break; }
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

            if child == title_node {
                *match_node = true;
                continue;
            }

            if !*match_node {
                if let Some(result) = locate(doc, features, child, title_node, body_feat, match_node) {
                    return Some(result);
                }
                continue;
            }

            let tag = doc.node_name(child).unwrap_or("").to_lowercase();
            if matches!(tag.as_str(), "script" | "style" | "link" | "noscript" | "a" | "footer" | "textarea") {
                continue;
            }
            if !super::feature::is_visible_node(child, doc) { continue; }
            if features.is_filter_node(child, doc) { continue; }

            if let Some(feat) = features.get(child) {
                let denom = (body_feat.exclude_a_text_len + 1) as f64;
                let hit_attr = super::feature::hit_content_attribute(child, doc);
                let ratio = feat.exclude_a_text_len as f64 / denom;

                // C++: text_len > 64 && HitContentAttribute && ratio > 0.35
                if feat.text_len > 64 && hit_attr && ratio > 0.35 {
                    let inner = super::content::get_next_content_node(doc, child);
                    if let Some(inner_id) = inner {
                        if features.is_content_node(inner_id, doc, body_feat.exclude_a_text_len, true) {
                            if let Some(p) = doc.parent(inner_id) { return Some(p); }
                        }
                    }
                    return Some(child);
                }
                // C++ negative checks
                if *match_node && feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 { continue; }
                if *match_node && feat.tag_a_nc > 30 && feat.click_image_count > 3 { continue; }
                if (tag == "ul" || tag == "ol") && feat.tag_a_nc > 100 { continue; }
                if feat.tag_a_nc >= 101 && feat.max_exclude_a_text_len < 20.0 { continue; }
                // C++: match_node && exclude/(body_exclude+1) > 0.6
                if *match_node && ratio > 0.6 {
                    return Some(child);
                }
            }

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
    content_node: NodeId, title_node: NodeId, body_id: NodeId, include_images: bool) -> String {
    // C++ GetContent flow

    // Determine if title is inside content node
    // When content_node == title_node, treat as not-in-content (title IS the content root,
    // so there's nothing to "match" — process everything from the start)
    let title_in_cont = content_node != title_node && is_contains_node(doc, content_node, title_node);

    // C++ GetContent: pass time_node directly (GetTimeNodeArea is computed but not used)
    let raw_time_node = find_time_in_content(doc, content_node, title_node, body_id);
    let time_in_cont = raw_time_node.map_or(false, |tn| is_contains_node(doc, content_node, tn));

    // Traverse content
    let mut match_node = !(title_in_cont || time_in_cont);
    let mut cur_text = String::new();
    let mut para_list: Vec<String> = Vec::new();
    let mut in_code_tag = false;
    let mut is_end = false;

    traverse_content(
        doc, features, content_node, title_node, title_in_cont,
        raw_time_node, time_in_cont, &mut match_node, &mut cur_text,
        &mut para_list, &mut in_code_tag, &mut is_end, include_images,
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
    raw_time_node: Option<NodeId>,
    time_in_cont: bool,
    match_node: &mut bool,
    cur_text: &mut String,
    para_list: &mut Vec<String>,
    in_code_tag: &mut bool,
    is_end: &mut bool,
    include_images: bool,
) {
    if *is_end { return; }
    let p_tag_name = doc.node_name(node).unwrap_or("").to_lowercase();
    let mut li_index = 0usize;

    for child in doc.children(node) {
        if *is_end { break; }
        li_index += 1;
        let mut start = *match_node;

        // C++ TraverseContent: time_in_cont checked with priority; title_in_cont only when !time_in_cont
        if time_in_cont || title_in_cont {
            if time_in_cont {
                if let Some(tn) = raw_time_node {
                    // C++: child == time_node (exact match, not contains)
                    if child == tn {
                        *match_node = true;
                        start = true;
                        let tc = doc.text_content(tn).trim().len();
                        if tc < 150 { continue; }
                    }
                }
            // C++: else if title_in_cont (only reached when !time_in_cont)
            } else if title_in_cont {
                // C++: child == title_node (exact match)
                if child == title_node {
                    *match_node = true;
                    start = true;
                    // C++ intentionally uses time_node (not title_node) for length check
                    let tc = raw_time_node
                        .map(|tn| doc.text_content(tn).trim().len())
                        .unwrap_or(0);
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
                    // C++: IsEndText(node_content) && text_len_ > 256 && node_content.length() < 64
                    // C++ IsEndText: stop when end signal found and we have enough content,
                    // OR when the end signal IS the first content (no accumulated text yet)
                    if is_end_text(trimmed) && trimmed.len() < 20
                        && (text_len_ > 256 || cur_text.is_empty())
                    {
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
    // textarea: extract text content by stripping HTML tags (raw HTML inside)
    if tag == "textarea" {
        let raw = doc.text_content(child);
        let stripped = strip_html_tags(&raw).trim().to_string();
        if !stripped.is_empty() {
            if !cur_text.is_empty() { cur_text.push('\n'); }
            cur_text.push_str(&stripped);
        }
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

            // C++ IsEndClassValue: only rel_art_line
            if let Some(class_val) = doc.attribute(child, "class") {
                let lc = class_val.to_ascii_lowercase();
                if lc.contains("rel_art_line") {
                    *is_end = true;
                    break;
                }
            }
            // Check if cur_text ends with end signal (for text that happens at paragraph level)
            let trimmed_text = cur_text.trim();
            if trimmed_text.len() < 20 && is_end_text(trimmed_text)
                && (text_len_ > 256 || cur_text.is_empty())
            {
                *is_end = true;
                break;
            }
            // Newline tag: flush paragraph, handle images/videos
            if NEWLINE_TAGS.contains(&tag.as_str()) {
                let raw = cur_text.trim().to_string();
                if !raw.is_empty() {
                    let is_end_signal = is_end_text(raw.trim()) && text_len_ > 256 && raw.trim().len() < 20;
                    if *in_code_tag { para_list.push(raw); }
                    else { let fp = format_paragraph(&raw); if !fp.is_empty() { para_list.push(fp); } }
                    if is_end_signal {
                        cur_text.clear();
                        *is_end = true;
                        break;
                    }
                }

                // Handle img
                if tag == "img" && include_images {
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
                title_in_cont, raw_time_node, time_in_cont,
                match_node, cur_text, para_list, in_code_tag, is_end, include_images);

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
        let absolute = resolve_url(&s);
        if !absolute.is_empty() && is_valid_image_node(doc, img_node) {
            para_list.push(format!("![]({})", absolute));
        }
    }
}

/// C++ IsValidImageNode: if img is wrapped in <a>, require href to end with image extension
/// or title to contain "查看原图"/"查看图片". Otherwise reject.
fn is_valid_image_node(doc: &xmloxide::Document, img_node: NodeId) -> bool {
    let mut a_node = None;
    let mut cur = doc.parent(img_node);
    while let Some(p) = cur {
        if doc.is_element(p) {
            if doc.node_name(p).map_or(false, |n| n.eq_ignore_ascii_case("a")) {
                a_node = Some(p);
                break;
            }
        }
        cur = doc.parent(p);
    }
    let Some(a) = a_node else { return true; };
    let href = doc.attribute(a, "href");
    match href {
        Some(h) => {
            let lower = h.to_ascii_lowercase();
            if lower.ends_with(".jpg") || lower.ends_with(".jpeg")
                || lower.ends_with(".png") || lower.ends_with(".gif")
                || lower.ends_with(".webp")
            {
                return true;
            }
        }
        None => return false,
    }
    if let Some(title) = doc.attribute(a, "title") {
        if title.contains("查看原图") || title.contains("查看图片") {
            return true;
        }
    }
    false
}

fn resolve_url(s: &str) -> String {
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("//") {
        if s.starts_with("//") { format!("https:{}", s) } else { s.to_string() }
    } else {
        String::new()
    }
}

fn serialize_xml_node(doc: &xmloxide::Document, id: NodeId, out: &mut String) {
    if doc.is_element(id) {
        if let Some(name) = doc.node_name(id) {
            out.push('<');
            out.push_str(name);
            for attr in doc.attributes(id) {
                let val = attr.value.replace('"', "&quot;");
                out.push(' ');
                out.push_str(&attr.name);
                out.push_str("=\"");
                out.push_str(&val);
                out.push('"');
            }
            out.push('>');
            for child in doc.children(id) {
                serialize_xml_node(doc, child, out);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    } else if let Some(text) = doc.node_text(id) {
        out.push_str(text);
    }
}

fn get_table_text(doc: &xmloxide::Document, node: NodeId) -> String {
    let mut out = String::new();
    serialize_xml_node(doc, node, &mut out);
    out
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

fn is_user_card(doc: &xmloxide::Document, node: NodeId) -> bool {
    // C++ TraverseUserCard: subtree scan for has_a, has_i, has_name, has_img, has_zixun
    let mut has_a = false;
    let mut has_i = false;
    let mut has_name = false;
    let mut has_img = false;
    let mut has_zixun = false;
    let mut text = String::new();
    traverse_user_card(doc, node, &mut has_a, &mut has_i,
        &mut has_name, &mut has_img, &mut has_zixun, &mut text);

    if text.len() < 200 && has_a && has_name && has_img && has_zixun {
        return true;
    }
    if hit_user_attribute(doc, node) {
        return true;
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

/// Strip HTML tags from a string (e.g. textarea content).
fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut in_entity = false;
    let mut entity = String::new();
    for c in s.chars() {
        if in_tag {
            if c == '>' { in_tag = false; }
            continue;
        }
        if c == '<' { in_tag = true; continue; }
        if c == '&' { in_entity = true; entity.clear(); continue; }
        if in_entity {
            if c == ';' {
                // decode common entities
                let decoded = match entity.as_str() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "nbsp" | "#160" => " ",
                    "quot" => "\"",
                    _ => "",
                };
                out.push_str(decoded);
                in_entity = false;
            } else {
                entity.push(c);
            }
            continue;
        }
        out.push(c);
    }
    // Collapse whitespace
    let mut prev_space = false;
    let collapsed: String = out.chars().filter(|&c| {
        if c.is_whitespace() && (c == ' ' || c == '\n') {
            if prev_space { return false; }
            prev_space = true;
        } else {
            prev_space = false;
        }
        true
    }).collect();
    collapsed
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
