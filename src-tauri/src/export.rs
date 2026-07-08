//! 账号数据导出：原始 ZIP 打包 + Kelivo 格式转换。

use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Local};
use tauri::Manager;

use crate::storage;
use crate::str_err::ToStringErr;

// ============================================================================
// 共享工具（从 lib.rs 迁入）
// ============================================================================

pub(crate) fn resolve_account_id_arg(
    account_id: Option<String>,
    account_id_camel: Option<String>,
) -> Result<String, String> {
    account_id
        .or(account_id_camel)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "缺少 account_id/accountId 参数".to_string())
}

pub(crate) fn value_to_non_empty_string(v: Option<&serde_json::Value>) -> Option<String> {
    match v {
        Some(serde_json::Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

// ============================================================================
// 导出统计
// ============================================================================

#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AccountExportStats {
    account_id: String,
    conversation_count: u64,
    conversation_file_count: u64,
    media_file_count: u64,
    total_file_count: u64,
    total_bytes: u64,
    estimated_zip_bytes: u64,
}

fn sanitize_file_component(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
            continue;
        }
        if ch.is_whitespace() {
            out.push('_');
            continue;
        }
        out.push(ch);
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "account".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_search_index_path(name: &str) -> bool {
    name == "search_index" || name == "search_mtimes.json"
}

fn count_files_and_bytes_recursive(root: &Path) -> Result<(u64, u64), String> {
    if !root.exists() {
        return Ok((0, 0));
    }

    let mut files: u64 = 0;
    let mut total_bytes: u64 = 0;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).str_err()? {
            let entry = entry.str_err()?;
            let name = entry.file_name();
            if is_search_index_path(name.to_str().unwrap_or("")) {
                continue;
            }
            let file_type = entry.file_type().str_err()?;
            if file_type.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            files += 1;
            total_bytes += entry.metadata().str_err()?.len();
        }
    }

    Ok((files, total_bytes))
}

fn account_export_user_label(account_dir: &Path, account_id: &str) -> String {
    let meta_file = account_dir.join("meta.json");
    if meta_file.exists() {
        if let Ok(raw) = std::fs::read_to_string(&meta_file) {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(email) = value_to_non_empty_string(meta.get("email")) {
                    let name = email.split('@').next().unwrap_or("").trim();
                    if !name.is_empty() {
                        return name.to_string();
                    }
                }
                if let Some(name) = value_to_non_empty_string(meta.get("name")) {
                    if !name.is_empty() {
                        return name;
                    }
                }
            }
        }
    }
    account_id.to_string()
}

fn build_account_export_stats(
    account_dir: &Path,
    account_id: &str,
) -> Result<AccountExportStats, String> {
    let conversations_dir = account_dir.join("conversations");
    let media_dir = account_dir.join("media");

    let conversation_file_count = storage::count_jsonl_files(&conversations_dir)?;
    let media_file_count = count_files_and_bytes_recursive(&media_dir)?.0;
    let (total_file_count, total_bytes) = count_files_and_bytes_recursive(account_dir)?;
    let conversation_count =
        storage::conversation_count_from_index(account_dir).unwrap_or(conversation_file_count);
    let estimated_zip_bytes = if total_bytes == 0 {
        0
    } else {
        ((total_bytes as f64) * 0.62).round() as u64
    };

    Ok(AccountExportStats {
        account_id: account_id.to_string(),
        conversation_count,
        conversation_file_count,
        media_file_count,
        total_file_count,
        total_bytes,
        estimated_zip_bytes,
    })
}

// ============================================================================
// ZIP 打包
// ============================================================================

/// 已压缩的媒体扩展名，使用 Stored 避免无效 Deflate
fn should_store(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some(
            "jpg"
                | "jpeg"
                | "png"
                | "gif"
                | "webp"
                | "avif"
                | "heic"
                | "heif"
                | "mp4"
                | "webm"
                | "mov"
                | "avi"
                | "mkv"
                | "mp3"
                | "aac"
                | "ogg"
                | "opus"
                | "flac"
                | "zip"
                | "gz"
                | "zst"
                | "br"
                | "xz"
                | "bz2"
        )
    )
}

fn zip_opts_deflate() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
}

fn zip_opts_stored() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
}

fn zip_account_dir(account_dir: &Path, zip_path: &Path) -> Result<(), String> {
    let folder_name = account_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "账号目录名称异常".to_string())?;

    let file = std::fs::File::create(zip_path).map_err(|e| format!("创建 zip 文件失败: {}", e))?;
    let mut zip_writer = zip::ZipWriter::new(file);
    let opts_deflate = zip_opts_deflate();
    let opts_stored = zip_opts_stored();

    // 递归收集所有文件
    let mut entries: Vec<std::path::PathBuf> = Vec::new();
    collect_files(account_dir, &mut entries).map_err(|e| format!("遍历目录失败: {}", e))?;

    for entry_path in &entries {
        let rel = entry_path
            .strip_prefix(account_dir)
            .map_err(|e| format!("路径计算失败: {}", e))?;
        // zip 内路径以 folder_name/ 为前缀
        let zip_entry_name = format!(
            "{}/{}",
            folder_name,
            rel.to_string_lossy().replace('\\', "/")
        );

        if entry_path.is_dir() {
            zip_writer
                .add_directory(&zip_entry_name, opts_stored)
                .map_err(|e| format!("添加目录失败: {}", e))?;
        } else {
            let opts = if should_store(entry_path) {
                opts_stored
            } else {
                opts_deflate
            };
            zip_writer
                .start_file(&zip_entry_name, opts)
                .map_err(|e| format!("添加文件失败: {}", e))?;
            let mut f =
                std::fs::File::open(entry_path).map_err(|e| format!("打开文件失败: {}", e))?;
            std::io::copy(&mut f, &mut zip_writer).map_err(|e| format!("写入 zip 失败: {}", e))?;
        }
    }

    zip_writer
        .finish()
        .map_err(|e| format!("zip 完成失败: {}", e))?;
    Ok(())
}

fn collect_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if is_search_index_path(name) {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

// ============================================================================
// Tauri 导出命令
// ============================================================================

#[tauri::command]
pub fn get_account_range_bytes(
    app: tauri::AppHandle,
    account_id: Option<String>,
    #[allow(non_snake_case)] accountId: Option<String>,
    after_date: Option<String>,
    #[allow(non_snake_case)] afterDate: Option<String>,
) -> Result<String, String> {
    let account_id = resolve_account_id_arg(account_id, accountId)?;
    let data_dir = app.path().app_data_dir().str_err()?;
    let account_dir = data_dir.join("accounts").join(&account_id);
    if !account_dir.exists() {
        return Err(format!("账号目录不存在: {}", account_id));
    }
    let conversations_dir = account_dir.join("conversations");
    let media_dir = account_dir.join("media");

    let after = after_date
        .or(afterDate)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if !conversations_dir.exists() {
        let result = serde_json::json!({ "totalBytes": 0u64 });
        return serde_json::to_string(&result).str_err();
    }

    let mut total_bytes: u64 = 0;

    for entry in std::fs::read_dir(&conversations_dir).str_err()? {
        let entry = entry.str_err()?;
        let path = entry.path();
        if !storage::is_jsonl_file(&path) {
            continue;
        }

        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let mut updated_at: Option<String> = None;
        let mut media_ids: Vec<String> = Vec::new();

        for line in raw.lines() {
            let s = line.trim();
            if s.is_empty() {
                continue;
            }
            let obj: serde_json::Value = match serde_json::from_str(s) {
                Ok(v) => v,
                Err(_) => continue,
            };
            match obj.get("type").and_then(|v| v.as_str()) {
                Some("meta") => {
                    if updated_at.is_none() {
                        updated_at = obj
                            .get("updatedAt")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                Some("message") => {
                    if let Some(atts) = obj.get("attachments").and_then(|v| v.as_array()) {
                        for att in atts {
                            if let Some(mid) = att.get("mediaId").and_then(|v| v.as_str()) {
                                if !mid.is_empty() {
                                    media_ids.push(mid.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if let Some(ref after_str) = after {
            let conv_updated = updated_at.as_deref().unwrap_or("");
            if !conv_updated.is_empty() && conv_updated < after_str.as_str() {
                continue;
            }
        }

        for mid in &media_ids {
            let file_path = media_dir.join(mid);
            if let Ok(meta_fs) = std::fs::metadata(&file_path) {
                total_bytes += meta_fs.len();
            }
        }
    }

    let result = serde_json::json!({ "totalBytes": total_bytes });
    serde_json::to_string(&result).str_err()
}

#[tauri::command]
pub fn get_account_export_stats(
    app: tauri::AppHandle,
    account_id: Option<String>,
    #[allow(non_snake_case)] accountId: Option<String>,
) -> Result<String, String> {
    let account_id = resolve_account_id_arg(account_id, accountId)?;
    let data_dir = app.path().app_data_dir().str_err()?;
    let account_dir = data_dir.join("accounts").join(&account_id);
    if !account_dir.exists() {
        return Err(format!("账号目录不存在: {}", account_id));
    }
    let stats = build_account_export_stats(&account_dir, &account_id)?;
    serde_json::to_string(&stats).str_err()
}

#[tauri::command]
pub fn export_account_zip(
    app: tauri::AppHandle,
    account_id: Option<String>,
    #[allow(non_snake_case)] accountId: Option<String>,
    output_dir: Option<String>,
    #[allow(non_snake_case)] outputDir: Option<String>,
) -> Result<String, String> {
    let account_id = resolve_account_id_arg(account_id, accountId)?;
    let data_dir = app.path().app_data_dir().str_err()?;
    let account_dir = data_dir.join("accounts").join(&account_id);
    if !account_dir.exists() {
        return Err(format!("账号目录不存在: {}", account_id));
    }

    let stats = build_account_export_stats(&account_dir, &account_id)?;
    let user_label = account_export_user_label(&account_dir, &account_id);
    let timestamp = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let file_name = format!(
        "gemini-{}-{}.zip",
        sanitize_file_component(&user_label),
        timestamp
    );

    let preferred_export_dir = output_dir
        .or(outputDir)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from);

    let export_dir = preferred_export_dir
        .unwrap_or_else(|| dirs::download_dir().unwrap_or_else(|| data_dir.join("exports")));
    if !export_dir.exists() {
        std::fs::create_dir_all(&export_dir).str_err()?;
    }
    if !export_dir.is_dir() {
        return Err(format!("导出目录不可用: {}", export_dir.display()));
    }

    let zip_path = export_dir.join(file_name);
    if zip_path.exists() {
        std::fs::remove_file(&zip_path).str_err()?;
    }

    zip_account_dir(&account_dir, &zip_path)?;
    let zip_size_bytes = std::fs::metadata(&zip_path).str_err()?.len();

    let result = serde_json::json!({
        "accountId": account_id,
        "zipPath": zip_path.to_string_lossy().to_string(),
        "fileName": zip_path.file_name().and_then(|s| s.to_str()).unwrap_or("export.zip"),
        "zipSizeBytes": zip_size_bytes,
        "conversationCount": stats.conversation_count,
        "conversationFileCount": stats.conversation_file_count,
        "mediaFileCount": stats.media_file_count,
        "totalFileCount": stats.total_file_count,
        "totalBytes": stats.total_bytes,
        "estimatedZipBytes": stats.estimated_zip_bytes,
    });
    serde_json::to_string(&result).str_err()
}

// ============================================================================
// Kelivo 导出
// ============================================================================

fn to_cst(utc_str: &str) -> String {
    let cst = FixedOffset::east_opt(8 * 3600).unwrap();
    if let Ok(dt) = DateTime::parse_from_rfc3339(utc_str) {
        let cst_dt = dt.with_timezone(&cst);
        return format!("{}+00:00", cst_dt.format("%Y-%m-%dT%H:%M:%S"));
    }
    utc_str.to_string()
}

fn to_cst_value(v: &serde_json::Value) -> serde_json::Value {
    match v.as_str() {
        Some(s) => serde_json::Value::String(to_cst(s)),
        None => v.clone(),
    }
}

fn parse_size(s: &str) -> Result<u64, String> {
    let upper = s.trim().to_uppercase();
    for (suffix, mult) in &[
        ("GB", 1u64 << 30),
        ("MB", 1u64 << 20),
        ("KB", 1u64 << 10),
        ("B", 1u64),
    ] {
        if upper.ends_with(suffix) {
            let num_str = upper[..upper.len() - suffix.len()].trim();
            let val: f64 = num_str
                .parse()
                .map_err(|_| format!("无法解析大小: {}", s))?;
            return Ok((val * (*mult as f64)).round() as u64);
        }
    }
    s.trim()
        .parse::<u64>()
        .map_err(|_| format!("无法解析大小: {}", s))
}

fn idx_to_label(mut n: usize) -> String {
    let mut label = String::new();
    n += 1;
    while n > 0 {
        let r = (n - 1) % 26;
        label.insert(0, (b'a' + r as u8) as char);
        n = (n - 1) / 26;
    }
    label
}

fn build_kelivo_content(text: &str, attachments: &[serde_json::Value]) -> String {
    let mut parts: Vec<String> = vec![text.to_string()];
    for att in attachments {
        let media_id = att.get("mediaId").and_then(|v| v.as_str()).unwrap_or("");
        if media_id.is_empty() {
            continue;
        }
        let mime = att
            .get("mimeType")
            .and_then(|v| v.as_str())
            .unwrap_or("application/octet-stream");
        if mime.starts_with("image/") {
            parts.push(format!("[image:/upload/{}]", media_id));
        } else {
            parts.push(format!(
                "[file:/upload/{mid}|{mid}|{mime}]",
                mid = media_id,
                mime = mime
            ));
        }
    }
    parts.join("\n")
}

struct KelivoItem {
    #[allow(dead_code)]
    conv_id: String,
    kelivo_conv: serde_json::Value,
    kelivo_msgs: Vec<serde_json::Value>,
    media_ids: Vec<String>,
    json_bytes: u64,
    media_bytes: u64,
}

fn parse_kelivo_jsonl(
    path: &Path,
    media_dir: &Path,
    after_date: Option<&str>,
) -> Result<Option<KelivoItem>, String> {
    let raw = std::fs::read_to_string(path).str_err()?;
    let mut meta: Option<serde_json::Value> = None;
    let mut messages: Vec<serde_json::Value> = Vec::new();

    for line in raw.lines() {
        let s = line.trim();
        if s.is_empty() {
            continue;
        }
        let obj: serde_json::Value = serde_json::from_str(s).str_err()?;
        match obj.get("type").and_then(|v| v.as_str()) {
            Some("meta") => {
                if meta.is_none() {
                    meta = Some(obj);
                }
            }
            Some("message") => messages.push(obj),
            _ => {}
        }
    }

    let meta = match meta {
        Some(m) => m,
        None => return Ok(None),
    };

    if let Some(after) = after_date {
        let updated_at = meta.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("");
        if !updated_at.is_empty() && updated_at < after {
            return Ok(None);
        }
    }

    let conv_id = meta
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let title = meta
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_at = to_cst_value(
        &meta
            .get("createdAt")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );
    let updated_at = to_cst_value(
        &meta
            .get("updatedAt")
            .cloned()
            .unwrap_or(serde_json::Value::Null),
    );

    let mut kelivo_msgs: Vec<serde_json::Value> = Vec::new();
    let mut message_ids: Vec<serde_json::Value> = Vec::new();
    let mut media_ids_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for msg in &messages {
        if msg.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }
        let text = msg.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let msg_id = msg
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let role_raw = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");
        let role = if role_raw == "model" {
            "assistant"
        } else {
            "user"
        };
        let attachments = msg
            .get("attachments")
            .and_then(|v| v.as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);
        let content = build_kelivo_content(text, attachments);
        let timestamp = to_cst_value(
            &msg.get("timestamp")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        );

        for att in attachments {
            if let Some(mid) = att.get("mediaId").and_then(|v| v.as_str()) {
                if !mid.is_empty() {
                    media_ids_set.insert(mid.to_string());
                }
            }
        }

        message_ids.push(serde_json::Value::String(msg_id.clone()));
        kelivo_msgs.push(serde_json::json!({
            "id": msg_id,
            "role": role,
            "content": content,
            "timestamp": timestamp,
            "modelId": msg.get("model").cloned().unwrap_or(serde_json::Value::Null),
            "providerId": "google",
            "totalTokens": null,
            "conversationId": conv_id,
            "isStreaming": false,
            "reasoningText": msg.get("thinking").cloned().unwrap_or(serde_json::Value::Null),
            "reasoningStartAt": null,
            "reasoningFinishedAt": null,
            "translation": null,
            "reasoningSegmentsJson": null,
            "groupId": null,
            "version": 0,
        }));
    }

    let kelivo_conv = serde_json::json!({
        "id": conv_id,
        "title": title,
        "createdAt": created_at,
        "updatedAt": updated_at,
        "messageIds": message_ids,
        "isPinned": false,
        "mcpServerIds": [],
        "assistantId": null,
        "truncateIndex": -1,
        "versionSelections": {},
        "summary": null,
        "lastSummarizedMessageCount": 0,
    });

    let conv_bytes = serde_json::to_string(&kelivo_conv)
        .unwrap_or_default()
        .len() as u64;
    let msgs_bytes: u64 = kelivo_msgs
        .iter()
        .map(|m| serde_json::to_string(m).unwrap_or_default().len() as u64)
        .sum();
    let json_bytes = conv_bytes + msgs_bytes;

    let media_ids: Vec<String> = media_ids_set.into_iter().collect();

    let media_bytes: u64 = media_ids
        .iter()
        .filter_map(|mid| {
            let p = media_dir.join(mid);
            p.metadata().ok().map(|m| m.len())
        })
        .sum();

    Ok(Some(KelivoItem {
        conv_id,
        kelivo_conv,
        kelivo_msgs,
        media_ids,
        json_bytes,
        media_bytes,
    }))
}

fn pack_bins(
    items: Vec<KelivoItem>,
    json_limit: Option<u64>,
    media_limit: Option<u64>,
) -> Vec<Vec<KelivoItem>> {
    if json_limit.is_none() && media_limit.is_none() {
        return vec![items];
    }

    let mut indexed: Vec<(usize, f64)> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let jn = json_limit
                .map(|lim| item.json_bytes as f64 / lim as f64)
                .unwrap_or(0.0);
            let mn = media_limit
                .map(|lim| item.media_bytes as f64 / lim as f64)
                .unwrap_or(0.0);
            (i, jn.max(mn))
        })
        .collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    struct Bin {
        json_used: u64,
        media_used: u64,
        indices: Vec<usize>,
    }

    let mut bins: Vec<Bin> = Vec::new();

    for (idx, _) in indexed {
        let jb = items[idx].json_bytes;
        let mb = items[idx].media_bytes;
        let exceeds = json_limit.map(|lim| jb > lim).unwrap_or(false)
            || media_limit.map(|lim| mb > lim).unwrap_or(false);

        if exceeds {
            bins.push(Bin {
                json_used: jb,
                media_used: mb,
                indices: vec![idx],
            });
            continue;
        }

        let mut placed = false;
        for bin in &mut bins {
            let json_ok = json_limit
                .map(|lim| bin.json_used + jb <= lim)
                .unwrap_or(true);
            let media_ok = media_limit
                .map(|lim| bin.media_used + mb <= lim)
                .unwrap_or(true);
            if json_ok && media_ok {
                bin.indices.push(idx);
                bin.json_used += jb;
                bin.media_used += mb;
                placed = true;
                break;
            }
        }

        if !placed {
            bins.push(Bin {
                json_used: jb,
                media_used: mb,
                indices: vec![idx],
            });
        }
    }

    let mut items_opt: Vec<Option<KelivoItem>> = items.into_iter().map(Some).collect();
    bins.into_iter()
        .map(|bin| {
            bin.indices
                .into_iter()
                .map(|i| items_opt[i].take().unwrap())
                .collect()
        })
        .collect()
}

fn write_kelivo_zip(
    zip_path: &Path,
    bin_items: &[KelivoItem],
    media_dir: &Path,
) -> Result<(usize, usize, usize, usize), String> {
    let all_convs: Vec<&serde_json::Value> = bin_items.iter().map(|it| &it.kelivo_conv).collect();
    let all_msgs: Vec<&serde_json::Value> = bin_items
        .iter()
        .flat_map(|it| it.kelivo_msgs.iter())
        .collect();
    let mut all_mids: Vec<&str> = bin_items
        .iter()
        .flat_map(|it| it.media_ids.iter().map(|s| s.as_str()))
        .collect();
    all_mids.sort_unstable();
    all_mids.dedup();

    let chats_obj = serde_json::json!({
        "version": 1,
        "conversations": all_convs,
        "messages": all_msgs,
        "toolEvents": {},
        "geminiThoughtSigs": {},
    });
    let chats_json = serde_json::to_string(&chats_obj).str_err()?;

    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent).str_err()?;
    }

    let file = std::fs::File::create(zip_path).str_err()?;
    let mut zw = zip::ZipWriter::new(file);
    let opts_deflate = zip_opts_deflate();
    let opts_stored = zip_opts_stored();

    zw.start_file("chats.json", opts_deflate).str_err()?;
    zw.write_all(chats_json.as_bytes()).str_err()?;

    let mut media_found = 0usize;
    let mut media_missing = 0usize;
    for mid in &all_mids {
        let src = media_dir.join(mid);
        if src.exists() {
            let opts = if should_store(&src) {
                opts_stored
            } else {
                opts_deflate
            };
            zw.start_file(format!("upload/{}", mid), opts).str_err()?;
            let mut f = std::fs::File::open(&src).str_err()?;
            std::io::copy(&mut f, &mut zw).str_err()?;
            media_found += 1;
        } else {
            media_missing += 1;
        }
    }

    zw.finish().str_err()?;

    Ok((all_convs.len(), all_msgs.len(), media_found, media_missing))
}

fn kelivo_export_impl(
    data_dir: &Path,
    account_id: &str,
    output_path: &Path,
    json_limit: Option<u64>,
    media_limit: Option<u64>,
    after_date: Option<&str>,
) -> Result<String, String> {
    let account_dir = data_dir.join("accounts").join(account_id);
    let conv_dir = account_dir.join("conversations");
    let media_dir = account_dir.join("media");

    if !conv_dir.exists() {
        return Err(format!("对话目录不存在: {}", conv_dir.display()));
    }

    let mut jsonl_files: Vec<PathBuf> = std::fs::read_dir(&conv_dir)
        .str_err()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| storage::is_jsonl_file(p))
        .collect();
    jsonl_files.sort();

    let mut items: Vec<KelivoItem> = Vec::new();
    let mut skipped = 0usize;

    for jsonl_path in &jsonl_files {
        match parse_kelivo_jsonl(jsonl_path, &media_dir, after_date) {
            Ok(Some(item)) => items.push(item),
            Ok(None) => skipped += 1,
            Err(_) => skipped += 1,
        }
    }

    let total_convs = items.len();
    let total_msgs: usize = items.iter().map(|it| it.kelivo_msgs.len()).sum();

    let bins = pack_bins(items, json_limit, media_limit);
    let multi = bins.len() > 1;

    let stem = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("kelivo_backup")
        .to_string();
    let suffix = output_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("zip")
        .to_string();
    let output_dir = output_path.parent().unwrap_or(output_path);

    let mut result_lines: Vec<String> = Vec::new();

    for (idx, bin_items) in bins.iter().enumerate() {
        let zip_path = if multi {
            let label = idx_to_label(idx);
            output_dir.join(format!(
                "{}_{}{}",
                label,
                stem,
                if suffix.is_empty() {
                    String::new()
                } else {
                    format!(".{}", suffix)
                }
            ))
        } else {
            output_path.to_path_buf()
        };

        let (conv_count, msg_count, media_found, media_missing) =
            write_kelivo_zip(&zip_path, bin_items, &media_dir)?;

        let size_mb = std::fs::metadata(&zip_path)
            .map(|m| m.len() as f64 / 1024.0 / 1024.0)
            .unwrap_or(0.0);

        let label_prefix = if multi {
            format!("[{}] ", idx_to_label(idx))
        } else {
            String::new()
        };
        result_lines.push(format!(
            "  {}{}  {} 对话  {} 消息  媒体 {}✓/{}✗  {:.1}MB",
            label_prefix,
            zip_path.file_name().and_then(|s| s.to_str()).unwrap_or(""),
            conv_count,
            msg_count,
            media_found,
            media_missing,
            size_mb,
        ));
    }

    let summary = if multi {
        format!(
            "[信息] 成功转换: {} 对话，{} 条消息，跳过 {}\n{}\n[完成] 共 {} 个包，输出到 {}",
            total_convs,
            total_msgs,
            skipped,
            result_lines.join("\n"),
            bins.len(),
            output_dir.display(),
        )
    } else {
        let zip_path = output_path;
        let size_mb = std::fs::metadata(zip_path)
            .map(|m| m.len() as f64 / 1024.0 / 1024.0)
            .unwrap_or(0.0);
        format!(
            "[信息] 成功转换: {} 对话，{} 条消息，跳过 {}\n{}\n[完成] 输出: {}  ({:.1} MB)",
            total_convs,
            total_msgs,
            skipped,
            result_lines.join("\n"),
            zip_path.display(),
            size_mb,
        )
    };

    Ok(summary)
}

#[tauri::command]
pub async fn export_account_kelivo(
    app: tauri::AppHandle,
    account_id: String,
    output_path: String,
    after_date: Option<String>,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let output = PathBuf::from(&output_path);
    let after = after_date.clone();

    tauri::async_runtime::spawn_blocking(move || {
        kelivo_export_impl(
            &data_dir,
            &account_id,
            &output,
            None,
            None,
            after.as_deref(),
        )
    })
    .await
    .str_err()?
}

#[tauri::command]
pub async fn export_account_kelivo_split(
    app: tauri::AppHandle,
    account_id: String,
    output_path: String,
    max_json: Option<String>,
    max_upload: Option<String>,
    after_date: Option<String>,
) -> Result<String, String> {
    let json_limit = match &max_json {
        Some(s) if !s.trim().is_empty() => Some(parse_size(s)?),
        _ => None,
    };
    let media_limit = match &max_upload {
        Some(s) if !s.trim().is_empty() => Some(parse_size(s)?),
        _ => None,
    };

    let data_dir = app.path().app_data_dir().str_err()?;
    let output = PathBuf::from(&output_path);
    let after = after_date.clone();

    tauri::async_runtime::spawn_blocking(move || {
        kelivo_export_impl(
            &data_dir,
            &account_id,
            &output,
            json_limit,
            media_limit,
            after.as_deref(),
        )
    })
    .await
    .str_err()?
}
