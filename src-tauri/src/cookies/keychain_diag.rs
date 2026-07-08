//! macOS Keychain 权限诊断：检测浏览器加密密钥可读性，输出可操作的中文提示。

/// Keychain 诊断结果
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeychainDiagResult {
    /// 浏览器名称
    pub browser: String,
    /// Keychain service 名称
    pub service: String,
    /// Keychain account 名称
    pub account: String,
    /// 是否可读
    pub accessible: bool,
    /// 错误详情（若不可读）
    pub detail: String,
    /// 用户可操作的建议
    pub suggestion: String,
}

/// 浏览器名 → (Keychain service, Keychain account)
const KEYCHAIN_MAP: &[(&str, &str, &str)] = &[
    ("Chrome", "Chrome Safe Storage", "Chrome"),
    ("Chromium", "Chromium Safe Storage", "Chromium"),
    ("Brave", "Brave Safe Storage", "Brave"),
    ("Edge", "Microsoft Edge Safe Storage", "Microsoft Edge"),
];

/// 检测单个 Keychain 条目是否可读。返回 (ok, detail)。
#[cfg(target_os = "macos")]
pub fn check_keychain_access(service: &str, account: &str) -> (bool, String) {
    let security_bin = "/usr/bin/security";
    if !std::path::Path::new(security_bin).exists() {
        return (false, "未找到 /usr/bin/security 命令".to_string());
    }

    let result = std::process::Command::new(security_bin)
        .args([
            "-q",
            "find-generic-password",
            "-w",
            "-a",
            account,
            "-s",
            service,
        ])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                (true, String::new())
            } else {
                let exit_code = output.status.code().unwrap_or(-1);
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                let detail = format!(
                    "Keychain 读取 \"{}\" 失败 (exit {}): {}",
                    service, exit_code, stderr
                );
                (false, detail)
            }
        }
        Err(e) => (false, format!("执行 security 命令异常: {}", e)),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn check_keychain_access(_service: &str, _account: &str) -> (bool, String) {
    (true, String::new())
}

/// 对指定浏览器执行 Keychain 诊断
pub fn diagnose_browser_keychain(browser_name: &str) -> Option<KeychainDiagResult> {
    let (_, service, account) = KEYCHAIN_MAP
        .iter()
        .find(|(name, _, _)| *name == browser_name)?;

    let (accessible, detail) = check_keychain_access(service, account);

    let suggestion = if accessible {
        String::new()
    } else if detail.contains("SecKeychainSearchCopyNext")
        || detail.contains("errSecItemNotFound")
        || detail.contains("could not be found")
    {
        format!(
            "浏览器 {} 的加密密钥在 Keychain 中不存在。可能原因：\n\
             1. 该浏览器从未在本机运行过\n\
             2. Keychain 已被重置\n\
             请确认 {} 已安装并至少启动过一次",
            browser_name, browser_name
        )
    } else if detail.contains("errSecAuthFailed")
        || detail.contains("User interaction is not allowed")
        || detail.contains("authorization")
    {
        format!(
            "Keychain 拒绝了对 {} 密钥的访问。请尝试：\n\
             1. 打开 钥匙串访问 (Keychain Access)\n\
             2. 找到 \"{}\" 条目\n\
             3. 右键 → 显示简介 → 访问控制\n\
             4. 添加本应用到允许列表，或选择\"允许所有应用程序访问此项目\"",
            browser_name, service
        )
    } else {
        format!(
            "无法读取 {} 的 Keychain 密钥。请检查：\n\
             1. 系统设置 → 隐私与安全性 → 完全磁盘访问权限\n\
             2. 钥匙串访问 → \"{}\" 条目的访问控制设置",
            browser_name, service
        )
    };

    Some(KeychainDiagResult {
        browser: browser_name.to_string(),
        service: service.to_string(),
        account: account.to_string(),
        accessible,
        detail,
        suggestion,
    })
}

/// 对所有已发现 cookie 文件的浏览器进行 Keychain 诊断（仅 macOS）。
/// 返回每个浏览器的诊断结果。
pub fn diagnose_keychain_for_browsers(browser_names: &[&str]) -> Vec<KeychainDiagResult> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = browser_names;
        return Vec::new();
    }

    #[cfg(target_os = "macos")]
    {
        let mut checked: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut results = Vec::new();
        for &name in browser_names {
            if checked.contains(name) {
                continue;
            }
            checked.insert(name.to_string());
            if let Some(diag) = diagnose_browser_keychain(name) {
                results.push(diag);
            }
        }
        results
    }
}

/// 综合诊断报告：cookie 文件 + Keychain + 权限
#[derive(Debug, Clone, serde::Serialize)]
pub struct CookieDiagnosticReport {
    pub cookie_files_found: usize,
    pub permission_issues: Vec<String>,
    pub keychain_diagnostics: Vec<KeychainDiagResult>,
    pub cookies_obtained: bool,
    pub key_cookies_present: bool,
    pub summary: String,
}

/// 执行完整的 cookie 可用性诊断。
/// 不会实际读取 cookie 值，仅检查各环节是否正常。
pub fn run_full_diagnostics() -> CookieDiagnosticReport {
    let (entries, permission_issues) = super::discover::discover_chrome_cookie_files();

    let browser_names: Vec<&str> = entries.iter().map(|e| e.browser_name.as_str()).collect();
    let keychain_diagnostics = diagnose_keychain_for_browsers(&browser_names);

    let keychain_ok = keychain_diagnostics.iter().any(|d| d.accessible);
    let has_files = !entries.is_empty();

    let summary = if !has_files && !permission_issues.is_empty() {
        "未找到浏览器 cookie 文件，存在权限问题。请在 系统设置 → 隐私与安全性 → 完全磁盘访问 中授权本应用。".to_string()
    } else if !has_files {
        "未找到已知浏览器的 cookie 文件。请确认已安装 Chrome/Chromium/Brave/Edge 并登录过 Google。"
            .to_string()
    } else if !keychain_ok && !keychain_diagnostics.is_empty() {
        let browsers: Vec<String> = keychain_diagnostics
            .iter()
            .map(|d| d.browser.clone())
            .collect();
        format!(
            "找到 cookie 文件但无法读取 Keychain 密钥（{}）。详见 keychain_diagnostics。",
            browsers.join(", ")
        )
    } else if has_files && keychain_ok {
        "环境检测正常：cookie 文件可访问，Keychain 密钥可读。".to_string()
    } else {
        "部分环境检测通过，请查看详细诊断。".to_string()
    };

    CookieDiagnosticReport {
        cookie_files_found: entries.len(),
        permission_issues,
        keychain_diagnostics,
        cookies_obtained: false, // Will be set by caller after actual read
        key_cookies_present: false,
        summary,
    }
}
