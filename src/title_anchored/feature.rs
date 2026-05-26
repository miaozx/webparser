use std::collections::HashMap;
use dom_query::{Document, NodeId, NodeRef, Selection};

#[derive(Default, Clone)]
pub struct NodeFeature {
    pub text_nc: usize,
    pub exclude_a_text_nc: usize,
    pub exclude_a_text_len: usize,
    pub text_len: usize,
    pub tag_a_nc: usize,
    pub has_recomment_title: bool,
    pub is_discard_node: bool,
    pub max_exclude_a_text_len: f64,
    pub max_sub_text_len: f64,
    pub click_image_count: usize,
    pub nonclick_image_count: usize,
}

pub struct FeatureTree {
    features: HashMap<NodeId, NodeFeature>,
    body_node_id: NodeId,
    pub body_text_len: usize,
    pub body_exclude_a_text_len: usize,
}

impl FeatureTree {
    pub fn build(doc: &Document) -> Self {
        let body_node = doc.body().unwrap_or_else(|| doc.root());
        let body_node_id = body_node.id;
        
        let mut features: HashMap<NodeId, NodeFeature> = HashMap::new();

        // Phase 1: bubble text leaf features up to all ancestor elements
        for node in body_node.descendants() {
            if !node.is_text() {
                continue;
            }
            let sel = Selection::from(node);
            let text = sel.text();
            let text_len = text.trim().len();
            if text_len == 0 {
                continue;
            }

            let mut current = node.parent();
            let mut inside_a = false;
            while let Some(anc) = current {
                if anc.is_element() {
                    let f = features.entry(anc.id).or_default();
                    f.text_nc += 1;
                    f.text_len += text_len;
                    if !inside_a {
                        f.exclude_a_text_nc += 1;
                        f.exclude_a_text_len += text_len;
                    }
                    if anc.node_name().map_or(false, |n| n.eq_ignore_ascii_case("a")) {
                        inside_a = true;
                    }
                }
                if anc.id == body_node_id {
                    break;
                }
                current = anc.parent();
            }
        }

        // Phase 2: count <a> descendants and check discard/recomment patterns
        let recomment_patterns = [
            "recomment", "related", "relevant", "ad", "advertisement",
            "sponsor", "recommend", "hot", "popular", "trending",
            "tuijian", "remen", "xiangguan",
        ];
        let discard_class_patterns = [
            "excellent_articles_box", "s-tip js-open-app", "photo-bar",
            "flex-content", "post-loop post-loop-list", "noLogin",
            "nav1", "login-page", "nav2", "nav3",
            "relevant_ask", "btn-zan", "article-content-items",
            "cms-art-series-con-new", "download_card", "_download_",
            "download-box", "article-donate", "views-num",
            "emoji-panel", "panel panel-default lasest-update",
            "ask-question-relate", "technician card", "more-article",
            "copynotice", "copyright", "login-box",
            "foot-template", "announce", "m-photo ",
            "erx-sidelist-related", "select-city",
            "footer", "Footer", "city-box",
            "navitems", "nav-list", "comment-page", "comment-list",
            "rmt-mobile-comment", "comment-respond", "b_comment_main",
            "pos_commentlist", "comments-content", "post-comments",
            "article-comments",
            "dianzan", "share", "main-catalog", "art_share",
            "articlesharebox", "headlist", "mainnav",
            "header-blue", "close", "head", "header",
            "main-nav", "comments", "nav",
            "wg-site-header", "navnode",
            "post-operate-comp main-operate", "site-keywords",
            "bottom", "global-nav", "imgtitle",
            "Detail_Statement", "relatedNews", "imgcenter",
            "footerBao", "newdigg", "tablep",
            "bdsshare", "avow", "cp-mod",
            "download-btn", "wpcom_myimg_wrap", "lv-query mt80",
            "fengxiang", "js-doctor-home", "js-consult-doctor-home",
            "entry-copyright", "add-wechat add-abox",
            "js_qrcode_img js_share_qrcode", "copy-con",
            "diff_removed", "diff_added",
        ];
        let discard_id_patterns = [
            "footer", "copyright", "foot",
            "header",
            "sidebar",
            "comment",
        ];

        // C++ HitAttributeValue patterns for div/p/section/span/ul tags
        let discard_common_patterns: &[&str] = &[
            "nav", "navbar", "navbox", "navigation", "subnav",
            "menu", "main_menu", "main_nav", "global-nav",
            "login", "logout", "signin", "signup", "register",
            "search", "searchbox", "search-condition",
            "share", "sharebox", "social", "sociable",
            "comment", "reply", "feedback",
            "footer", "foot", "bottom",
            "header", "head", "topbar", "toolbar",
            "sidebar", "side",
            "copyright", "copynotice", "statement",
            "cookie", "consent", "privacy",
            "banner", "ad-", "-ad-", "advertisement",
            "tags", "tag-list", "categories",
            "author", "byline", "timestamp", "dateline",
            "rating", "button", "download",
            "related", "recommend", "suggest",
            "popular", "trending", "hot",
            "outbrain", "taboola", "criteo",
            "newsletter", "subscription",
            "breadcrumb", "crumb",
            "overlay", "modal", "popup",
            "qrcode", "weixin", "weibo",
            "pagenav", "page-nav",
            "interestlist", "hotsearch", "hotnews",
        ];

        for node in body_node.descendants() {
            if !node.is_element() {
                continue;
            }
            if node.id == body_node_id {
                continue;
            }

            let tag = node.node_name().unwrap_or_default().to_lowercase();

            // Also check discard_common_patterns for div/p/section/span/ul tags (C++ HitAttributeValue)
            if matches!(tag.as_str(), "div" | "p" | "section" | "span" | "ul") {
                let sel_tmp = Selection::from(node);
                if let Some(class) = sel_tmp.attr("class") {
                    let lower = class.to_ascii_lowercase();
                    for pat in discard_common_patterns {
                        if lower.contains(pat) {
                            features.entry(node.id).or_default().is_discard_node = true;
                            break;
                        }
                    }
                }
            }
            // footer/select tags always discard (C++ HitAttributeValue)
            if matches!(tag.as_str(), "footer" | "select") {
                features.entry(node.id).or_default().is_discard_node = true;
            }

            let tag_a_nc = count_a_descendants(node);
            if tag_a_nc > 0 {
                features.entry(node.id).or_default().tag_a_nc = tag_a_nc;
            }

            let sel = Selection::from(node);
            if check_recomment(&sel, &recomment_patterns) {
                features.entry(node.id).or_default().has_recomment_title = true;
            }
            if check_discard(&sel, &discard_class_patterns, &discard_id_patterns) {
                features.entry(node.id).or_default().is_discard_node = true;
            }

            // C++ IsVisibleNode: check display:none
            if !is_visible_node(&sel) {
                features.entry(node.id).or_default().is_discard_node = true;
            }
        }

        // Phase 2b: image counting (C++ BuildImageFeature)
        for img_node in body_node.descendants() {
            if !img_node.is_element() {
                continue;
            }
            let tag = img_node.node_name().unwrap_or_default();
            if !tag.eq_ignore_ascii_case("img") {
                continue;
            }
            // Check if img is inside <a> tag
            let mut clickable = false;
            let mut cur = img_node.parent();
            while let Some(anc) = cur {
                if anc.is_element() {
                    if let Some(atag) = anc.node_name() {
                        if atag.eq_ignore_ascii_case("a") {
                            clickable = true;
                            break;
                        }
                    }
                }
                if anc.id == body_node_id {
                    break;
                }
                cur = anc.parent();
            }
            // Bubble up image count to all ancestors
            let mut cur2 = img_node.parent();
            while let Some(anc) = cur2 {
                if anc.is_element() {
                    if let Some(f) = features.get_mut(&anc.id) {
                        if clickable {
                            f.click_image_count += 1;
                        } else {
                            f.nonclick_image_count += 1;
                        }
                    }
                }
                if anc.id == body_node_id {
                    break;
                }
                cur2 = anc.parent();
            }
        }

        // Phase 3: post-order compute max_* from children
        let all_elements: Vec<NodeRef> = body_node
            .descendants()
            .into_iter()
            .filter(|n| n.is_element() && n.id != body_node_id)
            .collect();

        let mut parent_to_children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &all_elements {
            if let Some(parent) = node.parent() {
                parent_to_children.entry(parent.id).or_default().push(node.id);
            }
        }

        // Process bottom-up (reverse order = children before parents)
        for node in all_elements.iter().rev() {
            let children = parent_to_children.get(&node.id).cloned().unwrap_or_default();
            let child_features: Vec<&NodeFeature> = children
                .iter()
                .filter_map(|cid| features.get(cid))
                .collect();

            if child_features.is_empty() {
                continue;
            }

            let max_text = child_features
                .iter()
                .map(|f| f.text_len as f64)
                .fold(0.0f64, f64::max);
            let max_ex = child_features
                .iter()
                .map(|f| f.exclude_a_text_len as f64)
                .fold(0.0f64, f64::max);

            if let Some(f) = features.get_mut(&node.id) {
                f.max_sub_text_len = max_text;
                f.max_exclude_a_text_len = max_ex;
            }
        }

        let body_feat = features.get(&body_node_id).cloned().unwrap_or_default();
        Self {
            features,
            body_node_id,
            body_text_len: body_feat.text_len,
            body_exclude_a_text_len: body_feat.exclude_a_text_len,
        }
    }

    pub fn get(&self, node: &Selection) -> Option<&NodeFeature> {
        let id = node.nodes().first()?.id;
        self.features.get(&id)
    }

    pub fn get_by_id(&self, id: NodeId) -> Option<&NodeFeature> {
        self.features.get(&id)
    }

    pub fn is_filter_node(&self, node: &Selection) -> bool {
        let Some(feat) = self.get(node) else {
            return false;
        };
        if feat.is_discard_node {
            return true;
        }
        if feat.has_recomment_title {
            return true;
        }
        if self.is_list_node(node) {
            return true;
        }
        false
    }

    pub fn is_list_node(&self, node: &Selection) -> bool {
        let Some(feat) = self.get(node) else {
            return false;
        };
        let total_len = feat.text_len.max(1);
        if feat.tag_a_nc > 0 && feat.tag_a_nc as f64 / total_len as f64 > 0.3 {
            let text_density = if total_len > 0 {
                feat.text_nc as f64 / total_len as f64
            } else {
                0.0
            };
            if text_density > 0.0 && text_density < 0.5 {
                return true;
            }
        }
        false
    }

    pub fn is_high_score_node(&self, node: &Selection) -> bool {
        let Some(feat) = self.get(node) else {
            return false;
        };
        if feat.exclude_a_text_len < 200 {
            return false;
        }
        let body_ratio = self.body_exclude_a_text_len.max(1);
        (feat.exclude_a_text_len as f64 / body_ratio as f64) > 0.02
    }

    pub fn content_score(&self, node: &Selection) -> f64 {
        let Some(feat) = self.get(node) else {
            return 0.0;
        };
        let mut score = feat.exclude_a_text_len as f64;

        // Penalize link density
        if feat.tag_a_nc > 0 && feat.text_len > 0 {
            let link_density = feat.tag_a_nc as f64 / feat.text_len as f64;
            if link_density > 0.5 {
                score *= 0.1;
            } else if link_density > 0.2 {
                score *= 0.5;
            }
        }

        // Penalize discard/recomment nodes
        if feat.is_discard_node {
            score *= 0.01;
        }
        if feat.has_recomment_title {
            score *= 0.05;
        }

        score
    }
}

fn count_a_descendants(node: NodeRef) -> usize {
    let mut count = 0;
    for desc in node.descendants() {
        if desc.id == node.id {
            continue;
        }
        if desc.is_element() && desc.node_name().map_or(false, |n| n.eq_ignore_ascii_case("a")) {
            count += 1;
        }
    }
    count
}

/// Simplified IsNavHeader for time search (no features needed)
pub fn is_nav_header_by_node(node: &dom_query::NodeRef) -> bool {
    if let Some(tag) = node.node_name() {
        let lower = tag.to_lowercase();
        if lower == "nav" {
            return true;
        }
    }
    let sel = Selection::from(*node);
    if let Some(class) = sel.attr("class") {
        let lower = class.to_ascii_lowercase();
        if lower == "nav" || lower == "menu_nav" || lower == "main_nav"
            || lower == "logo" || lower == "navbar-header"
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

pub fn is_visible_node(sel: &Selection) -> bool {
    if let Some(style) = sel.attr("style") {
        let lower = style.to_ascii_lowercase();
        if lower.contains("display:none") || lower.contains("display: none") {
            return false;
        }
    }
    true
}

/// C++ HasChildTable: check if a <table> has nested child <table>
pub fn has_child_table(sel: &Selection) -> bool {
    let Some(root) = sel.nodes().first() else {
        return false;
    };
    for node in root.descendants() {
        if node.id == root.id {
            continue;
        }
        if node.is_element() {
            if let Some(tag) = node.node_name() {
                if tag.eq_ignore_ascii_case("table") {
                    return true;
                }
            }
        }
    }
    false
}

fn check_recomment(sel: &Selection, patterns: &[&str]) -> bool {
    let class = sel.attr("class").unwrap_or_default().to_lowercase();
    let id = sel.attr("id").unwrap_or_default().to_lowercase();
    patterns.iter().any(|p| class.contains(p) || id.contains(p))
}

fn check_discard(sel: &Selection, class_patterns: &[&str], id_patterns: &[&str]) -> bool {
    let class = sel.attr("class").unwrap_or_default().to_lowercase();
    let id = sel.attr("id").unwrap_or_default().to_lowercase();
    class_patterns.iter().any(|p| class.contains(p))
        || id_patterns.iter().any(|p| id.contains(p))
}
