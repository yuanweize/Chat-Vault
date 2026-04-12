//! 导出编排逻辑：export_list_only、sync_single_conversation。
//!
//! 对应 Python gemini_export.py 中的同名方法 + gemini_export_cli.py 的 export_incremental。
//! Phase 5: 直接作为 async 方法调用，不再走子进程 IPC。

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::{json, Value};

/// 轻量取消令牌（基于 AtomicBool）
#[derive(Clone)]
pub struct CancellationToken {
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

use crate::gemini_api::media_download::DownloadStats;
use crate::gemini_api::GeminiExporter;
use crate::protocol::{coerce_epoch_seconds, summary_to_epoch_seconds};
use crate::storage;
use crate::str_err::ToStringErr;
use crate::turn_parser;

/// export_list_only 返回值
#[derive(Debug, Clone)]
pub struct ListSyncResult {
    pub remote_count: usize,
    pub lost_count: usize,
    pub updated_ids: Vec<String>,
}

/// sync_single_conversation 返回值
#[derive(Debug, Clone)]
pub struct ConvSyncResult {
    pub conversation_id: String,
}

// ============================================================================
// export_list_only
// ============================================================================

impl GeminiExporter {
    /// 仅同步会话列表（分页），不拉取对话详情。
    ///
    /// `stop_on_unchanged`: 命中首个本地已有且时间戳相同的会话时提前终止。
    pub async fn export_list_only(
        &self,
        output_dir: &Path,
        stop_on_unchanged: bool,
        cancel: &CancellationToken,
    ) -> Result<ListSyncResult, String> {
        let base_dir = output_dir.to_path_buf();
        std::fs::create_dir_all(&base_dir).str_err()?;

        let account_info = self.resolve_account_info_readonly().await?;
        let account_id = &account_info["id"].as_str().unwrap_or("").to_string();
        let account_dir = base_dir.join("accounts").join(account_id);
        let conv_dir = account_dir.join("conversations");
        let media_dir = account_dir.join("media");

        std::fs::create_dir_all(&conv_dir).str_err()?;
        std::fs::create_dir_all(&media_dir).str_err()?;

        log::info!("仅同步列表到: {}", account_dir.display());

        let (existing_order, existing_index) = storage::load_conversations_index(&account_dir);
        let sync_state = storage::load_sync_state(&account_dir);
        let full_sync = sync_state.get("fullSync").and_then(|v| v.as_object());

        let started_at_default = chrono::Utc::now().to_rfc3339();
        let mut started_at = started_at_default.clone();
        let baseline_existing_ids: Vec<String> = existing_order.clone();

        let mut fetched_order: Vec<String> = Vec::new();
        let mut fetched_seen: HashSet<String> = HashSet::new();
        let mut resume_cursor: Option<String> = None;

        // 断点续传
        if let Some(fs) = full_sync {
            if fs.get("phase").and_then(|v| v.as_str()) == Some("listing") {
                if let Some(sa) = fs.get("startedAt").and_then(|v| v.as_str()) {
                    started_at = sa.to_string();
                }
                resume_cursor = fs
                    .get("listingCursor")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                if let Some(ids) = fs.get("listingFetchedIds").and_then(|v| v.as_array()) {
                    for id in ids {
                        if let Some(s) = id.as_str().filter(|s| !s.is_empty()) {
                            if fetched_seen.insert(s.to_string()) {
                                fetched_order.push(s.to_string());
                            }
                        }
                    }
                }
            }
        }

        if resume_cursor.is_some() {
            log::info!("检测到上次列表同步中断，继续从 cursor 拉取...");
        } else {
            log::info!("从第一页开始拉取列表...");
            fetched_order.clear();
            fetched_seen.clear();
        }

        let mut conv_index: HashMap<String, Value> = existing_index.clone();
        let mut updated_ids: Vec<String> = Vec::new();
        let mut stop_early = false;
        let mut cursor = resume_cursor;
        let mut page = 0u32;

        loop {
            page += 1;
            let t_page = std::time::Instant::now();
            let (chats, next_cursor) = self.get_chats_page(cursor.as_deref()).await?;

            if chats.is_empty() && next_cursor.is_none() {
                // 完成
                break;
            }

            for chat in &chats {
                let bare_id = crate::protocol::strip_c_prefix(&chat.id);
                if bare_id.is_empty() {
                    continue;
                }

                let chat_val = json!({
                    "id": chat.id,
                    "title": chat.title,
                    "latest_update_ts": chat.latest_update_ts,
                    "latest_update_iso": chat.latest_update_iso,
                });

                let existing = conv_index
                    .get(&bare_id)
                    .or_else(|| existing_index.get(&bare_id));
                conv_index.insert(
                    bare_id.clone(),
                    storage::build_summary_from_chat_listing(&chat_val, existing),
                );

                if fetched_seen.insert(bare_id.clone()) {
                    fetched_order.push(bare_id.clone());
                }

                let remote_ts = chat.latest_update_ts;
                let local_ts = summary_to_epoch_seconds(
                    existing_index.get(&bare_id).unwrap_or(&Value::Null),
                );

                if let (Some(r_ts), Some(l_ts)) = (remote_ts, local_ts) {
                    if r_ts > l_ts {
                        updated_ids.push(bare_id.clone());
                    } else if stop_on_unchanged && r_ts == l_ts {
                        log::info!("  [stop] 命中未更新会话，停止列表扫描: {}", bare_id);
                        stop_early = true;
                        break;
                    }
                }
            }

            if stop_early {
                break;
            }

            if cancel.is_cancelled() {
                return Err("用户取消列表同步".to_string());
            }

            log::info!(
                "  第 {} 页: {} 个对话 (累计 {}) {}ms",
                page,
                chats.len(),
                fetched_order.len(),
                t_page.elapsed().as_millis()
            );

            // 每页落盘
            let phase = if next_cursor.is_none() {
                "done"
            } else {
                "listing"
            };
            persist_list_state(
                &account_dir,
                account_id,
                &account_info,
                &base_dir,
                &started_at,
                phase,
                next_cursor.as_deref(),
                &fetched_order,
                &baseline_existing_ids,
                &conv_index,
                &existing_index,
                stop_early,
            )?;

            match next_cursor {
                Some(t) => cursor = Some(t),
                None => break,
            }
        }

        // 最终落盘
        let lost_count = persist_list_state(
            &account_dir,
            account_id,
            &account_info,
            &base_dir,
            &started_at,
            "done",
            None,
            &fetched_order,
            &baseline_existing_ids,
            &conv_index,
            &existing_index,
            stop_early,
        )?;

        if stop_early {
            log::info!(
                "列表同步完成（提前终止）: 共 {} 个对话",
                fetched_order.len()
            );
        } else {
            log::info!("列表同步完成: 共 {} 个对话", fetched_order.len());
        }

        Ok(ListSyncResult {
            remote_count: fetched_order.len(),
            lost_count,
            updated_ids,
        })
    }
}

/// 落盘列表同步状态，返回 lost_count
fn persist_list_state(
    account_dir: &Path,
    account_id: &str,
    account_info: &Value,
    base_dir: &Path,
    started_at: &str,
    phase: &str,
    cursor: Option<&str>,
    fetched_order: &[String],
    baseline_existing_ids: &[String],
    conv_index: &HashMap<String, Value>,
    existing_index: &HashMap<String, Value>,
    stopped_early: bool,
) -> Result<usize, String> {
    let now_iso = chrono::Utc::now().to_rfc3339();
    let remote_count = fetched_order.len();
    let mut lost_count = 0usize;

    let summaries: Vec<Value> = if phase == "done" {
        if stopped_early {
            build_partial_summaries(fetched_order, conv_index, existing_index, baseline_existing_ids)
        } else {
            let total_cap = fetched_order.len() + baseline_existing_ids.len();
            let mut result = Vec::with_capacity(total_cap);
            let mut remote_set = HashSet::with_capacity(fetched_order.len());
            for cid in fetched_order {
                if let Some(summary) = conv_index.get(cid).or_else(|| existing_index.get(cid)) {
                    result.push(summary.clone());
                    remote_set.insert(cid.as_str());
                }
            }
            for cid in baseline_existing_ids {
                if remote_set.contains(cid.as_str()) {
                    continue;
                }
                result.push(storage::build_lost_summary(cid, existing_index.get(cid)));
                lost_count += 1;
            }
            result
        }
    } else {
        build_partial_summaries(fetched_order, conv_index, existing_index, baseline_existing_ids)
    };

    let listing_cursor = if phase == "done" { None } else { cursor };
    let listing_fetched_ids: Vec<Value> = if phase == "done" {
        Vec::new()
    } else {
        fetched_order.iter().map(|s| json!(s)).collect()
    };

    // 读取现有 pendingConversations
    let current_state = storage::load_sync_state(account_dir);
    let pending = current_state
        .get("pendingConversations")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    storage::write_conversations_index(account_dir, account_id, &now_iso, &summaries)
        .str_err()?;
    storage::write_sync_state(
        account_dir,
        &json!({
            "version": 1,
            "accountId": account_id,
            "updatedAt": now_iso,
            "concurrency": 1,
            "fullSync": {
                "phase": phase,
                "startedAt": started_at,
                "listingCursor": listing_cursor,
                "listingTotal": if phase == "done" { json!(remote_count) } else { Value::Null },
                "listingFetched": remote_count,
                "listingFetchedIds": listing_fetched_ids,
                "conversationsToFetch": [],
                "conversationsFetched": 0,
                "conversationsFailed": [],
                "completedAt": if phase == "done" { json!(now_iso) } else { Value::Null },
                "errorMessage": Value::Null,
                "baselineIds": if phase != "done" { json!(baseline_existing_ids) } else { Value::Null },
                "lostCount": if phase == "done" { json!(lost_count) } else { Value::Null },
            },
            "pendingConversations": pending,
        }),
    )
    .str_err()?;

    let mut info = account_info.clone();
    let info_obj = info.as_object_mut().unwrap();
    info_obj.insert("conversationCount".into(), json!(summaries.len()));
    if phase == "done" {
        info_obj.insert("remoteConversationCount".into(), json!(remote_count));
        info_obj.insert("lastSyncResult".into(), json!("success"));
    }
    info_obj.insert("lastSyncAt".into(), json!(now_iso));

    storage::write_accounts_json(base_dir, &info).str_err()?;
    storage::write_account_meta(account_dir, &info).str_err()?;

    Ok(lost_count)
}

fn build_partial_summaries(
    fetched_order: &[String],
    conv_index: &HashMap<String, Value>,
    existing_index: &HashMap<String, Value>,
    baseline_existing_ids: &[String],
) -> Vec<Value> {
    let cap = fetched_order.len() + baseline_existing_ids.len();
    let mut summaries = Vec::with_capacity(cap);
    let mut seen = HashSet::with_capacity(cap);
    for cid in fetched_order {
        let summary = conv_index.get(cid).or_else(|| existing_index.get(cid));
        if let Some(s) = summary {
            if seen.insert(cid.as_str()) {
                summaries.push(s.clone());
            }
        }
    }
    for cid in baseline_existing_ids {
        if seen.contains(cid.as_str()) {
            continue;
        }
        if let Some(s) = existing_index.get(cid) {
            summaries.push(s.clone());
            seen.insert(cid.as_str());
        }
    }
    summaries
}

// ============================================================================
// sync_single_conversation
// ============================================================================

impl GeminiExporter {
    /// 解析账号信息（只读版本，使用已有 authuser）
    pub async fn resolve_account_info_readonly(&self) -> Result<Value, String> {
        use crate::cookies::list_accounts;
        use crate::protocol::email_to_account_id;

        // 如果有外部指定的 account_id
        if let Some(ref override_id) = self.account_id_override {
            let email = self
                .account_email_override
                .clone()
                .or_else(|| {
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
            let authuser = self
                .authuser
                .as_ref()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                .cloned();

            return Ok(json!({
                "id": override_id,
                "email": email.unwrap_or_default(),
                "name": name,
                "avatarText": avatar_text,
                "avatarColor": "#667eea",
                "conversationCount": 0,
                "remoteConversationCount": Value::Null,
                "lastSyncAt": Value::Null,
                "lastSyncResult": Value::Null,
                "authuser": authuser,
            }));
        }

        // 从 user_spec 或 ListAccounts 获取 email
        let mut email: Option<String> = None;
        if let Some(ref spec) = self.user_spec {
            if spec.contains('@') {
                email = Some(spec.to_lowercase());
            }
        }

        if email.is_none() {
            if let Ok(mappings) =
                list_accounts::discover_email_authuser_mapping(&self.cookies).await
            {
                let authuser_str = self
                    .authuser
                    .as_ref()
                    .filter(|s| s.chars().all(|c| c.is_ascii_digit()));
                if let Some(au) = authuser_str {
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
            let authuser = self
                .authuser
                .as_ref()
                .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
                .cloned();

            return Ok(json!({
                "id": safe_id,
                "email": e,
                "name": name,
                "avatarText": avatar_text,
                "avatarColor": "#667eea",
                "conversationCount": 0,
                "remoteConversationCount": Value::Null,
                "lastSyncAt": Value::Null,
                "lastSyncResult": Value::Null,
                "authuser": authuser,
            }));
        }

        // 兜底
        let authuser = self
            .authuser
            .clone()
            .unwrap_or_else(|| "0".to_string());
        let acc_id = format!("user_{}", authuser);
        Ok(json!({
            "id": acc_id,
            "email": "",
            "name": acc_id,
            "avatarText": "U",
            "avatarColor": "#667eea",
            "conversationCount": 0,
            "remoteConversationCount": Value::Null,
            "lastSyncAt": Value::Null,
            "lastSyncResult": Value::Null,
            "authuser": authuser,
        }))
    }

    /// 同步单个会话详情（含媒体），并更新该账号本地索引。
    pub async fn sync_single_conversation(
        &self,
        conversation_id: &str,
        output_dir: &Path,
        cancel: &CancellationToken,
    ) -> Result<ConvSyncResult, String> {
        let base_dir = output_dir.to_path_buf();
        std::fs::create_dir_all(&base_dir).str_err()?;

        let account_info = self.resolve_account_info_readonly().await?;
        let account_id = account_info["id"].as_str().unwrap_or("").to_string();
        let account_dir = base_dir.join("accounts").join(&account_id);
        let conv_dir = account_dir.join("conversations");
        let media_dir = account_dir.join("media");

        std::fs::create_dir_all(&conv_dir).str_err()?;
        std::fs::create_dir_all(&media_dir).str_err()?;

        let bare_id = crate::protocol::strip_c_prefix(conversation_id);
        let conv_id = crate::protocol::ensure_c_prefix(conversation_id);
        let jsonl_file = conv_dir.join(format!("{}.jsonl", bare_id));
        let local_jsonl_exists = jsonl_file.exists();
        let detail_mode = if local_jsonl_exists {
            "incremental"
        } else {
            "full"
        };

        log::info!("同步单会话: {}", conv_id);

        // 失败媒体重试
        let mut pre_stats = DownloadStats::default();
        let retry_result = self
            .retry_failed_media_for_conversation(&jsonl_file, &account_dir, &media_dir, &mut pre_stats)
            .await;
        if retry_result.attempted > 0 || retry_result.missing_url > 0 {
            log::info!(
                "  [media-retry] attempted={}, recovered={}, failed={}, missing_url={}",
                retry_result.attempted, retry_result.recovered, retry_result.failed, retry_result.missing_url
            );
        }

        let (_, existing_index) = storage::load_conversations_index(&account_dir);
        let existing_summary = existing_index.get(&bare_id).cloned().unwrap_or(json!({}));
        let existing_status = existing_summary
            .get("status")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("normal")
            .to_string();

        let latest_update_ts = existing_summary
            .get("remoteHash")
            .and_then(|v| coerce_epoch_seconds(v));
        let chat_info = json!({
            "id": conv_id,
            "title": existing_summary.get("title").and_then(|v| v.as_str()).unwrap_or(&bare_id),
            "latest_update_ts": latest_update_ts,
            "latest_update_iso": existing_summary.get("updatedAt"),
        });
        let title = chat_info["title"].as_str().unwrap_or(&bare_id).to_string();

        // 抓取详情（逐页拉取，每页更新索引中的条数）
        let existing_turn_ids = if local_jsonl_exists {
            storage::build_existing_turn_id_set_new(&jsonl_file)
        } else {
            HashSet::new()
        };
        let is_incremental = detail_mode == "incremental";

        // 已有消息数（用于增量模式的基数）
        let existing_msg_count = existing_summary
            .get("messageCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let mut raw_turns: Vec<Value> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page_num = 0u32;

        loop {
            if cancel.is_cancelled() {
                return Err("用户取消单会话同步".to_string());
            }

            let (page_turns, next_cursor) = self
                .get_chat_detail_page(&conv_id, cursor.as_deref())
                .await?;

            if page_turns.is_empty() && next_cursor.is_none() {
                break;
            }

            // 增量模式：遇到已有 turn 即停止
            let mut hit_existing = false;
            if is_incremental {
                for turn in &page_turns {
                    let tid = storage::turn_id_from_raw_pub(turn);
                    if let Some(ref tid) = tid {
                        if existing_turn_ids.contains(tid) {
                            hit_existing = true;
                            break;
                        }
                    }
                    raw_turns.push(turn.clone());
                }
            } else {
                raw_turns.extend(page_turns);
            }

            page_num += 1;

            // 每页拉取后更新索引中的消息条数（粗估：每个 turn ≈ 2 条消息行）
            if page_num > 1 || hit_existing {
                let estimated_count = if is_incremental {
                    existing_msg_count + raw_turns.len() * 2
                } else {
                    raw_turns.len() * 2
                };
                update_intermediate_message_count(
                    &account_dir, &account_id, &bare_id,
                    &existing_index, estimated_count, &existing_summary,
                );
            }

            if hit_existing {
                break;
            }

            match next_cursor {
                Some(t) => cursor = Some(t),
                None => break,
            }
        }

        let (raw_turns, removed_turns) = storage::dedupe_raw_turns_by_id(&raw_turns);
        if removed_turns > 0 {
            log::info!("  [dedupe] 分页结果去重: {} 个重复 turn", removed_turns);
        }

        if cancel.is_cancelled() {
            return Err("用户取消单会话同步".to_string());
        }

        // 加载媒体清单
        let mut global_seen_urls = storage::load_media_manifest(&account_dir);
        let mut global_used_names: HashSet<String> = global_seen_urls.values().cloned().collect();
        if media_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&media_dir) {
                for entry in entries.flatten() {
                    if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                        if let Some(name) = entry.file_name().to_str() {
                            global_used_names.insert(name.to_string());
                        }
                    }
                }
            }
        }

        let mut media_stats = DownloadStats {
            media_downloaded: pre_stats.media_downloaded,
            media_failed: pre_stats.media_failed,
        };

        let summary = if detail_mode == "incremental" && local_jsonl_exists {
            self.sync_conversation_incremental(
                &raw_turns,
                &conv_id,
                &bare_id,
                &account_id,
                &title,
                &chat_info,
                &jsonl_file,
                &account_dir,
                &media_dir,
                &mut global_seen_urls,
                &mut global_used_names,
                &mut media_stats,
                &existing_summary,
                &existing_status,
                cancel,
            )
            .await?
        } else {
            self.sync_conversation_full(
                &raw_turns,
                &conv_id,
                &bare_id,
                &account_id,
                &title,
                &chat_info,
                &jsonl_file,
                &account_dir,
                &media_dir,
                &mut global_seen_urls,
                &mut global_used_names,
                &mut media_stats,
                &existing_status,
                cancel,
            )
            .await?
        };

        // 更新索引：直接构建排序列表，避免额外 clone
        let mut summaries = Vec::with_capacity(existing_index.len() + 1);
        for (key, val) in &existing_index {
            if key != &bare_id {
                summaries.push(val.clone());
            }
        }
        summaries.push(summary);
        summaries.sort_by(|a, b| {
            let ts_a = updated_sort_num(a);
            let ts_b = updated_sort_num(b);
            ts_b.partial_cmp(&ts_a).unwrap_or(std::cmp::Ordering::Equal)
        });

        let now_iso = chrono::Utc::now().to_rfc3339();
        let mut info = account_info.clone();
        let info_obj = info.as_object_mut().unwrap();
        info_obj.insert("conversationCount".into(), json!(summaries.len()));
        let current_remote = info_obj
            .get("remoteConversationCount")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        info_obj.insert(
            "remoteConversationCount".into(),
            json!(current_remote.max(summaries.len() as i64)),
        );
        info_obj.insert("lastSyncAt".into(), json!(now_iso));
        info_obj.insert("lastSyncResult".into(), json!("success"));

        storage::write_accounts_json(&base_dir, &info).str_err()?;
        storage::write_account_meta(&account_dir, &info).str_err()?;
        storage::write_conversations_index(&account_dir, &account_id, &now_iso, &summaries)
            .str_err()?;

        // 更新 sync_state 中的 pendingConversations
        let mut state = storage::load_sync_state(&account_dir);
        let state_obj = state.as_object_mut().unwrap();
        state_obj.insert("updatedAt".into(), json!(now_iso));
        // 移除已完成的 pending entry
        if let Some(pending) = state_obj.get_mut("pendingConversations").and_then(|v| v.as_array_mut()) {
            pending.retain(|item| {
                item.get("id").and_then(|v| v.as_str()) != Some(&bare_id)
            });
        }
        storage::write_sync_state(&account_dir, &state).str_err()?;

        log::info!("单会话完成: {}", conv_id);

        Ok(ConvSyncResult {
            conversation_id: conv_id,
        })
    }

    /// 全量模式同步单个会话
    async fn sync_conversation_full(
        &self,
        raw_turns: &[Value],
        conv_id: &str,
        bare_id: &str,
        account_id: &str,
        title: &str,
        chat_info: &Value,
        jsonl_file: &Path,
        account_dir: &Path,
        media_dir: &Path,
        global_seen_urls: &mut HashMap<String, String>,
        global_used_names: &mut HashSet<String>,
        media_stats: &mut DownloadStats,
        existing_status: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        log::info!("  轮次: {}", raw_turns.len());

        let mut parsed_turns: Vec<Value> = raw_turns
            .iter()
            .map(|t| turn_parser::parse_turn_to_value(t))
            .collect();
        turn_parser::normalize_turn_media_first_seen_values(&mut parsed_turns);

        let batch_list = self.assign_media_ids_and_collect_downloads(
            &mut parsed_turns,
            media_dir,
            global_seen_urls,
            global_used_names,
        );

        let rows = storage::turns_to_jsonl_rows(&parsed_turns, conv_id, account_id, title, chat_info, media_dir);
        storage::write_jsonl_rows(jsonl_file, &rows).str_err()?;

        let mut failed_items = Vec::new();
        if !batch_list.is_empty() {
            log::info!("  媒体文件: {} 个（去重后）", batch_list.len());
            failed_items = self.download_media_batch(&batch_list, media_stats).await;
            storage::save_media_manifest(account_dir, global_seen_urls)
                .str_err()?;
        }

        // 更新媒体失败标记
        let batch_media_ids: HashSet<String> = batch_list.iter().map(|i| i.media_id.clone()).collect();
        let failed_map: HashMap<String, String> = failed_items
            .iter()
            .map(|i| (i.media_id.clone(), i.error.clone()))
            .collect();
        let recovered_ids: HashSet<String> = batch_media_ids
            .difference(&failed_map.keys().cloned().collect())
            .cloned()
            .collect();
        let _ = storage::update_jsonl_media_failure_flags(jsonl_file, &failed_map, &recovered_ids);

        if cancel.is_cancelled() {
            return Err("用户取消单会话同步".to_string());
        }

        build_conversation_summary(jsonl_file, bare_id, title, &rows, existing_status)
    }

    /// 增量模式同步单个会话
    async fn sync_conversation_incremental(
        &self,
        raw_turns: &[Value],
        conv_id: &str,
        bare_id: &str,
        account_id: &str,
        title: &str,
        chat_info: &Value,
        jsonl_file: &Path,
        account_dir: &Path,
        media_dir: &Path,
        global_seen_urls: &mut HashMap<String, String>,
        global_used_names: &mut HashSet<String>,
        media_stats: &mut DownloadStats,
        existing_summary: &Value,
        existing_status: &str,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        if raw_turns.is_empty() {
            // 无新增 turn，返回现有 summary
            let mut summary = if existing_summary.is_object() {
                existing_summary.clone()
            } else {
                json!({})
            };
            if let Some(obj) = summary.as_object_mut() {
                obj.insert("id".into(), json!(bare_id));
            }
            return Ok(summary);
        }

        let mut parsed_new_turns: Vec<Value> = raw_turns
            .iter()
            .map(|t| turn_parser::parse_turn_to_value(t))
            .collect();
        turn_parser::normalize_turn_media_first_seen_values(&mut parsed_new_turns);

        let batch_list = self.assign_media_ids_and_collect_downloads(
            &mut parsed_new_turns,
            media_dir,
            global_seen_urls,
            global_used_names,
        );

        let new_rows_full =
            storage::turns_to_jsonl_rows(&parsed_new_turns, conv_id, account_id, title, chat_info, media_dir);
        let new_meta = &new_rows_full[0];
        let new_msg_rows = &new_rows_full[1..];

        // 读取现有 message 行
        let mut existing_msg_rows = Vec::new();
        let mut existing_created_at: Option<String> = None;
        if jsonl_file.exists() {
            let content = std::fs::read_to_string(jsonl_file).str_err()?;
            let mut meta_found = false;
            for line in content.lines() {
                let s = line.trim();
                if s.is_empty() {
                    continue;
                }
                let row: Value = match serde_json::from_str(s) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if !meta_found && row.get("type").and_then(|v| v.as_str()) == Some("meta") {
                    meta_found = true;
                    existing_created_at = row.get("createdAt").and_then(|v| v.as_str()).map(|s| s.to_string());
                    continue;
                }
                existing_msg_rows.push(row);
            }
        }

        // 合并
        let (merged_msg_rows, removed_msg_rows) =
            storage::merge_message_rows_for_write(new_msg_rows, &existing_msg_rows)
                .str_err()?;
        if removed_msg_rows > 0 {
            log::info!("  [dedupe] 合并写盘去重: {} 行", removed_msg_rows);
        }

        let mut meta = new_meta.clone();
        if let Some(ca) = existing_created_at {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("createdAt".into(), json!(ca));
            }
        }

        let mut all_rows = vec![meta];
        all_rows.extend(merged_msg_rows);
        storage::write_jsonl_rows(jsonl_file, &all_rows).str_err()?;

        let mut failed_items = Vec::new();
        if !batch_list.is_empty() {
            log::info!("  媒体文件: {} 个（去重后）", batch_list.len());
            failed_items = self.download_media_batch(&batch_list, media_stats).await;
            storage::save_media_manifest(account_dir, global_seen_urls)
                .str_err()?;
        }

        let batch_media_ids: HashSet<String> = batch_list.iter().map(|i| i.media_id.clone()).collect();
        let failed_map: HashMap<String, String> = failed_items
            .iter()
            .map(|i| (i.media_id.clone(), i.error.clone()))
            .collect();
        let recovered_ids: HashSet<String> = batch_media_ids
            .difference(&failed_map.keys().cloned().collect())
            .cloned()
            .collect();
        let _ = storage::update_jsonl_media_failure_flags(jsonl_file, &failed_map, &recovered_ids);

        if cancel.is_cancelled() {
            return Err("用户取消单会话同步".to_string());
        }

        log::info!("  新增 turn: {}", raw_turns.len());
        build_conversation_summary(jsonl_file, bare_id, title, &all_rows, existing_status)
    }
}

/// 从 JSONL 构建会话摘要
fn build_conversation_summary(
    jsonl_file: &Path,
    bare_id: &str,
    title: &str,
    fallback_rows: &[Value],
    existing_status: &str,
) -> Result<Value, String> {
    let rows = storage::read_jsonl_rows(jsonl_file);
    let rows_ref = if rows.is_empty() { fallback_rows } else { &rows };

    let meta_row = rows_ref
        .iter()
        .find(|r| r.get("type").and_then(|v| v.as_str()) == Some("meta"))
        .cloned()
        .unwrap_or(json!({}));

    let msg_rows: Vec<&Value> = rows_ref
        .iter()
        .filter(|r| r.get("type").and_then(|v| v.as_str()) == Some("message"))
        .collect();

    let has_media = msg_rows.iter().any(|r| {
        r.get("attachments")
            .and_then(|v| v.as_array())
            .map(|a| !a.is_empty())
            .unwrap_or(false)
    });
    let owned_msg_rows: Vec<Value> = msg_rows.iter().map(|r| (*r).clone()).collect();
    let has_failed_data = storage::rows_has_failed_data(&owned_msg_rows);
    let (image_count, video_count, _audio_count) = storage::count_media_types_from_rows(&owned_msg_rows);

    let mut last_text = String::new();
    for r in msg_rows.iter().rev() {
        if let Some(text) = r.get("text").and_then(|v| v.as_str()) {
            if !text.is_empty() {
                last_text = text.chars().take(80).collect();
                break;
            }
        }
    }

    let status = if existing_status == "hidden" {
        "hidden"
    } else {
        "normal"
    };

    Ok(json!({
        "id": bare_id,
        "title": title,
        "lastMessage": last_text,
        "messageCount": msg_rows.len(),
        "hasMedia": has_media,
        "hasFailedData": has_failed_data,
        "imageCount": image_count,
        "videoCount": video_count,
        "updatedAt": meta_row.get("updatedAt"),
        "remoteHash": meta_row.get("remoteHash"),
        "status": status,
    }))
}

fn updated_sort_num(summary: &Value) -> f64 {
    summary
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|dt| dt.timestamp() as f64)
        })
        .unwrap_or(0.0)
}

/// 每页拉取后，更新索引中该会话的 messageCount（粗估值），让前端轮询及时看到进度。
fn update_intermediate_message_count(
    account_dir: &Path,
    account_id: &str,
    bare_id: &str,
    existing_index: &HashMap<String, Value>,
    estimated_count: usize,
    existing_summary: &Value,
) {
    let mut updated = existing_summary.clone();
    if let Some(obj) = updated.as_object_mut() {
        obj.insert("id".into(), json!(bare_id));
        obj.insert("messageCount".into(), json!(estimated_count));
    }
    // 直接构建排序列表，避免 clone 整个 index
    let mut summaries = Vec::with_capacity(existing_index.len());
    for (key, val) in existing_index {
        if key == bare_id {
            summaries.push(updated.clone());
        } else {
            summaries.push(val.clone());
        }
    }
    if !existing_index.contains_key(bare_id) {
        summaries.push(updated);
    }
    summaries.sort_by(|a, b| {
        let ts_a = updated_sort_num(a);
        let ts_b = updated_sort_num(b);
        ts_b.partial_cmp(&ts_a).unwrap_or(std::cmp::Ordering::Equal)
    });
    let now_iso = chrono::Utc::now().to_rfc3339();
    let _ = storage::write_conversations_index(account_dir, account_id, &now_iso, &summaries);
}
