//! robots.txt 判定:只取 `User-agent: *` / `myagent` 组,最长匹配规则优先。
//!
//! 取不到 robots(网络错误 / 非 2xx)一律放行 —— 合规性判断,不因缺文件整站不可用。

/// robots.txt 只需前缀规则,读取上限。
const ROBOTS_MAX_BYTES: usize = 256 * 1024;

/// 抓取前检查 robots.txt。取不到(网络错误/非 2xx)一律放行。
pub(super) async fn robots_allows(client: &reqwest::Client, url: &reqwest::Url) -> bool {
    let Ok(robots_url) = url.join("/robots.txt") else {
        return true;
    };
    let Ok(resp) = client
        .get(robots_url)
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
    else {
        return true;
    };
    if !resp.status().is_success() {
        return true;
    }
    let mut buf = Vec::new();
    let mut resp = resp;
    while let Ok(Some(chunk)) = resp.chunk().await {
        buf.extend_from_slice(&chunk);
        if buf.len() > ROBOTS_MAX_BYTES {
            break;
        }
    }
    let body = String::from_utf8_lossy(&buf);
    robots_allows_path(&body, url.path(), "myagent")
}

/// robots.txt 的一个规则组:`User-agent` 列表 + `(是否允许, 匹配模式)` 规则列表。
type RobotsGroup = (Vec<String>, Vec<(bool, String)>);

/// robots.txt 规则判定:只取 `User-agent: *` 或 `myagent` 组,最长匹配规则优先。
pub(super) fn robots_allows_path(body: &str, path: &str, ua: &str) -> bool {
    let mut groups: Vec<RobotsGroup> = vec![(Vec::new(), Vec::new())];
    for raw in body.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let key = k.trim().to_ascii_lowercase();
        let val = v.trim().to_string();
        match key.as_str() {
            "user-agent" => {
                // 规则之后再出现 user-agent,视为新的一组
                if groups.last().is_some_and(|(_, rules)| !rules.is_empty()) {
                    groups.push((Vec::new(), Vec::new()));
                }
                if let Some((agents, _)) = groups.last_mut() {
                    agents.push(val.to_ascii_lowercase());
                }
            }
            "allow" | "disallow" => {
                if let Some((_, rules)) = groups.last_mut() {
                    rules.push((key == "allow", val));
                }
            }
            _ => {}
        }
    }

    let ua = ua.to_ascii_lowercase();
    let mut best: Option<(usize, bool)> = None;
    for (agents, rules) in &groups {
        if !agents.iter().any(|a| a == "*" || a == &ua) {
            continue;
        }
        for (allow, pattern) in rules {
            if !pattern_matches(pattern, path) {
                continue;
            }
            // 最长规则更具体;同长度时保留先出现的规则
            if best.is_none_or(|(len, _)| pattern.len() > len) {
                best = Some((pattern.len(), *allow));
            }
        }
    }
    best.map(|(_, allow)| allow).unwrap_or(true)
}

/// robots 规则匹配:`*` 通配、尾部 `$` 精确到路径结尾;空 pattern 表示全站允许。
fn pattern_matches(pattern: &str, path: &str) -> bool {
    let p = pattern.trim();
    if p.is_empty() {
        return false;
    }
    let exact_tail = p.ends_with('$');
    let body = p.trim_end_matches('$');
    if !body.contains('*') {
        return if exact_tail {
            path == body
        } else {
            path.starts_with(body)
        };
    }
    let parts: Vec<&str> = body.split('*').collect();
    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if i == 0 {
            if !path.starts_with(part) {
                return false;
            }
            pos = part.len();
        } else {
            let Some(k) = path[pos..].find(part) else {
                return false;
            };
            pos += k + part.len();
        }
    }
    if exact_tail {
        path.ends_with(parts.last().copied().unwrap_or(""))
    } else {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_prefers_longest_matching_rule() {
        let body = "User-agent: *\nDisallow: /private\nAllow: /private/public\n";
        assert!(!robots_allows_path(body, "/private/x", "myagent"));
        assert!(robots_allows_path(body, "/private/public/a", "myagent"));
        assert!(robots_allows_path(body, "/index.html", "myagent"));

        let wildcard = "User-agent: *\nDisallow: /*/search\n";
        assert!(!robots_allows_path(wildcard, "/zh/search", "myagent"));
        assert!(robots_allows_path(wildcard, "/zh/docs", "myagent"));

        // 其他 UA 组不影响 myagent;空 Disallow 表示全站允许
        let other_only = "User-agent: Googlebot\nDisallow: /\n";
        assert!(robots_allows_path(other_only, "/a", "myagent"));
        assert!(robots_allows_path(
            "User-agent: *\nDisallow:\n",
            "/a",
            "myagent"
        ));
    }
}
