//! HTML Preprocessing for Special Sites
//!
//! Some websites embed content in non-HTML formats (JavaScript state objects,
//! JSON within textareas, CSS-generated content, etc.). These preprocessors
//! extract the content and return clean HTML for the standard pipeline.

use base64::Engine as _;

/// Extract article content from 36kr pages.
///
/// 36kr embeds article content in `<script>window.initialState=...`.
/// Older versions used base64 + AES-128-ECB encryption; newer versions
/// store the data directly as plain JSON in the initial state.
pub fn extract_36kr_content(html: &str) -> Option<String> {
    let needle = "window.initialState=";
    let start_pos = html.find(needle)?;

    let after_start = &html[start_pos + needle.len()..];
    let end_pos = after_start.find("</script>")?;
    let json_str = after_start[..end_pos].trim();

    if json_str.is_empty() {
        return None;
    }

    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Navigate to the article data — same path for both old and new formats
    let article_detail = data.get("articleDetail")?;
    let article_detail_data = article_detail.get("articleDetailData")?;
    let data_obj = article_detail_data.get("data")?;

    let widget_content = data_obj.get("widgetContent")?.as_str()?;
    if widget_content.is_empty() {
        // New format didn't have content inline; try old encrypted format
        let state = data.get("state")?.as_str()?;
        let engine = base64::engine::general_purpose::STANDARD;
        let decoded = engine.decode(state).ok()?;
        let key: &[u8; 16] = b"efabccee-b754-4c";
        let decrypted = aes_128_ecb_decrypt(key, &decoded)?;
        let decrypted_str = String::from_utf8(decrypted).ok()?;
        let decode_json: serde_json::Value = serde_json::from_str(&decrypted_str).ok()?;
        let old_article_detail = decode_json.get("articleDetail")?;
        let old_data = old_article_detail
            .get("articleDetailData")?
            .get("data")?;
        let widget_content = old_data.get("widgetContent")?.as_str()?;
        let title = old_data
            .get("widgetTitle")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let publish_time_str = old_data
            .get("publishTime")
            .and_then(|v| v.as_i64())
            .map(|ts| {
                let secs = ts / 1000;
                chrono::DateTime::from_timestamp(secs, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        return Some(format!(
            r#"<html><body><h1>{title}</h1><div><p class="time">{publish_time_str}</p><div class="content">{widget_content}</div></div></body></html>"#
        ));
    }

    let title = data_obj
        .get("widgetTitle")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let publish_time_str = data_obj
        .get("publishTime")
        .and_then(|v| v.as_i64())
        .map(|ts| {
            let secs = ts / 1000;
            chrono::DateTime::from_timestamp(secs, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default()
        })
        .unwrap_or_default();

    let html_target = format!(
        r#"<html><body><h1>{title}</h1><div><p class="time">{publish_time_str}</p><div class="content">{widget_content}</div></div></body></html>"#
    );

    Some(html_target)
}

/// Extract poem content from Baidu Hanyu pages.
///
/// Baidu Hanyu embeds poem data in a `<textarea class="poem-body-content-value">`
/// as a JSON array. The JSON contains the poem body, title, author, dynasty,
/// annotations, translations, analysis, and background information.
pub fn extract_baidu_hanyu_content(html: &str) -> Option<String> {
    let start_marker = "poem-body-content-value\">";
    let start_pos = html.find(start_marker)?;

    let after_start = &html[start_pos + start_marker.len()..];
    let end_pos = after_start.find("</textarea>")?;
    let content = after_start[..end_pos].trim();

    if content.is_empty() {
        return None;
    }

    let data: serde_json::Value = serde_json::from_str(content).ok()?;
    let ret_array = data.get("ret_array")?.as_array()?;

    let mut body = String::new();
    let mut zhushi = String::new();
    let mut about = String::new();
    let mut means = String::new();
    let mut shangxi = String::new();
    let mut basic_description = String::new();
    let mut display_name = String::new();
    let mut title = String::new();
    let mut dynasty = String::new();
    let mut background = String::new();
    let mut main_points = String::new();
    let mut explain = String::new();

    for element in ret_array {
        if element.get("display_name").and_then(|v| v.as_array()).is_some() {
            if let Some(display_arr) = element["display_name"].as_array() {
                if let Some(first) = display_arr.first().and_then(|v| v.as_str()) {
                    if title.is_empty() {
                        title = first.to_string();
                    }
                }
            }
        }

        if let Some(body_elem) = element.get("body").and_then(|v| v.as_array()) {
            for text in body_elem {
                if let Some(t) = text.as_str() {
                    body.push_str(t);
                    body.push('\n');
                }
            }
            if !body.is_empty() {
                body = body.replace("</br>", "\n");
            }
        }

        if let Some(zhushi_elem) = element.get("zhushi").and_then(|v| v.as_array()) {
            for item in zhushi_elem {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    zhushi.push_str(text);
                }
            }
        }

        if let Some(about_elem) = element.get("about").and_then(|v| v.as_array()) {
            for item in about_elem {
                if let Some(t) = item.as_str() {
                    if !about.is_empty() {
                        about.push_str(", ");
                    }
                    about.push_str(t);
                }
            }
        }

        if let Some(means_elem) = element.get("means").and_then(|v| v.as_array()) {
            for item in means_elem {
                if let Some(t) = item.as_str() {
                    means.push_str(t);
                }
            }
        }

        if let Some(shangxi_elem) = element.get("shangxi").and_then(|v| v.as_array()) {
            for item in shangxi_elem {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    shangxi.push_str(text);
                }
            }
        }

        if let Some(author_arr) = element.get("literature_author").and_then(|v| v.as_array()) {
            if let Some(first) = author_arr.first() {
                if let Some(basic_desc_arr) =
                    first.get("basic_description").and_then(|v| v.as_array())
                {
                    for item in basic_desc_arr {
                        if let Some(t) = item.as_str() {
                            basic_description.push_str(t);
                        }
                    }
                }
                if let Some(display_arr) =
                    first.get("display_name").and_then(|v| v.as_array())
                {
                    if let Some(first_name) = display_arr.first().and_then(|v| v.as_str()) {
                        display_name = first_name.to_string();
                    }
                }
            }
        }

        if let Some(dynasty_arr) = element.get("dynasty").and_then(|v| v.as_array()) {
            if let Some(first) = dynasty_arr.first().and_then(|v| v.as_str()) {
                dynasty = first.to_string();
            }
        }

        if let Some(appreciation) = element.get("appreciation").and_then(|v| v.as_array()) {
            if let Some(first) = appreciation.first() {
                if let Some(bg) = first.get("background").and_then(|v| v.as_str()) {
                    background = bg.to_string();
                }
            }
        }

        if let Some(explain_arr) = element.get("explain").and_then(|v| v.as_array()) {
            if let Some(first) = explain_arr.first() {
                if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                    explain = text.to_string();
                }
            }
        }

        if let Some(main_points_arr) = element.get("mainPoints").and_then(|v| v.as_array()) {
            if let Some(first) = main_points_arr.first() {
                if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                    main_points = text.to_string();
                }
            }
        }
    }

    if body.is_empty() {
        return None;
    }

    let mut content_html = r#"<div class="poem-detail-body">"#.to_string();

    if !title.is_empty() {
        content_html.push_str(&format!("<h1>{title}</h1>"));
    }
    if !display_name.is_empty() {
        content_html.push_str(&format!(
            "<div><p>作者：{display_name}   朝代：{dynasty}</p></div>"
        ));
    }
    if !body.is_empty() {
        content_html.push_str(&format!("<div><p>{body}</p></div>"));
    }
    if !about.is_empty() {
        content_html.push_str(&format!("<div><p>标签：{about}</p></div>"));
    }
    if !means.is_empty() {
        content_html.push_str(&format!("<div><h2>译文</h2><p>{means}</p></div>"));
    }
    if !zhushi.is_empty() {
        content_html.push_str(&format!("<div><h2>注释</h2><p>{zhushi}</p></div>"));
    }
    if !explain.is_empty() {
        content_html.push_str(&format!("<div><h2>讲解</h2><p>{explain}</p></div>"));
    }
    if !shangxi.is_empty() {
        content_html.push_str(&format!("<div><h2>赏析</h2><p>{shangxi}</p></div>"));
    }
    if !background.is_empty() {
        content_html.push_str(&format!("<div><h2>背景</h2><p>{background}</p></div>"));
    }
    if !main_points.is_empty() {
        content_html.push_str(&format!(
            "<div><h2>知识锦囊</h2><p>{main_points}</p></div>"
        ));
    }
    if !basic_description.is_empty() {
        content_html.push_str(&format!(
            "<div><h2>作者介绍</h2><p>{basic_description}</p></div>"
        ));
    }

    Some(format!("<html><body>{content_html}</body></html>"))
}

/// Preprocess gov.cn HTML to normalize broken structure.
///
/// gov.cn pages often have malformed HTML with extra html/body tags
/// (the CMS concatenates multiple partial templates). This function
/// strips all html/body tags and reconstructs a clean structure.
pub fn preprocess_gov_cn_html(html: &str) -> Option<String> {
    let mut result = html.to_string();
    result = result.replace("<html>", "");
    result = result.replace("</html>", "");
    result = format!("<html>{result}</html>");
    result = remove_extra_body_tags(&result);
    result = retain_last_body_tag(&result);
    Some(result)
}

/// Remove all body tag occurrences from HTML.
fn remove_extra_body_tags(html: &str) -> String {
    html.replace("<body>", "")
        .replace("</body>", "")
}

/// Remove all body tags and add a single `<body>` pair inside `<html>`.
fn retain_last_body_tag(html: &str) -> String {
    let without_body = html.replace("<body>", "").replace("</body>", "");
    if without_body.starts_with("<html>") && without_body.ends_with("</html>") {
        let inner = &without_body[6..without_body.len() - 7];
        format!("<html><body>{inner}</body></html>")
    } else {
        format!("<body>{without_body}</body>")
    }
}

/// AES-128-ECB decryption with PKCS7 padding.
fn aes_128_ecb_decrypt(key: &[u8; 16], data: &[u8]) -> Option<Vec<u8>> {
    use aes::cipher::{generic_array::GenericArray, BlockDecrypt, KeyInit};

    let cipher = aes::Aes128::new(GenericArray::from_slice(key));

    let block_size = 16;
    if data.is_empty() || data.len() % block_size != 0 {
        return None;
    }

    let mut buf = data.to_vec();
    for chunk in buf.chunks_mut(block_size) {
        let block = GenericArray::from_mut_slice(chunk);
        cipher.decrypt_block(block);
    }

    // PKCS7 unpadding - last byte indicates how many bytes of padding
    let pad_len = buf.last().copied()? as usize;
    if pad_len == 0 || pad_len > block_size {
        // Not PKCS7 padded, return as-is
        return Some(buf);
    }
    // Verify all padding bytes match
    let all_padding = buf[buf.len() - pad_len..]
        .iter()
        .all(|&b| b as usize == pad_len);
    if !all_padding {
        return Some(buf);
    }
    buf.truncate(buf.len() - pad_len);
    Some(buf)
}

/// Check if the URL should use 36kr preprocessing.
pub fn is_36kr_url(url: &str) -> bool {
    url.starts_with("https://36kr.com/p")
        || url.starts_with("https://www.36kr.com/p")
        || url.starts_with("https://m.36kr.com/p")
}

/// Check if the URL should use Baidu Hanyu preprocessing.
pub fn is_baidu_hanyu_url(url: &str) -> bool {
    url.starts_with("https://hanyu.baidu.com") && url.contains("detail")
}

/// Check if the URL should use Gov.cn preprocessing.
pub fn is_gov_cn_url(url: &str) -> bool {
    url.starts_with("https://www.gov.cn")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_36kr_url() {
        assert!(is_36kr_url("https://36kr.com/p/123456"));
        assert!(is_36kr_url("https://www.36kr.com/p/123456"));
        assert!(is_36kr_url("https://m.36kr.com/p/123456"));
        assert!(!is_36kr_url("https://36kr.com/other"));
    }

    #[test]
    fn test_is_baidu_hanyu_url() {
        assert!(is_baidu_hanyu_url("https://hanyu.baidu.com/shiwen/detail?pid=123"));
        assert!(!is_baidu_hanyu_url("https://hanyu.baidu.com"));
        assert!(!is_baidu_hanyu_url("https://other.com"));
    }

    #[test]
    fn test_is_gov_cn_url() {
        assert!(is_gov_cn_url("https://www.gov.cn/zhengce/content_123.html"));
        assert!(!is_gov_cn_url("https://other.gov.cn"));
    }

    #[test]
    fn test_preprocess_gov_cn() {
        let html = "<html><body><p>content</p></body></html>";
        let result = preprocess_gov_cn_html(html);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.contains("<p>content</p>"));
        assert!(result.starts_with("<html>"));
        assert!(result.ends_with("</html>"));
    }

    #[test]
    fn test_preprocess_gov_cn_malformed() {
        let html = "<html><body><p>part1</p></body><body><p>part2</p></body></html>";
        let result = preprocess_gov_cn_html(html);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.starts_with("<html><body>"));
        assert!(result.contains("<p>part1</p>"));
        assert!(result.contains("<p>part2</p>"));
        // Should have exactly one body tag pair
        assert_eq!(result.matches("<body>").count(), 1);
        assert_eq!(result.matches("</body>").count(), 1);
    }

    #[test]
    fn test_remove_extra_body_tags() {
        let result = remove_extra_body_tags(
            "<html><body>content</body><body>more</body></html>",
        );
        assert_eq!(result, "<html>contentmore</html>");
    }

    #[test]
    fn test_retain_last_body_tag() {
        let result = retain_last_body_tag("<html><p>content</p></html>");
        assert_eq!(result, "<html><body><p>content</p></body></html>");
    }

    #[test]
    fn test_retain_last_body_tag_no_html() {
        let result = retain_last_body_tag("content");
        assert_eq!(result, "<body>content</body>");
    }

    #[test]
    fn test_aes_ecb_decrypt_invalid_data() {
        let key = b"efabccee-b754-4c";
        assert!(aes_128_ecb_decrypt(key, b"").is_none());
        assert!(aes_128_ecb_decrypt(key, b"short").is_none());
    }

    #[test]
    fn test_extract_36kr_no_marker() {
        let html = "<html><body>no script here</body></html>";
        assert!(extract_36kr_content(html).is_none());
    }

    #[test]
    fn test_extract_baidu_hanyu_no_marker() {
        let html = "<html><body>no textarea</body></html>";
        assert!(extract_baidu_hanyu_content(html).is_none());
    }

    #[test]
    #[ignore]
    fn live_baidu_hanyu_extraction() {
        let html = std::fs::read_to_string("/tmp/hanyu_detail2.html")
            .expect("run: curl -sL 'https://hanyu.baidu.com/shici/detail?pid=38a52978fb6f4cfd8bcc25fc2db2c0fa' -o /tmp/hanyu_detail2.html");

        let url = "https://hanyu.baidu.com/shici/detail?pid=38a52978fb6f4cfd8bcc25fc2db2c0fa";
        assert!(super::is_baidu_hanyu_url(url));

        let processed = super::extract_baidu_hanyu_content(&html)
            .expect("preprocessor should extract content");

        assert!(processed.contains("床前明月光"), "content should have poem body");
        assert!(processed.contains("静夜思"), "should have title");
        assert!(processed.contains("李白"), "should have author");
        assert!(processed.contains("赏析"), "should have shangxi section");

        let opts = crate::Options {
            url: Some(url.to_string()),
            ..crate::Options::default()
        };
        let result = crate::extract_with_options(&processed, &opts)
            .expect("extraction should succeed");

        assert!(result.content_text.contains("床前明月光"),
            "extracted text should contain poem: {}", &result.content_text[..200.min(result.content_text.len())]);
        assert!(result.content_text.len() > 200, "extracted content should be substantial");
        eprintln!("Baidu Hanyu test: extracted {} chars", result.content_text.len());
    }

    #[test]
    #[ignore]
    fn live_36kr_extraction() {
        let html = std::fs::read_to_string("/tmp/36kr_test.html")
            .expect("run: curl -s 'https://36kr.com/p/3826046008742530' -o /tmp/36kr_test.html");

        let processed = super::extract_36kr_content(&html)
            .expect("preprocessor should extract content");

        assert!(processed.contains("一枚估值110亿美元"), "content should have article text: {}", &processed[..200]);
        assert!(processed.contains("<h1>"), "should have title tag");
        assert!(!processed.contains("widgetContent"), "raw JSON key should not leak");

        let opts = crate::Options {
            url: Some("https://36kr.com/p/3826046008742530".to_string()),
            ..crate::Options::default()
        };
        let result = crate::extract_with_options(&processed, &opts)
            .expect("extraction should succeed");

        assert!(result.content_text.contains("一枚估值110亿美元"),
            "extracted text should contain article, got: {}", &result.content_text[..200.min(result.content_text.len())]);
        assert!(result.content_text.len() > 500, "extracted content should be substantial");
        eprintln!("36kr live test: extracted {} chars", result.content_text.len());
    }

    #[test]
    #[ignore]
    fn live_gov_cn_yaowen_extraction() {
        let html = std::fs::read_to_string("/tmp/govcn_yaowen.html")
            .expect("run: curl -sL 'https://www.gov.cn/yaowen/liebiao/202605/content_7070204.htm' -o /tmp/govcn_yaowen.html");

        let url = "https://www.gov.cn/yaowen/liebiao/202605/content_7070204.htm";
        assert!(super::is_gov_cn_url(url));

        // Test WITHOUT preprocessing (should fail or produce poor results)
        let opts = crate::Options {
            url: Some(url.to_string()),
            ..crate::Options::default()
        };
        let result_no_pre = crate::extract_with_options(&html, &opts)
            .expect("extraction without preprocess should not panic");
        let len_no_pre = result_no_pre.content_text.len();
        eprintln!("Without preprocess: {} chars", len_no_pre);

        // Test WITH preprocessing
        let processed = super::preprocess_gov_cn_html(&html)
            .expect("preprocessor should process html");

        assert!(processed.starts_with("<html>"), "should start with <html>");
        assert!(processed.ends_with("</html>"), "should end with </html>");
        assert_eq!(processed.matches("<body>").count(), 1, "should have exactly one <body>");
        assert_eq!(processed.matches("</body>").count(), 1, "should have exactly one </body>");

        let result = crate::extract_with_options(&processed, &opts)
            .expect("extraction should succeed");

        let len_with_pre = result.content_text.len();
        eprintln!("With preprocess: {} chars", len_with_pre);
        eprintln!("Improvement: {} chars", len_with_pre as isize - len_no_pre as isize);

        assert!(result.content_text.contains("李强"),
            "should contain article content: {}", &result.content_text[..200.min(result.content_text.len())]);
    }
}
