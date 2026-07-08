//! Gemini 协议常量、错误类型、batchexecute 响应解析，及通用工具函数。

use chrono::{DateTime, TimeZone, Utc};
use regex::Regex;
use std::sync::OnceLock;

// ============================================================================
// 配置常量
// ============================================================================
pub const GEMINI_BASE: &str = "https://gemini.google.com";
pub const BATCH_SIZE: usize = 20;
pub const DETAIL_PAGE_SIZE: usize = 10;

pub const REQUEST_DELAY: f64 = 0.30;
pub const REQUEST_JITTER_MIN: f64 = 0.00;
pub const REQUEST_JITTER_MAX: f64 = 0.30;
pub const REQUEST_JITTER_MODE: f64 = 0.14;
/// batchexecute 失败后重试前的暂停秒数
pub const REQUEST_RETRY_PAUSE_SECONDS: f64 = 2.0;

/// 浏览器 User-Agent（从本地 Chrome 检测，失败时用 fallback）。
/// 保留为函数调用的 wrapper，方便旧代码引用。
pub fn browser_user_agent() -> &'static str {
    crate::browser_info::build_user_agent()
}

/// 浏览器 Accept-Language（从本地 Chrome 偏好读取，失败时用 fallback）。
pub fn browser_accept_language() -> &'static str {
    crate::browser_info::detect_accept_language()
}

// ============================================================================
// 错误类型
// ============================================================================
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("HTTP 200 但响应数据为空（session/cookie 已过期）")]
    SessionExpired,
}

// ============================================================================
// 通用工具
// ============================================================================

/// 时间戳（秒）转 ISO 8601 UTC 字符串
pub fn to_iso_utc(ts: Option<i64>) -> Option<String> {
    let ts = ts?;
    Utc.timestamp_opt(ts, 0).single().map(|dt| dt.to_rfc3339())
}

/// 将值强制转为 epoch 秒数
pub fn coerce_epoch_seconds(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Number(n) => n.as_i64(),
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            trimmed.parse::<i64>().ok()
        }
        _ => None,
    }
}

/// ISO 8601 字符串转 epoch 秒数
pub fn iso_to_epoch_seconds(iso_text: &str) -> Option<i64> {
    let candidate = iso_text.trim();
    if candidate.is_empty() {
        return None;
    }
    // Handle "Z" suffix
    let normalized = if candidate.ends_with('Z') {
        format!("{}+00:00", &candidate[..candidate.len() - 1])
    } else {
        candidate.to_string()
    };
    DateTime::parse_from_rfc3339(&normalized)
        .ok()
        .map(|dt| dt.timestamp())
        .or_else(|| {
            // Try chrono's more lenient ISO 8601 parsing
            chrono::NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|ndt| ndt.and_utc().timestamp())
        })
}

/// 从 summary 中提取 epoch 秒数（优先 remoteHash，其次 updatedAt）
pub fn summary_to_epoch_seconds(summary: &serde_json::Value) -> Option<i64> {
    let obj = summary.as_object()?;
    if let Some(rh) = obj.get("remoteHash") {
        if let Some(ts) = coerce_epoch_seconds(rh) {
            return Some(ts);
        }
    }
    if let Some(ua) = obj.get("updatedAt") {
        if let Some(s) = ua.as_str() {
            return iso_to_epoch_seconds(s);
        }
    }
    None
}

/// 账号目录 ID：邮箱小写后将非字母数字替换为下划线
pub fn email_to_account_id(email: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"[^a-z0-9]").unwrap());
    let normalized = email.trim().to_lowercase();
    re.replace_all(&normalized, "_").into_owned()
}

/// 邮箱脱敏：保留本地部分前3位
pub fn mask_email(email: &str) -> String {
    if email.is_empty() {
        return String::new();
    }
    match email.find('@') {
        Some(0) | None => {
            if email.len() > 3 {
                format!("{}***", &email[..3])
            } else {
                email.to_string()
            }
        }
        Some(at_pos) => {
            let visible = &email[..at_pos.min(3)];
            format!("{}***{}", visible, &email[at_pos..])
        }
    }
}

/// 确保对话 ID 带 c_ 前缀（用于 API 调用）
pub fn ensure_c_prefix(chat_id: &str) -> String {
    let cid = chat_id.trim();
    if cid.is_empty() {
        return cid.to_string();
    }
    if cid.starts_with("c_") {
        cid.to_string()
    } else {
        format!("c_{}", cid)
    }
}

/// 去除对话 ID 的 c_ 前缀（用于本地存储/显示）
pub fn strip_c_prefix(chat_id: &str) -> String {
    let cid = chat_id.trim();
    if let Some(stripped) = cid.strip_prefix("c_") {
        stripped.to_string()
    } else {
        cid.to_string()
    }
}

/// 诊断认证页面
pub fn diagnose_auth_page(html: &str, final_url: &str) -> String {
    let text = html.to_lowercase();
    let url_text = final_url.to_lowercase();
    let mut hints = Vec::new();

    if url_text.contains("accounts.google.com") || text.contains("servicelogin") {
        hints.push("命中 Google 登录页");
    }
    if url_text.contains("consent.google.com") {
        hints.push("命中 consent 页面");
    }
    if text.contains("unusual traffic") || url_text.contains("/sorry/") {
        hints.push("可能触发异常流量风控");
    }
    if text.contains("recaptcha") || text.contains("g-recaptcha") || text.contains("captcha") {
        hints.push("可能触发验证码挑战");
    }
    if hints.is_empty() {
        hints.push("页面结构变化或返回非 Gemini app 页面");
    }
    hints.join("；")
}

/// 从聊天列表条目提取最新更新时间（秒级时间戳）
pub fn extract_chat_latest_update(chat_item: &serde_json::Value) -> Option<i64> {
    let arr = chat_item.as_array()?;
    if arr.len() <= 5 {
        return None;
    }
    let field = &arr[5];
    let inner = field.as_array()?;
    if inner.is_empty() {
        return None;
    }
    inner[0].as_i64()
}

// ============================================================================
// batchexecute 响应解析
// ============================================================================

/// 按字符偏移（非字节偏移）从 str 中取子串。
/// batchexecute 响应中的长度前缀是 Unicode 字符数，非字节数。
fn char_substr(s: &str, char_start: usize, char_len: usize) -> &str {
    let mut iter = s.char_indices();
    // 定位 char_start
    let byte_start = if char_start == 0 {
        0
    } else {
        match iter.nth(char_start - 1) {
            Some((_, _)) => match iter.next() {
                Some((byte_pos, _)) => byte_pos,
                None => s.len(),
            },
            None => s.len(),
        }
    };
    // 从 byte_start 开始数 char_len 个字符
    let remaining = &s[byte_start..];
    let byte_end = remaining
        .char_indices()
        .nth(char_len)
        .map(|(pos, _)| byte_start + pos)
        .unwrap_or(s.len());
    &s[byte_start..byte_end]
}

/// 逐条产出 batchexecute 响应中的 wrb.fr 条目 (rpcid, raw_data)
///
/// 注意：响应中的长度前缀是 Unicode 字符数（与 Python len() 一致），
/// 而非 UTF-8 字节数。含多字节字符时必须按字符偏移切片。
fn iter_batchexecute_wrb_items(resp_text: &str) -> Vec<(String, Option<String>)> {
    let mut body = resp_text;
    // Strip )]}' prefix
    if body.starts_with(")]}'") {
        if let Some(nl_pos) = body.find('\n') {
            body = &body[nl_pos + 1..];
        }
    }
    let body = body.trim_start_matches(['\n', '\r']);

    let mut results = Vec::new();
    // char_pos 按字符偏移追踪
    let mut char_pos: usize = 0;
    let total_chars = body.chars().count();

    while char_pos < total_chars {
        // skip whitespace
        let rest = char_substr(body, char_pos, total_chars - char_pos);
        let skipped = rest
            .chars()
            .take_while(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
            .count();
        char_pos += skipped;
        if char_pos >= total_chars {
            break;
        }

        // find newline for length line
        let rest = char_substr(body, char_pos, total_chars - char_pos);
        let nl_offset = match rest.find('\n') {
            Some(byte_offset) => rest[..byte_offset].chars().count(),
            None => break,
        };
        let length_str = char_substr(body, char_pos, nl_offset);
        let length: usize = match length_str.parse() {
            Ok(l) => l,
            Err(_) => break,
        };
        char_pos += nl_offset + 1; // skip past newline

        let chunk_char_len = length.min(total_chars - char_pos);
        let chunk = char_substr(body, char_pos, chunk_char_len);
        char_pos += chunk_char_len;

        for line_data in chunk.split('\n') {
            let line_data = line_data.trim();
            if line_data.is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line_data) {
                if let Some(outer_arr) = parsed.as_array() {
                    for item in outer_arr {
                        if let Some(arr) = item.as_array() {
                            if arr.len() >= 2 && arr[0].as_str() == Some("wrb.fr") {
                                let rpcid = arr[1].as_str().unwrap_or("").to_string();
                                let raw = if arr.len() > 2 {
                                    arr[2].as_str().map(|s| s.to_string())
                                } else {
                                    None
                                };
                                results.push((rpcid, raw));
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

/// 解析 batchexecute 响应，返回 [(rpcid, data)] 仅含有效数据条目
pub fn parse_batchexecute_response(resp_text: &str) -> Vec<(String, serde_json::Value)> {
    let mut items = Vec::new();
    for (rpcid, raw) in iter_batchexecute_wrb_items(resp_text) {
        if let Some(raw_str) = raw {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&raw_str) {
                items.push((rpcid, data));
            }
        }
    }
    items
}

/// 检测响应中指定 rpcid 是否存在服务端会话错误
pub fn has_batchexecute_session_error(resp_text: &str, rpcid: &str) -> bool {
    for (rid, raw) in iter_batchexecute_wrb_items(resp_text) {
        if rid == rpcid && raw.is_none() {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_to_account_id() {
        assert_eq!(email_to_account_id("User@Gmail.Com"), "user_gmail_com");
        assert_eq!(
            email_to_account_id("  test.user@example.com  "),
            "test_user_example_com"
        );
    }

    #[test]
    fn test_ensure_c_prefix() {
        assert_eq!(ensure_c_prefix("abc123"), "c_abc123");
        assert_eq!(ensure_c_prefix("c_abc123"), "c_abc123");
        assert_eq!(ensure_c_prefix("  c_abc  "), "c_abc");
    }

    #[test]
    fn test_strip_c_prefix() {
        assert_eq!(strip_c_prefix("c_abc123"), "abc123");
        assert_eq!(strip_c_prefix("abc123"), "abc123");
        assert_eq!(strip_c_prefix("  c_abc  "), "abc");
    }

    #[test]
    fn test_mask_email() {
        assert_eq!(mask_email("user@gmail.com"), "use***@gmail.com");
        assert_eq!(mask_email("ab@x.com"), "ab***@x.com");
        assert_eq!(mask_email(""), "");
    }

    #[test]
    fn test_to_iso_utc() {
        assert!(to_iso_utc(Some(1700000000)).is_some());
        assert!(to_iso_utc(None).is_none());
    }

    #[test]
    fn test_iso_to_epoch_seconds() {
        let ts = iso_to_epoch_seconds("2023-11-14T22:13:20Z");
        assert_eq!(ts, Some(1700000000));
    }

    #[test]
    fn test_parse_batchexecute() {
        let resp = r#")]}'

30
[["wrb.fr","FooRpc","[1,2]"]]
"#;
        let items = parse_batchexecute_response(resp);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0, "FooRpc");
    }

    #[test]
    fn test_has_session_error() {
        let resp = r#")]}'

28
[["wrb.fr","FooRpc",null]]
"#;
        assert!(has_batchexecute_session_error(resp, "FooRpc"));
        assert!(!has_batchexecute_session_error(resp, "BarRpc"));
    }
}
