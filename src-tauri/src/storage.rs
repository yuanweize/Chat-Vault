//! Gemini 数据持久化：JSONL 读写、账号元数据、sync state、媒体清单、对话索引。

use crate::protocol::{coerce_epoch_seconds, iso_to_epoch_seconds, to_iso_utc};
use crate::str_err::ToStringErr;
use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use uuid::Uuid;

// ============================================================================
// 对话状态常量
// ============================================================================
pub const CONVERSATION_STATUS_NORMAL: &str = "normal";
pub const CONVERSATION_STATUS_LOST: &str = "lost";
pub const CONVERSATION_STATUS_HIDDEN: &str = "hidden";

// ============================================================================
// JSONL 读写与去重
// ============================================================================

pub fn read_jsonl_rows(jsonl_file: &Path) -> Vec<Value> {
    if !jsonl_file.exists() {
        return Vec::new();
    }
    let content = match std::fs::read_to_string(jsonl_file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub fn write_jsonl_rows(jsonl_file: &Path, rows: &[Value]) -> Result<()> {
    let mut content = String::new();
    for row in rows {
        content.push_str(&serde_json::to_string(row)?);
        content.push('\n');
    }
    std::fs::write(jsonl_file, content)?;
    Ok(())
}

/// 从原始 turn JSON 提取 turn_id（公开版本，供 gemini_api 使用）
pub fn turn_id_from_raw_pub(raw_turn: &Value) -> Option<String> {
    turn_id_from_raw(raw_turn)
}

fn turn_id_from_raw(raw_turn: &Value) -> Option<String> {
    let arr = raw_turn.as_array()?;
    let ids = arr.first()?.as_array()?;
    if ids.len() > 1 {
        ids[1].as_str().map(|s| s.to_string())
    } else {
        ids.first()?.as_str().map(|s| s.to_string())
    }
}

/// Generic dedup: items without an ID (extractor returns None or empty) are always kept.
fn dedupe_by_id<F>(items: &[Value], id_extractor: F) -> (Vec<Value>, usize)
where
    F: Fn(&Value) -> Option<String>,
{
    let mut deduped = Vec::with_capacity(items.len());
    let mut seen = HashSet::with_capacity(items.len());
    let mut removed = 0;
    for item in items {
        if let Some(id) = id_extractor(item) {
            if !id.is_empty() {
                if seen.contains(&id) {
                    removed += 1;
                    continue;
                }
                seen.insert(id);
            }
        }
        deduped.push(item.clone());
    }
    (deduped, removed)
}

pub fn dedupe_raw_turns_by_id(raw_turns: &[Value]) -> (Vec<Value>, usize) {
    dedupe_by_id(raw_turns, |v| turn_id_from_raw(v))
}

pub fn dedupe_message_rows_by_id(rows: &[Value]) -> (Vec<Value>, usize) {
    dedupe_by_id(rows, |v| get_row_id(v).map(|s| s.to_string()))
}

fn get_row_id(row: &Value) -> Option<&str> {
    row.as_object()?.get("id")?.as_str()
}

fn message_row_sort_num(row: &Value) -> f64 {
    let obj = match row.as_object() {
        Some(o) => o,
        None => return f64::NEG_INFINITY,
    };
    let ts = match obj.get("timestamp").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() => s,
        _ => return f64::NEG_INFINITY,
    };
    iso_to_epoch_seconds(ts)
        .map(|e| e as f64)
        .unwrap_or(f64::NEG_INFINITY)
}

fn is_message_rows_sorted(rows: &[Value]) -> bool {
    let mut prev = f64::NEG_INFINITY;
    for row in rows {
        let cur = message_row_sort_num(row);
        if cur < prev {
            return false;
        }
        prev = cur;
    }
    true
}

pub fn merge_message_rows_for_write(
    new_msg_rows: &[Value],
    existing_msg_rows: &[Value],
) -> Result<(Vec<Value>, usize)> {
    let (new_deduped, removed_new) = dedupe_message_rows_by_id(new_msg_rows);

    let new_ids: HashSet<String> = new_deduped
        .iter()
        .filter_map(|r| get_row_id(r))
        .filter(|id| !id.is_empty())
        .map(|s| s.to_string())
        .collect();

    let mut removed_existing_by_new = 0usize;
    let existing_without_new: Vec<Value> = existing_msg_rows
        .iter()
        .filter(|row| {
            if let Some(id) = get_row_id(row) {
                if !id.is_empty() && new_ids.contains(id) {
                    removed_existing_by_new += 1;
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect();

    let (existing_deduped, removed_existing_dup) = dedupe_message_rows_by_id(&existing_without_new);
    let removed_total = removed_new + removed_existing_by_new + removed_existing_dup;

    if !is_message_rows_sorted(&new_deduped) {
        anyhow::bail!("new_msg_rows 必须按 timestamp 升序");
    }
    if !is_message_rows_sorted(&existing_deduped) {
        anyhow::bail!("existing_msg_rows 必须按 timestamp 升序");
    }

    // Linear merge
    let mut merged = Vec::with_capacity(new_deduped.len() + existing_deduped.len());
    let (mut i, mut j) = (0, 0);
    while i < new_deduped.len() && j < existing_deduped.len() {
        if message_row_sort_num(&new_deduped[i]) <= message_row_sort_num(&existing_deduped[j]) {
            merged.push(new_deduped[i].clone());
            i += 1;
        } else {
            merged.push(existing_deduped[j].clone());
            j += 1;
        }
    }
    merged.extend_from_slice(&new_deduped[i..]);
    merged.extend_from_slice(&existing_deduped[j..]);

    Ok((merged, removed_total))
}

// ============================================================================
// 媒体文件 / 清单
// ============================================================================

pub fn is_media_file_ready(media_dir: &Path, media_id: &str) -> bool {
    if media_id.is_empty() {
        return false;
    }
    let p = media_dir.join(media_id);
    p.exists() && p.metadata().map(|m| m.len() > 0).unwrap_or(false)
}

pub fn load_media_manifest(dir: &Path) -> HashMap<String, String> {
    let manifest_file = dir.join("media_manifest.json");
    if !manifest_file.exists() {
        return HashMap::new();
    }
    let content = match std::fs::read_to_string(&manifest_file) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let data: Value = match serde_json::from_str(&content) {
        Ok(d) => d,
        Err(_) => return HashMap::new(),
    };
    let url_map = data
        .as_object()
        .and_then(|o| o.get("url_to_name"))
        .and_then(|v| v.as_object());
    match url_map {
        Some(map) => map
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        None => HashMap::new(),
    }
}

pub fn save_media_manifest(dir: &Path, url_to_name: &HashMap<String, String>) -> Result<()> {
    let manifest_file = dir.join("media_manifest.json");
    let data = json!({ "url_to_name": url_to_name });
    std::fs::write(
        &manifest_file,
        serde_json::to_string_pretty(&data)?,
    )?;
    Ok(())
}

pub fn build_media_id_to_url_map(account_dir: &Path) -> HashMap<String, String> {
    let url_to_name = load_media_manifest(account_dir);
    let mut media_to_url = HashMap::new();
    for (url, media_name) in &url_to_name {
        media_to_url.entry(media_name.clone()).or_insert_with(|| url.clone());
    }
    media_to_url
}

// ============================================================================
// 失败媒体扫描与重试标记
// ============================================================================

pub struct FailedMediaEntry {
    pub media_id: String,
    pub url: Option<String>,
    pub error: String,
}

pub fn scan_failed_media_from_rows(
    rows: &[Value],
    media_dir: &Path,
    media_id_to_url: &HashMap<String, String>,
) -> (Vec<FailedMediaEntry>, HashSet<String>) {
    let mut pending = Vec::new();
    let mut recovered = HashSet::new();
    let mut seen_pending = HashSet::new();

    for row in rows {
        let obj = match row.as_object() {
            Some(o) if o.get("type").and_then(|v| v.as_str()) == Some("message") => o,
            _ => continue,
        };
        let attachments = match obj.get("attachments").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for att in attachments {
            let att_obj = match att.as_object() {
                Some(o) => o,
                None => continue,
            };
            let media_id = match att_obj.get("mediaId").and_then(|v| v.as_str()) {
                Some(id) if !id.is_empty() => id,
                _ => continue,
            };
            let file_ready = is_media_file_ready(media_dir, media_id);
            let marked_failed = att_obj
                .get("downloadFailed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if file_ready {
                if marked_failed {
                    recovered.insert(media_id.to_string());
                }
                continue;
            }
            if seen_pending.contains(media_id) {
                continue;
            }
            seen_pending.insert(media_id.to_string());

            let error = att_obj
                .get("downloadError")
                .and_then(|v| v.as_str())
                .unwrap_or("download_failed")
                .to_string();
            pending.push(FailedMediaEntry {
                media_id: media_id.to_string(),
                url: media_id_to_url.get(media_id).cloned(),
                error,
            });
        }
    }
    (pending, recovered)
}

pub fn update_jsonl_media_failure_flags(
    jsonl_file: &Path,
    failed_error_map: &HashMap<String, String>,
    recovered_ids: &HashSet<String>,
) -> Result<HashMap<String, usize>> {
    let mut rows = read_jsonl_rows(jsonl_file);
    if rows.is_empty() {
        let mut r = HashMap::new();
        r.insert("marked".into(), 0);
        r.insert("cleared".into(), 0);
        return Ok(r);
    }

    let mut marked = 0usize;
    let mut cleared = 0usize;
    let mut changed = false;

    for row in &mut rows {
        let obj = match row.as_object_mut() {
            Some(o) if o.get("type").and_then(|v| v.as_str()) == Some("message") => o,
            _ => continue,
        };
        let attachments = match obj.get_mut("attachments").and_then(|v| v.as_array_mut()) {
            Some(a) => a,
            None => continue,
        };
        for att in attachments {
            let att_obj = match att.as_object_mut() {
                Some(o) => o,
                None => continue,
            };
            let media_id = match att_obj.get("mediaId").and_then(|v| v.as_str()) {
                Some(id) if !id.is_empty() => id.to_string(),
                _ => continue,
            };

            if recovered_ids.contains(&media_id) {
                let had_failed = att_obj.contains_key("downloadFailed");
                let had_error = att_obj.contains_key("downloadError");
                if had_failed {
                    att_obj.remove("downloadFailed");
                }
                if had_error {
                    att_obj.remove("downloadError");
                }
                if had_failed || had_error {
                    changed = true;
                    cleared += 1;
                }
                continue;
            }

            if let Some(error_text) = failed_error_map.get(&media_id) {
                let current_failed = att_obj
                    .get("downloadFailed")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let current_error = att_obj
                    .get("downloadError")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if !current_failed || current_error != error_text {
                    att_obj.insert("downloadFailed".into(), json!(true));
                    att_obj.insert("downloadError".into(), json!(error_text));
                    changed = true;
                    marked += 1;
                }
            }
        }
    }

    if changed {
        write_jsonl_rows(jsonl_file, &rows)?;
    }

    let mut result = HashMap::new();
    result.insert("marked".into(), marked);
    result.insert("cleared".into(), cleared);
    Ok(result)
}

// ============================================================================
// Turn ID / 行集合工具
// ============================================================================

pub fn build_existing_turn_id_set(existing_rows: &[Value]) -> HashSet<String> {
    existing_rows
        .iter()
        .filter_map(|row| {
            row.as_object()?
                .get("turn_id")?
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .collect()
}

pub fn latest_ts_from_rows(rows: &[Value]) -> Option<i64> {
    rows.iter()
        .filter_map(|row| row.as_object()?.get("timestamp")?.as_i64())
        .max()
}

pub fn build_existing_turn_id_set_new(jsonl_file: &Path) -> HashSet<String> {
    let mut ids = HashSet::new();
    if !jsonl_file.exists() {
        return ids;
    }
    let content = match std::fs::read_to_string(jsonl_file) {
        Ok(c) => c,
        Err(_) => return ids,
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let row: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if row.get("type").and_then(|v| v.as_str()) != Some("message") {
            continue;
        }
        if let Some(msg_id) = row.get("id").and_then(|v| v.as_str()) {
            if msg_id.ends_with("_u") || msg_id.ends_with("_m") {
                ids.insert(msg_id[..msg_id.len() - 2].to_string());
            }
        }
    }
    ids
}

pub fn count_message_rows_new(jsonl_file: &Path) -> usize {
    if !jsonl_file.exists() {
        return 0;
    }
    let content = match std::fs::read_to_string(jsonl_file) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|row| row.get("type").and_then(|v| v.as_str()) == Some("message"))
        .count()
}

pub fn count_media_types_from_rows(rows: &[Value]) -> (usize, usize, usize) {
    let (mut images, mut videos, mut audios) = (0, 0, 0);
    for row in rows {
        let obj = match row.as_object() {
            Some(o) if o.get("type").and_then(|v| v.as_str()) == Some("message") => o,
            _ => continue,
        };
        let attachments = match obj.get("attachments").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for att in attachments {
            if let Some(mime) = att.as_object().and_then(|o| o.get("mimeType")).and_then(|v| v.as_str()) {
                let lower = mime.to_lowercase();
                if lower.starts_with("image/") {
                    images += 1;
                } else if lower.starts_with("video/") {
                    videos += 1;
                } else if lower.starts_with("audio/") {
                    audios += 1;
                }
            }
        }
    }
    (images, videos, audios)
}

pub fn rows_has_failed_data(rows: &[Value]) -> bool {
    for row in rows {
        let obj = match row.as_object() {
            Some(o) if o.get("type").and_then(|v| v.as_str()) == Some("message") => o,
            _ => continue,
        };
        let attachments = match obj.get("attachments").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for att in attachments {
            if att
                .as_object()
                .and_then(|o| o.get("downloadFailed"))
                .and_then(|v| v.as_bool())
                == Some(true)
            {
                return true;
            }
        }
    }
    false
}

pub fn remote_hash_from_jsonl(jsonl_file: &Path) -> Option<String> {
    if !jsonl_file.exists() {
        return None;
    }
    let content = std::fs::read_to_string(jsonl_file).ok()?;
    let first_line = content.lines().next()?.trim();
    if first_line.is_empty() {
        return None;
    }
    let row: Value = serde_json::from_str(first_line).ok()?;
    if row.get("type")?.as_str()? != "meta" {
        return None;
    }
    row.get("remoteHash")?.as_str().map(|s| s.to_string())
}

// ============================================================================
// Turn → JSONL 转换
// ============================================================================

fn sort_parsed_turns_by_timestamp(parsed_turns: &[Value]) -> Vec<Value> {
    if parsed_turns.is_empty() {
        return Vec::new();
    }
    let mut indexed: Vec<(usize, &Value)> = parsed_turns.iter().enumerate().collect();
    indexed.sort_by(|(idx_a, a), (idx_b, b)| {
        let ts_a = a
            .as_object()
            .and_then(|o| o.get("timestamp"))
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX);
        let ts_b = b
            .as_object()
            .and_then(|o| o.get("timestamp"))
            .and_then(|v| v.as_i64())
            .unwrap_or(i64::MAX);
        ts_a.cmp(&ts_b).then(idx_a.cmp(idx_b))
    });
    indexed.into_iter().map(|(_, v)| v.clone()).collect()
}

pub fn turns_to_jsonl_rows(
    parsed_turns: &[Value],
    conv_id: &str,
    account_id: &str,
    title: &str,
    chat_info: &Value,
    media_dir: &Path,
) -> Vec<Value> {
    let now_iso = Utc::now().to_rfc3339();
    let bare_id = crate::protocol::strip_c_prefix(conv_id);
    let ordered_turns = sort_parsed_turns_by_timestamp(parsed_turns);

    let ts_list: Vec<i64> = ordered_turns
        .iter()
        .filter_map(|t| t.as_object()?.get("timestamp")?.as_i64())
        .collect();
    let created_at_ts = ts_list.iter().copied().min();

    let chat_obj = chat_info.as_object();
    let remote_ts = chat_obj
        .and_then(|o| o.get("latest_update_ts"))
        .and_then(|v| coerce_epoch_seconds(v))
        .or_else(|| ts_list.iter().copied().max());

    let updated_at = to_iso_utc(remote_ts).or_else(|| {
        chat_obj
            .and_then(|o| o.get("latest_update_iso"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    });

    let created_at = to_iso_utc(created_at_ts)
        .or_else(|| updated_at.clone())
        .unwrap_or_else(|| now_iso.clone());

    let remote_hash = remote_ts.map(|ts| ts.to_string());

    let mut rows = vec![json!({
        "type": "meta",
        "id": bare_id,
        "accountId": account_id,
        "title": title,
        "createdAt": created_at,
        "updatedAt": updated_at,
        "remoteHash": remote_hash,
    })];

    for turn in &ordered_turns {
        let turn_obj = match turn.as_object() {
            Some(o) => o,
            None => continue,
        };
        let turn_id = turn_obj
            .get("turn_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string().replace("-", ""));

        let ts = turn_obj
            .get("timestamp")
            .and_then(|v| v.as_i64())
            .and_then(|t| to_iso_utc(Some(t)))
            .unwrap_or_else(|| now_iso.clone());

        // User message
        let user = turn_obj.get("user").and_then(|v| v.as_object());
        let user_text = user
            .and_then(|u| u.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let user_attachments = build_attachments(user.and_then(|u| u.get("files")));

        rows.push(json!({
            "type": "message",
            "id": format!("{}_u", turn_id),
            "role": "user",
            "text": user_text,
            "attachments": user_attachments,
            "timestamp": ts,
        }));

        // Assistant message
        let asst = turn_obj.get("assistant").and_then(|v| v.as_object());
        let asst_text = asst
            .and_then(|a| a.get("text"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let asst_attachments = build_attachments(asst.and_then(|a| a.get("files")));
        let model = asst
            .and_then(|a| a.get("model"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Deep Research report turn：assistant 行用 ai[12][8]["57"][0][5] 的报告完成时间
        // （user 行保持原 turn[4][0]，不受影响）。字段缺失时回落到 turn ts。
        let model_ts = asst
            .and_then(|a| a.get("deep_research"))
            .and_then(|dr| {
                if dr.get("type").and_then(|v| v.as_str()) == Some("report") {
                    dr.get("completion_ts").and_then(|v| v.as_i64())
                } else {
                    None
                }
            })
            .and_then(|t| to_iso_utc(Some(t)))
            .unwrap_or_else(|| ts.clone());

        let mut model_row = json!({
            "type": "message",
            "id": format!("{}_m", turn_id),
            "role": "model",
            "text": asst_text,
            "attachments": asst_attachments,
            "timestamp": model_ts,
            "model": model,
        });

        if let Some(thinking) = asst.and_then(|a| a.get("thinking")).and_then(|v| v.as_str()) {
            if !thinking.is_empty() {
                model_row["thinking"] = json!(thinking);
            }
        }
        if let Some(music_meta) = asst.and_then(|a| a.get("music_meta")) {
            if !music_meta.is_null() {
                model_row["musicMeta"] = music_meta.clone();
            }
        }
        if let Some(gen_meta) = asst.and_then(|a| a.get("gen_meta")) {
            if !gen_meta.is_null() {
                model_row["genMeta"] = gen_meta.clone();
            }
        }
        if let Some(deep_research) = asst.and_then(|a| a.get("deep_research")) {
            if !deep_research.is_null() {
                let mut dr = deep_research.clone();
                // 报告正文外置到 media 文件
                if dr.get("type").and_then(|v| v.as_str()) == Some("report") {
                    if let Some(text) = dr.get("report_text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            let media_id = format!("{}.md", Uuid::new_v4().to_string().replace("-", ""));
                            let size_bytes = text.as_bytes().len();
                            let char_count = text.chars().count();
                            let _ = std::fs::write(media_dir.join(&media_id), text.as_bytes());
                            dr.as_object_mut().map(|o| {
                                o.remove("report_text");
                                o.insert("report_media_id".to_string(), json!(media_id));
                                o.insert("size_bytes".to_string(), json!(size_bytes));
                                o.insert("char_count".to_string(), json!(char_count));
                            });
                        }
                    }
                    // 调研过程外置到 JSON media 文件，并注入统计字段
                    if let Some(entries) = dr.get("progress").and_then(|v| v.as_array()).cloned() {
                        if !entries.is_empty() {
                            let entry_count = entries.len();
                            let mut rounds: i64 = 0;
                            let mut thinking_count: usize = 0;
                            let mut web_count: usize = 0;
                            let mut file_count: usize = 0;
                            for e in &entries {
                                match e.get("type").and_then(|v| v.as_str()) {
                                    Some("thinking") => {
                                        thinking_count += 1;
                                        if let Some(r) = e.get("round").and_then(|v| v.as_i64()) {
                                            if r + 1 > rounds { rounds = r + 1; }
                                        }
                                    }
                                    Some("web_search") => web_count += 1,
                                    Some("file_search") => file_count += 1,
                                    _ => {}
                                }
                            }
                            let payload = Value::Array(entries);
                            let serialized = serde_json::to_vec(&payload).unwrap_or_else(|_| b"[]".to_vec());
                            let media_id = format!("{}.json", Uuid::new_v4().to_string().replace("-", ""));
                            let size_bytes = serialized.len();
                            let _ = std::fs::write(media_dir.join(&media_id), &serialized);
                            dr.as_object_mut().map(|o| {
                                o.remove("progress");
                                o.insert("progress_media_id".to_string(), json!(media_id));
                                o.insert("progress_size_bytes".to_string(), json!(size_bytes));
                                o.insert("entry_count".to_string(), json!(entry_count));
                                o.insert("rounds".to_string(), json!(rounds));
                                o.insert("thinking_count".to_string(), json!(thinking_count));
                                o.insert("web_count".to_string(), json!(web_count));
                                o.insert("file_count".to_string(), json!(file_count));
                            });
                        }
                    }
                }
                model_row["deepResearch"] = dr;
            }
        }
        if let Some(canvas_arr) = asst.and_then(|a| a.get("canvas")).and_then(|v| v.as_array()) {
            if !canvas_arr.is_empty() {
                let mut externalized: Vec<Value> = Vec::new();
                for canvas in canvas_arr {
                    let mut cv = canvas.clone();
                    // Canvas 代码内容外置到 media 文件
                    if let Some(content) = cv.get("content").and_then(|v| v.as_str()) {
                        if !content.is_empty() {
                            let ext = cv.get("filename")
                                .and_then(|v| v.as_str())
                                .and_then(|f| f.rsplit('.').next())
                                .unwrap_or("txt");
                            let media_id = format!("{}.{}", Uuid::new_v4().to_string().replace("-", ""), ext);
                            let size_bytes = content.as_bytes().len();
                            let char_count = content.chars().count();
                            let _ = std::fs::write(media_dir.join(&media_id), content.as_bytes());
                            cv.as_object_mut().map(|o| {
                                o.remove("content");
                                o.insert("content_media_id".to_string(), json!(media_id));
                                o.insert("size_bytes".to_string(), json!(size_bytes));
                                o.insert("char_count".to_string(), json!(char_count));
                            });
                        }
                    }
                    externalized.push(cv);
                }
                model_row["canvas"] = json!(externalized);
            }
        }
        // content_blocks 直接透传（已由 turn_parser 生成）
        if let Some(blocks) = asst.and_then(|a| a.get("content_blocks")).and_then(|v| v.as_array()) {
            if !blocks.is_empty() {
                model_row["contentBlocks"] = json!(blocks);
            }
        }
        rows.push(model_row);
    }

    // 标记 action_card 消息为 hidden（仅处理 message 行，跳过第一行 meta）
    mark_action_card_hidden(&mut rows);

    rows
}

/// 检测 action_card 消息并标记 hidden: true，同时标记其前面关联的 user 消息。
fn mark_action_card_hidden(rows: &mut [Value]) {
    // 收集 message 行的索引
    let msg_indices: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.get("type").and_then(|v| v.as_str()) == Some("message"))
        .map(|(i, _)| i)
        .collect();

    let mut to_hide: HashSet<usize> = HashSet::new();
    for (pos, &row_idx) in msg_indices.iter().enumerate() {
        let text = rows[row_idx]
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if is_action_card_text(text) {
            to_hide.insert(row_idx);
            // 向前找关联的 user 消息
            for prev_pos in (0..pos).rev() {
                let prev_idx = msg_indices[prev_pos];
                let role = rows[prev_idx]
                    .get("role")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if role == "user" {
                    to_hide.insert(prev_idx);
                    break;
                }
                if role == "model" {
                    break;
                }
            }
        }
    }

    for idx in to_hide {
        if let Some(obj) = rows[idx].as_object_mut() {
            obj.insert("hidden".into(), serde_json::json!(true));
        }
    }
}

fn is_action_card_text(text: &str) -> bool {
    text.contains("action_card_content")
        || text.trim() == "没问题，我可以帮忙。在这些媒体服务提供方中，你想使用哪个？"
}

fn build_attachments(files: Option<&Value>) -> Vec<Value> {
    let arr = match files.and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|f| {
            let obj = f.as_object()?;
            let media_id = obj.get("media_id")?.as_str().filter(|s| !s.is_empty())?;
            let mime = obj
                .get("mime")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut item = json!({ "mediaId": media_id, "mimeType": mime });
            if let Some(preview_id) = obj.get("preview_media_id").and_then(|v| v.as_str()) {
                if !preview_id.is_empty() {
                    item["previewMediaId"] = json!(preview_id);
                }
            }
            Some(item)
        })
        .collect()
}

// ============================================================================
// 账号元数据 / sync state / conversations 索引
// ============================================================================

pub fn write_accounts_json(base_dir: &Path, account_info: &Value) -> Result<()> {
    let accounts_file = base_dir.join("accounts.json");
    let now_iso = Utc::now().to_rfc3339();
    let info = account_info.as_object().unwrap();
    let account_id = info["id"].as_str().unwrap();

    // 读取现有账号列表，保留原始顺序
    let mut existing_ordered: Vec<Value> = Vec::new();
    if accounts_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&accounts_file) {
            if let Ok(data) = serde_json::from_str::<Value>(&content) {
                if let Some(accounts) = data.get("accounts").and_then(|v| v.as_array()) {
                    for a in accounts {
                        if a.get("id").and_then(|v| v.as_str()).is_some() {
                            existing_ordered.push(a.clone());
                        }
                    }
                }
            }
        }
    }

    let existing_account = existing_ordered
        .iter()
        .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(account_id))
        .cloned()
        .unwrap_or(json!({}));
    let authuser = info
        .get("authuser")
        .filter(|v| !v.is_null())
        .or_else(|| existing_account.get("authuser"));

    let new_entry = json!({
        "id": account_id,
        "email": info.get("email").and_then(|v| v.as_str()).unwrap_or(""),
        "addedAt": existing_account.get("addedAt").and_then(|v| v.as_str()).unwrap_or(&now_iso),
        "dataDir": format!("accounts/{}", account_id),
        "authuser": authuser,
    });

    // 原地更新或追加，保持原始顺序
    let mut found = false;
    for entry in &mut existing_ordered {
        if entry.get("id").and_then(|v| v.as_str()) == Some(account_id) {
            *entry = new_entry.clone();
            found = true;
            break;
        }
    }
    if !found {
        existing_ordered.push(new_entry);
    }

    let data = json!({
        "version": 1,
        "updatedAt": now_iso,
        "accounts": existing_ordered,
    });
    std::fs::write(&accounts_file, serde_json::to_string_pretty(&data)?)?;
    Ok(())
}

pub fn write_account_meta(account_dir: &Path, account_info: &Value) -> Result<()> {
    let info = account_info.as_object().unwrap();
    let meta = json!({
        "version": 1,
        "id": info.get("id"),
        "name": info.get("name").and_then(|v| v.as_str()).unwrap_or(""),
        "email": info.get("email").and_then(|v| v.as_str()).unwrap_or(""),
        "avatarText": info.get("avatarText").and_then(|v| v.as_str()).unwrap_or("?"),
        "avatarColor": info.get("avatarColor").and_then(|v| v.as_str()).unwrap_or("#667eea"),
        "conversationCount": info.get("conversationCount").and_then(|v| v.as_i64()).unwrap_or(0),
        "remoteConversationCount": info.get("remoteConversationCount"),
        "lastSyncAt": info.get("lastSyncAt"),
        "lastSyncResult": info.get("lastSyncResult"),
        "authuser": info.get("authuser"),
    });
    std::fs::write(
        account_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;
    Ok(())
}

pub fn write_conversations_index(
    account_dir: &Path,
    account_id: &str,
    updated_at: &str,
    summaries: &[Value],
) -> Result<()> {
    let data = json!({
        "version": 1,
        "accountId": account_id,
        "updatedAt": updated_at,
        "totalCount": summaries.len(),
        "items": summaries,
    });
    std::fs::write(
        account_dir.join("conversations.json"),
        serde_json::to_string_pretty(&data)?,
    )?;
    Ok(())
}

pub fn write_sync_state(account_dir: &Path, state: &Value) -> Result<()> {
    std::fs::write(
        account_dir.join("sync_state.json"),
        serde_json::to_string_pretty(state)?,
    )?;
    Ok(())
}

pub fn load_sync_state(account_dir: &Path) -> Value {
    let sync_file = account_dir.join("sync_state.json");
    if !sync_file.exists() {
        return json!({});
    }
    std::fs::read_to_string(&sync_file)
        .ok()
        .and_then(|c| serde_json::from_str::<Value>(&c).ok())
        .filter(|v| v.is_object())
        .unwrap_or(json!({}))
}

pub fn load_conversations_index(account_dir: &Path) -> (Vec<String>, HashMap<String, Value>) {
    let conv_file = account_dir.join("conversations.json");
    if !conv_file.exists() {
        return (Vec::new(), HashMap::new());
    }
    let data: Value = match std::fs::read_to_string(&conv_file)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
    {
        Some(d) => d,
        None => return (Vec::new(), HashMap::new()),
    };

    let items = match data.get("items").and_then(|v| v.as_array()) {
        Some(a) => a,
        None => return (Vec::new(), HashMap::new()),
    };

    let mut ordered_ids = Vec::new();
    let mut index_map = HashMap::new();
    for item in items {
        if let Some(cid) = item.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            ordered_ids.push(cid.to_string());
            index_map.insert(cid.to_string(), item.clone());
        }
    }
    (ordered_ids, index_map)
}

// ============================================================================
// 对话摘要构建
// ============================================================================

fn normalize_conversation_status(value: Option<&str>, default: Option<&str>) -> String {
    let fallback = default.unwrap_or(CONVERSATION_STATUS_NORMAL);
    match value {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => fallback.to_string(),
    }
}

fn status_for_remote_summary(existing: Option<&Value>) -> String {
    let current_status = existing
        .and_then(|e| e.get("status"))
        .and_then(|v| v.as_str());
    let normalized = normalize_conversation_status(current_status, None);
    if normalized == CONVERSATION_STATUS_HIDDEN {
        CONVERSATION_STATUS_HIDDEN.to_string()
    } else {
        CONVERSATION_STATUS_NORMAL.to_string()
    }
}

fn get_int_field(obj: &Value, key: &str, default: i64) -> i64 {
    obj.get(key)
        .and_then(|v| v.as_i64())
        .filter(|&v| v >= 0)
        .unwrap_or(default)
}

pub fn build_lost_summary(bare_id: &str, existing: Option<&Value>) -> Value {
    let empty = json!({});
    let e = existing.unwrap_or(&empty);
    let last_message = e
        .get("lastMessage")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let message_count = get_int_field(e, "messageCount", 0);
    let image_count = get_int_field(e, "imageCount", 0);
    let video_count = get_int_field(e, "videoCount", 0);

    json!({
        "id": bare_id,
        "title": e.get("title").and_then(|v| v.as_str()).unwrap_or(bare_id),
        "lastMessage": last_message,
        "messageCount": message_count,
        "hasMedia": e.get("hasMedia").and_then(|v| v.as_bool()).unwrap_or(false),
        "hasFailedData": e.get("hasFailedData").and_then(|v| v.as_bool()).unwrap_or(false),
        "imageCount": image_count,
        "videoCount": video_count,
        "updatedAt": e.get("updatedAt"),
        "remoteHash": e.get("remoteHash"),
        "status": CONVERSATION_STATUS_LOST,
    })
}

pub fn build_summary_from_chat_listing(chat: &Value, existing: Option<&Value>) -> Value {
    let empty = json!({});
    let e = existing.unwrap_or(&empty);
    let status = status_for_remote_summary(existing);
    let bare_id = crate::protocol::strip_c_prefix(
        chat.get("id").and_then(|v| v.as_str()).unwrap_or(""),
    );
    let title = chat
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| e.get("title").and_then(|v| v.as_str()).unwrap_or(""));

    let (updated_at, remote_hash) = match chat
        .get("latest_update_ts")
        .and_then(|v| coerce_epoch_seconds(v))
    {
        Some(ts) => (to_iso_utc(Some(ts)), Some(ts.to_string())),
        None => (
            e.get("updatedAt").and_then(|v| v.as_str()).map(|s| s.to_string()),
            e.get("remoteHash").and_then(|v| v.as_str()).map(|s| s.to_string()),
        ),
    };

    let msg_count = get_int_field(e, "messageCount", 0);
    let image_count = get_int_field(e, "imageCount", 0);
    let video_count = get_int_field(e, "videoCount", 0);

    json!({
        "id": bare_id,
        "title": title,
        "lastMessage": e.get("lastMessage").and_then(|v| v.as_str()).unwrap_or(""),
        "messageCount": msg_count,
        "hasMedia": e.get("hasMedia").and_then(|v| v.as_bool()).unwrap_or(false),
        "hasFailedData": e.get("hasFailedData").and_then(|v| v.as_bool()).unwrap_or(false),
        "imageCount": image_count,
        "videoCount": video_count,
        "updatedAt": updated_at,
        "remoteHash": remote_hash,
        "status": status,
    })
}

pub fn filter_display_rows(msg_rows: &[Value]) -> Vec<Value> {
    let mut to_remove = HashSet::new();
    for (i, row) in msg_rows.iter().enumerate() {
        let text = match row.as_object().and_then(|o| o.get("text")).and_then(|v| v.as_str()) {
            Some(t) => t,
            None => continue,
        };
        if is_action_card_text(text) {
            to_remove.insert(i);
            // Find preceding user message to remove
            for j in (0..i).rev() {
                let role = msg_rows[j]
                    .as_object()
                    .and_then(|o| o.get("role"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if role == "user" {
                    to_remove.insert(j);
                    break;
                }
                if role == "model" {
                    break;
                }
            }
        }
    }
    msg_rows
        .iter()
        .enumerate()
        .filter(|(i, _)| !to_remove.contains(i))
        .map(|(_, v)| v.clone())
        .collect()
}

// ============================================================================
// 目录级文件计数
// ============================================================================

pub fn is_jsonl_file(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()) == Some("jsonl")
}

pub fn count_jsonl_files(conversations_dir: &Path) -> Result<u64, String> {
    if !conversations_dir.exists() {
        return Ok(0);
    }
    let mut count: u64 = 0;
    for entry in std::fs::read_dir(conversations_dir).str_err()? {
        let entry = entry.str_err()?;
        let path = entry.path();
        let file_type = entry.file_type().str_err()?;
        if !file_type.is_file() {
            continue;
        }
        if is_jsonl_file(&path) {
            count += 1;
        }
    }
    Ok(count)
}

pub fn conversation_count_from_index(account_dir: &Path) -> Option<u64> {
    let index_file = account_dir.join("conversations.json");
    if !index_file.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&index_file).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    if let Some(items) = parsed.get("items").and_then(|v| v.as_array()) {
        return Some(items.len() as u64);
    }
    parsed.get("totalCount").and_then(|v| v.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_read_write_jsonl() {
        let mut tmp = NamedTempFile::new().unwrap();
        writeln!(tmp, r#"{{"a":1}}"#).unwrap();
        writeln!(tmp, r#"{{"b":2}}"#).unwrap();
        let rows = read_jsonl_rows(tmp.path());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["a"], 1);
    }

    #[test]
    fn test_dedupe_message_rows() {
        let rows = vec![
            json!({"id": "a", "text": "1"}),
            json!({"id": "a", "text": "2"}),
            json!({"id": "b", "text": "3"}),
        ];
        let (deduped, removed) = dedupe_message_rows_by_id(&rows);
        assert_eq!(deduped.len(), 2);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_filter_display_rows() {
        let rows = vec![
            json!({"role": "user", "text": "hello"}),
            json!({"role": "model", "text": "action_card_content blah"}),
            json!({"role": "user", "text": "real question"}),
            json!({"role": "model", "text": "real answer"}),
        ];
        let filtered = filter_display_rows(&rows);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0]["text"], "real question");
    }

    #[test]
    fn test_build_lost_summary() {
        let existing = json!({
            "title": "test chat",
            "messageCount": 10,
            "imageCount": 2,
            "videoCount": 0,
        });
        let summary = build_lost_summary("abc123", Some(&existing));
        assert_eq!(summary["status"], "lost");
        assert_eq!(summary["title"], "test chat");
        assert_eq!(summary["messageCount"], 10);
    }

    #[test]
    fn test_turns_to_jsonl_externalizes_report() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let media_dir = tmp_dir.path().join("media");
        std::fs::create_dir_all(&media_dir).unwrap();

        let report_text = "# 完整报告\n\n这是一份很长的研究报告...";
        let turn = json!({
            "turn_id": "t1",
            "timestamp": 1700000000,
            "user": { "text": "研究问题", "files": [] },
            "assistant": {
                "text": "已生成报告",
                "thinking": "",
                "model": "gemini-2.0",
                "files": [],
                "deep_research": {
                    "type": "report",
                    "title": "研究报告标题",
                    "report_text": report_text,
                    "research_id": "uuid-123",
                    "document_id": "doc-456"
                }
            }
        });

        let rows = turns_to_jsonl_rows(
            &[turn],
            "conv_1", "account_1", "测试对话",
            &json!({}),
            &media_dir,
        );

        // 应有 3 行: meta + user + model
        assert_eq!(rows.len(), 3);
        let model_row = &rows[2];
        let dr = model_row.get("deepResearch").unwrap();

        // report_text 应被移除，替换为 report_media_id
        assert!(dr.get("report_text").is_none(), "report_text 不应在 JSONL 中");
        let media_id = dr.get("report_media_id").and_then(|v| v.as_str()).unwrap();
        assert!(media_id.ends_with(".md"));

        // media 文件应存在且内容正确
        let file_content = std::fs::read_to_string(media_dir.join(media_id)).unwrap();
        assert_eq!(file_content, report_text);

        // 其他字段应保留
        assert_eq!(dr["type"], "report");
        assert_eq!(dr["title"], "研究报告标题");
        assert_eq!(dr["research_id"], "uuid-123");

        // 大小与字符数应注入
        assert_eq!(dr["size_bytes"], json!(report_text.as_bytes().len()));
        assert_eq!(dr["char_count"], json!(report_text.chars().count()));
    }

    #[test]
    fn test_turns_to_jsonl_externalizes_canvas() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let media_dir = tmp_dir.path().join("media");
        std::fs::create_dir_all(&media_dir).unwrap();

        let canvas_content_a = "<!DOCTYPE html><html><body><h1>Page A</h1></body></html>";
        let canvas_content_b = "<!DOCTYPE html><html><body><h1>Page B</h1></body></html>";
        let turn = json!({
            "turn_id": "t2",
            "timestamp": 1700000000,
            "user": { "text": "做两个网页", "files": [] },
            "assistant": {
                "text": "已创建",
                "thinking": "",
                "model": "gemini-2.0",
                "files": [],
                "canvas": [
                    {
                        "title": "页面A",
                        "filename": "page-a.html",
                        "content": canvas_content_a,
                        "language": "html",
                        "document_id": "doc-a"
                    },
                    {
                        "title": "页面B",
                        "filename": "page-b.html",
                        "content": canvas_content_b,
                        "language": "html",
                        "document_id": "doc-b"
                    }
                ],
                "content_blocks": [
                    {"kind": "text", "text": "介绍"},
                    {"kind": "canvas", "canvas_index": 0},
                    {"kind": "text", "text": "中间段"},
                    {"kind": "canvas", "canvas_index": 1}
                ]
            }
        });

        let rows = turns_to_jsonl_rows(
            &[turn],
            "conv_2", "account_1", "测试对话",
            &json!({}),
            &media_dir,
        );

        let model_row = &rows[2];
        let cv_arr = model_row.get("canvas").unwrap().as_array().unwrap();
        assert_eq!(cv_arr.len(), 2);

        // 第一个 canvas
        let cv0 = &cv_arr[0];
        assert!(cv0.get("content").is_none(), "content 不应在 JSONL 中");
        let media_id_a = cv0.get("content_media_id").and_then(|v| v.as_str()).unwrap();
        assert!(media_id_a.ends_with(".html"));
        let file_a = std::fs::read_to_string(media_dir.join(media_id_a)).unwrap();
        assert_eq!(file_a, canvas_content_a);
        assert_eq!(cv0["title"], "页面A");
        assert_eq!(cv0["size_bytes"], json!(canvas_content_a.as_bytes().len()));

        // 第二个 canvas
        let cv1 = &cv_arr[1];
        let media_id_b = cv1.get("content_media_id").and_then(|v| v.as_str()).unwrap();
        let file_b = std::fs::read_to_string(media_dir.join(media_id_b)).unwrap();
        assert_eq!(file_b, canvas_content_b);
        assert_eq!(cv1["title"], "页面B");

        // contentBlocks 应透传
        let blocks = model_row.get("contentBlocks").unwrap().as_array().unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0]["kind"], "text");
        assert_eq!(blocks[1]["kind"], "canvas");
        assert_eq!(blocks[1]["canvas_index"], 0);
    }

    #[test]
    fn test_turns_to_jsonl_externalizes_research_progress() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let media_dir = tmp_dir.path().join("media");
        std::fs::create_dir_all(&media_dir).unwrap();

        let progress = json!([
            { "type": "thinking", "title": "规划第一轮", "description": "思考...", "round": 0 },
            { "type": "web_search", "url": "https://a.example.com", "page_title": "A" },
            { "type": "web_search", "url": "https://b.example.com", "page_title": "B" },
            { "type": "thinking", "title": "规划第二轮", "description": "继续思考...", "round": 1 },
            { "type": "file_search", "filename": "notes.pdf" }
        ]);
        let turn = json!({
            "turn_id": "t3",
            "timestamp": 1700000000,
            "user": { "text": "研究问题", "files": [] },
            "assistant": {
                "text": "已生成报告",
                "thinking": "",
                "model": "gemini-2.0",
                "files": [],
                "deep_research": {
                    "type": "report",
                    "title": "报告",
                    "report_text": "# 报告正文",
                    "progress": progress.clone()
                }
            }
        });

        let rows = turns_to_jsonl_rows(
            &[turn],
            "conv_3", "account_1", "测试对话",
            &json!({}),
            &media_dir,
        );

        let dr = rows[2].get("deepResearch").unwrap();
        // 原 progress 数组应被移除
        assert!(dr.get("progress").is_none(), "progress 数组不应在 JSONL 中");

        // media 文件存在且内容等于原数组
        let media_id = dr.get("progress_media_id").and_then(|v| v.as_str()).unwrap();
        assert!(media_id.ends_with(".json"));
        let content = std::fs::read_to_string(media_dir.join(media_id)).unwrap();
        let parsed: Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed, progress);

        // 统计字段正确
        assert_eq!(dr["entry_count"], json!(5));
        assert_eq!(dr["rounds"], json!(2));
        assert_eq!(dr["thinking_count"], json!(2));
        assert_eq!(dr["web_count"], json!(2));
        assert_eq!(dr["file_count"], json!(1));
        assert!(dr.get("progress_size_bytes").and_then(|v| v.as_u64()).unwrap() > 0);
    }

    #[test]
    fn test_report_without_progress_has_no_progress_fields() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let media_dir = tmp_dir.path().join("media");
        std::fs::create_dir_all(&media_dir).unwrap();

        let turn = json!({
            "turn_id": "t4",
            "timestamp": 1700000000,
            "user": { "text": "q", "files": [] },
            "assistant": {
                "text": "a",
                "thinking": "",
                "model": "gemini-2.0",
                "files": [],
                "deep_research": {
                    "type": "report",
                    "title": "报告",
                    "report_text": "正文"
                }
            }
        });

        let rows = turns_to_jsonl_rows(
            &[turn],
            "conv_4", "account_1", "测试",
            &json!({}),
            &media_dir,
        );
        let dr = rows[2].get("deepResearch").unwrap();
        assert!(dr.get("progress_media_id").is_none());
        assert!(dr.get("entry_count").is_none());
        assert!(dr.get("rounds").is_none());
    }
}
