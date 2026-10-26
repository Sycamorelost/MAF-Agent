//! Provider 实例组装与 base_url 安全校验。

use crate::provider::{Provider, keychain, rotation};

use super::keys::{build_openai, own_candidates, resolve_spec};

/// 根据参数构造 Provider(chat_stream / summarize_memory 等命令共用)。
/// base_url / display_name 优先用显式入参,缺省回退到模板(base_url 为空则报错);
/// api_key 一律从 keychain 按 provider_id 取。
pub(crate) fn build_provider(
    provider_id: &str,
    model: &str,
    custom_base_url: Option<&str>,
    custom_display_name: Option<&str>,
) -> Result<Box<dyn Provider>, String> {
    build_provider_with_stream_options(
        provider_id,
        model,
        custom_base_url,
        custom_display_name,
        false,
    )
}

/// H3:build_provider 的可变体,允许开启 stream_options.include_usage(研究链路
/// 采集真实 Token 用量;不兼容的中转站由调用方用 false 重建回退)。
pub(crate) fn build_provider_with_stream_options(
    provider_id: &str,
    model: &str,
    custom_base_url: Option<&str>,
    custom_display_name: Option<&str>,
    stream_options: bool,
) -> Result<Box<dyn Provider>, String> {
    // 非对话路径(图片/翻译/研究/记忆提炼)统一使用 slot 0(历史单 Key 语义);
    // slot 0 缺失时(用户把 Key 都配在 1..N 槽位),回退到轮换顺序中的第一把可用 Key,
    // 仅取一把、不做故障轮换,保持非对话路径的单 Key 语义。
    let api_key = match keychain::load_slot(provider_id, 0)? {
        Some(k) => k,
        None => {
            let cfg = rotation::config_for(provider_id);
            own_candidates(provider_id, &cfg)
                .into_iter()
                .find_map(|c| keychain::load_slot(provider_id, c.slot).ok().flatten())
                .ok_or_else(|| format!("provider {} 未配置 API Key", provider_id))?
        }
    };
    let spec = resolve_spec(provider_id, model, custom_base_url, custom_display_name)?;
    Ok(build_openai(&spec, &api_key, stream_options))
}

/// M1 安全校验:base_url 仅允许 https;例外放行本机回环(http://localhost / 127.0.0.1 / [::1]),
/// 防止 API Key 经明文 http 外发到第三方主机(CWE-319)。
pub(crate) fn validate_base_url_scheme(base_url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(base_url.trim()).map_err(|e| format!("base_url 无法解析: {e}"))?;
    match u.scheme() {
        "https" => Ok(()),
        "http" => {
            // url crate 对 IPv6 字面量返回带方括号形式([::1]),统一去括号后再判定
            let host = u.host_str().unwrap_or_default();
            let h = host.trim_start_matches('[').trim_end_matches(']');
            if h == "localhost" || h == "127.0.0.1" || h == "::1" {
                Ok(())
            } else {
                Err("base_url 仅允许 https;http 仅限本机回环(localhost/127.0.0.1/[::1])".into())
            }
        }
        s => Err(format!("base_url 协议不支持: {s}(仅允许 https)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M1 回归:明文 http 指向第三方主机必须被拒绝。
    #[test]
    fn build_provider_rejects_plain_http() {
        assert!(validate_base_url_scheme("http://evil.com").is_err());
        assert!(validate_base_url_scheme("http://evil.com:8080/v1").is_err());
        assert!(validate_base_url_scheme("ftp://evil.com").is_err());
        assert!(validate_base_url_scheme("not-a-url").is_err());
    }

    /// M1 回归:https 与 本机回环 http 放行。
    #[test]
    fn build_provider_allows_https_and_localhost_http() {
        assert!(validate_base_url_scheme("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url_scheme("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url_scheme("http://localhost:11434").is_ok());
        assert!(validate_base_url_scheme("http://[::1]:11434").is_ok());
    }
}
