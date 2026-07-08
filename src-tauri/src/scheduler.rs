//! 定时同步调度器
//!
//! 在后台按照用户配置的间隔自动触发同步任务。
//! 调度器在 Tauri setup 阶段启动，独立于 UI 线程运行。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::settings::AppSettings;
use crate::str_err::ToStringErr;

// ============================================================================
// 调度器
// ============================================================================

/// 后台同步调度器
pub struct SyncScheduler {
    /// 是否正在运行同步
    is_syncing: Arc<AtomicBool>,
    /// 请求停止
    stop_requested: Arc<AtomicBool>,
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            is_syncing: Arc::new(AtomicBool::new(false)),
            stop_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 启动后台调度循环
    pub fn start(&self, app_handle: AppHandle) {
        let is_syncing = self.is_syncing.clone();
        let stop_requested = self.stop_requested.clone();

        tauri::async_runtime::spawn(async move {
            log::info!("[Scheduler] 后台同步调度器已启动");

            // 首次启动时检查是否需要立即同步
            {
                let data_dir = match app_handle.path().app_data_dir() {
                    Ok(d) => d,
                    Err(e) => {
                        log::error!("[Scheduler] 无法获取数据目录: {}", e);
                        return;
                    }
                };
                let settings = AppSettings::load(&data_dir);
                if settings.sync_on_startup && settings.auto_sync_enabled {
                    log::info!("[Scheduler] 启动时同步已启用，等待 10 秒后执行...");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    if !stop_requested.load(Ordering::SeqCst) {
                        trigger_auto_sync(&app_handle, &settings, &is_syncing).await;
                    }
                }
            }

            // 主循环：每分钟检查是否到了同步时间
            let mut elapsed_minutes: u32 = 0;

            loop {
                if stop_requested.load(Ordering::SeqCst) {
                    log::info!("[Scheduler] 收到停止信号，退出调度循环");
                    break;
                }

                // 每分钟 tick
                tokio::time::sleep(Duration::from_secs(60)).await;
                elapsed_minutes += 1;

                // 重新读取最新设置（用户可能随时修改）
                let data_dir = match app_handle.path().app_data_dir() {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let settings = AppSettings::load(&data_dir);

                if !settings.auto_sync_enabled {
                    elapsed_minutes = 0;
                    continue;
                }

                let interval_minutes = settings.sync_interval.to_minutes();
                if elapsed_minutes >= interval_minutes {
                    elapsed_minutes = 0;
                    trigger_auto_sync(&app_handle, &settings, &is_syncing).await;
                }
            }
        });
    }

    /// 请求停止调度器
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::SeqCst);
    }

    /// 是否正在同步
    pub fn is_syncing(&self) -> bool {
        self.is_syncing.load(Ordering::SeqCst)
    }
}

/// 触发自动同步所有配置的账号
async fn trigger_auto_sync(
    app_handle: &AppHandle,
    settings: &AppSettings,
    is_syncing: &Arc<AtomicBool>,
) {
    if is_syncing.load(Ordering::SeqCst) {
        log::info!("[Scheduler] 同步正在进行中，跳过本次");
        return;
    }

    is_syncing.store(true, Ordering::SeqCst);

    log::info!("[Scheduler] 开始自动同步...");

    // 发送事件通知前端
    let _ = app_handle.emit(
        "auto-sync-started",
        serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }),
    );

    // 获取要同步的账号列表
    let data_dir = match app_handle.path().app_data_dir() {
        Ok(d) => d,
        Err(e) => {
            log::error!("[Scheduler] 无法获取数据目录: {}", e);
            is_syncing.store(false, Ordering::SeqCst);
            return;
        }
    };

    let account_ids = get_sync_accounts(&data_dir, settings);

    if account_ids.is_empty() {
        log::info!("[Scheduler] 没有找到需要同步的账号");
        is_syncing.store(false, Ordering::SeqCst);
        return;
    }

    // 为每个账号排入同步任务
    // 使用现有的 enqueue_job 机制，与手动同步走同一逻辑
    for account_id in &account_ids {
        log::info!("[Scheduler] 排入同步任务: {}", account_id);

        // 通过事件通知 worker_host 执行同步
        let _ = app_handle.emit(
            "scheduler-enqueue-job",
            serde_json::json!({
                "type": "sync_incremental",
                "accountId": account_id,
            }),
        );
    }

    // 注意：实际同步完成由 worker_host 处理和通知
    // 这里只是排入任务队列
    is_syncing.store(false, Ordering::SeqCst);

    log::info!("[Scheduler] 已排入 {} 个账号的同步任务", account_ids.len());
}

/// 获取需要同步的账号 ID 列表
fn get_sync_accounts(data_dir: &std::path::Path, settings: &AppSettings) -> Vec<String> {
    // 如果用户指定了账号列表，直接使用
    if !settings.sync_account_ids.is_empty() {
        return settings.sync_account_ids.clone();
    }

    // 否则，从 accounts.json 读取所有账号
    let accounts_file = data_dir.join("accounts.json");
    if !accounts_file.exists() {
        return Vec::new();
    }

    match std::fs::read_to_string(&accounts_file) {
        Ok(content) => {
            if let Ok(registry) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(accounts) = registry.get("accounts").and_then(|v| v.as_array()) {
                    return accounts
                        .iter()
                        .filter_map(|a| a.get("id").and_then(|v| v.as_str()))
                        .map(|s| s.to_string())
                        .collect();
                }
            }
            Vec::new()
        }
        Err(_) => Vec::new(),
    }
}

// ============================================================================
// Tauri 命令
// ============================================================================

#[tauri::command]
pub fn get_scheduler_status(app: AppHandle) -> Result<serde_json::Value, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let settings = AppSettings::load(&data_dir);

    Ok(serde_json::json!({
        "autoSyncEnabled": settings.auto_sync_enabled,
        "syncIntervalMinutes": settings.sync_interval.to_minutes(),
        "syncAccountIds": settings.sync_account_ids,
    }))
}

/// 手动触发全部账号同步（从托盘菜单或设置界面调用）
#[tauri::command]
pub fn trigger_manual_sync_all(app: AppHandle) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let settings = AppSettings::load(&data_dir);
    let account_ids = get_sync_accounts(&data_dir, &settings);

    if account_ids.is_empty() {
        return Err("没有找到可同步的账号".to_string());
    }

    for account_id in &account_ids {
        let _ = app.emit(
            "scheduler-enqueue-job",
            serde_json::json!({
                "type": "sync_incremental",
                "accountId": account_id,
            }),
        );
    }

    log::info!("手动触发同步 {} 个账号", account_ids.len());
    Ok(())
}
