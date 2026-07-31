//! 应用设置持久化模块
//!
//! 所有用户配置都通过 settings.json 持久化到 AppDataDir，
//! 前端通过 Tauri 命令读写，**不硬编码任何配置值**。

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::str_err::ToStringErr;

// ============================================================================
// 数据结构
// ============================================================================

/// 同步间隔选项
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SyncInterval {
    Minutes30,
    Hour1,
    Hours3,
    #[default]
    Hours6,
    Hours12,
    Hours24,
    Custom(u32), // 自定义分钟数
}

impl SyncInterval {
    pub fn to_minutes(&self) -> u32 {
        match self {
            SyncInterval::Minutes30 => 30,
            SyncInterval::Hour1 => 60,
            SyncInterval::Hours3 => 180,
            SyncInterval::Hours6 => 360,
            SyncInterval::Hours12 => 720,
            SyncInterval::Hours24 => 1440,
            SyncInterval::Custom(m) => *m,
        }
    }
}

/// 导出格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    #[default]
    Markdown,
    Pdf,
    Json,
    Jsonl,
    Kelivo,
    KelivoSplit,
}

/// 自动锁定策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum AutoLockPolicy {
    Immediately,
    Minutes1,
    Minutes5,
    Minutes15,
    Minutes30,
    #[default]
    Never,
}

impl AutoLockPolicy {
    pub fn to_seconds(&self) -> Option<u64> {
        match self {
            AutoLockPolicy::Immediately => Some(0),
            AutoLockPolicy::Minutes1 => Some(60),
            AutoLockPolicy::Minutes5 => Some(300),
            AutoLockPolicy::Minutes15 => Some(900),
            AutoLockPolicy::Minutes30 => Some(1800),
            AutoLockPolicy::Never => None,
        }
    }
}

/// 主题
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Theme {
    #[default]
    Auto,
    Light,
    Dark,
}

/// 完整的应用设置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    // ── 同步设置 ──
    /// 自动同步间隔
    #[serde(default)]
    pub sync_interval: SyncInterval,
    /// 启动时立即同步
    #[serde(default)]
    pub sync_on_startup: bool,
    /// 同步完成后显示通知
    #[serde(default = "default_true")]
    pub show_sync_notification: bool,
    /// 要同步的账号 ID 列表（空 = 全部账号）
    #[serde(default)]
    pub sync_account_ids: Vec<String>,
    /// 启用自动同步
    #[serde(default = "default_true")]
    pub auto_sync_enabled: bool,

    // ── 存储设置 ──
    /// 自定义数据存储路径（空 = 默认 AppDataDir）
    #[serde(default)]
    pub custom_data_directory: String,
    /// 默认导出格式
    #[serde(default)]
    pub default_export_format: ExportFormat,

    // ── 运行设置 ──
    /// 后台运行（关闭窗口时最小化到托盘）
    #[serde(default = "default_true")]
    pub run_in_background: bool,
    /// 隐藏 Dock 图标 (macOS)
    #[serde(default)]
    pub hide_dock_icon: bool,
    /// 开机自启
    #[serde(default)]
    pub start_on_login: bool,

    // ── 安全设置 ──
    /// 版本化密码哈希（PBKDF2-SHA256；旧版 SHA-256 在验证后自动迁移）
    #[serde(default)]
    pub password_hash: String,
    /// 自动锁定策略
    #[serde(default)]
    pub auto_lock_policy: AutoLockPolicy,

    // ── 外观设置 ──
    /// 主题
    #[serde(default)]
    pub theme: Theme,
    /// 语言
    #[serde(default = "default_language")]
    pub language: String,
    /// 侧边栏宽度
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u32,
}

fn default_true() -> bool {
    true
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_sidebar_width() -> u32 {
    280
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            sync_interval: SyncInterval::default(),
            sync_on_startup: false,
            show_sync_notification: true,
            sync_account_ids: Vec::new(),
            auto_sync_enabled: true,
            custom_data_directory: String::new(),
            default_export_format: ExportFormat::default(),
            run_in_background: true,
            hide_dock_icon: false,
            start_on_login: false,
            password_hash: String::new(),
            auto_lock_policy: AutoLockPolicy::default(),
            theme: Theme::default(),
            language: default_language(),
            sidebar_width: default_sidebar_width(),
        }
    }
}

// ============================================================================
// 持久化
// ============================================================================

const SETTINGS_FILE: &str = "settings.json";

impl AppSettings {
    /// 从 AppDataDir 加载设置，文件不存在则返回默认值
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(SETTINGS_FILE);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// 保存设置到 AppDataDir
    pub fn save(&self, data_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(data_dir).str_err()?;
        let path = data_dir.join(SETTINGS_FILE);
        let content = serde_json::to_string_pretty(self).str_err()?;
        std::fs::write(&path, content).str_err()?;
        Ok(())
    }

    /// 获取实际的数据目录路径
    pub fn effective_data_dir(&self, default_dir: &Path) -> std::path::PathBuf {
        if self.custom_data_directory.is_empty() {
            default_dir.to_path_buf()
        } else {
            std::path::PathBuf::from(&self.custom_data_directory)
        }
    }
}

// ============================================================================
// 密码工具
// ============================================================================

const PASSWORD_SCHEME: &str = "pbkdf2-sha256";
const PASSWORD_ITERATIONS: u32 = 210_000;
const PASSWORD_SALT_LEN: usize = 16;
const PASSWORD_HASH_LEN: usize = 32;

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right.iter())
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn legacy_sha256(password: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    Sha256::digest(password.as_bytes()).to_vec()
}

fn is_legacy_password_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 使用随机盐和 PBKDF2-SHA256 生成版本化密码哈希。
pub fn hash_password(password: &str) -> String {
    use pbkdf2::pbkdf2_hmac;
    use rand::RngCore;
    use sha2::Sha256;

    let mut salt = [0u8; PASSWORD_SALT_LEN];
    rand::rng().fill_bytes(&mut salt);
    let mut derived = [0u8; PASSWORD_HASH_LEN];
    pbkdf2_hmac::<Sha256>(
        password.as_bytes(),
        &salt,
        PASSWORD_ITERATIONS,
        &mut derived,
    );
    format!(
        "{PASSWORD_SCHEME}${PASSWORD_ITERATIONS}${}${}",
        encode_hex(&salt),
        encode_hex(&derived),
    )
}

/// 验证密码
pub fn verify_password(password: &str, hash: &str) -> bool {
    if hash.is_empty() {
        return true; // 未设置密码
    }
    if is_legacy_password_hash(hash) {
        let Some(expected) = decode_hex(hash) else {
            return false;
        };
        return constant_time_equal(&legacy_sha256(password), &expected);
    }

    use pbkdf2::pbkdf2_hmac;
    use sha2::Sha256;
    let mut parts = hash.split('$');
    if parts.next() != Some(PASSWORD_SCHEME) {
        return false;
    }
    let Some(iterations) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
        return false;
    };
    let (Some(salt), Some(expected), None) = (
        parts.next().and_then(decode_hex),
        parts.next().and_then(decode_hex),
        parts.next(),
    ) else {
        return false;
    };
    if expected.len() != PASSWORD_HASH_LEN || !(10_000..=1_000_000).contains(&iterations) {
        return false;
    }
    let mut derived = vec![0u8; expected.len()];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), &salt, iterations, &mut derived);
    constant_time_equal(&derived, &expected)
}

// ============================================================================
// Tauri 命令
// ============================================================================

#[tauri::command]
pub fn load_settings(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let settings = AppSettings::load(&data_dir);
    let mut value = serde_json::to_value(&settings).str_err()?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "passwordHash".to_string(),
            serde_json::Value::String(
                if settings.password_hash.is_empty() {
                    ""
                } else {
                    "configured"
                }
                .to_string(),
            ),
        );
    }
    Ok(value)
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: serde_json::Value) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let mut parsed: AppSettings = serde_json::from_value(settings).str_err()?;
    // 密码只能通过 set_password 修改；永不信任或回写前端传来的占位值。
    parsed.password_hash = AppSettings::load(&data_dir).password_hash;
    parsed.save(&data_dir)?;

    // 通知调度器重新加载配置
    // （调度器会在下次 tick 时读取最新设置）
    log::info!("设置已保存");
    Ok(())
}

#[tauri::command]
pub fn set_password(
    app: tauri::AppHandle,
    current_password: String,
    new_password: String,
) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let mut settings = AppSettings::load(&data_dir);

    // 验证当前密码
    if !settings.password_hash.is_empty()
        && !verify_password(&current_password, &settings.password_hash)
    {
        return Err("CURRENT_PASSWORD_INVALID".to_string());
    }

    // 设置新密码（空字符串 = 清除密码）
    if new_password.is_empty() {
        settings.password_hash = String::new();
    } else {
        settings.password_hash = hash_password(&new_password);
    }

    settings.save(&data_dir)?;
    log::info!("密码已更新");
    Ok(())
}

#[tauri::command]
pub fn verify_unlock(app: tauri::AppHandle, password: String) -> Result<bool, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let mut settings = AppSettings::load(&data_dir);
    let verified = verify_password(&password, &settings.password_hash);
    if verified && is_legacy_password_hash(&settings.password_hash) {
        settings.password_hash = hash_password(&password);
        settings.save(&data_dir)?;
        log::info!("旧版密码哈希已迁移到 PBKDF2-SHA256");
    }
    Ok(verified)
}

#[tauri::command]
pub fn has_password(app: tauri::AppHandle) -> Result<bool, String> {
    let data_dir = app.path().app_data_dir().str_err()?;
    let settings = AppSettings::load(&data_dir);
    Ok(!settings.password_hash.is_empty())
}

use tauri::Manager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_is_salted_and_verifiable() {
        let first = hash_password("correct horse battery staple");
        let second = hash_password("correct horse battery staple");
        assert_ne!(first, second);
        assert!(first.starts_with("pbkdf2-sha256$"));
        assert!(verify_password("correct horse battery staple", &first));
        assert!(!verify_password("wrong", &first));
    }

    #[test]
    fn legacy_sha256_passwords_remain_verifiable() {
        let legacy = encode_hex(&legacy_sha256("legacy password"));
        assert!(verify_password("legacy password", &legacy));
        assert!(!verify_password("wrong", &legacy));
    }
}
