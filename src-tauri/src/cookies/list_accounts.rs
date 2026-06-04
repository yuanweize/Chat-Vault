//! Google ListAccounts API 调用：发现 email ↔ authuser 映射。

use crate::browser_info;
use crate::protocol::GEMINI_BASE;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// 单个账号映射条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct AccountMapping {
    pub email: String,
    pub authuser: Option<String>,
    pub redirect_url: Option<String>,
}

fn post_message_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?s)postMessage\('(.+?)'\s*,\s*'[^']*'\)")
            .unwrap()
    })
}

/// 构建 Cookie 请求头值
fn build_cookie_header(cookies: &HashMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 对 unicode escape 序列做解码（\uXXXX → 字符）
fn decode_unicode_escapes(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek() {
                Some('x') => {
                    chars.next(); // consume 'x'
                    let mut hex = String::new();
                    for _ in 0..2 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 2 {
                        if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                            result.push(byte as char);
                            continue;
                        }
                    }
                    result.push('\\');
                    result.push('x');
                    result.push_str(&hex);
                }
                Some('u') => {
                    chars.next(); // consume 'u'
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4 {
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(c) = char::from_u32(code) {
                                result.push(c);
                                continue;
                            }
                        }
                    }
                    // Fallback: output as-is
                    result.push('\\');
                    result.push('u');
                    result.push_str(&hex);
                }
                Some('/') => {
                    chars.next();
                    result.push('/');
                }
                Some('\\') => {
                    chars.next();
                    result.push('\\');
                }
                Some('n') => {
                    chars.next();
                    result.push('\n');
                }
                Some('r') => {
                    chars.next();
                    result.push('\r');
                }
                Some('t') => {
                    chars.next();
                    result.push('\t');
                }
                Some('"') => {
                    chars.next();
                    result.push('"');
                }
                _ => {
                    result.push('\\');
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

/// 通过 ListAccounts API 获取 email ↔ authuser 映射。
///
/// 需要传入已从浏览器读取的 Google cookies。
pub async fn discover_email_authuser_mapping(
    cookies: &HashMap<String, String>,
) -> Result<Vec<AccountMapping>, String> {
    let list_accounts_url = "https://accounts.google.com/ListAccounts";
    let params = [
        ("authuser", "0"),
        ("listPages", "1"),
        ("fwput", "10"),
        ("rdr", "2"),
        ("pid", "658"),
        ("gpsia", "1"),
        ("source", "ogb"),
        ("atic", "1"),
        ("mo", "1"),
        ("mn", "1"),
        ("hl", "zh-CN"),
        ("ts", "641"),
    ];

    let cookie_header = build_cookie_header(cookies);

    let client = reqwest::Client::builder()
        .user_agent(browser_info::build_user_agent())
        .redirect(reqwest::redirect::Policy::limited(10))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

    let resp = client
        .get(list_accounts_url)
        .query(&params)
        .header("Accept-Language", browser_info::detect_accept_language())
        .header("sec-ch-ua", browser_info::build_sec_ch_ua())
        .header("sec-ch-ua-mobile", "?0")
        .header("sec-ch-ua-platform", browser_info::platform_hint())
        .header("Referer", format!("{}/app", GEMINI_BASE))
        .header("Origin", GEMINI_BASE)
        .header("Cookie", cookie_header)
        .send()
        .await
        .map_err(|e| format!("ListAccounts 请求失败: {}", e))?;

    let status = resp.status();

    if !status.is_success() {
        return Err(format!("ListAccounts HTTP {}", status.as_u16()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("读取 ListAccounts 响应失败: {}", e))?;

    log::info!("ListAccounts HTTP {}, body length={}", status.as_u16(), body.len());

    parse_list_accounts_response(&body)
}

/// 解析 ListAccounts 响应，提取账号映射。
fn parse_list_accounts_response(body: &str) -> Result<Vec<AccountMapping>, String> {
    // Try postMessage format first
    if let Some(captures) = post_message_re().captures(body) {
        if let Some(m) = captures.get(1) {
            let payload_raw = m.as_str();
            let payload_unescaped = payload_raw.replace("\\/", "/");
            let payload = decode_unicode_escapes(&payload_unescaped);
            log::info!("ListAccounts postMessage payload matched, length={}", payload.len());
            return parse_list_accounts_json(&payload);
        }
    }

    // Fallback: try to find raw JSON array directly (newer response format)
    let trimmed = body.trim();
    for prefix in &[")]}'", ")]}'\n"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.starts_with('[') {
                log::info!("ListAccounts JSON (stripped prefix) matched, length={}", rest.len());
                return parse_list_accounts_json(rest);
            }
        }
    }
    if trimmed.starts_with('[') {
        log::info!("ListAccounts raw JSON matched, length={}", trimmed.len());
        return parse_list_accounts_json(trimmed);
    }

    log::warn!("ListAccounts 响应无法匹配任何格式，body length={}", body.len());
    Err(format!(
        "ListAccounts 响应格式无法解析（body length={}）",
        body.len()
    ))
}

fn parse_list_accounts_json(payload: &str) -> Result<Vec<AccountMapping>, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| format!("ListAccounts JSON 解析失败: {}", e))?;

    let rows = parsed
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "ListAccounts 数据结构异常：缺少 parsed[1] 数组".to_string())?;

    let mut result = Vec::new();
    let mut seen_email: HashSet<String> = HashSet::new();

    for row in rows {
        let row_arr = match row.as_array() {
            Some(a) if a.len() >= 4 => a,
            _ => continue,
        };

        let email = match row_arr[3].as_str() {
            Some(s) if !s.trim().is_empty() => s.trim().to_lowercase(),
            _ => continue,
        };

        if seen_email.contains(&email) {
            continue;
        }
        seen_email.insert(email.clone());

        let authuser = row_arr.get(7).and_then(|v| {
            if let Some(n) = v.as_i64() {
                Some(n.to_string())
            } else if let Some(s) = v.as_str() {
                if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() {
                    Some(s.to_string())
                } else {
                    None
                }
            } else {
                None
            }
        });

        let redirect_url = authuser.as_ref().map(|au| {
            if au == "0" {
                format!("{}/app", GEMINI_BASE)
            } else {
                format!("{}/u/{}/app", GEMINI_BASE, au)
            }
        });

        result.push(AccountMapping {
            email,
            authuser,
            redirect_url,
        });
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_unicode_escapes() {
        assert_eq!(decode_unicode_escapes(r"hello\u0020world"), "hello world");
        assert_eq!(decode_unicode_escapes(r"a\/b"), "a/b");
        assert_eq!(
            decode_unicode_escapes(r"\u4f60\u597d"),
            "你好"
        );
        // \xNN hex escapes (Google ListAccounts format)
        assert_eq!(decode_unicode_escapes(r"\x5b\x22hi\x22\x5d"), r#"["hi"]"#);
        assert_eq!(decode_unicode_escapes(r"\x41\x42"), "AB");
    }

    #[test]
    fn test_build_cookie_header() {
        let mut cookies = HashMap::new();
        cookies.insert("SID".into(), "abc".into());
        cookies.insert("HSID".into(), "def".into());
        let header = build_cookie_header(&cookies);
        // Order may vary, but both should be present
        assert!(header.contains("SID=abc"));
        assert!(header.contains("HSID=def"));
        assert!(header.contains("; "));
    }

    #[test]
    fn test_parse_list_accounts_response() {
        // Simulate a ListAccounts response body
        let body = r#"<script>
            window.parent.postMessage('["",[["gaia_id_1","","","user1@gmail.com",null,null,null,0]]]', '*');
        </script>"#;
        let result = parse_list_accounts_response(body).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].email, "user1@gmail.com");
        assert_eq!(result[0].authuser.as_deref(), Some("0"));
        assert!(result[0].redirect_url.as_deref().unwrap().contains("/app"));
    }

    #[test]
    fn test_parse_list_accounts_multi() {
        let body = r#"<script>
            window.parent.postMessage('["",[["id1","","","alice@gmail.com",null,null,null,0],["id2","","","bob@gmail.com",null,null,null,1]]]', '*');
        </script>"#;
        let result = parse_list_accounts_response(body).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].email, "alice@gmail.com");
        assert_eq!(result[0].authuser.as_deref(), Some("0"));
        assert_eq!(result[1].email, "bob@gmail.com");
        assert_eq!(result[1].authuser.as_deref(), Some("1"));
        assert!(result[1].redirect_url.as_deref().unwrap().contains("/u/1/app"));
    }

    #[test]
    fn test_parse_list_accounts_dedup() {
        let body = r#"<script>
            window.parent.postMessage('["",[["id1","","","user@gmail.com",null,null,null,0],["id2","","","user@gmail.com",null,null,null,1]]]', '*');
        </script>"#;
        let result = parse_list_accounts_response(body).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_parse_list_accounts_hex_escapes() {
        // Real Google format uses \xNN hex escapes
        let body = r#"<script>window.parent.postMessage('\x5b\x22gaia.l.a.r\x22,\x5b\x5b\x22gaia.l.a\x22,1,\x22Name\x22,\x22test@gmail.com\x22,null,1,1,0,null,1\x5d\x5d\x5d', '*');</script>"#;
        let result = parse_list_accounts_response(body).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].email, "test@gmail.com");
        assert_eq!(result[0].authuser.as_deref(), Some("0"));
    }

    #[test]
    fn test_parse_list_accounts_missing_postmessage() {
        let body = "<html><body>No accounts here</body></html>";
        let result = parse_list_accounts_response(body);
        assert!(result.is_err());
    }
}
