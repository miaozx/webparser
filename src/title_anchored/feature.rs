use std::collections::{HashMap, HashSet};
use xmloxide::NodeId;
use xmloxide::tree::NodeKind;

use super::end_signals;

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
    pub fn body_node_id(&self) -> NodeId {
        self.body_node_id
    }

    pub fn find_body(doc: &xmloxide::Document) -> Option<NodeId> {
        let root = doc.root_element()?;
        for child in doc.children(root) {
            if doc.node_name(child).map_or(false, |n| n.eq_ignore_ascii_case("body")) {
                return Some(child);
            }
        }
        Some(root)
    }

    pub fn has_discard_ancestor(&self, doc: &xmloxide::Document, id: NodeId) -> bool {
        let mut cur = doc.parent(id);
        while let Some(anc) = cur {
            if self.features.get(&anc).map_or(false, |f| f.is_discard_node) {
                return true;
            }
            if anc == self.body_node_id {
                break;
            }
            cur = doc.parent(anc);
        }
        false
    }

    pub fn build(doc: &xmloxide::Document) -> Self {
        let Some(body_node) = Self::find_body(doc) else {
            return Self {
                features: HashMap::new(),
                body_node_id: doc.root_element().unwrap_or(doc.root()),
                body_text_len: 0,
                body_exclude_a_text_len: 0,
            };
        };
        let body_node_id = body_node;

        let mut features: HashMap<NodeId, NodeFeature> = HashMap::new();

        let mut discard_node_set: HashSet<NodeId> = HashSet::new();

        // Phase 1 & 2: iterate all text nodes and bubble features up
        // C++: nonclickable_text_len for h-tag end-text early stop
        let mut nonclickable_text_len = 0usize;
        let mut text_node_count = 0usize;
        let mut stop = false;
        const MAX_TEXT_NODES: usize = 3000;

        for leaf in doc.descendants(body_node_id) {
            if !matches!(doc.node(leaf).kind, NodeKind::Text { .. }) {
                continue;
            }
            let text = doc.node_text(leaf).unwrap_or("");
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }

            // C++ 3000 node limit: stop after 3000 text nodes
            if stop {
                break;
            }
            text_node_count += 1;
            if text_node_count > MAX_TEXT_NODES {
                stop = true;
                break;
            }

            // C++ GetAllLeafNode skips script/style/link/noscript/select/option elements entirely.
            // Their text children must NOT contribute to features.
            let in_skip_tag = {
                let mut sk = doc.parent(leaf);
                let mut skip = false;
                while let Some(a) = sk {
                    if let Some(n) = doc.node_name(a) {
                        let t = n.to_lowercase();
                        if matches!(t.as_str(), "script" | "noscript" | "link" | "style" | "select" | "option") {
                            skip = true;
                            break;
                        }
                        if a == body_node_id { break; }
                    }
                    sk = doc.parent(a);
                }
                skip
            };
            if in_skip_tag { continue; }

            let text_len = trimmed.len();
            let visual_text_len = text_len as f64 / 3.0;

            // C++ GetAllLeafNode: is_in_a_tag and in_h_tag are recursive flags set at the
            // TEXT NODE level (not per ancestor). is_in_a_tag controls whether the text
            // contributes to nonclickable_text_len and exclude_a_text.
            // Pre-scan ancestors to determine if this text is inside an <a> tag.
            let is_text_in_a = {
                let mut cur = doc.parent(leaf);
                let mut found = false;
                while let Some(anc) = cur {
                    if let Some(n) = doc.node_name(anc) {
                        if n.eq_ignore_ascii_case("a") {
                            found = true;
                            break;
                        }
                    }
                    if anc == body_node_id { break; }
                    cur = doc.parent(anc);
                }
                found
            };
            // C++ nonclickable_text_len: accumulated once per text node (not per ancestor)
            if !is_text_in_a {
                nonclickable_text_len += text_len;
            }

            let mut current = doc.parent(leaf);
            // inside_h: tracked in ancestor loop because h-tag IS an ancestor of
            // the text node (same as C++ recursive in_h_tag flag for ancestors)
            let mut inside_h = false;
            let mut tag_a_cnt: usize = 0;  // C++: persists up the ancestor chain
            while let Some(anc) = current {
                let tag = doc.node_name(anc).unwrap_or("").to_lowercase();

                if tag == "a" {
                    tag_a_cnt = 1;  // C++: set hit_a_tag = true, tag_a_cnt = 1
                }
                if matches!(tag.as_str(), "h1" | "h2" | "h3" | "h4") {
                    inside_h = true;
                }

                let f = features.entry(anc).or_default();

                // C++: check discard_node_set first — if previously identified as discard,
                // accumulate features on this node but stop propagation to ancestors
                let already_discard = discard_node_set.contains(&anc);

                // C++ BuildNodeFeature: !text_feature.is_in_a_tag controls exclude_a_text
                // This is the TEXT NODE's property, not per-ancestor.
                f.text_nc += 1;
                f.text_len += text_len;
                f.tag_a_nc += tag_a_cnt;
                if !is_text_in_a {
                    f.exclude_a_text_nc += 1;
                    f.exclude_a_text_len += text_len;
                }

                if f.max_sub_text_len < visual_text_len {
                    f.max_sub_text_len = visual_text_len;
                }
                if !is_text_in_a && f.max_exclude_a_text_len < visual_text_len {
                    f.max_exclude_a_text_len = visual_text_len;
                }

                // C++: h-tag + end-text -> has_recomment_title + nonclickable_text_len > 128 stop
                if inside_h && end_signals::is_end_text(trimmed) {
                    f.has_recomment_title = true;
                    if nonclickable_text_len > 128 {
                        stop = true;
                        break;
                    }
                }

                if already_discard {
                    // C++: features accumulated, then break to stop propagation to ancestors
                    break;
                }

                // C++: check HitAttributeFilter (matches discard patterns)
                if check_discard_patterns(anc, doc, f, &tag) {
                    // Features already accumulated, add to discard set and stop propagation
                    discard_node_set.insert(anc);
                    break;
                }

                if stop {
                    break;
                }

                if anc == body_node_id {
                    break;
                }
                current = doc.parent(anc);
            }
        }

        // Phase 2b: image counting (C++ BuildImageFeature)
        for img_node in doc.descendants(body_node_id) {
            if !doc.is_element(img_node) {
                continue;
            }
            let tag = doc.node_name(img_node).unwrap_or("");
            if !tag.eq_ignore_ascii_case("img") {
                continue;
            }
            let mut clickable = false;
            let mut cur = doc.parent(img_node);
            while let Some(anc) = cur {
                if doc.is_element(anc) {
                    if doc.node_name(anc).map_or(false, |n| n.eq_ignore_ascii_case("a")) {
                        clickable = true;
                        break;
                    }
                }
                if anc == body_node_id {
                    break;
                }
                cur = doc.parent(anc);
            }
            // C++: start from the img node itself, walk up
            let mut cur2 = Some(img_node);
            while let Some(anc) = cur2 {
                if doc.is_element(anc) {
                    if let Some(f) = features.get_mut(&anc) {
                        if clickable {
                            f.click_image_count += 1;
                        } else {
                            f.nonclick_image_count += 1;
                        }
                    }
                }
                if anc == body_node_id {
                    break;
                }
                cur2 = doc.parent(anc);
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

    pub fn get(&self, id: NodeId) -> Option<&NodeFeature> {
        self.features.get(&id)
    }

    pub fn is_filter_node(&self, id: NodeId, doc: &xmloxide::Document) -> bool {
        // C++ IsFilterNode: IsListNode || HitAttributeFilter
        if self.is_list_node(id, doc) {
            return true;
        }
        if let Some(feat) = self.features.get(&id) {
            if feat.is_discard_node {
                return true;
            }
        }
        false
    }

    pub fn is_list_node(&self, id: NodeId, doc: &xmloxide::Document) -> bool {
        let Some(feat) = self.features.get(&id) else {
            return false;
        };
        // C++: text_nc >= 3 && exclude_a_text_nc == 0
        if feat.text_nc >= 3 && feat.exclude_a_text_nc == 0 {
            return true;
        }
        // C++: text_nc >= 5 && exclude_a_text_len < 20
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
        // C++: <ul> special check: li_count >= 3 && li_count == tag_a_nc
        if doc.node_name(id).map_or(false, |n| n.eq_ignore_ascii_case("ul")) {
            let mut li_count = 0usize;
            let mut tag_a_nc_in_li = 0usize;
            for child in doc.children(id) {
                if doc.is_element(child)
                    && doc.node_name(child).map_or(false, |n| n.eq_ignore_ascii_case("li"))
                {
                    li_count += 1;
                    if let Some(child_feat) = self.features.get(&child) {
                        if child_feat.tag_a_nc == 0 {
                            return false;
                        }
                        tag_a_nc_in_li += child_feat.tag_a_nc;
                    }
                }
            }
            if li_count >= 3 && li_count == tag_a_nc_in_li {
                return true;
            }
        }
        false
    }

    pub fn is_content_node(&self, id: NodeId, doc: &xmloxide::Document, body_exclude_len: usize,
        match_node: bool) -> bool {
        let Some(feat) = self.features.get(&id) else {
            return false;
        };
        let tag = doc.node_name(id).unwrap_or("").to_lowercase();
        // C++: exclude_a_text_len * 1.0 / (body_exclude_a_text_len + 1)
        let denom = (body_exclude_len + 1) as f64;
        let ratio = feat.exclude_a_text_len as f64 / denom;

        // C++ IsContentNode: text_len > 64 && HitContentAttribute && exclude_a_text/body_exclude > 0.35
        if feat.text_len > 64
            && hit_content_attribute(id, doc)
            && (feat.exclude_a_text_len as f64 / denom) > 0.35
        {
            return true;
        }

        // C++ negative: has_recomment_title && tag_a_nc > 3 && click_image_count > 3
        if match_node && feat.has_recomment_title && feat.tag_a_nc > 3 && feat.click_image_count > 3 {
            return false;
        }
        // C++ negative: match_node && tag_a_nc > 30 && click_image_count > 3
        if match_node && feat.tag_a_nc > 30 && feat.click_image_count > 3 {
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
        if match_node && ratio > 0.6 {
            return true;
        }

        false
    }
}

pub fn hit_content_attribute(id: NodeId, doc: &xmloxide::Document) -> bool {
    let tag = doc.node_name(id).unwrap_or("").to_lowercase();
    if tag != "section" && tag != "div" && tag != "main" && tag != "article" {
        return false;
    }
    // C++: exact == matches for short general keywords;
    // substring find() for compound patterns to avoid false positives.
    let exact_keywords = ["article", "cell", "story", "content", "post", "entry"];
    let substring_keywords = [
        "n18_art_content", "postlist", "wz_conten", "content-box",
        "answer-list", "main-column", "main-article",
        "post-text", "post-body", "art_content",
        "post-content", "post_content", "post-entry",
        "editorlightgallery", "postentry",
        "article-text", "articletext", "articleText",
        "entry-content", "article-content", "article_contont",
        "article__content", "article-body",
        "mainText", "article__body", "article_body",
        "articleBody", "articlebody",
        "post-bodycopy", "storycontent", "story-content",
        "postarea", "art-postcontent", "theme-content",
        "artibody", "blog-content", "section-content",
        "single-content", "single-post",
        "main-column", "articlecont",
        "wpb_text_column", "story-body", "field-body",
        "fulltext", "container-fluid", "bodyContent", "FULTEX",
        "art_pic_card art_content", "content-main",
        "main_content", "main-content", "content_main",
        "content-body", "contentBody", "content__body",
        "main-content", "page-content",
        "index_bbs-post-web-main",
    ];

    let check_attr = |lower: &str| -> bool {
        // Exact matches (C++ attr == "...")
        if exact_keywords.iter().any(|&kw| lower == kw) {
            return true;
        }
        // Substring matches (C++ attr.find("...") != npos)
        if substring_keywords.iter().any(|&kw| lower.contains(kw)) {
            return true;
        }
        // C++: attr.find("article ") == 0 (prefix match)
        if lower.starts_with("article ") {
            return true;
        }
        false
    };

    if let Some(class_val) = doc.attribute(id, "class") {
        if check_attr(&class_val.to_ascii_lowercase()) {
            return true;
        }
    }
    if let Some(id_val) = doc.attribute(id, "id") {
        if check_attr(&id_val.to_ascii_lowercase()) {
            return true;
        }
    }
    false
}

pub fn is_visible_node(id: NodeId, doc: &xmloxide::Document) -> bool {
    if let Some(style) = doc.attribute(id, "style") {
        let lower = style.to_ascii_lowercase();
        if lower.contains("display:none") || lower.contains("display: none") {
            return false;
        }
    }
    true
}

pub fn has_child_table(id: NodeId, doc: &xmloxide::Document) -> bool {
    for child in doc.descendants(id) {
        if child == id {
            continue;
        }
        if doc.is_element(child) {
            if doc.node_name(child).map_or(false, |n| n.eq_ignore_ascii_case("table")) {
                return true;
            }
        }
    }
    false
}

pub fn is_nav_header_by_node(id: NodeId, doc: &xmloxide::Document, body_id: NodeId,
    features: Option<&FeatureTree>) -> bool {
    if let Some(tag) = doc.node_name(id) {
        let tag = tag.to_lowercase();
        if let Some(feats) = features {
            if let Some(feat) = feats.get(id) {
                if let Some(body_feat) = feats.get(body_id) {
                    if body_feat.text_len > 0
                        && (feat.text_len as f64 / body_feat.text_len as f64) > 0.80
                    {
                        return false;
                    }
                }
            }
        }
        if tag == "nav" {
            return true;
        }
    }
    if let Some(class_val) = doc.attribute(id, "class") {
        let lower = class_val.to_ascii_lowercase();
        if lower == "nav" || lower == "menu_nav" || lower == "main_nav"
            || lower == "logo" || lower == "navbar-header"
        {
            return true;
        }
    }
    if let Some(id_val) = doc.attribute(id, "id") {
        if id_val == "MainMenu" {
            return true;
        }
    }
    false
}

// ---- Helper functions for pattern matching ----

fn check_discard_patterns(anc: NodeId, doc: &xmloxide::Document, f: &mut NodeFeature, tag: &str) -> bool {
    let mut discard = f.is_discard_node;
    let class_val = doc.attribute(anc, "class").map(|s| s.to_ascii_lowercase());
    let id_val = doc.attribute(anc, "id").map(|s| s.to_ascii_lowercase());

    // Check class patterns
    if let Some(ref cls) = class_val {
        if contains_recomment(cls) {
            f.has_recomment_title = true;
        }
        if matches_discard_class(cls) {
            discard = true;
        }
        if matches!(tag, "div" | "p" | "section" | "span" | "ul") {
            if matches_hit_attr_value(tag, cls) || has_suffix_match(cls) || has_prefix_match(cls) {
                discard = true;
            }
        }
        if cls.starts_with("comments") || cls.starts_with("Comments")
            || cls.starts_with("comment-") || cls.starts_with("dsq-comments")
            || cls.starts_with("disqus_thread")
        {
            discard = true;
        }
    }

    // Check id patterns
    if let Some(ref id) = id_val {
        if contains_recomment(id) {
            f.has_recomment_title = true;
        }
        if matches_discard_id(id) {
            discard = true;
        }
        if matches!(tag, "div" | "p" | "section" | "span" | "ul") {
            if matches_hit_attr_value(tag, id) || has_suffix_match(id) || has_prefix_match(id) {
                discard = true;
            }
        }
        if id.starts_with("comments") || id.starts_with("Comments")
            || id.starts_with("comment-") || id.starts_with("dsq-comments")
            || id.starts_with("disqus_thread")
        {
            discard = true;
        }
    }

    // C++: specific tag-based discard
    if tag == "footer" || tag == "select" {
        discard = true;
    }

    // display:none check (C++ IsVisibleNode)
    if let Some(style_val) = doc.attribute(anc, "style") {
        let s = style_val.to_ascii_lowercase();
        if s.contains("display:none") || s.contains("display: none") {
            discard = true;
        }
    }

    f.is_discard_node = discard;
    discard
}

fn contains_recomment(s: &str) -> bool {
    let patterns = [
        "recomment", "related", "relevant", "ad", "advertisement",
        "sponsor", "recommend", "hot", "popular", "trending",
        "tuijian", "remen", "xiangguan",
    ];
    patterns.iter().any(|p| s.contains(p))
}

fn matches_discard_class(s: &str) -> bool {
    let patterns = [
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
        // Common C++ discard patterns (from discard_common_patterns, was dead code)
        "sidebar", "side", "menu", "navbar", "navbox",
        "navigation", "subnav", "toolbar", "topbar",
        "rating", "tag-list", "tags", "categories",
        "byline", "dateline", "timestamp", "breadcrumb",
        "advertisement", "banner", "cookie", "consent",
        "privacy", "newsletter", "subscription", "social",
        "crumb", "overlay", "modal", "popup",
        "interestlist", "hotsearch", "hotnews",
        "syndication", "user-info", "user-profile",
        "article-infos", "go_top", "feedback",
    ];
    patterns.iter().any(|p| s.contains(p))
}

fn matches_discard_id(s: &str) -> bool {
    let patterns = [
        "comments", "comment-", "dsq-comments", "disqus_thread",
        "login", "search", "subscribe",
    ];
    patterns.iter().any(|p| s.contains(p) || s.starts_with(p))
}

fn matches_hit_attr_value(tag: &str, attr: &str) -> bool {
    // C++ HitAttributeValue logic for div/p/section/span/ul
    // C++ comment prefix patterns
    if attr.starts_with("comments") || attr.starts_with("Comments")
        || attr.starts_with("comment-") || attr.starts_with("dsq-comments")
        || attr.starts_with("disqus_thread")
    {
        return true;
    }
    // C++ substring contains patterns (attr.find(...) != npos) for div/p/section/span/ul
    let substring_patterns = [
        "related", "viral", "social", "sociable", "-relevant",
        "relative_news", "relative_special", "relative_expert",
        "recommend news-list", "footnav", "footbg", "_foot_", "_popup ",
        "rec-list", "recommend-list", "recommend-box",
        "categories", "MainHeader", "syndication", "dpsp-content",
        "embedded", "embed", "newsletter",
        "subnav", "cookie", "tag-list",
        "banner", "avigation", "navbar", "navbox",
        "byline", "rating", "attachment", "timestamp",
        "user-info", "user-profile", "-ad-", "-icon",
        "article-infos", "nfoline", "go_top",
        "shengming", "mzsmcontent", "articlepraise",
        "outbrain", "taboola", "criteo",
        "options", "modal-content",
        "permission", "most-popular",
        "mol-factbox", "message-container", "zlylin", "bmdh",
        "premium", "paid-content", "paidcontent",
        "blurred", "obfuscated", "comments-title", "nocomments",
        "search-condition", "-reply-", "hotsearch", "hotnews",
        "reader-comments", "akismet", "-nav-", "_nav-",
        "suggest-links", "go-home", "sharebox",
        "interestlist", "pagenav", "searchbox",
        "xglinks", "qrcode", "feedback", "tab-list",
        " ad ", "consent", "daohang",
        // author/tags/menu patterns (C++ also checks these for div/p/section/span/ul)
        "author", "tags", "menu",
    ];
    for pat in &substring_patterns {
        if attr.contains(pat) {
            return true;
        }
    }
    // C++ EndsWith patterns for div/p/section/span/ul
    let suffix_patterns = [
        "-nav", "_nav", "-login", "-logout",
        "_statement", "-comments", "_DOWNLOAD",
        "_header", "Nav",
    ];
    for suffix in &suffix_patterns {
        if attr.ends_with(suffix) {
            return true;
        }
    }
    // C++ StartsWith patterns
    if attr.starts_with("login") || attr.starts_with("ZendeskForm")
        || attr.starts_with("post-nav")
    {
        return true;
    }
    // C++ exact matches for div/p/section/span/ul
    let exact_matches = [
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
        "search", "nav", "download", "statement",
        "contact", "weixin", "menu", "wnav",
        "foot", "whead", "weibo", "login",
    ];
    if exact_matches.contains(&attr) {
        return true;
    }
    false
}

fn has_suffix_match(attr: &str) -> bool {
    // C++ EndsWith patterns
    let suffixes = [
        "-nav", "_nav", "-login", "-logout",
        "_statement", "-comments", "_DOWNLOAD",
        "_header", "Nav",
    ];
    for suffix in suffixes {
        if attr.ends_with(suffix) {
            return true;
        }
    }
    // C++ startsWith patterns for div/p/section/span/ul
    if attr.starts_with("login") || attr.starts_with("login")
        || attr.starts_with("ZendeskForm") || attr.starts_with("post-nav")
    {
        return true;
    }
    false
}

fn has_prefix_match(attr: &str) -> bool {
    attr.starts_with("login") || attr.starts_with("ZendeskForm") || attr.starts_with("post-nav")
}
