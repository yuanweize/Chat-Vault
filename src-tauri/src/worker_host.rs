//! In-process worker：直接在 Tauri 主进程内调度任务，不再依赖 Python 子进程。
//!
//! Phase 5 架构：
//! - 每个账号一个 `ArcSwap<GeminiExporter>`，无锁读取，session 过期时原子替换
//! - 任务通过 `tokio::spawn` 调度，进度事件直接 `AppHandle::emit()`
//! - 取消机制使用 `CancellationToken`（AtomicBool，替代 `.cancel_requested` 文件 flag）
//! - 每个账号同一时刻只有一个任务，无并发竞态

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use crate::cookies;
use crate::str_err::ToStringErr;
use crate::sync::CancellationToken;
use crate::gemini_api::GeminiExporter;

const WORKER_EVENT_JOB_STATE: &str = "worker://job_state";

const MAX_SESSION_RETRIES: u32 = 1;

// ============================================================================
// 公开类型
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueJobRequest {
    #[serde(rename = "type")]
    pub job_type: String,
    pub account_id: String,
    pub conversation_id: Option<String>,
}

impl EnqueueJobRequest {
    fn validate(&self) -> Result<(), String> {
        if self.account_id.trim().is_empty() {
            return Err("accountId 不能为空".to_string());
        }
        match self.job_type.as_str() {
            "sync_list" | "sync_full" | "sync_incremental" => Ok(()),
            "sync_conversation" => {
                let conv_id = self
                    .conversation_id
                    .as_ref()
                    .map(|v| v.trim())
                    .unwrap_or("");
                if conv_id.is_empty() {
                    Err("sync_conversation 需要 conversationId".to_string())
                } else {
                    Ok(())
                }
            }
            _ => Err(format!("不支持的任务类型: {}", self.job_type)),
        }
    }
}

// ============================================================================
// 内部类型
// ============================================================================

struct AccountSession {
    exporter: ArcSwap<GeminiExporter>,
}

struct JobContext {
    job_id: String,
    job_type: String,
    account_id: String,
    conversation_id: Option<String>,
}

// ============================================================================
// WorkerHost
// ============================================================================

pub struct WorkerHost {
    app: AppHandle,
    output_dir: PathBuf,
    /// 按账号缓存的 exporter session
    sessions: Mutex<HashMap<String, Arc<AccountSession>>>,
    /// 按账号追踪活跃任务的取消令牌
    active_cancels: Mutex<HashMap<String, CancellationToken>>,
    /// Cookie 缓存（多账号共享同一组浏览器 cookies）
    cookies_cache: Mutex<Option<Arc<HashMap<String, String>>>>,
    /// 递增 job ID
    next_job_id: AtomicU64,
    /// 是否正在关闭
    shutting_down: AtomicBool,
}

impl WorkerHost {
    fn new(app: AppHandle, output_dir: PathBuf) -> Self {
        Self {
            app,
            output_dir,
            sessions: Mutex::new(HashMap::new()),
            active_cancels: Mutex::new(HashMap::new()),
            cookies_cache: Mutex::new(None),
            next_job_id: AtomicU64::new(1),
            shutting_down: AtomicBool::new(false),
        }
    }

    /// 读取 cookies（带缓存）。
    /// Windows 上由 open_google_login 登录成功后通过 set_cookies 注入；
    /// macOS/Linux 上从本机浏览器读取。
    async fn get_cookies(&self) -> Result<Arc<HashMap<String, String>>, String> {
        let mut cache = self.cookies_cache.lock().await;
        if let Some(ref c) = *cache {
            return Ok(Arc::clone(c));
        }

        #[cfg(target_os = "windows")]
        {
            return Err("Windows 上需要先通过 WebView2 登录获取 cookies（请点击登录按钮）".to_string());
        }

        #[cfg(not(target_os = "windows"))]
        {
            let cookies = tokio::task::spawn_blocking(|| {
                cookies::get_cookies_from_local_browser()
            })
            .await
            .map_err(|e| format!("cookies 读取任务失败: {}", e))?
            .map_err(|e| format!("cookies 读取失败: {}", e))?;

            if cookies.is_empty() {
                return Err("本机浏览器 cookies 读取结果为空".to_string());
            }
            let arc_cookies = Arc::new(cookies);
            *cache = Some(Arc::clone(&arc_cookies));
            Ok(arc_cookies)
        }
    }

    /// 外部注入 cookies（Windows WebView2 登录后使用）
    #[cfg(target_os = "windows")]
    pub async fn set_cookies(&self, cookies: HashMap<String, String>) {
        let mut cache = self.cookies_cache.lock().await;
        *cache = Some(Arc::new(cookies));
        // 清空已有 session，下次任务会用新 cookies 重建
        self.sessions.lock().await.clear();
    }

    /// 从 accounts.json 读取指定账号的 authuser 和 email
    fn read_account_mapping(&self, account_id: &str) -> Result<(Option<String>, Option<String>), String> {
        let accounts_file = self.output_dir.join("accounts.json");
        if !accounts_file.exists() {
            return Err("accounts.json 不存在，请先导入账号".to_string());
        }
        let content = std::fs::read_to_string(&accounts_file).str_err()?;
        let data: Value = serde_json::from_str(&content).str_err()?;
        let rows = data
            .get("accounts")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "accounts.json 格式错误".to_string())?;

        for item in rows {
            if item.get("id").and_then(|v| v.as_str()) == Some(account_id) {
                let authuser = item
                    .get("authuser")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string());
                let email = item
                    .get("email")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_lowercase());
                return Ok((authuser, email));
            }
        }
        Err(format!("未找到账号映射: {}", account_id))
    }

    /// 获取或创建某个账号的 exporter
    async fn get_exporter(self: &Arc<Self>, account_id: &str) -> Result<Arc<GeminiExporter>, String> {
        {
            let sessions = self.sessions.lock().await;
            if let Some(session) = sessions.get(account_id) {
                return Ok(session.exporter.load_full());
            }
        }

        let exporter = self.create_exporter(account_id).await?;
        let arc_exporter = Arc::new(exporter);
        let session = Arc::new(AccountSession {
            exporter: ArcSwap::new(arc_exporter.clone()),
        });

        self.sessions
            .lock()
            .await
            .insert(account_id.to_string(), session);

        Ok(arc_exporter)
    }

    /// 创建新的 exporter 实例（含 init_auth）
    async fn create_exporter(&self, account_id: &str) -> Result<GeminiExporter, String> {
        let cookies = self.get_cookies().await?;
        let (authuser, email) = self.read_account_mapping(account_id)?;

        let mut exporter = GeminiExporter::new(
            HashMap::clone(&cookies),
            authuser,
            Some(account_id.to_string()),
            email,
        );
        exporter.init_auth().await?;
        Ok(exporter)
    }

    /// 刷新某个账号的 exporter session（重新读取 cookies）
    async fn refresh_session(self: &Arc<Self>, account_id: &str) -> Result<Arc<GeminiExporter>, String> {
        log::warn!("session 过期，重建 exporter (account={})", account_id);

        // 清空 cookie 缓存
        *self.cookies_cache.lock().await = None;

        let exporter = self.create_exporter(account_id).await?;
        let key_fields: Vec<&str> = ["__Secure-1PSID", "__Secure-1PSIDTS"]
            .iter()
            .filter(|k| exporter.cookies.contains_key(**k))
            .copied()
            .collect();
        log::info!(
            "已从 Chrome 读取到 {} 个 cookies，关键字段: {:?}",
            exporter.cookies.len(),
            key_fields
        );
        let arc_exporter = Arc::new(exporter);

        let sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(account_id) {
            session.exporter.store(arc_exporter.clone());
        }

        log::info!("重建成功 ✓");
        Ok(arc_exporter)
    }

    /// 执行函数，失败时重建 exporter 重试一次。
    ///
    /// 流程：执行 → 失败 → 重建 exporter → 重试 → 仍失败 → 放弃。
    async fn run_with_retry<F, Fut, T>(
        self: &Arc<Self>,
        account_id: &str,
        mut make_fut: F,
    ) -> Result<T, String>
    where
        F: FnMut(Arc<GeminiExporter>) -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let mut exporter = self.get_exporter(account_id).await?;

        for attempt in 0..=MAX_SESSION_RETRIES {
            match make_fut(exporter.clone()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if is_cancelled_error(&e) {
                        return Err(e);
                    }
                    if attempt < MAX_SESSION_RETRIES {
                        log::warn!(
                            "任务失败 (attempt {})，重建 exporter 重试: {}",
                            attempt + 1,
                            e
                        );
                        exporter = self.refresh_session(account_id).await?;
                        continue;
                    }
                    return Err(e);
                }
            }
        }
        unreachable!()
    }

    // ========================================================================
    // 事件发送
    // ========================================================================

    fn emit_job_state(
        &self,
        ctx: &JobContext,
        state: &str,
        phase: Option<&str>,
        progress: Option<Value>,
        error: Option<Value>,
    ) {
        let mut payload = json!({
            "jobId": ctx.job_id,
            "state": state,
            "type": ctx.job_type,
            "accountId": ctx.account_id,
        });
        if let Some(cid) = &ctx.conversation_id {
            payload["conversationId"] = json!(cid);
        }
        if let Some(p) = phase {
            payload["phase"] = json!(p);
        }
        if let Some(prog) = progress {
            payload["progress"] = prog;
        }
        if let Some(err) = error {
            payload["error"] = err;
        }
        let _ = self.app.emit(WORKER_EVENT_JOB_STATE, payload);
    }

    // ========================================================================
    // 任务执行
    // ========================================================================

    async fn execute_sync_list(
        self: &Arc<Self>,
        ctx: &JobContext,
        stop_on_unchanged: bool,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let output_dir = self.output_dir.clone();
        let account_id = ctx.account_id.clone();

        self.run_with_retry(&account_id, |exporter| {
            let output_dir = output_dir.clone();
            let cancel = cancel.clone();
            async move {
                let result = exporter
                    .export_list_only(&output_dir, stop_on_unchanged, &cancel)
                    .await?;
                Ok(json!({
                    "total": result.remote_count,
                    "updatedIds": result.updated_ids,
                }))
            }
        })
        .await
    }

    async fn execute_sync_conversation(
        self: &Arc<Self>,
        ctx: &JobContext,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let conversation_id = ctx
            .conversation_id
            .as_ref()
            .ok_or_else(|| "sync_conversation 缺少 conversationId".to_string())?
            .clone();
        let output_dir = self.output_dir.clone();
        let account_id = ctx.account_id.clone();

        self.run_with_retry(&account_id, |exporter| {
            let output_dir = output_dir.clone();
            let cancel = cancel.clone();
            let cid = conversation_id.clone();
            async move {
                exporter
                    .sync_single_conversation(&cid, &output_dir, &cancel)
                    .await?;
                Ok(json!({ "conversationId": cid }))
            }
        })
        .await
    }

    async fn execute_sync_full(
        self: &Arc<Self>,
        ctx: &JobContext,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let account_id = &ctx.account_id;
        let mut success_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut failed_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

        log::info!("[sync_full] 开始全量同步: account={}", account_id);

        let items_before = load_conversation_items(&self.output_dir, account_id);
        let before_set: std::collections::HashSet<String> =
            load_conversation_ids(&items_before).into_iter().collect();

        // 1) 失败重试
        let retry_ids = collect_failed_conversation_ids(&self.output_dir, account_id, &items_before);
        log::info!("[retry_failed] 失败记录重试: {}", retry_ids.len());
        let retry_result = self
            .sync_conversation_batch(ctx, &retry_ids, "retry_failed", cancel)
            .await?;
        merge_batch_result(&retry_result, &mut success_ids, &mut failed_ids);

        // 2) 空对话补齐
        let empty_ids: Vec<String> = collect_empty_conversation_ids(&items_before)
            .into_iter()
            .filter(|id| !success_ids.contains(id))
            .collect();
        log::info!("[sync_empty] 空会话补齐: {}", empty_ids.len());
        let empty_result = self
            .sync_conversation_batch(ctx, &empty_ids, "sync_empty", cancel)
            .await?;
        merge_batch_result(&empty_result, &mut success_ids, &mut failed_ids);

        // 3) 拉最新列表
        self.emit_job_state(ctx, "running", Some("refresh_list"), None, None);
        log::info!("[refresh_list] 拉取最新列表");
        let list_result = self.execute_sync_list(ctx, true, cancel).await?;
        let after_ids = load_conversation_ids(&load_conversation_items(
            &self.output_dir,
            account_id,
        ));
        let updated_ids_from_list: std::collections::HashSet<String> =
            extract_string_vec(&list_result, "updatedIds").into_iter().collect();

        let new_ids: Vec<String> = after_ids
            .iter()
            .filter(|id| !before_set.contains(*id) && !success_ids.contains(*id))
            .cloned()
            .collect();
        log::info!("[sync_new] 新增会话同步: {}", new_ids.len());
        let new_result = self
            .sync_conversation_batch(ctx, &new_ids, "sync_new", cancel)
            .await?;
        merge_batch_result(&new_result, &mut success_ids, &mut failed_ids);

        // 4) 有更新的老会话
        let remaining_old_ids: Vec<String> = after_ids
            .iter()
            .filter(|id| {
                before_set.contains(*id)
                    && !success_ids.contains(*id)
                    && updated_ids_from_list.contains(*id)
            })
            .cloned()
            .collect();
        log::info!(
            "[sync_old] 剩余老会话检查更新: {}",
            remaining_old_ids.len()
        );
        let old_result = self
            .sync_conversation_batch(ctx, &remaining_old_ids, "sync_old", cancel)
            .await?;
        merge_batch_result(&old_result, &mut success_ids, &mut failed_ids);

        let total = after_ids.len();
        self.emit_job_state(
            ctx,
            "running",
            Some("sync_old"),
            Some(json!({"current": total, "total": total})),
            None,
        );
        log::info!(
            "[sync_full] 全量同步结束: total={}, failed={}",
            total,
            failed_ids.len()
        );

        Ok(json!({
            "total": total,
            "failed": failed_ids.len(),
            "progress": { "current": total, "total": total },
        }))
    }

    /// 批量同步对话
    async fn sync_conversation_batch(
        self: &Arc<Self>,
        parent_ctx: &JobContext,
        conv_ids: &[String],
        phase: &str,
        cancel: &CancellationToken,
    ) -> Result<BatchResult, String> {
        let total = conv_ids.len();
        if total == 0 {
            self.emit_job_state(
                parent_ctx,
                "running",
                Some(phase),
                Some(json!({"current": 0, "total": 0})),
                None,
            );
            return Ok(BatchResult::default());
        }

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        log::info!("[{}] 进度: 0/{}", phase, total);

        for (idx, cid) in conv_ids.iter().enumerate() {
            let t_conv = std::time::Instant::now();
            let sub_ctx = JobContext {
                job_id: format!("{}:conv:{}:{}", parent_ctx.job_id, phase, cid),
                job_type: "sync_conversation".to_string(),
                account_id: parent_ctx.account_id.clone(),
                conversation_id: Some(cid.clone()),
            };

            self.emit_job_state(
                &sub_ctx,
                "running",
                Some(phase),
                Some(json!({"current": idx, "total": total})),
                None,
            );

            match self.execute_sync_conversation(&sub_ctx, cancel).await {
                Ok(_) => {
                    self.emit_job_state(
                        &sub_ctx,
                        "done",
                        Some(phase),
                        Some(json!({"current": idx + 1, "total": total})),
                        None,
                    );
                    succeeded.push(cid.clone());
                }
                Err(e) => {
                    if cancel.is_cancelled() || is_cancelled_error(&e) {
                        return Err(e);
                    }
                    self.emit_job_state(
                        &sub_ctx,
                        "failed",
                        Some(phase),
                        Some(json!({"current": idx + 1, "total": total})),
                        Some(to_error_payload(&e)),
                    );
                    failed.push(cid.clone());
                }
            }

            if cancel.is_cancelled() {
                return Err(format!(
                    "用户取消，已处理 {}/{} 个对话",
                    idx + 1,
                    total
                ));
            }

            self.emit_job_state(
                parent_ctx,
                "running",
                Some(phase),
                Some(json!({"current": idx + 1, "total": total})),
                None,
            );

            log::info!(
                "[{}] 进度: {}/{} ok={} fail={} cid={} {}ms",
                phase, idx + 1, total, succeeded.len(), failed.len(), cid,
                t_conv.elapsed().as_millis()
            );
        }

        Ok(BatchResult { succeeded, failed })
    }

    async fn execute_sync_incremental(
        self: &Arc<Self>,
        ctx: &JobContext,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        // 增量同步 = sync_list(stop_on_unchanged=true) + 对有更新的会话逐个 sync_conversation
        let account_id = &ctx.account_id;

        // 1) 同步列表
        let list_result = self.execute_sync_list(ctx, true, cancel).await?;
        let updated_ids = extract_string_vec(&list_result, "updatedIds");

        // 2) 同步有更新的会话
        if !updated_ids.is_empty() {
            log::info!(
                "[sync_incremental] 需要更新 {} 个会话",
                updated_ids.len()
            );
            let _ = self
                .sync_conversation_batch(ctx, &updated_ids, "sync_updated", cancel)
                .await?;
        }

        // 3) 补齐空会话
        let items = load_conversation_items(&self.output_dir, account_id);
        let empty_ids: Vec<String> = collect_empty_conversation_ids(&items);
        if !empty_ids.is_empty() {
            log::info!("[sync_incremental] 补齐 {} 个空会话", empty_ids.len());
            let _ = self
                .sync_conversation_batch(ctx, &empty_ids, "sync_empty", cancel)
                .await?;
        }

        Ok(json!({}))
    }

    /// 分发任务
    async fn execute_job(
        self: &Arc<Self>,
        ctx: &JobContext,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        match ctx.job_type.as_str() {
            "sync_list" => self.execute_sync_list(ctx, false, cancel).await,
            "sync_conversation" => self.execute_sync_conversation(ctx, cancel).await,
            "sync_full" => self.execute_sync_full(ctx, cancel).await,
            "sync_incremental" => self.execute_sync_incremental(ctx, cancel).await,
            _ => Err(format!("未知任务类型: {}", ctx.job_type)),
        }
    }

    /// 提交任务
    async fn enqueue(self: &Arc<Self>, req: &EnqueueJobRequest) -> Result<String, String> {
        let job_id = format!(
            "job_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            self.next_job_id.fetch_add(1, Ordering::SeqCst)
        );

        let ctx = JobContext {
            job_id: job_id.clone(),
            job_type: req.job_type.clone(),
            account_id: req.account_id.trim().to_string(),
            conversation_id: req
                .conversation_id
                .as_ref()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
        };

        // 创建取消令牌
        let cancel = CancellationToken::new();
        {
            let mut cancels = self.active_cancels.lock().await;
            cancels.insert(ctx.account_id.clone(), cancel.clone());
        }

        self.emit_job_state(&ctx, "queued", None, None, None);

        let host = Arc::clone(self);
        let account_id_for_cleanup = ctx.account_id.clone();

        tokio::spawn(async move {
            host.emit_job_state(&ctx, "running", None, None, None);

            let result = host.execute_job(&ctx, &cancel).await;

            match result {
                Ok(result_val) => {
                    let done_progress = result_val.get("progress").cloned();
                    host.emit_job_state(&ctx, "done", None, done_progress, None);
                }
                Err(ref e) if cancel.is_cancelled() || is_cancelled_error(e) => {
                    log::warn!("任务被用户取消: {}", e);
                    host.emit_job_state(&ctx, "cancelled", None, None, None);
                }
                Err(ref e) => {
                    log::error!("任务失败: {}", e);
                    host.emit_job_state(
                        &ctx,
                        "failed",
                        None,
                        None,
                        Some(to_error_payload(e)),
                    );
                }
            }

            // 清理取消令牌
            let mut cancels = host.active_cancels.lock().await;
            cancels.remove(&account_id_for_cleanup);
        });

        Ok(job_id)
    }

    /// 取消指定账号的活跃任务
    async fn cancel_account_job(&self, account_id: &str) {
        let cancels = self.active_cancels.lock().await;
        if let Some(token) = cancels.get(account_id) {
            token.cancel();
            log::info!("已发送取消信号: account={}", account_id);
        }
    }

    fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        // 所有活跃令牌取消
        if let Ok(cancels) = self.active_cancels.try_lock() {
            for (_, token) in cancels.iter() {
                token.cancel();
            }
        }
    }
}

// ============================================================================
// 全局单例
// ============================================================================

static HOST: OnceLock<Arc<WorkerHost>> = OnceLock::new();

pub fn init_worker_host(app: AppHandle, output_dir: PathBuf) -> Result<(), String> {
    let host = Arc::new(WorkerHost::new(app, output_dir));
    HOST.set(host)
        .map_err(|_| "WorkerHost 已初始化".to_string())
}

fn get_host() -> Result<Arc<WorkerHost>, String> {
    HOST.get()
        .cloned()
        .ok_or_else(|| "WorkerHost 未初始化".to_string())
}

pub async fn enqueue_job_async(req: EnqueueJobRequest) -> Result<String, String> {
    req.validate()?;
    let host = get_host()?;
    host.enqueue(&req).await
}

pub async fn cancel_job_async(account_id: &str) -> Result<(), String> {
    let host = get_host()?;
    host.cancel_account_job(account_id).await;
    Ok(())
}

/// 注入 cookies 到 WorkerHost 缓存（Windows WebView2 登录后调用）
#[cfg(target_os = "windows")]
pub async fn set_worker_cookies(cookies: HashMap<String, String>) -> Result<(), String> {
    let host = get_host()?;
    host.set_cookies(cookies).await;
    Ok(())
}

pub fn shutdown_worker_host() {
    if let Some(host) = HOST.get() {
        host.shutdown();
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

#[derive(Default)]
struct BatchResult {
    succeeded: Vec<String>,
    failed: Vec<String>,
}

fn merge_batch_result(
    result: &BatchResult,
    success_ids: &mut std::collections::HashSet<String>,
    failed_ids: &mut std::collections::HashSet<String>,
) {
    success_ids.extend(result.succeeded.iter().cloned());
    failed_ids.extend(result.failed.iter().cloned());
    for id in &result.succeeded {
        failed_ids.remove(id);
    }
}

fn is_cancelled_error(e: &str) -> bool {
    e.contains("用户取消") || e.contains("cancelled")
}

fn to_error_payload(e: &str) -> Value {
    json!({
        "code": "WORKER_ERROR",
        "message": e,
        "retryable": false,
    })
}

/// 从 JSON Value 中提取指定 key 对应的字符串数组
fn extract_string_vec(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default()
}

/// 读取 conversations.json 的 items
fn load_conversation_items(output_dir: &Path, account_id: &str) -> Vec<Value> {
    let conv_index = output_dir
        .join("accounts")
        .join(account_id)
        .join("conversations.json");
    std::fs::read_to_string(&conv_index)
        .ok()
        .and_then(|c| serde_json::from_str::<Value>(&c).ok())
        .and_then(|d| d.get("items")?.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|v| v.is_object())
        .collect()
}

/// 从 items 提取有效会话 ID（排除 lost）
fn load_conversation_ids(items: &[Value]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in items {
        let status = row
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("normal");
        if status == "lost" {
            continue;
        }
        if let Some(cid) = row.get("id").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            if seen.insert(cid.to_string()) {
                out.push(cid.to_string());
            }
        }
    }
    out
}

/// 收集 JSONL 中有失败标记的对话 ID
fn collect_failed_conversation_ids(
    output_dir: &Path,
    account_id: &str,
    items: &[Value],
) -> Vec<String> {
    let conv_dir = output_dir
        .join("accounts")
        .join(account_id)
        .join("conversations");
    let mut ids = std::collections::HashSet::new();

    let item_map: HashMap<String, &Value> = items
        .iter()
        .filter_map(|row| {
            let cid = row.get("id")?.as_str()?.trim().to_string();
            if cid.is_empty() {
                None
            } else {
                Some((cid, row))
            }
        })
        .collect();

    if conv_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&conv_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("\"downloadFailed\": true")
                        || content.contains("\"downloadFailed\":true")
                    {
                        let cid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                        if !cid.is_empty() {
                            if let Some(row) = item_map.get(cid) {
                                let status = row
                                    .get("status")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("normal");
                                if status != "lost" {
                                    ids.insert(cid.to_string());
                                }
                            } else {
                                ids.insert(cid.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    ids.into_iter().collect()
}

/// 收集 messageCount=0 的空对话
fn collect_empty_conversation_ids(items: &[Value]) -> Vec<String> {
    items
        .iter()
        .filter_map(|row| {
            let status = row.get("status").and_then(|v| v.as_str()).unwrap_or("normal");
            if status == "lost" {
                return None;
            }
            let msg_count = row.get("messageCount").and_then(|v| v.as_i64()).unwrap_or(-1);
            if msg_count != 0 {
                return None;
            }
            row.get("id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .collect()
}
