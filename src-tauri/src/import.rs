//! ZIP 导入：解压、合并对话/媒体、更新索引。

use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::search;
use crate::storage;
use crate::str_err::ToStringErr;

// ============================================================================
// 导入内部逻辑
// ============================================================================

fn find_account_dir_in_zip(tmp_dir: &Path) -> Result<PathBuf, String> {
    if let Ok(entries) = std::fs::read_dir(tmp_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let dir = entry.path();
                if dir.join("conversations").is_dir() || dir.join("meta.json").is_file() {
                    return Ok(dir);
                }
            }
        }
    }
    if tmp_dir.join("conversations").is_dir() {
        return Ok(tmp_dir.to_path_buf());
    }
    Err("ZIP 中未找到有效账号数据（应包含 conversations/ 目录或 meta.json）".to_string())
}

/// 从 JSONL 文件的 meta 行构建一条索引条目
fn summary_from_jsonl_meta(jsonl_file: &Path) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(jsonl_file).ok()?;
    let mut meta: Option<serde_json::Value> = None;
    let mut message_count: usize = 0;
    let mut has_media = false;
    let mut image_count: usize = 0;
    let mut video_count: usize = 0;
    let mut last_text = String::new();

    for line in raw.lines() {
        let s = line.trim();
        if s.is_empty() { continue; }
        let obj: serde_json::Value = match serde_json::from_str(s) {
            Ok(v) => v,
            Err(_) => continue,
        };
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("meta") => { if meta.is_none() { meta = Some(obj); } }
            Some("message") => {
                if obj.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) { continue; }
                message_count += 1;
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    if !text.is_empty() { last_text = text.to_string(); }
                }
                if let Some(atts) = obj.get("attachments").and_then(|v| v.as_array()) {
                    for att in atts {
                        let mime = att.get("mimeType").and_then(|v| v.as_str()).unwrap_or("");
                        if mime.starts_with("image/") { image_count += 1; has_media = true; }
                        else if mime.starts_with("video/") { video_count += 1; has_media = true; }
                        else if !mime.is_empty() { has_media = true; }
                    }
                }
            }
            _ => {}
        }
    }

    let meta = meta?;
    let id = meta.get("id").and_then(|v| v.as_str())?.to_string();
    let title = meta.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let updated_at = meta.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let created_at = meta.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let remote_hash = meta.get("remoteHash").cloned().unwrap_or(serde_json::Value::Null);
    let last_msg: String = last_text.chars().take(80).collect();

    Some(serde_json::json!({
        "id": id,
        "title": title,
        "lastMessage": last_msg,
        "messageCount": message_count,
        "hasMedia": has_media,
        "imageCount": image_count,
        "videoCount": video_count,
        "updatedAt": updated_at,
        "createdAt": created_at,
        "remoteHash": remote_hash,
    }))
}

/// 从目标目录中所有 JSONL 文件重建 conversations.json 索引
fn rebuild_conversations_index(target_dir: &Path) -> Result<(), String> {
    let target_conv_dir = target_dir.join("conversations");
    let target_conv_file = target_dir.join("conversations.json");

    let mut items: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();

    // 先加载已有索引
    if target_conv_file.exists() {
        if let Ok(raw) = std::fs::read_to_string(&target_conv_file) {
            if let Some(existing) = serde_json::from_str::<serde_json::Value>(&raw)
                .ok()
                .and_then(|v| v.get("items").and_then(|a| a.as_array()).cloned())
            {
                for item in existing {
                    if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                        items.insert(id.to_string(), item);
                    }
                }
            }
        }
    }

    // 从所有 JSONL 文件重建/更新条目
    if target_conv_dir.exists() {
        for entry in std::fs::read_dir(&target_conv_dir).str_err()? {
            let entry = entry.str_err()?;
            let path = entry.path();
            if !storage::is_jsonl_file(&path) { continue; }
            if let Some(summary) = summary_from_jsonl_meta(&path) {
                if let Some(id) = summary.get("id").and_then(|v| v.as_str()) {
                    items.insert(id.to_string(), summary);
                }
            }
        }
    }

    let items_vec: Vec<serde_json::Value> = items.into_values().collect();
    let out = serde_json::json!({ "items": items_vec });
    std::fs::write(&target_conv_file, serde_json::to_string_pretty(&out).str_err()?)
        .str_err()
}

fn merge_jsonl(existing_path: &Path, src_path: &Path) -> Result<(), String> {
    let parse = |raw: &str| -> (Option<serde_json::Value>, Vec<serde_json::Value>) {
        let mut meta = None;
        let mut msgs = Vec::new();
        for line in raw.lines() {
            let s = line.trim();
            if s.is_empty() { continue; }
            let Ok(obj) = serde_json::from_str::<serde_json::Value>(s) else { continue; };
            match obj.get("type").and_then(|v| v.as_str()) {
                Some("meta") => { if meta.is_none() { meta = Some(obj); } }
                Some("message") => msgs.push(obj),
                _ => {}
            }
        }
        (meta, msgs)
    };

    let existing_raw = std::fs::read_to_string(existing_path).str_err()?;
    let src_raw = std::fs::read_to_string(src_path).str_err()?;
    let (existing_meta, existing_msgs) = parse(&existing_raw);
    let (src_meta, src_msgs) = parse(&src_raw);

    let existing_updated = existing_meta.as_ref()
        .and_then(|m| m.get("updatedAt").and_then(|v| v.as_str())).unwrap_or("");
    let src_updated = src_meta.as_ref()
        .and_then(|m| m.get("updatedAt").and_then(|v| v.as_str())).unwrap_or("");
    let src_is_newer = src_updated > existing_updated;

    let winner_meta = if src_is_newer { src_meta } else { existing_meta };
    let winner_meta = winner_meta.unwrap_or_else(|| serde_json::json!({"type": "meta"}));

    let mut msg_map: std::collections::HashMap<String, serde_json::Value> = std::collections::HashMap::new();
    let (loser_msgs, winner_msgs) = if src_is_newer {
        (&existing_msgs, &src_msgs)
    } else {
        (&src_msgs, &existing_msgs)
    };
    for msg in loser_msgs {
        let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !id.is_empty() { msg_map.insert(id, msg.clone()); }
    }
    for msg in winner_msgs {
        let id = msg.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !id.is_empty() { msg_map.insert(id, msg.clone()); }
    }

    let mut merged: Vec<serde_json::Value> = msg_map.into_values().collect();
    merged.sort_by(|a, b| {
        let ta = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        ta.cmp(tb)
    });

    let mut out = serde_json::to_string(&winner_meta).str_err()?;
    out.push('\n');
    for msg in &merged {
        out.push_str(&serde_json::to_string(msg).str_err()?);
        out.push('\n');
    }
    std::fs::write(existing_path, out).str_err()
}

fn do_import(src_dir: &Path, target_dir: &Path) -> Result<String, String> {
    let src_conv_dir = src_dir.join("conversations");
    let src_media_dir = src_dir.join("media");
    let target_conv_dir = target_dir.join("conversations");
    let target_media_dir = target_dir.join("media");

    std::fs::create_dir_all(&target_conv_dir).str_err()?;
    std::fs::create_dir_all(&target_media_dir).str_err()?;

    let mut imported_convs: usize = 0;
    let mut merged_convs: usize = 0;
    let mut imported_media: usize = 0;
    let mut skipped_media: usize = 0;

    if src_conv_dir.exists() {
        for entry in std::fs::read_dir(&src_conv_dir).str_err()? {
            let entry = entry.str_err()?;
            let path = entry.path();
            if !entry.file_type().str_err()?.is_file() { continue; }
            if !storage::is_jsonl_file(&path) { continue; }
            let target_path = target_conv_dir.join(path.file_name().unwrap());
            if target_path.exists() {
                merge_jsonl(&target_path, &path)?;
                merged_convs += 1;
            } else {
                std::fs::copy(&path, &target_path).str_err()?;
                imported_convs += 1;
            }
        }
    }

    if src_media_dir.exists() {
        for entry in std::fs::read_dir(&src_media_dir).str_err()? {
            let entry = entry.str_err()?;
            if !entry.file_type().str_err()?.is_file() { continue; }
            let target_path = target_media_dir.join(entry.file_name());
            if target_path.exists() {
                skipped_media += 1;
            } else {
                std::fs::copy(&entry.path(), &target_path).str_err()?;
                imported_media += 1;
            }
        }
    }

    rebuild_conversations_index(target_dir)?;

    serde_json::to_string(&serde_json::json!({
        "importedConversations": imported_convs,
        "mergedConversations": merged_convs,
        "importedMedia": imported_media,
        "skippedMedia": skipped_media,
    }))
    .str_err()
}

fn import_account_zip_impl(data_dir: &Path, account_id: &str, zip_path: &Path) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开 ZIP 失败: {}", e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取 ZIP 格式失败: {}", e))?;

    let tmp_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let tmp_dir = std::env::temp_dir().join(format!("gemini_import_{}", tmp_id));
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("创建临时目录失败: {}", e))?;

    let extract_result: Result<(), String> = (|| {
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).str_err()?;
            let out_path = match entry.enclosed_name() {
                Some(p) => tmp_dir.join(p),
                None => continue,
            };
            if entry.is_dir() {
                std::fs::create_dir_all(&out_path).str_err()?;
            } else {
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent).str_err()?;
                }
                let mut data = Vec::new();
                entry.read_to_end(&mut data).str_err()?;
                std::fs::write(&out_path, &data).str_err()?;
            }
        }
        Ok(())
    })();

    if let Err(e) = extract_result {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(format!("解压 ZIP 失败: {}", e));
    }

    let src_dir = match find_account_dir_in_zip(&tmp_dir) {
        Ok(d) => d,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }
    };

    let target_dir = data_dir.join("accounts").join(account_id);
    let result = do_import(&src_dir, &target_dir);
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if result.is_ok() {
        let conv_count = storage::count_jsonl_files(&target_dir.join("conversations")).unwrap_or(0);
        let meta_file = target_dir.join("meta.json");
        if meta_file.exists() {
            if let Ok(raw) = std::fs::read_to_string(&meta_file) {
                if let Ok(mut meta) = serde_json::from_str::<serde_json::Value>(&raw) {
                    if let Some(obj) = meta.as_object_mut() {
                        obj.insert("conversationCount".to_string(), serde_json::json!(conv_count));
                        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                        obj.insert("lastSyncAt".to_string(), serde_json::json!(now));
                        obj.insert("lastSyncResult".to_string(), serde_json::json!("success"));
                        if let Ok(serialized) = serde_json::to_string_pretty(&meta) {
                            let _ = std::fs::write(&meta_file, serialized);
                        }
                    }
                }
            }
        }
    }

    result
}

// ============================================================================
// Tauri 导入命令
// ============================================================================

#[tauri::command]
pub async fn import_account_zip(
    app: tauri::AppHandle,
    account_id: String,
    zip_path: String,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let zip = PathBuf::from(&zip_path);

    let account_id_clone = account_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        import_account_zip_impl(&data_dir, &account_id_clone, &zip)
    })
    .await
    .str_err()??;

    let account_dir = app.path().app_data_dir().str_err()?.join("accounts").join(&account_id);
    let conversations_dir = account_dir.join("conversations");
    if let Ok(index) = search::open_or_create_index(&account_dir) {
        let _ = search::index_all(&index, &account_dir, &conversations_dir);
    }

    Ok(result)
}
