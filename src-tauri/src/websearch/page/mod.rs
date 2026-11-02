//! web_fetch:免 API Key 的网页正文抓取(只读内置工具)。
//!
//! 与搜索链路共用「请求 → 文本」的目标,但抓取任意用户给出的链接,安全策略更严:
//! 1. 只允许 http/https;重定向逐跳复检(最多 5 跳),防 file:// 降级与重定向环;
//! 2. 拦截本机/私网/云元数据地址(127/10/172.16-31/192.168/169.254/::1 等),防 SSRF;
//! 3. 抓取前检查 robots.txt(取不到时按放行处理,避免无 robots 的站点整站不可用);
//! 4. 30s 超时、10MB 响应上限、按 content-type 过滤二进制;
//! 5. 正文转 Markdown 后按字符截断,并在结果里标注「外部不可信内容」。
//!
//! 已知限制:只按 IP 字面量与已知主机名拦截,不解析域名实际 IP(防 DNS 回环需额外
//! 依赖);需要登录或纯前端渲染的页面可能抓不到正文。
//!
//! 按职责拆分:`fetch`(URL 校验 / 重定向 / 读取解码 / 取段)、`robots`(robots.txt 判定)、
//! `extract`(去站点模板与纯链接行)、`markdown`(HTML → Markdown)、
//! `chunk`(分块 + BM25 相关段落选择)、`render`(给模型的输出格式)。
//!
//! 长页面不再简单截断开头:`query` 命中时按相关度选段,未命中时可用 `start_index` 续读。

mod chunk;
mod extract;
mod fetch;
mod markdown;
mod render;
mod robots;

pub use fetch::fetch_page;
pub use render::render_for_model;

/// 一次网页抓取的结果。
#[derive(Debug, Clone)]
pub struct Page {
    pub title: String,
    /// 最终地址(跟随重定向后),便于模型引用原文位置。
    pub url: String,
    pub content: String,
    pub truncated: bool,
    pub total_chars: usize,
    /// 本次内容在清理后正文中的起始字符偏移(按 query 选段时为 0)。
    pub offset: usize,
    /// 是否按检索意图(`query`)挑选了最相关段落。
    pub query_selected: bool,
}
