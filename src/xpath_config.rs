//! XPath configuration for URL-matched extraction rules.
//!
//! Rules map URL patterns to xpath selector expressions for content and metadata fields.
//!
//! # Config Format
//!
//! ```json
//! {
//!   "url.pattern/recipe": {
//!     "title": { "xpath": "//*[@class=\"title\"]", "single_node": true },
//!     "content": {
//!       "xpath": ["//*[@class=\"main\"]", "//*[@class=\"content\"]"],
//!       "filter_xpath": ["//*[@class=\"ad\"]", "//*[@class=\"footer\"]"]
//!     }
//!   }
//! }
//! ```

use regex::Regex;

/// Configuration field for a single extraction target.
#[derive(Debug, Clone)]
pub struct XpathConfigField {
    /// Field name: "content", "title", "publish_time", "author", "head_title", etc.
    pub field_name: String,

    /// Xpath selector list (ordered; first match wins).
    pub xpath_list: Vec<String>,

    /// Xpath selectors for nodes to exclude from extraction.
    pub filter_xpath_list: Vec<String>,

    /// Whether to extract only the first matching node's content.
    pub single_node: bool,
}

/// XPath configuration with URL-matched extraction rules.
#[derive(Debug, Clone)]
pub struct XpathConfig {
    rules: Vec<XpathRule>,
}

#[derive(Debug, Clone)]
struct XpathRule {
    url_regex: Regex,
    fields: Vec<XpathConfigField>,
}

impl XpathConfig {
    /// Create config from JSON string.
    ///
    /// URL pattern keys use dots matched literally (auto-escaped to `\.`).
    ///
    /// # Errors
    ///
    /// Returns error description if JSON is invalid or a pattern fails to compile.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let raw: std::collections::BTreeMap<String, std::collections::BTreeMap<String, FieldRaw>> =
            serde_json::from_str(json_str)
                .map_err(|e| format!("Invalid XpathConfig JSON: {e}"))?;

        let mut rules = Vec::new();
        for (url_key, fields_map) in raw {
            let escaped = url_key.replace('.', "\\.");
            let url_regex = Regex::new(&escaped)
                .map_err(|e| format!("Invalid URL pattern '{url_key}': {e}"))?;

            let fields: Vec<XpathConfigField> = fields_map
                .into_iter()
                .map(|(name, f)| {
                    let xpath_list = match f.xpath {
                        Some(XpathValue::Single(s)) => vec![s],
                        Some(XpathValue::Multiple(v)) => v,
                        None => Vec::new(),
                    };
                    XpathConfigField {
                        field_name: name,
                        xpath_list,
                        filter_xpath_list: f.filter_xpath.unwrap_or_default(),
                        single_node: f.single_node.unwrap_or(false),
                    }
                })
                .collect();

            rules.push(XpathRule { url_regex, fields });
        }

        Ok(Self { rules })
    }

    /// Match a URL against all rules, returning fields for the first match.
    pub fn match_url(&self, url: &str) -> Option<Vec<XpathConfigField>> {
        for rule in &self.rules {
            if rule.url_regex.is_match(url) {
                return Some(rule.fields.clone());
            }
        }
        None
    }

    /// Number of configured rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the config is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

// ---- JSON deserialization types ----

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum XpathValue {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(serde::Deserialize)]
struct FieldRaw {
    xpath: Option<XpathValue>,
    #[serde(default)]
    filter_xpath: Option<Vec<String>>,
    #[serde(default)]
    single_node: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_real_config_file() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/config.json");
        let json = std::fs::read_to_string(&path).expect("read config/config.json");
        let config = XpathConfig::from_json(&json).expect("parse config");
        // Should have many rules.
        assert!(config.len() > 5, "expected many rules, got {}", config.len());
        // Verify a known URL matches.
        assert!(
            config.match_url("https://m.meishichina.com/recipe/123").is_some(),
            "meishichina should match"
        );
        assert!(
            config.match_url("https://blog.csdn.net/article/details/12345").is_some(),
            "csdn should match"
        );
        // Unknown URL should not match.
        assert!(
            config.match_url("https://example.com/unknown").is_none(),
            "unknown should not match"
        );
    }

    #[test]
    fn test_from_json_dict_format() {
        let json = r#"{
            "example.com/article": {
                "title": { "xpath": "//*[@class='title']", "single_node": true },
                "content": { "xpath": "//*[@class='content']" }
            }
        }"#;
        let config = XpathConfig::from_json(json).expect("valid config");
        assert_eq!(config.len(), 1);

        let fields = config
            .match_url("https://example.com/article/some-post")
            .expect("should match");
        assert_eq!(fields.len(), 2);

        let title = fields.iter().find(|f| f.field_name == "title").expect("title");
        assert_eq!(title.xpath_list, vec!["//*[@class='title']"]);
        assert!(title.single_node);
    }

    #[test]
    fn test_xpath_as_array() {
        let json = r#"{
            "test.com": {
                "title": { "xpath": ["//h1", "//*[@class='title']"], "single_node": true },
                "content": {
                    "xpath": ["//*[@class='main']", "//*[@class='content']"],
                    "filter_xpath": ["//*[@class='ad']"]
                }
            }
        }"#;
        let config = XpathConfig::from_json(json).expect("valid config");
        let fields = config.match_url("https://test.com/page").expect("should match");

        let title = fields.iter().find(|f| f.field_name == "title").expect("title");
        assert_eq!(title.xpath_list, vec!["//h1", "//*[@class='title']"]);

        let content = fields.iter().find(|f| f.field_name == "content").expect("content");
        assert_eq!(content.xpath_list, vec!["//*[@class='main']", "//*[@class='content']"]);
        assert_eq!(content.filter_xpath_list, vec!["//*[@class='ad']"]);
    }

    #[test]
    fn test_no_match() {
        let json = r#"{"other.com/page": { "content": { "xpath": "//div" } }}"#;
        let config = XpathConfig::from_json(json).expect("valid config");
        assert!(config.match_url("https://example.com/test").is_none());
    }

    #[test]
    fn test_dot_matches_literally() {
        let json = r#"{"m.site.com/recipe": { "content": { "xpath": "//div" } }}"#;
        let config = XpathConfig::from_json(json).expect("valid config");
        assert!(config.match_url("https://m.site.com/recipe/123").is_some());
        assert!(config.match_url("https://mXsite.com/recipe/123").is_none());
    }
}
