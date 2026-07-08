//! 业务 API：init_auth、聊天列表、对话详情、账号解析。
//!
//! 对应 Python GeminiExporter 中的：
//! - `init_auth` — 从 HTML 提取 at/bl/fsid
//! - `get_chats_page` / `get_all_chats` — MaZiqc RPC 分页
//! - `get_chat_detail_page` / `get_chat_detail` / `get_chat_detail_incremental` — hNvQHb RPC
//! - `_resolve_authuser` / `_resolve_account_info` / `list_user_options`

use regex::Regex;
use serde_json::json;
use std::collections::HashSet;
use std::sync::OnceLock;

use crate::browser_info;
use crate::cookies::list_accounts;
use crate::protocol::{
    diagnose_auth_page, email_to_account_id, extract_chat_latest_update, to_iso_utc, BATCH_SIZE,
    DETAIL_PAGE_SIZE, GEMINI_BASE,
};
use crate::storage;
use crate::str_err::ToStringErr;

use super::GeminiExporter;

/// 聊天列表条目
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatListItem {
    pub id: String,
    pub title: String,
    pub latest_update_ts: Option<i64>,
    pub latest_update_iso: Option<String>,
}

/// 账号信息
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub id: String,
    pub email: String,
    pub name: String,
    pub avatar_text: String,
    pub avatar_color: String,
    pub conversation_count: i64,
    pub remote_conversation_count: Option<i64>,
    pub last_sync_at: Option<String>,
    pub last_sync_result: Option<String>,
    pub authuser: Option<String>,
}

/// list_user_options 返回的用户选项
#[derive(Debug, Clone, serde::Serialize)]
pub struct UserOption {
    pub email: String,
    pub authuser: Option<String>,
    pub gemini_ok: Option<bool>,
    pub f_sid: Option<String>,
    pub redirect_url: Option<String>,
}

// ============================================================================
// 正则编译缓存
// ============================================================================

fn at_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""SNlM0e":"([^"]+)""#).unwrap())
}

fn bl_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""cfb2h":"([^"]+)""#).unwrap())
}

fn fsid_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#""FdrFJe":"(-?\d+)""#).unwrap())
}

fn title_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap())
}

// ============================================================================
// init_auth
// ============================================================================

impl GeminiExporter {
    /// 从 Gemini 页面提取认证参数 (at, bl, f.sid)。
    pub async fn init_auth(&mut self) -> Result<(), String> {
        log::info!("获取认证参数...");
        self.ensure_authuser().await?; // 对应 Python _authuser_params() 的懒解析
        let params = self.authuser_params();
        if let Some(ref au) = self.authuser {
            log::info!("  使用 authuser: {}", au);
            let _ = &params; // already includes authuser
        }

        let url = format!("{}/app", GEMINI_BASE);
        let navigate_headers: Vec<(&str, &str)> = vec![
            ("accept", browser_info::NAVIGATE_ACCEPT),
            ("sec-fetch-dest", browser_info::NAVIGATE_SEC_FETCH_DEST),
            ("sec-fetch-mode", browser_info::NAVIGATE_SEC_FETCH_MODE),
            ("sec-fetch-site", browser_info::NAVIGATE_SEC_FETCH_SITE),
            ("sec-fetch-user", browser_info::NAVIGATE_SEC_FETCH_USER),
            (
                "upgrade-insecure-requests",
                browser_info::NAVIGATE_UPGRADE_INSECURE_REQUESTS,
            ),
            (
                "x-browser-channel",
                browser_info::NAVIGATE_X_BROWSER_CHANNEL,
            ),
            ("x-browser-year", browser_info::browser_year()),
            ("x-browser-copyright", browser_info::browser_copyright()),
        ];
        let resp = self
            .client_get_with_retry(&url, &params, 6, &navigate_headers)
            .await?;

        let status = resp.status();
        let final_url = resp.url().to_string();
        let html = resp.text().await.str_err()?;

        if !status.is_success() {
            return Err(format!("获取 Gemini 页面失败: HTTP {}", status.as_u16()));
        }

        // 提取 SNlM0e (at token)
        match at_re().captures(&html) {
            Some(caps) => {
                self.at = Some(caps[1].to_string());
            }
            None => {
                let page_title = title_re()
                    .captures(&html)
                    .map(|c| c[1].split_whitespace().collect::<Vec<_>>().join(" "))
                    .unwrap_or_else(|| "-".to_string());
                let diagnosis = diagnose_auth_page(&html, &final_url);
                return Err(format!(
                    "无法提取 CSRF token (SNlM0e); url={}; title={}; hint={}",
                    final_url, page_title, diagnosis
                ));
            }
        }

        // 提取 cfb2h (bl)
        self.bl = bl_re()
            .captures(&html)
            .map(|c| c[1].to_string())
            .or_else(|| Some("boq_assistant-bard-web-server_20260210.04_p0".to_string()));

        // 提取 FdrFJe (fsid)
        self.fsid = fsid_re()
            .captures(&html)
            .map(|c| c[1].to_string())
            .or_else(|| Some("0".to_string()));

        Ok(())
    }

    // ========================================================================
    // authuser 解析
    // ========================================================================

    /// 解析用户指定账号：支持索引(0/1/2...)或邮箱。
    pub async fn resolve_authuser(&mut self) -> Result<(), String> {
        let user_spec = match &self.user_spec {
            Some(s) => s.clone(),
            None => return Ok(()),
        };

        // 纯数字直接当 authuser
        if user_spec.chars().all(|c| c.is_ascii_digit()) {
            self.authuser = Some(user_spec);
            return Ok(());
        }

        let email = user_spec.to_lowercase();

        // 优先走 ListAccounts 映射
        match list_accounts::discover_email_authuser_mapping(&self.cookies).await {
            Ok(mappings) => {
                for item in &mappings {
                    if item.email == email {
                        if let Some(ref au) = item.authuser {
                            self.authuser = Some(au.clone());
                            log::info!("  authuser: {} (ListAccounts 映射)", au);
                            return Ok(());
                        }
                    }
                }
            }
            Err(_) => {}
        }

        // 尝试通过页面内容匹配
        for idx in 0..10 {
            let params = vec![("authuser", idx.to_string())];
            match self
                .client_get_with_retry(&format!("{}/app", GEMINI_BASE), &params, 1, &[])
                .await
            {
                Ok(resp) => {
                    let text = resp.text().await.unwrap_or_default();
                    if text.to_lowercase().contains(&email) {
                        self.authuser = Some(idx.to_string());
                        log::info!("  authuser: {} (邮箱匹配)", idx);
                        return Ok(());
                    }
                }
                Err(_) => continue,
            }
        }

        // 无法匹配索引时，直接透传
        self.authuser = Some(user_spec);
        log::warn!("未匹配到邮箱对应索引，改为直接使用邮箱作为 authuser");
        Ok(())
    }

    /// 确保 authuser 已解析
    pub async fn ensure_authuser(&mut self) -> Result<(), String> {
        if self.authuser.is_none() {
            self.resolve_authuser().await?;
        }
        Ok(())
    }

    // ========================================================================
    // 聊天列表
    // ========================================================================

    /// 拉取单页聊天列表。
    ///
    /// cursor 为 None 时拉第一页；否则按 next token 拉后续页。
    pub async fn get_chats_page(
        &self,
        cursor: Option<&str>,
    ) -> Result<(Vec<ChatListItem>, Option<String>), String> {
        let payload = match cursor {
            None => json!([BATCH_SIZE, null, [0, null, 1]]).to_string(),
            Some(c) => json!([BATCH_SIZE, c]).to_string(),
        };

        let result = self.batchexecute("MaZiqc", &payload, "/app").await?;

        if !result.is_array() {
            return Ok((Vec::new(), None));
        }
        let arr = result.as_array().unwrap();

        let next_token = arr
            .get(1)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        let raw_chats = arr
            .get(2)
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut items = Vec::new();
        for chat in &raw_chats {
            let chat_arr = match chat.as_array() {
                Some(a) if a.len() > 1 => a,
                _ => continue,
            };
            let conv_id = chat_arr[0].as_str().unwrap_or("").to_string();
            let title = chat_arr
                .get(1)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let latest_update_ts = extract_chat_latest_update(chat);
            let latest_update_iso = latest_update_ts.and_then(|ts| to_iso_utc(Some(ts)));

            items.push(ChatListItem {
                id: conv_id,
                title,
                latest_update_ts,
                latest_update_iso,
            });
        }

        Ok((items, next_token))
    }

    /// 获取所有聊天列表（含分页）。
    pub async fn get_all_chats(&self) -> Result<Vec<ChatListItem>, String> {
        log::info!("获取聊天列表...");
        let mut all_chats = Vec::new();
        let mut page = 0u32;
        let mut cursor: Option<String> = None;

        loop {
            page += 1;
            let (items, next_token) = self.get_chats_page(cursor.as_deref()).await?;

            if items.is_empty() && next_token.is_none() {
                if page == 1 {
                    log::debug!("首屏未拿到聊天列表");
                }
                break;
            }

            let count = items.len();
            all_chats.extend(items);
            log::info!(
                "  第 {} 页: {} 个对话 (累计 {})",
                page,
                count,
                all_chats.len()
            );

            match next_token {
                Some(t) => cursor = Some(t),
                None => break,
            }
        }

        log::info!("  共 {} 个对话", all_chats.len());
        Ok(all_chats)
    }

    // ========================================================================
    // 对话详情
    // ========================================================================

    /// 拉取单页会话详情。
    ///
    /// 返回: (raw_turns, next_cursor)
    pub async fn get_chat_detail_page(
        &self,
        conv_id: &str,
        cursor: Option<&str>,
    ) -> Result<(Vec<serde_json::Value>, Option<String>), String> {
        let source_path = format!("/app/{}", crate::protocol::strip_c_prefix(conv_id));
        let payload = json!([conv_id, DETAIL_PAGE_SIZE, cursor, 1, [1], [4], null, 1]).to_string();

        let result = self.batchexecute("hNvQHb", &payload, &source_path).await?;

        if !result.is_array() {
            return Ok((Vec::new(), None));
        }
        let arr = result.as_array().unwrap();

        let turns = arr
            .first()
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let next_cursor = arr
            .get(1)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        Ok((turns, next_cursor))
    }

    /// 获取单个对话的完整内容（含分页）。
    pub async fn get_chat_detail(&self, conv_id: &str) -> Result<Vec<serde_json::Value>, String> {
        let mut all_turns = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let (turns, next_cursor) = self
                .get_chat_detail_page(conv_id, cursor.as_deref())
                .await?;

            if turns.is_empty() && next_cursor.is_none() {
                break;
            }

            all_turns.extend(turns);

            match next_cursor {
                Some(t) => cursor = Some(t),
                None => break,
            }
        }

        Ok(all_turns)
    }

    /// 增量抓取单个对话：遇到已存在 turn_id 即停止向旧页翻。
    pub async fn get_chat_detail_incremental(
        &self,
        conv_id: &str,
        existing_turn_ids: &HashSet<String>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let mut all_new_turns = Vec::new();
        let source_path = format!("/app/{}", crate::protocol::strip_c_prefix(conv_id));

        let payload = json!([conv_id, DETAIL_PAGE_SIZE, null, 1, [1], [4], null, 1]).to_string();
        let result = self.batchexecute("hNvQHb", &payload, &source_path).await?;

        let mut current_result = result;
        loop {
            let arr = match current_result.as_array() {
                Some(a) => a,
                None => break,
            };

            let turns = arr
                .first()
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            if turns.is_empty() {
                break;
            }

            let mut hit_existing = false;
            for turn in &turns {
                let tid = storage::turn_id_from_raw_pub(turn);
                if let Some(ref tid) = tid {
                    if existing_turn_ids.contains(tid) {
                        hit_existing = true;
                        break;
                    }
                }
                all_new_turns.push(turn.clone());
            }

            if hit_existing {
                break;
            }

            let next_token = arr
                .get(1)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());

            match next_token {
                Some(t) => {
                    let payload =
                        json!([conv_id, DETAIL_PAGE_SIZE, t, 1, [1], [4], null, 1]).to_string();
                    current_result = self.batchexecute("hNvQHb", &payload, &source_path).await?;
                }
                None => break,
            }
        }

        Ok(all_new_turns)
    }

    // ========================================================================
    // 账号信息
    // ========================================================================

    /// 解析当前账号信息。
    pub async fn resolve_account_info(&mut self) -> Result<AccountInfo, String> {
        let mut email: Option<String> = None;

        self.ensure_authuser().await?;
        let authuser_str = self
            .authuser
            .as_ref()
            .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
            .cloned();

        // 如果有外部指定的 account_id
        if let Some(ref override_id) = self.account_id_override {
            email = self.account_email_override.clone().or_else(|| {
                self.user_spec
                    .as_ref()
                    .filter(|s| s.contains('@'))
                    .map(|s| s.to_lowercase())
            });

            let name = email
                .as_ref()
                .map(|e| e.split('@').next().unwrap_or("").to_string())
                .unwrap_or_else(|| override_id.clone());
            let avatar_text = name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());

            return Ok(AccountInfo {
                id: override_id.clone(),
                email: email.unwrap_or_default(),
                name,
                avatar_text,
                avatar_color: "#667eea".to_string(),
                conversation_count: 0,
                remote_conversation_count: None,
                last_sync_at: None,
                last_sync_result: None,
                authuser: authuser_str,
            });
        }

        // 从 user_spec 或 ListAccounts 获取 email
        if let Some(ref spec) = self.user_spec {
            if spec.contains('@') {
                email = Some(spec.to_lowercase());
            }
        }

        if email.is_none() {
            if let Ok(mappings) =
                list_accounts::discover_email_authuser_mapping(&self.cookies).await
            {
                if let Some(ref au) = authuser_str {
                    for m in &mappings {
                        if m.authuser.as_ref() == Some(au) {
                            email = Some(m.email.clone());
                            break;
                        }
                    }
                }
                if email.is_none() && !mappings.is_empty() {
                    email = Some(mappings[0].email.clone());
                }
            }
        }

        if let Some(ref e) = email {
            let safe_id = email_to_account_id(e);
            let name = e.split('@').next().unwrap_or("").to_string();
            let avatar_text = name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_else(|| "?".to_string());

            return Ok(AccountInfo {
                id: safe_id,
                email: e.clone(),
                name,
                avatar_text,
                avatar_color: "#667eea".to_string(),
                conversation_count: 0,
                remote_conversation_count: None,
                last_sync_at: None,
                last_sync_result: None,
                authuser: authuser_str,
            });
        }

        // 兜底
        let authuser = authuser_str.unwrap_or_else(|| "0".to_string());
        let acc_id = format!("user_{}", authuser);
        Ok(AccountInfo {
            id: acc_id.clone(),
            email: String::new(),
            name: acc_id,
            avatar_text: "U".to_string(),
            avatar_color: "#667eea".to_string(),
            conversation_count: 0,
            remote_conversation_count: None,
            last_sync_at: None,
            last_sync_result: None,
            authuser: Some(authuser),
        })
    }

    /// 列出可选邮箱及其 authuser 映射。
    pub async fn list_user_options(&self) -> Result<Vec<UserOption>, String> {
        let mappings = list_accounts::discover_email_authuser_mapping(&self.cookies).await?;

        // 去重
        let mut dedup = std::collections::HashMap::<String, &list_accounts::AccountMapping>::new();
        for item in &mappings {
            if item.email.is_empty() {
                continue;
            }
            dedup
                .entry(item.email.clone())
                .and_modify(|existing| {
                    if existing.authuser.is_none() && item.authuser.is_some() {
                        *existing = item;
                    }
                })
                .or_insert(item);
        }

        let mut result = Vec::new();
        for (email, item) in &dedup {
            let mut gemini_ok: Option<bool> = None;
            let mut fsid: Option<String> = None;

            if let Some(ref au) = item.authuser {
                let mut probe =
                    GeminiExporter::new(self.cookies.clone(), Some(au.clone()), None, None);
                match probe.init_auth().await {
                    Ok(()) => {
                        gemini_ok = Some(true);
                        fsid = probe.fsid.clone();
                    }
                    Err(_) => {
                        gemini_ok = Some(false);
                    }
                }
            }

            result.push(UserOption {
                email: email.clone(),
                authuser: item.authuser.clone(),
                gemini_ok,
                f_sid: fsid,
                redirect_url: item.redirect_url.clone(),
            });
        }

        result.sort_by(|a, b| {
            let a_none = a.authuser.is_none() as u8;
            let b_none = b.authuser.is_none() as u8;
            a_none
                .cmp(&b_none)
                .then_with(|| {
                    let a_num: u32 = a
                        .authuser
                        .as_ref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(999);
                    let b_num: u32 = b
                        .authuser
                        .as_ref()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(999);
                    a_num.cmp(&b_num)
                })
                .then_with(|| a.email.cmp(&b.email))
        });

        Ok(result)
    }
}
