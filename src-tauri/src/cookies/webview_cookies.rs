use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::Path;

use super::chrome_decrypt;
use super::domain;

/// Subpath from the WebView2 data_directory to the cookie SQLite file.
const COOKIE_SUBPATH: &str = "EBWebView/Default/Network/Cookies";

/// Subpath from the WebView2 data_directory to the Local State file (holds the AES key).
const LOCAL_STATE_SUBPATH: &str = "EBWebView/Local State";

/// Read and decrypt Google-domain cookies from the WebView2 persistent data directory.
///
/// WebView2 on Windows uses the same Chromium cookie format as Chrome:
/// - "v10" prefix + AES-256-GCM with Local State key  (NOT AES-128-CBC like macOS v10)
/// - No prefix → raw DPAPI blob
///
/// Returns the same `HashMap<String, String>` format as `get_cookies_from_local_browser()`.
pub fn read_webview_session_cookies(webview_data_dir: &Path) -> Result<HashMap<String, String>> {
    let cookie_path = webview_data_dir.join(COOKIE_SUBPATH);
    let local_state_path = webview_data_dir.join(LOCAL_STATE_SUBPATH);

    if !cookie_path.exists() {
        anyhow::bail!(
            "WebView2 cookie file not found: {} (user may not have logged in yet)",
            cookie_path.display()
        );
    }

    let key = chrome_decrypt::get_local_state_key(&local_state_path)?;

    // Try copy-then-read first; if the file is locked by a running WebView2
    // instance, fall back to opening the DB directly in immutable mode.
    match super::tempfile_copy(&cookie_path) {
        Ok(tmp) => {
            let result = read_cookies_from_db(&tmp, &key);
            let _ = std::fs::remove_file(&tmp);
            result
        }
        Err(_) => {
            log::info!("WebView2 cookie file locked, opening in immutable mode");
            read_cookies_from_db_immutable(&cookie_path, &key)
        }
    }
}

/// Open SQLite in immutable mode — bypasses file locks held by a running WebView2.
fn read_cookies_from_db_immutable(db_path: &Path, key: &[u8]) -> Result<HashMap<String, String>> {
    // SQLite URI on Windows: forward slashes, encode spaces, file:/ prefix
    let path_str = db_path
        .to_string_lossy()
        .replace('\\', "/")
        .replace(' ', "%20");
    // Try multiple URI formats — Windows SQLite URI handling can be finicky
    let uris = [
        format!("file:///{}?immutable=1", path_str),
        format!("file:/{}?immutable=1", path_str),
        format!("file:{}?immutable=1", path_str),
    ];
    let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI;
    let mut last_err = None;
    for uri in &uris {
        log::info!("尝试 immutable URI: {}", uri);
        match Connection::open_with_flags(uri, flags) {
            Ok(conn) => return read_cookies_from_conn(&conn, key),
            Err(e) => {
                log::warn!("immutable URI 失败 [{}]: {}", uri, e);
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap())
        .with_context(|| format!("所有 immutable URI 格式均失败: {}", db_path.display()))
}

fn read_cookies_from_db(db_path: &Path, key: &[u8]) -> Result<HashMap<String, String>> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("Failed to open SQLite: {}", db_path.display()))?;
    read_cookies_from_conn(&conn, key)
}

fn read_cookies_from_conn(conn: &Connection, key: &[u8]) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(
        "SELECT host_key, name, value, encrypted_value, path FROM cookies \
         WHERE host_key LIKE '%google.com%'",
    )?;

    let rows = stmt.query_map([], |row| {
        let domain: String = row.get(0)?;
        let name: String = row.get(1)?;
        let plaintext_value: String = row.get(2)?;
        let encrypted_value: Vec<u8> = row.get(3)?;
        let path: String = row.get(4)?;
        Ok((domain, name, plaintext_value, encrypted_value, path))
    })?;

    let mut cookie_rows = Vec::new();
    for row in rows {
        let (domain, name, plaintext_value, encrypted_value, path) = row?;

        let value = if !plaintext_value.is_empty() {
            plaintext_value
        } else if !encrypted_value.is_empty() {
            match decrypt_webview2_cookie(&encrypted_value, key) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!(
                        "webview2 decrypt failed for '{}' on {}: {}",
                        name,
                        domain,
                        e
                    );
                    continue;
                }
            }
        } else {
            String::new()
        };

        if !domain::is_google_domain(&domain) {
            continue;
        }

        cookie_rows.push(super::CookieRow {
            name,
            value,
            domain,
            path,
        });
    }

    let items: Vec<_> = cookie_rows
        .iter()
        .map(|r| domain::CookieItem {
            name: r.name.clone(),
            value: r.value.clone(),
            domain: r.domain.clone(),
        })
        .collect();

    Ok(domain::select_preferred_google_cookies(&items))
}

/// Decrypt a WebView2 cookie value.
///
/// On Windows, the "v10" prefix uses AES-256-GCM (same algorithm as macOS "v20"),
/// NOT the AES-128-CBC that macOS "v10" uses. Unprefixed values are raw DPAPI blobs.
fn decrypt_webview2_cookie(encrypted: &[u8], key: &[u8]) -> Result<String> {
    if encrypted.is_empty() {
        return Ok(String::new());
    }

    // Windows v10 = AES-256-GCM with nonce in first 12 bytes (same algorithm as v20)
    if encrypted.len() >= 3 && &encrypted[..3] == b"v10" {
        return chrome_decrypt::decrypt_v20(&encrypted[3..], key);
    }

    // Fallback: raw DPAPI blob (older WebView2 / pre-AES scheme)
    decrypt_dpapi_fallback(encrypted)
}

fn decrypt_dpapi_fallback(data: &[u8]) -> Result<String> {
    use std::ptr;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: ptr::null_mut(),
    };

    let success = unsafe {
        CryptUnprotectData(
            &mut input,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            &mut output,
        )
    };

    if success == 0 {
        anyhow::bail!("DPAPI CryptUnprotectData failed");
    }

    if output.pbData.is_null() {
        anyhow::bail!("DPAPI CryptUnprotectData returned null pointer");
    }

    // RAII guard: ensure LocalFree is called even if .to_vec() panics
    struct DpapiGuard(*mut u8);
    impl Drop for DpapiGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                extern "system" {
                    fn LocalFree(hmem: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
                }
                unsafe {
                    LocalFree(self.0 as _);
                }
            }
        }
    }
    let _guard = DpapiGuard(output.pbData);

    let result =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();

    String::from_utf8(result).context("DPAPI decrypted data is not UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_missing_data_dir_returns_err() {
        let result = read_webview_session_cookies(&PathBuf::from("C:/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_cookie_subpaths() {
        let base = PathBuf::from("C:/data");
        assert_eq!(
            base.join(COOKIE_SUBPATH),
            PathBuf::from("C:/data/EBWebView/Default/Network/Cookies")
        );
        assert_eq!(
            base.join(LOCAL_STATE_SUBPATH),
            PathBuf::from("C:/data/EBWebView/Local State")
        );
    }
}
