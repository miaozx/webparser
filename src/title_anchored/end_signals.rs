const END_PATTERNS: &[&str] = &[
    "声明",
    "免责声明",
    "编辑",
    "投稿",
    "责编",
    "责任编辑",
    "关键词",
    "相关文章",
    "相关阅读",
    "更多精彩内容",
    "本文由",
    "来源",
    "阅读更多",
    "关注我们",
    "欢迎转载",
    "未经授权",
    "本文来源",
    "本文编辑",
    "本文责编",
    "本文为",
    "本文来自",
    "本文转自",
    "本文版权",
    "本文系",
    "本文仅",
    "本文只",
    "更多精彩",
    "更多内容",
    "延伸阅读",
    "扩展阅读",
    "推荐阅读",
    "猜你喜欢",
    "你可能会喜欢",
    "你可能还喜欢",
    "分享到",
    "分享至",
    "分享按钮",
    "版权声明",
    "联系我们",
    "广告合作",
    "商务合作",
    "加入我们",
    "关于我们",
    "友情链接",
    "友情提示",
    "特别声明",
    "特别提示",
    "温馨提示",
    "注：",
    "备注",
    "注释",
    "小编",
    "作者",
    "校对",
    "审核",
    "编审",
    "监制",
    "投稿邮箱",
    "联系邮箱",
    "责任编辑：",
    "责辑：",
    "作者：",
    "来源：",
    "推荐：",
    "热门",
    "大家都在看",
    "大家都在搜",
    "热门文章",
    "热门推荐",
    "热点新闻",
    "最新文章",
    "最新推荐",
    "推荐文章",
    "精彩推荐",
    "特别推荐",
    "每日推荐",
    "精选推荐",
    "你可能感兴趣",
    "为您推荐",
    "相关推荐",
    "推荐产品",
    "推荐商品",
];

const NEGATIVE_ANCHOR_PATTERNS: &[&str] = &[
    "上一篇",
    "下一篇",
    "上一页",
    "下一页",
    "返回",
    "返回列表",
    "返回顶部",
    "更多",
    "more",
    "点击",
    "点击这里",
    "点击进入",
    "进入",
    "查看",
    "查看详情",
    "详情",
    "详情介绍",
    "了解详情",
    "阅读全文",
    "全文阅读",
    "立即阅读",
    "马上阅读",
    "下载",
    "立即下载",
    "注册",
    "立即注册",
    "登录",
    "立即登录",
    "订阅",
    "立即订阅",
    "购买",
    "立即购买",
    "咨询",
    "在线咨询",
    "客服",
    "联系客服",
    "电话",
    "热线",
    "邮箱",
    "地址",
    "关于",
    "关于我们",
    "免责",
    "免责声明",
    "版权",
    "版权声明",
    "隐私",
    "隐私政策",
    "条款",
    "服务条款",
    "帮助",
    "帮助中心",
    "FAQ",
    "常见问题",
    "搜索",
    "站内搜索",
    "友情链接",
    "合作伙伴",
    "广告",
    "广告合作",
    "Related News"
];

pub fn has_end_signal(text: &str) -> bool {
    let text = text.trim();
    if text.len() < 4 {
        return false;
    }
    for pattern in END_PATTERNS {
        if text.contains(pattern) {
            return true;
        }
    }
    false
}

/// C++ IsEndText: comprehensive end signal detection for article content.
/// Matches by substring (contains) so it covers more cases than exact match.
pub fn is_end_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    // Exact matches from C++ IsEndText
    let exact_patterns = [
        "上一页", "下一页", "延伸閱讀", "推荐帖", "相关主题：",
        "【扩展阅读】", "-END-", "上一章", "关联文章推荐",
        "更多精彩内容", "更多相关榜单", "下一章", "上一集",
        "下一集", "相关推荐", "编辑推荐：", "相關文章",
        "更多相关百科知识", "相关文章", "延伸阅读", "最新文章",
        "网友评论", "相关问答", "为你推荐", "相关新闻",
        "推荐阅读", "活动推荐", "为您推荐", "猜你喜欢",
        "热门游戏", "近期文章", "相关问题", "大家都在看",
        "精彩推荐", "大家在看", "免责声明", "相关软件",
        "相关阅读", "上一篇", "下一篇",
    ];
    if exact_patterns.contains(&t) {
        return true;
    }

    // Substring patterns from C++ IsEndText
    let substring_patterns = [
        "相关文章：", "往期推荐:", "上一篇：", "上一篇:", "下一篇：", "下一篇:",
        "上一篇 >", "下一篇 >", "«上一页", "←上一章",
        "-相关推荐-", "版权和免责声明", "版权说明：", "版权声明：",
        "免责声明：", "网友评论：", "--免责声明--", "相关内容：",
        "猜你喜欢：", "我的更多文章：", "相关文档推荐", "更多相关文章",
        "您还感兴趣的文章推荐", "看过这篇文章的人还喜欢",
        "【推荐阅读】", "相关阅读：", "相关阅读:", "推荐阅读：",
    ];
    for pat in &substring_patterns {
        if t.contains(pat) {
            return true;
        }
    }

    false
}

pub fn contains_negative_anchor_text(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    for pattern in NEGATIVE_ANCHOR_PATTERNS {
        if text.contains(pattern) {
            return true;
        }
    }
    false
}
