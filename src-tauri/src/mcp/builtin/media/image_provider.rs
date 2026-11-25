use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

const MAX_DOWNLOAD_BYTES: usize = 20 * 1024 * 1024;

/// 服务商返回的图片载荷:优先 base64,部分服务商只给 URL。
#[derive(Debug, PartialEq)]
enum Payload {
    B64(String),
    Url(String),
}

fn parse_response(v: &Value) -> Result<Payload, String> {
    let item = v
        .get("data")
        .and_then(|d| d.get(0))
        .ok_or_else(|| "响应缺少 data[0]".to_string())?;
    if let Some(b) = item.get("b64_json").and_then(Value::as_str)
        && !b.trim().is_empty()
    {
        return Ok(Payload::B64(b.trim().to_string()));
    }
    if let Some(u) = item
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|u| !u.is_empty())
    {
        return Ok(Payload::Url(u.to_string()));
    }
    Err("响应中既没有 b64_json 也没有 url".into())
}

async fn fetch_url(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(format!("不支持的图片地址:{url}"));
    }
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("下载图片失败:{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载图片失败:HTTP {}", resp.status()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取图片数据失败:{e}"))?;
    if bytes.len() > MAX_DOWNLOAD_BYTES {
        return Err("图片超过 20MB 上限".into());
    }
    Ok(bytes.to_vec())
}

/// 调用 OpenAI 兼容的图像生成接口,返回服务商原始图片字节。
pub(super) async fn generate(
    base_url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    size: &str,
) -> Result<Vec<u8>, String> {
    let mut body = json!({ "model": model, "prompt": prompt, "n": 1 });
    let is_gpt_image = model.starts_with("gpt-image");
    if size != "auto" || is_gpt_image {
        body["size"] = json!(size);
    }
    // 不传 response_format:gpt-image 不接受它,而多数 OpenAI 兼容端点(如智谱)
    // 默认返回 url;解析层已同时兼容 b64_json 与 url。

    let url = format!("{}/images/generations", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败:{e}"))?;

    let mut attempt = 0;
    let resp = loop {
        attempt += 1;
        match client
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
        {
            Ok(r) => break r,
            Err(e) if attempt < 2 => {
                log::warn!("图片生成请求失败,重试一次: {e}");
                tokio::time::sleep(Duration::from_millis(400)).await;
            }
            Err(e) => return Err(format!("网络请求失败:{e}")),
        }
    };

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let brief: String = text.chars().take(400).collect();
        return Err(format!("服务商返回 HTTP {status}:{brief}"));
    }
    let json: Value = resp.json().await.map_err(|e| format!("解析响应失败:{e}"))?;
    let payload = parse_response(&json)?;

    let bytes = match payload {
        Payload::B64(b) => STANDARD
            .decode(b.as_bytes())
            .map_err(|e| format!("base64 解码失败:{e}"))?,
        Payload::Url(u) => fetch_url(&client, &u).await?,
    };
    if bytes.is_empty() {
        return Err("服务商返回了空图片".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_b64_over_url() {
        let v = json!({ "data": [{ "b64_json": "QUJD", "url": "https://x/y.png" }] });
        assert_eq!(parse_response(&v).unwrap(), Payload::B64("QUJD".into()));
    }

    #[test]
    fn falls_back_to_url() {
        let v = json!({ "data": [{ "url": "https://x/y.png" }] });
        assert_eq!(
            parse_response(&v).unwrap(),
            Payload::Url("https://x/y.png".into())
        );
    }

    #[test]
    fn reports_missing_payload() {
        assert!(parse_response(&json!({ "data": [] })).is_err());
        assert!(parse_response(&json!({})).is_err());
    }
}
