//! 媒体下载：单文件/批量下载、cookie 鉴权、重定向跟踪、media_id 分配、失败重试。
//!
//! 对应 Python GeminiExporter 中的：
//! - `_download_one_media_no_cdp`
//! - `_download_media_batch_no_cdp`
//! - `_assign_media_ids_and_collect_downloads`
//! - `_retry_failed_media_for_conversation`

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::str_err::ToStringErr;
use reqwest::header;
use url::Url;

use crate::browser_info;
use crate::media::{append_authuser, infer_media_type, is_protected_media_url, media_log_fields};
use crate::protocol::GEMINI_BASE;
use crate::storage;

use super::GeminiExporter;

/// 构建不跟踪重定向的轻量客户端（媒体下载专用）
fn build_no_redirect_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .expect("Failed to build no-redirect client")
}

/// 单个待下载媒体项
#[derive(Debug, Clone)]
pub struct MediaDownloadItem {
    pub url: String,
    pub filepath: PathBuf,
    pub media_id: String,
    pub media_type: Option<String>,
}

/// 下载失败项
#[derive(Debug, Clone, serde::Serialize)]
pub struct FailedDownloadItem {
    pub media_id: String,
    pub url: String,
    pub error: String,
}

/// 批量下载统计
#[derive(Debug, Default)]
pub struct DownloadStats {
    pub media_downloaded: usize,
    pub media_failed: usize,
}

/// 重试结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetryResult {
    pub attempted: usize,
    pub recovered: usize,
    pub failed: usize,
    pub missing_url: usize,
    pub flag_marked: usize,
    pub flag_cleared: usize,
}

// ============================================================================
// 单文件下载
// ============================================================================

impl GeminiExporter {
    /// 下载单个媒体文件（手动跟踪重定向，最多 8 跳）。
    ///
    /// protected host 才注入 cookie header。
    /// 返回 Ok(Some(bytes)) 成功，Ok(None) 下载失败但非致命，Err 致命错误。
    async fn download_one_media(
        &self,
        url: &str,
        cookie_header: &str,
        referer: &str,
        media_type: Option<&str>,
        media_hint: Option<&str>,
    ) -> Result<Option<Vec<u8>>, String> {
        let no_redirect_client = build_no_redirect_client();
        let mut current_url = url.to_string();

        for _hop in 0..8 {
            let mut headers = header::HeaderMap::new();
            headers.insert(
                header::ACCEPT,
                header::HeaderValue::from_static(
                    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8",
                ),
            );
            headers.insert(
                header::ACCEPT_LANGUAGE,
                header::HeaderValue::from_str(browser_info::detect_accept_language())
                    .unwrap_or_else(|_| header::HeaderValue::from_static("en-US,en;q=0.9")),
            );
            if let Ok(val) = header::HeaderValue::from_str(referer) {
                headers.insert(header::REFERER, val);
            }
            headers.insert(
                header::USER_AGENT,
                header::HeaderValue::from_str(browser_info::build_user_agent())
                    .unwrap_or_else(|_| header::HeaderValue::from_static("Mozilla/5.0")),
            );

            // protected host 才注入 cookie
            if is_protected_media_url(&current_url) {
                if let Ok(val) = header::HeaderValue::from_str(cookie_header) {
                    headers.insert(header::COOKIE, val);
                }
            }

            let mut resp_result = None;
            for attempt in 1..=3 {
                self.before_request("media_http_get").await?;
                let r = no_redirect_client
                    .get(&current_url)
                    .headers(headers.clone())
                    .send()
                    .await;
                
                match &r {
                    Ok(res) if res.status().is_server_error() => {
                        log::warn!("[media-retry] 5xx 服务端错误 {}, 第 {}/3 次重试 | {}", res.status(), attempt, current_url);
                    }
                    Ok(_) => {
                        resp_result = Some(r);
                        break;
                    }
                    Err(e) => {
                        log::warn!("[media-retry] 网络波动 {}, 第 {}/3 次重试 | {}", e, attempt, current_url);
                    }
                }

                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt as u64)).await;
                } else {
                    resp_result = Some(r);
                }
            }

            let resp = match resp_result.expect("Should have a result") {
                Ok(r) => r,
                Err(e) => {
                    let fields = media_log_fields(Some(&current_url), media_type, media_hint);
                    log::warn!(
                        "[media-fail] 彻底失败，重试多次依然异常: {} | media={} domain={}",
                        e,
                        fields.media,
                        fields.domain
                    );
                    return Ok(None);
                }
            };

            let status = resp.status();
            let fields = media_log_fields(Some(&current_url), media_type, media_hint);

            if status.is_redirection() {
                let location = resp
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok());
                match location {
                    Some(loc) => {
                        // 相对 URL 拼接
                        current_url = match Url::parse(&current_url) {
                            Ok(base) => base
                                .join(loc)
                                .map(|u| u.to_string())
                                .unwrap_or_else(|_| loc.to_string()),
                            Err(_) => loc.to_string(),
                        };
                        continue;
                    }
                    None => {
                        log::warn!(
                            "[media-fail] 重定向缺少 location | media={} domain={}",
                            fields.media,
                            fields.domain
                        );
                        return Ok(None);
                    }
                }
            }

            if status.is_success() {
                let bytes = resp.bytes().await.str_err()?;
                return Ok(Some(bytes.to_vec()));
            }

            log::warn!(
                "[media-fail] 非200状态码={} | media={} domain={}",
                status.as_u16(),
                fields.media,
                fields.domain
            );
            return Ok(None);
        }

        let fields = media_log_fields(Some(url), media_type, media_hint);
        log::warn!(
            "[media-fail] 重定向次数超限 | media={} domain={}",
            fields.media,
            fields.domain
        );
        Ok(None)
    }

    // ========================================================================
    // 批量下载
    // ========================================================================

    /// 批量下载媒体文件。返回失败项列表。
    pub async fn download_media_batch(
        &self,
        media_list: &[MediaDownloadItem],
        stats: &mut DownloadStats,
    ) -> Vec<FailedDownloadItem> {
        let mut failed_items = Vec::new();
        if media_list.is_empty() {
            return failed_items;
        }

        let authuser = self.authuser.clone();
        let cookie_header = self.build_media_cookie_header();
        let referer = match &authuser {
            Some(au) => format!("{}/u/{}/app", GEMINI_BASE, au),
            None => format!("{}/app", GEMINI_BASE),
        };

        for item in media_list {
            if item.filepath.exists() {
                stats.media_downloaded += 1;
                continue;
            }

            let t_media = std::time::Instant::now();

            // 带 authuser 的 URL
            let candidates = match &authuser {
                Some(au) => vec![append_authuser(&item.url, au)],
                None => vec![item.url.clone()],
            };

            let media_hint = Some(item.media_id.as_str());
            let media_type = item.media_type.as_deref();

            let mut content: Option<Vec<u8>> = None;
            for candidate_url in &candidates {
                match self
                    .download_one_media(
                        candidate_url,
                        &cookie_header,
                        &referer,
                        media_type,
                        media_hint,
                    )
                    .await
                {
                    Ok(Some(bytes)) => {
                        content = Some(bytes);
                        break;
                    }
                    Ok(None) => continue,
                    Err(e) => {
                        // 用户取消等致命错误
                        log::warn!("[media] 致命错误，中止批量下载: {}", e);
                        return failed_items;
                    }
                }
            }

            if let Some(bytes) = content {
                if let Some(parent) = item.filepath.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = std::fs::write(&item.filepath, &bytes) {
                    let fname = item
                        .filepath
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("?");
                    log::error!("[media-fail] 写入文件失败: {} - {}", fname, e);
                    stats.media_failed += 1;
                    failed_items.push(FailedDownloadItem {
                        media_id: item.media_id.clone(),
                        url: item.url.clone(),
                        error: format!("write_failed: {}", e),
                    });
                } else {
                    let size_mb = bytes.len() as f64 / (1024.0 * 1024.0);
                    let fields = media_log_fields(
                        Some(&item.url),
                        item.media_type.as_deref(),
                        Some(item.media_id.as_str()),
                    );
                    log::info!(
                        "[media] ok: {} {:.2}MB media={} domain={} {}ms",
                        item.media_id,
                        size_mb,
                        fields.media,
                        fields.domain,
                        t_media.elapsed().as_millis()
                    );
                    stats.media_downloaded += 1;
                }
            } else {
                let fields = media_log_fields(Some(&item.url), media_type, media_hint);
                log::warn!(
                    "[media-fail] 媒体下载失败，已跳过: {} | media={} domain={}",
                    item.filepath
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("?"),
                    fields.media,
                    fields.domain
                );
                stats.media_failed += 1;
                failed_items.push(FailedDownloadItem {
                    media_id: item.media_id.clone(),
                    url: item.url.clone(),
                    error: "download_failed".to_string(),
                });
            }
        }

        failed_items
    }

    // ========================================================================
    // media_id 分配
    // ========================================================================

    /// 为 parsed_turns 中的媒体分配 media_id，收集待下载列表。
    ///
    /// `global_seen_urls`: url → filename 映射（复用已见 URL）
    /// `global_used_names`: 已使用的文件名集合（避免冲突）
    ///
    /// 返回待下载项列表。
    pub fn assign_media_ids_and_collect_downloads(
        &self,
        parsed_turns: &mut [serde_json::Value],
        media_dir: &Path,
        global_seen_urls: &mut HashMap<String, String>,
        global_used_names: &mut HashSet<String>,
    ) -> Vec<MediaDownloadItem> {
        let mut batch_list = Vec::new();

        for turn in parsed_turns.iter_mut() {
            let turn_obj = match turn.as_object_mut() {
                Some(o) => o,
                None => continue,
            };

            for role_key in &["user", "assistant"] {
                let files = match turn_obj
                    .get_mut(*role_key)
                    .and_then(|v| v.as_object_mut())
                    .and_then(|o| o.get_mut("files"))
                    .and_then(|v| v.as_array_mut())
                {
                    Some(f) => f,
                    None => continue,
                };

                for f in files.iter_mut() {
                    let f_obj = match f.as_object_mut() {
                        Some(o) => o,
                        None => continue,
                    };

                    let url = match f_obj.get("url").and_then(|v| v.as_str()) {
                        Some(u) if !u.is_empty() => u.to_string(),
                        _ => continue,
                    };

                    let media_id = if let Some(fname) = global_seen_urls.get(&url) {
                        fname.clone()
                    } else {
                        let media_type = f_obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let ext = determine_extension(media_type, f_obj);
                        let fname = loop {
                            let stem = uuid::Uuid::new_v4().to_string().replace('-', "");
                            let candidate = format!("{}.{}", stem, ext);
                            if !global_used_names.contains(&candidate) {
                                break candidate;
                            }
                        };
                        global_used_names.insert(fname.clone());
                        global_seen_urls.insert(url.clone(), fname.clone());
                        fname
                    };

                    f_obj.insert(
                        "media_id".to_string(),
                        serde_json::Value::String(media_id.clone()),
                    );

                    let target = media_dir.join(&media_id);
                    if !target.exists()
                        && !batch_list
                            .iter()
                            .any(|b: &MediaDownloadItem| b.filepath == target)
                    {
                        batch_list.push(MediaDownloadItem {
                            url: url.clone(),
                            filepath: target,
                            media_id: media_id.clone(),
                            media_type: f_obj
                                .get("type")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        });
                    }
                }
            }
        }

        batch_list
    }

    // ========================================================================
    // 失败重试
    // ========================================================================

    /// 对指定会话的 JSONL 重试失败媒体下载。
    pub async fn retry_failed_media_for_conversation(
        &self,
        jsonl_file: &Path,
        account_dir: &Path,
        media_dir: &Path,
        stats: &mut DownloadStats,
    ) -> RetryResult {
        let empty_result = RetryResult {
            attempted: 0,
            recovered: 0,
            failed: 0,
            missing_url: 0,
            flag_marked: 0,
            flag_cleared: 0,
        };

        if !jsonl_file.exists() {
            return empty_result;
        }

        let rows = storage::read_jsonl_rows(jsonl_file);
        if rows.is_empty() {
            return empty_result;
        }

        let media_id_to_url = storage::build_media_id_to_url_map(account_dir);
        let (pending, recovered_existing) =
            storage::scan_failed_media_from_rows(&rows, media_dir, &media_id_to_url);

        if pending.is_empty() && recovered_existing.is_empty() {
            return empty_result;
        }

        let downloadable: Vec<_> = pending
            .iter()
            .filter(|p| p.url.as_ref().map(|u| !u.is_empty()).unwrap_or(false))
            .collect();
        let missing_url: Vec<_> = pending.iter().filter(|p| p.url.is_none()).collect();

        let retry_batch: Vec<MediaDownloadItem> = downloadable
            .iter()
            .map(|item| MediaDownloadItem {
                url: item.url.clone().unwrap_or_default(),
                filepath: media_dir.join(&item.media_id),
                media_id: item.media_id.clone(),
                media_type: Some(infer_media_type(&item.media_id).to_string()),
            })
            .collect();

        let failed_items = if !retry_batch.is_empty() {
            self.download_media_batch(&retry_batch, stats).await
        } else {
            Vec::new()
        };

        let mut failed_map: HashMap<String, String> = failed_items
            .iter()
            .map(|item| (item.media_id.clone(), item.error.clone()))
            .collect();
        for item in &missing_url {
            failed_map.insert(item.media_id.clone(), "missing_manifest_url".to_string());
        }

        let attempted_ids: HashSet<String> =
            downloadable.iter().map(|i| i.media_id.clone()).collect();
        let mut recovered_ids: HashSet<String> = recovered_existing;
        for id in &attempted_ids {
            if !failed_map.contains_key(id) {
                recovered_ids.insert(id.clone());
            }
        }

        let flag_stats =
            storage::update_jsonl_media_failure_flags(jsonl_file, &failed_map, &recovered_ids)
                .unwrap_or_default();

        RetryResult {
            attempted: attempted_ids.len(),
            recovered: recovered_ids.len(),
            failed: failed_map.len(),
            missing_url: missing_url.len(),
            flag_marked: *flag_stats.get("marked").unwrap_or(&0),
            flag_cleared: *flag_stats.get("cleared").unwrap_or(&0),
        }
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 根据媒体类型和文件名推断扩展名
fn determine_extension(
    media_type: &str,
    f_obj: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let default_ext = match media_type {
        "video" => "mp4",
        "audio" => "mp3",
        "attachment" => "bin",
        _ => "jpg",
    };

    let raw_name = f_obj.get("filename").and_then(|v| v.as_str()).unwrap_or("");

    if !raw_name.is_empty() {
        let known_exts = [
            "jpg", "jpeg", "png", "webp", "gif", "bmp", "mp4", "mov", "webm", "mkv", "mp3", "m4a",
            "wav", "aac", "flac", "ogg", // attachment / document types
            "md", "txt", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "csv", "json", "xml",
            "html", "htm", "rtf", "zip", "tar", "gz",
        ];
        if let Some(ext) = Path::new(raw_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
        {
            if known_exts.contains(&ext.as_str()) {
                return ext;
            }
        }
    }

    default_ext.to_string()
}
