//! 用户导入参考资料的注入与落盘目录。

use super::ResearchParams;

/// 参考资料注入总量上限(字符):超出即截断,防止撑爆上下文并拖慢首 token。
pub const REF_TOTAL_BUDGET_CHARS: usize = 30_000;
/// 单份参考资料注入上限(字符)。
pub const REF_FILE_BUDGET_CHARS: usize = 12_000;

const REF_BLOCK_HEADER: &str = "【用户导入的参考资料】以下内容是用户提供的本地文件正文,视为可信事实依据,可直接引用;不得虚构参考资料之外的事实。";

/// 用户导入参考资料目录:`<data_dir>/research/references`。
pub fn references_dir() -> std::path::PathBuf {
    crate::paths::data_dir().join("research").join("references")
}

/// 导入时缓存的提取文本目录:`<data_dir>/research/references/extracted`。
pub fn references_cache_dir() -> std::path::PathBuf {
    references_dir().join("extracted")
}

impl ResearchParams {
    /// 参考资料注入块:读取 data/research/references/ 下用户导入的文件
    /// (优先读导入时缓存的提取文本,缺失时回退实时解析)。
    /// 逐个做文件名白名单校验(禁路径分隔符/上跳),目录必须已落盘且规范化后仍在 references 内;
    /// 施加单文件与总量双重字符上限。返回 None = 无可用参考资料。
    pub fn references_block(&self) -> Option<String> {
        if self.reference_files.is_empty() {
            return None;
        }
        let dir = references_dir();
        let root = dir.canonicalize().ok()?;
        let cache = references_cache_dir();
        let mut body = String::new();
        let mut used = 0usize;
        for name in &self.reference_files {
            if used >= REF_TOTAL_BUDGET_CHARS {
                break;
            }
            // 文件名白名单:不得为空 / 含路径分隔符 / 含上跳序列
            if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
                continue;
            }
            let canon = match dir.join(name).canonicalize() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if !canon.starts_with(&root) || !canon.is_file() {
                continue;
            }
            // 优先用导入时缓存的提取文本(研究启动不重新解析大文件)
            let text = match std::fs::read_to_string(cache.join(format!("{name}.txt"))) {
                Ok(t) => Some(t),
                Err(_) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    crate::attachment::extract_text(&canon, crate::attachment::classify(name))
                }))
                .ok()
                .and_then(Result::ok)
                .flatten(),
            };
            let Some(text) = text else {
                continue;
            };
            let budget = REF_FILE_BUDGET_CHARS.min(REF_TOTAL_BUDGET_CHARS - used);
            let part = crate::attachment::clamp_text(&text, budget);
            used += part.chars().count();
            body.push_str(&format!("\n【参考资料:{name}】\n{part}\n"));
        }
        if body.is_empty() {
            None
        } else {
            Some(format!("{REF_BLOCK_HEADER}\n{body}"))
        }
    }
}
