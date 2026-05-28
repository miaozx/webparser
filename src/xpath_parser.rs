//! XPath-based content parser.
//!
//! Uses `xmloxide` (pure Rust libxml2 reimplementation) for native XPath
//! evaluation, supporting the full XPath 1.0 syntax used in config rules
//! (`following-sibling`, `preceding-sibling`, `/text()`, `/..`, etc.).

use crate::dom::{self, Document as DomDocument, Selection};
use crate::xpath_config::XpathConfigField;
use crate::Options;
use xmloxide::html5::parse_html5;
use xmloxide::xpath::{evaluate, XPathValue};
use xmloxide::Document as XmlDocument;

/// Result of xpath-based content extraction.
pub struct XpathParseResult {
    pub content_text: String,
    pub content_html: String,
    pub title: Option<String>,
    pub publish_time: Option<String>,
    pub author: Option<String>,
    pub description: Option<String>,
}

/// Parse HTML using xpath rules matched to the given URL.
/// Parses the HTML internally.
pub fn parse_with_xpath(
    html: &str,
    url: &str,
    fields: &[XpathConfigField],
    options: &Options,
) -> Option<XpathParseResult> {
    let doc = parse_html5(html).ok()?;
    parse_with_xpath_doc(&doc, url, fields, options)
}

/// Parse using xpath rules with a pre-parsed xmloxide Document.
/// Allows sharing the parsed document with title_anchored extraction.
pub fn parse_with_xpath_doc(
    doc: &XmlDocument,
    _url: &str,
    fields: &[XpathConfigField],
    options: &Options,
) -> Option<XpathParseResult> {
    if fields.is_empty() {
        return None;
    }

    let root = doc.root_element()?;

    let title = extract_metadata_field_xml(doc, root, fields, "title");
    let publish_time = extract_metadata_field_xml(doc, root, fields, "publish_time");
    let author = extract_metadata_field_xml(doc, root, fields, "author");
    let description = extract_metadata_field_xml(doc, root, fields, "description");

    let content_field = fields.iter().find(|f| f.field_name == "content")?;
    let (content_text, content_html) = extract_content_xml(doc, root, content_field, options)?;

    Some(XpathParseResult {
        content_text,
        content_html,
        title: title.filter(|s| !s.is_empty()),
        publish_time: publish_time.filter(|s| !s.is_empty()),
        author: author.filter(|s| !s.is_empty()),
        description: description.filter(|s| !s.is_empty()),
    })
}

/// Evaluate xpath string and return matched nodes.
fn xpath_nodes(doc: &XmlDocument, ctx: xmloxide::NodeId, expr: &str) -> Option<Vec<xmloxide::NodeId>> {
    let result = evaluate(doc, ctx, expr).ok()?;
    match result {
        XPathValue::NodeSet(nodes) => Some(nodes),
        XPathValue::String(_) | XPathValue::Number(_) | XPathValue::Boolean(_) => None,
    }
}

/// Evaluate xpath and return the text content of the first matched node.
fn xpath_text(doc: &XmlDocument, ctx: xmloxide::NodeId, expr: &str) -> Option<String> {
    let result = evaluate(doc, ctx, expr).ok()?;
    match result {
        XPathValue::NodeSet(nodes) => {
            let id = *nodes.first()?;
            // For element nodes, use text_content; for attribute/text nodes, use node_text.
            if doc.is_element(id) {
                Some(doc.text_content(id))
            } else {
                doc.node_text(id).map(|s| s.to_string())
            }
        }
        XPathValue::String(s) => Some(s),
        XPathValue::Number(n) => Some(n.to_string()),
        XPathValue::Boolean(b) => Some(b.to_string()),
    }
}

/// Extract a metadata field (title, publish_time, author, description).
fn extract_metadata_field_xml(
    doc: &XmlDocument,
    ctx: xmloxide::NodeId,
    fields: &[XpathConfigField],
    field_name: &str,
) -> Option<String> {
    let field = fields.iter().find(|f| f.field_name == field_name)?;

    for xpath in &field.xpath_list {
        // Check for attribute extraction: `//meta/@content`
        if let Some(attr_pos) = xpath.rfind("/@") {
            let node_xpath = &xpath[..attr_pos];
            let attr_name = &xpath[attr_pos + 2..];
            if let Some(nodes) = xpath_nodes(doc, ctx, node_xpath) {
                if field.single_node {
                    if let Some(id) = nodes.first() {
                        if let Some(val) = doc.attribute(*id, attr_name) {
                            let trimmed = val.trim().to_string();
                            if !trimmed.is_empty() {
                                return Some(trimmed);
                            }
                        }
                    }
                } else {
                    let mut values = Vec::new();
                    for id in &nodes {
                        if let Some(val) = doc.attribute(*id, attr_name) {
                            let trimmed = val.trim().to_string();
                            if !trimmed.is_empty() {
                                values.push(trimmed);
                            }
                        }
                    }
                    if !values.is_empty() {
                        return Some(values.join(", "));
                    }
                }
            }
        } else {
            // Text content extraction.
            if let Some(text) = xpath_text(doc, ctx, xpath) {
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
        }
    }
    None
}

/// Extract content field: find node by xpath, apply filter_xpath, extract text/html.
fn extract_content_xml(
    doc: &XmlDocument,
    ctx: xmloxide::NodeId,
    field: &XpathConfigField,
    options: &Options,
) -> Option<(String, String)> {
    // Try each xpath in order.
    for xpath in &field.xpath_list {
        let nodes = xpath_nodes(doc, ctx, xpath)?;
        if nodes.is_empty() {
            continue;
        }

        // Get the first matching node.
        let node_id = nodes[0];
        let node_html = node_to_html(doc, node_id);

        // Skip if content is too small.
        let text_len = doc.text_content(node_id).trim().len();
        if text_len < 10 {
            continue;
        }

        // Apply filter_xpath exclusions.
        if !field.filter_xpath_list.is_empty() {
            let filtered_html = apply_filter_xpath_html(&node_html, &field.filter_xpath_list);
            // Parse the filtered HTML with dom_query for text extraction.
            let dom_doc = DomDocument::from(filtered_html.as_str());
            let body = dom_doc.select("body > *");
            let text = if body.length() > 0 {
                crate::extract::extract_filtered_text(&body, options)
            } else {
                dom_doc.text().trim().to_string()
            };
            let html = if body.length() > 0 {
                crate::extract::extract_filtered_html(&body, options)
            } else {
                filtered_html
            };
            if text.len() >= 10 {
                return Some((text, html));
            }
        } else {
            // Use raw text content directly (avoids link-density filtering issues
            // that can strip inline-linked content like baidu health answers).
            let text = doc.text_content(node_id).trim().to_string();
            let text_len = text.len();
            if text_len >= 10 {
                return Some((text, node_html.to_string()));
            }
        }
    }
    None
}

/// Serialize an xmloxide node and its descendants to an HTML string.
fn node_to_html(doc: &XmlDocument, node_id: xmloxide::NodeId) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    serialize_node(doc, node_id, &mut out);
    out
}

fn serialize_node(doc: &XmlDocument, id: xmloxide::NodeId, out: &mut String) {
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
                serialize_node(doc, child, out);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
    } else if let Some(text) = doc.node_text(id) {
        out.push_str(text);
    }
}

/// Apply filter_xpath selectors using xmloxide for native xpath evaluation.
/// Parses the node HTML with xmloxide, finds and removes matching nodes,
/// then serializes back to HTML for dom_query processing.
fn apply_filter_xpath_html(html: &str, filter_xpaths: &[String]) -> String {
    // Parse with xmloxide for proper xpath evaluation.
    let mut filter_doc = match parse_html5(html) {
        Ok(d) => d,
        Err(_) => return html.to_string(),
    };
    let filter_root = match filter_doc.root_element() {
        Some(r) => r,
        None => return html.to_string(),
    };

    for xpath in filter_xpaths {
        // Collect node IDs in a block so the immutable borrow of filter_doc
        // ends before the mutable remove_node call.
        let to_remove: Vec<xmloxide::NodeId> = {
            match evaluate(&filter_doc, filter_root, xpath) {
                Ok(XPathValue::NodeSet(nodes)) => nodes,
                _ => continue,
            }
        };
        for id in to_remove {
            filter_doc.remove_node(id);
        }
    }

    // Serialize remaining content back to HTML.
    let result_html = node_to_html(&filter_doc, filter_root);
    if result_html.trim().is_empty() {
        html.to_string()
    } else {
        result_html
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xmloxide_xpath_basic() {
        let html = r#"<html><body><div class="content"><p>hello</p></div></body></html>"#;
        let doc = parse_html5(html).unwrap();
        let root = doc.root_element().unwrap();
        let nodes = xpath_nodes(&doc, root, "//*[@class='content']").unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(doc.text_content(nodes[0]).trim(), "hello");
    }

    #[test]
    fn test_xpath_text_content() {
        let html = r#"<html><body><h1 class="title">My Title</h1></body></html>"#;
        let doc = parse_html5(html).unwrap();
        let root = doc.root_element().unwrap();
        let text = xpath_text(&doc, root, "//*[@class='title']").unwrap();
        assert_eq!(text.trim(), "My Title");
    }

    #[test]
    fn test_xpath_attribute() {
        let html = r#"<html><head><meta property="og:title" content="Test Page"/></head></html>"#;
        let doc = parse_html5(html).unwrap();
        let root = doc.root_element().unwrap();
        let text = xpath_text(&doc, root, "//meta[@property='og:title']/@content").unwrap();
        assert_eq!(text.trim(), "Test Page");
    }

}
