//! 浏览器信息检测：从本地 Chrome 安装读取版本号、语言偏好，
//! 生成接近真实 Chrome 的 HTTP headers。

use std::sync::OnceLock;

/// 检测失败时的 fallback Chrome 版本
const FALLBACK_CHROME_VERSION: &str = "131.0.0.0";
const FALLBACK_CHROME_MAJOR: &str = "131";

// ============================================================================
// Chrome 版本检测
// ============================================================================

/// 从本地 Chrome 安装读取完整版本号（如 "146.0.0.0"）。
pub fn detect_chrome_version() -> Option<String> {
    detect_chrome_version_inner()
}

#[cfg(target_os = "macos")]
fn detect_chrome_version_inner() -> Option<String> {
    // 读取 Info.plist 并用字符串匹配提取 CFBundleShortVersionString
    let plist_path = "/Applications/Google Chrome.app/Contents/Info.plist";
    let content = std::fs::read_to_string(plist_path).ok()?;
    // 格式:
    //   <key>CFBundleShortVersionString</key>
    //   <string>146.0.0.0</string>
    let key = "CFBundleShortVersionString";
    let key_pos = content.find(key)?;
    let after_key = &content[key_pos + key.len()..];
    let string_start = after_key.find("<string>")?;
    let value_start = string_start + "<string>".len();
    let remaining = &after_key[value_start..];
    let string_end = remaining.find("</string>")?;
    let version = remaining[..string_end].trim().to_string();
    if version.is_empty() || !version.chars().next()?.is_ascii_digit() {
        return None;
    }
    Some(version)
}

#[cfg(target_os = "windows")]
fn detect_chrome_version_inner() -> Option<String> {
    // 尝试读注册表
    use std::process::Command;
    let output = Command::new("reg")
        .args([
            "query",
            r"HKLM\SOFTWARE\Google\Chrome\BLBeacon",
            "/v",
            "version",
        ])
        .output()
        .ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout);
        // 格式: "    version    REG_SZ    146.0.0.0"
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with("version") || line.contains("version") {
                if let Some(ver) = line.split_whitespace().last() {
                    if ver.contains('.') && ver.chars().next().map_or(false, |c| c.is_ascii_digit())
                    {
                        return Some(ver.to_string());
                    }
                }
            }
        }
    }
    // Fallback: 检查常见安装路径下的版本目录
    let chrome_dir = r"C:\Program Files\Google\Chrome\Application";
    if let Ok(entries) = std::fs::read_dir(chrome_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.contains('.')
                && name.chars().next().map_or(false, |c| c.is_ascii_digit())
                && entry.file_type().map_or(false, |ft| ft.is_dir())
            {
                return Some(name.to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_chrome_version_inner() -> Option<String> {
    use std::process::Command;
    let output = Command::new("google-chrome")
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // "Google Chrome 146.0.0.0"
    for word in text.split_whitespace().rev() {
        if word.contains('.') && word.chars().next().map_or(false, |c| c.is_ascii_digit()) {
            return Some(word.to_string());
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn detect_chrome_version_inner() -> Option<String> {
    None
}

/// 返回 Chrome 主版本号（如 "146"），检测失败时使用 fallback。
pub fn chrome_major_version() -> &'static str {
    static MAJOR: OnceLock<String> = OnceLock::new();
    MAJOR.get_or_init(|| {
        detect_chrome_version()
            .and_then(|v| v.split('.').next().map(|s| s.to_string()))
            .unwrap_or_else(|| FALLBACK_CHROME_MAJOR.to_string())
    })
}

/// 返回完整 Chrome 版本（如 "146.0.0.0"），检测失败时使用 fallback。
fn chrome_full_version() -> &'static str {
    static VER: OnceLock<String> = OnceLock::new();
    VER.get_or_init(|| {
        detect_chrome_version().unwrap_or_else(|| FALLBACK_CHROME_VERSION.to_string())
    })
}

// ============================================================================
// User-Agent
// ============================================================================

/// 生成 User-Agent 字符串，使用本地 Chrome 版本。
pub fn build_user_agent() -> &'static str {
    static UA: OnceLock<String> = OnceLock::new();
    UA.get_or_init(|| {
        let platform = platform_ua_string();
        let version = chrome_full_version();
        format!(
            "Mozilla/5.0 ({}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{} Safari/537.36",
            platform, version
        )
    })
}

/// UA 中的平台标识
fn platform_ua_string() -> &'static str {
    if cfg!(target_os = "macos") {
        "Macintosh; Intel Mac OS X 10_15_7"
    } else if cfg!(target_os = "windows") {
        "Windows NT 10.0; Win64; x64"
    } else {
        "X11; Linux x86_64"
    }
}

// ============================================================================
// sec-ch-ua
// ============================================================================

/// 生成 sec-ch-ua header 值。
pub fn build_sec_ch_ua() -> &'static str {
    static CH_UA: OnceLock<String> = OnceLock::new();
    CH_UA.get_or_init(|| {
        let major = chrome_major_version();
        format!(
            "\"Chromium\";v=\"{}\", \"Not-A.Brand\";v=\"24\", \"Google Chrome\";v=\"{}\"",
            major, major
        )
    })
}

// ============================================================================
// sec-ch-ua-platform
// ============================================================================

/// 返回 sec-ch-ua-platform 值。
pub fn platform_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "\"macOS\""
    } else if cfg!(target_os = "windows") {
        "\"Windows\""
    } else {
        "\"Linux\""
    }
}

// ============================================================================
// accept-language
// ============================================================================

/// 从本地 Chrome 偏好设置读取 accept-language，失败则用 fallback。
pub fn detect_accept_language() -> &'static str {
    static LANG: OnceLock<String> = OnceLock::new();
    LANG.get_or_init(|| {
        detect_accept_language_inner().unwrap_or_else(|| "en-US,en;q=0.9".to_string())
    })
}

fn detect_accept_language_inner() -> Option<String> {
    let prefs_path = chrome_preferences_path()?;
    let content = std::fs::read_to_string(prefs_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let lang = parsed
        .get("intl")
        .and_then(|v| v.get("accept_languages"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(lang.to_string())
}

#[cfg(target_os = "macos")]
fn chrome_preferences_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join("Library/Application Support/Google/Chrome/Default/Preferences");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn chrome_preferences_path() -> Option<std::path::PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let path = std::path::PathBuf::from(local_app_data)
        .join(r"Google\Chrome\User Data\Default\Preferences");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn chrome_preferences_path() -> Option<std::path::PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join(".config/google-chrome/Default/Preferences");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn chrome_preferences_path() -> Option<std::path::PathBuf> {
    None
}

// ============================================================================
// 便利常量：导航请求（GET 页面）专用 headers
// ============================================================================

pub const NAVIGATE_ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8,application/signed-exchange;v=b3;q=0.7";
pub const NAVIGATE_SEC_FETCH_DEST: &str = "document";
pub const NAVIGATE_SEC_FETCH_MODE: &str = "navigate";
pub const NAVIGATE_SEC_FETCH_SITE: &str = "none";
pub const NAVIGATE_SEC_FETCH_USER: &str = "?1";
pub const NAVIGATE_UPGRADE_INSECURE_REQUESTS: &str = "1";
pub const NAVIGATE_X_BROWSER_CHANNEL: &str = "stable";

/// 生成 x-browser-year header 值（当前年份）。
pub fn browser_year() -> &'static str {
    static YEAR: OnceLock<String> = OnceLock::new();
    YEAR.get_or_init(|| chrono::Utc::now().format("%Y").to_string())
}

/// 生成 x-browser-copyright header 值。
pub fn browser_copyright() -> &'static str {
    static COPYRIGHT: OnceLock<String> = OnceLock::new();
    COPYRIGHT.get_or_init(|| {
        format!(
            "Copyright {} Google LLC. All Rights reserved.",
            browser_year()
        )
    })
}

// ============================================================================
// 便利常量：API 请求（POST batchexecute）专用 headers
// ============================================================================

pub const API_ACCEPT: &str = "*/*";
pub const API_SEC_FETCH_DEST: &str = "empty";
pub const API_SEC_FETCH_MODE: &str = "cors";
pub const API_SEC_FETCH_SITE: &str = "same-origin";
pub const API_ORIGIN: &str = "https://gemini.google.com";
pub const API_REFERER: &str = "https://gemini.google.com/";
