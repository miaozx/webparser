use crate::dom::Selection;

const END_PATTERNS: &[&str] = &[
    "声明",
    "免责声明",
    "编辑",
    "投稿",
    "责编",
    "责任编辑",
    "关键词",
    "相关文章",
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

/// Negative anchor text patterns — if a link's text matches, the surrounding
/// node has high link density from non-content links.
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

pub fn contains_negative_anchor_text(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    for pattern in NEGATIVE_ANCHOR_PATTERNS {
        if text.contains(pattern) {
            return true;
        }
    }
    false
}

pub fn is_high_link_density(el: &Selection) -> bool {
    let total_text = el.text();
    let total_len = total_text.trim().len();
    if total_len < 50 {
        return false;
    }
    let mut link_text_len = 0usize;
    for node in el.select("a").nodes() {
        let sel = crate::dom::Selection::from(*node);
        let link_text = sel.text();
        let t = link_text.trim();
        for pattern in NEGATIVE_ANCHOR_PATTERNS {
            if t.contains(pattern) {
                link_text_len += t.len();
                break;
            }
        }
    }
    if total_len == 0 {
        return false;
    }
    link_text_len as f64 / total_len as f64 > 0.5
}
