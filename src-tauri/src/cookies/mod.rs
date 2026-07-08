mod chrome_decrypt;
mod discover;
pub mod domain;
pub mod keychain_diag;
pub mod list_accounts;
#[cfg(target_os = "windows")]
mod webview_cookies;

pub use chrome_decrypt::decrypt_chrome_cookie_value;
pub use discover::{discover_chrome_cookie_files, CookieFileEntry};
pub use domain::{is_google_domain, normalize_cookie_domain, select_preferred_google_cookies};
pub use keychain_diag::{
    check_keychain_access, diagnose_browser_keychain, diagnose_keychain_for_browsers,
    run_full_diagnostics, CookieDiagnosticReport, KeychainDiagResult,
};
pub use list_accounts::{discover_email_authuser_mapping, AccountMapping};
#[cfg(target_os = "windows")]
pub use webview_cookies::read_webview_session_cookies;

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

/// A single cookie row from Chrome's SQLite database.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CookieRow {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
}

/// Read and decrypt all Google-domain cookies from a Chrome Cookies SQLite file.
pub fn read_chrome_cookies(cookie_db_path: &Path, browser_name: &str) -> Result<Vec<CookieRow>> {
    // Copy the db to a temp file to avoid locking issues with a running browser
    let tmp = tempfile_copy(cookie_db_path)?;
    let conn = Connection::open(&tmp)
        .with_context(|| format!("Failed to open SQLite: {}", cookie_db_path.display()))?;

    // Chrome stores cookies in the `cookies` table
    let mut stmt = conn.prepare(
        "SELECT host_key, name, value, encrypted_value, path FROM cookies \
         WHERE host_key LIKE '%google.com%'",
    )?;

    let key = chrome_decrypt::get_browser_key(browser_name)?;

    let rows = stmt.query_map([], |row| {
        let domain: String = row.get(0)?;
        let name: String = row.get(1)?;
        let plaintext_value: String = row.get(2)?;
        let encrypted_value: Vec<u8> = row.get(3)?;
        let path: String = row.get(4)?;
        Ok((domain, name, plaintext_value, encrypted_value, path))
    })?;

    let mut result = Vec::new();
    for row in rows {
        let (domain, name, plaintext_value, encrypted_value, path) = row?;

        let value = if !plaintext_value.is_empty() {
            plaintext_value
        } else if !encrypted_value.is_empty() {
            match chrome_decrypt::decrypt_chrome_cookie_value(&encrypted_value, &key) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("decrypt failed for cookie '{}' on {}: {}", name, domain, e);
                    continue;
                }
            }
        } else {
            String::new()
        };

        if !domain::is_google_domain(&domain) {
            continue;
        }

        result.push(CookieRow {
            name,
            value,
            domain,
            path,
        });
    }

    // Clean up temp file
    let _ = std::fs::remove_file(&tmp);
    Ok(result)
}

/// Copy a file to a temp location (avoids SQLite WAL locking issues).
pub(crate) fn tempfile_copy(src: &Path) -> Result<std::path::PathBuf> {
    let mut tmp = std::env::temp_dir();
    let stem = src
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    tmp.push(format!("gemini_cookie_verify_{}", stem));
    std::fs::copy(src, &tmp)
        .with_context(|| format!("Failed to copy {} to temp", src.display()))?;

    // Also copy WAL and SHM files if they exist
    let src_wal = src.with_extension("sqlite-wal");
    if src_wal.exists() {
        let wal_name = format!(
            "{}-wal",
            src.file_name().unwrap_or_default().to_string_lossy()
        );
        let src_wal2 = src.with_file_name(&wal_name);
        let dst_wal = tmp.with_file_name(format!(
            "{}-wal",
            tmp.file_name().unwrap_or_default().to_string_lossy()
        ));
        if src_wal2.exists() {
            let _ = std::fs::copy(&src_wal2, &dst_wal);
        }
    }
    // Chrome uses "Cookies-wal" naming
    let src_wal_chrome = src.with_file_name(format!(
        "{}-wal",
        src.file_name().unwrap_or_default().to_string_lossy()
    ));
    let dst_wal = tmp.with_file_name(format!(
        "{}-wal",
        tmp.file_name().unwrap_or_default().to_string_lossy()
    ));
    if src_wal_chrome.exists() {
        let _ = std::fs::copy(&src_wal_chrome, &dst_wal);
    }

    let src_shm = src.with_file_name(format!(
        "{}-shm",
        src.file_name().unwrap_or_default().to_string_lossy()
    ));
    let dst_shm = tmp.with_file_name(format!(
        "{}-shm",
        tmp.file_name().unwrap_or_default().to_string_lossy()
    ));
    if src_shm.exists() {
        let _ = std::fs::copy(&src_shm, &dst_shm);
    }

    Ok(tmp)
}

/// High-level: discover browsers, read & decrypt, select preferred Google cookies.
/// On failure, performs Keychain diagnostics (macOS) and outputs actionable hints.
pub fn get_cookies_from_local_browser() -> Result<HashMap<String, String>> {
    log::info!("尝试从本机浏览器读取 cookies...");

    let (entries, permission_issues) = discover::discover_chrome_cookie_files();

    if !permission_issues.is_empty() {
        log::warn!("检测到权限问题:");
        for issue in &permission_issues {
            log::warn!("  {}", issue);
        }
    }

    let key_cookies = ["__Secure-1PSID", "__Secure-1PSIDTS"];

    for entry in &entries {
        let label = format!("{}/{}", entry.browser_name, entry.profile_name);
        match read_chrome_cookies(Path::new(&entry.cookie_file), &entry.browser_name) {
            Ok(rows) => {
                let items: Vec<_> = rows
                    .iter()
                    .map(|r| domain::CookieItem {
                        name: r.name.clone(),
                        value: r.value.clone(),
                        domain: r.domain.clone(),
                    })
                    .collect();
                let selected = domain::select_preferred_google_cookies(&items);
                if selected.is_empty() {
                    log::info!("  - {}: 未读取到可用 cookie", label);
                    continue;
                }
                if key_cookies.iter().any(|k| selected.contains_key(*k)) {
                    log::info!("  - {}: 成功读取 {} 个 cookies", label, selected.len());
                    return Ok(selected);
                }
                log::warn!(
                    "  - {}: 已读取 {} 个 cookies，但缺少关键登录态",
                    label,
                    selected.len()
                );
            }
            Err(e) => {
                log::warn!("  - {}: 读取失败 ({})", label, e);
            }
        }
    }

    if entries.is_empty() {
        log::warn!("未发现已知 cookie 文件");
    }

    // ── Keychain 事后诊断（仅 macOS，仅在找到了 cookie 文件但读取失败时） ──
    if !entries.is_empty() {
        let browser_names: Vec<&str> = entries.iter().map(|e| e.browser_name.as_str()).collect();
        let kc_diags = keychain_diag::diagnose_keychain_for_browsers(&browser_names);
        for diag in &kc_diags {
            if !diag.accessible {
                log::warn!("{}", diag.detail);
                if !diag.suggestion.is_empty() {
                    for line in diag.suggestion.lines() {
                        log::warn!("  {}", line);
                    }
                }
            }
        }
    }

    Ok(HashMap::new())
}
