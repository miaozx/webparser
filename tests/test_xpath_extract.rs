use std::fs;
use rs_trafilatura::{extract_with_options, Options, xpath_config::XpathConfig};

/// 测试 xpath 配置抽取。
/// 使用方式：
///   1. curl -sL 'https://目标网址' -o /tmp/test.html
///   2. cargo test --test test_xpath_extract -- --nocapture
#[test]
fn xpath_extract_test() {
    // 读取测试 HTML（需提前下载好）
    let html = fs::read_to_string("aa.html")
        .expect("请先下载 HTML: curl -sL 'https://目标网址' -o /tmp/test.html");

    // 加载 xpath 配置
    let config = XpathConfig::from_json(
        &fs::read_to_string("config/config.json").unwrap()
    ).expect("config/config.json 加载失败，请确认文件存在");

    // 修改为目标 URL（与 config.json 中的规则匹配）
    let url = "https://www.smzdm.com/p/ea8ca418bbdd";

    // 执行抽取
    let result = extract_with_options(&html, &Options {
        xpath_config: Some(config),
        url: Some(url.to_string()),
        include_images: true,
        output_markdown: true,
        ..Options::default()
    }).expect("抽取失败");

    // 打印结果
    println!("=== 标题 ===");
    println!("{:?}", result.metadata.title);

    println!("\n=== 正文 ({} 字) ===", result.content_text.len() / 2);
    println!("{}", result.content_text);

    if let Some(ref md) = result.content_markdown {
        println!("\n=== Markdown ({} 字) ===", md.len() / 2);
        println!("{}", md);
    }

    println!("\n=== 警告 ===");
    for w in &result.warnings {
        println!("  {}", w);
    }

    // 简单断言：验证有内容
    assert!(!result.content_text.is_empty(), "正文不应为空");
    assert!(result.content_text.len() > 50, "正文过短，可能未正确抽取");
}
