//! Gemini API 客户端：HTTP 封装、batchexecute RPC、业务 API、媒体下载。
//!
//! 对应 Python gemini_export.py 中的 GeminiExporter 类。
//! Phase 4 只实现"能发请求、能拿数据、能下媒体"的 API 能力层；
//! 编排逻辑（断点续传状态机、进度推送、cancel check）留给 Phase 5。

pub mod api;
pub mod batchexecute;
pub mod http_client;
pub mod media_download;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::Arc;

/// 媒体下载专用 Cookie 名称列表
pub const GOOGLE_MEDIA_COOKIE_NAMES: &[&str] = &[
    "AEC",
    "__Secure-BUCKET",
    "SID",
    "__Secure-1PSID",
    "__Secure-3PSID",
    "HSID",
    "SSID",
    "APISID",
    "SAPISID",
    "__Secure-1PAPISID",
    "__Secure-3PAPISID",
    "NID",
    "__Secure-1PSIDTS",
    "__Secure-3PSIDTS",
    "GOOGLE_ABUSE_EXEMPTION",
    "SIDCC",
    "__Secure-1PSIDCC",
    "__Secure-3PSIDCC",
];

/// Gemini API 客户端，对应 Python GeminiExporter class。
///
/// 持有 HTTP 客户端、认证参数、请求状态。整个 export 层是 async 的。
pub struct GeminiExporter {
    /// 从浏览器读取的 Google cookies (name → value)
    pub cookies: HashMap<String, String>,
    /// 用户指定的账号：可以是 authuser 索引 ("0"/"1") 或邮箱
    pub user_spec: Option<String>,
    /// 外部指定的 account_id（覆盖自动推导）
    pub account_id_override: Option<String>,
    /// 外部指定的 email（覆盖自动推导）
    pub account_email_override: Option<String>,
    /// 解析后的 authuser 参数
    pub authuser: Option<String>,
    /// reqwest 异步 HTTP 客户端
    pub client: reqwest::Client,
    /// CSRF token (SNlM0e)
    pub at: Option<String>,
    /// 服务器版本 (cfb2h)
    pub bl: Option<String>,
    /// Session ID (FdrFJe)
    pub fsid: Option<String>,
    /// 请求 ID 计数器（每次 +100000）
    pub reqid: AtomicU32,
    /// 是否已发出过第一个请求（用于决定是否加延迟）
    pub request_started: AtomicBool,
    /// 取消信号
    pub cancelled: Arc<AtomicBool>,
}

impl GeminiExporter {
    /// 创建新的 GeminiExporter 实例。
    pub fn new(
        cookies: HashMap<String, String>,
        user_spec: Option<String>,
        account_id_override: Option<String>,
        account_email_override: Option<String>,
    ) -> Self {
        let client = http_client::build_http_client(&cookies);
        Self {
            cookies,
            user_spec: user_spec
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            account_id_override: account_id_override
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty()),
            account_email_override: account_email_override
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty()),
            authuser: None,
            client,
            at: None,
            bl: None,
            fsid: None,
            reqid: AtomicU32::new(100000),
            request_started: AtomicBool::new(false),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 获取 authuser 查询参数
    pub fn authuser_params(&self) -> Vec<(&str, String)> {
        match &self.authuser {
            Some(au) => vec![("authuser", au.clone())],
            None => vec![],
        }
    }

    /// 构建媒体下载专用 Cookie header
    pub fn build_media_cookie_header(&self) -> String {
        GOOGLE_MEDIA_COOKIE_NAMES
            .iter()
            .filter_map(|&name| {
                self.cookies
                    .get(name)
                    .map(|val| format!("{}={}", name, val))
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}
