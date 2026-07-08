use anyhow::{bail, Context, Result};

/// Decrypt a Chrome encrypted_value blob.
///
/// Chrome cookie encryption formats:
/// - v10 (macOS): "v10" prefix + AES-128-CBC with PBKDF2-derived key from Keychain password
/// - v20 (macOS): "v20" prefix + AES-256-GCM with key from Keychain (newer Chrome)
/// - DPAPI (Windows): "v10" prefix + AES-256-GCM with Local State key, or raw DPAPI blob
pub fn decrypt_chrome_cookie_value(encrypted: &[u8], key: &[u8]) -> Result<String> {
    if encrypted.is_empty() {
        return Ok(String::new());
    }

    if encrypted.len() >= 3 && &encrypted[..3] == b"v10" {
        return decrypt_v10(&encrypted[3..], key);
    }
    if encrypted.len() >= 3 && &encrypted[..3] == b"v20" {
        return decrypt_v20(&encrypted[3..], key);
    }

    #[cfg(target_os = "windows")]
    {
        return decrypt_dpapi_to_string(encrypted);
    }

    #[cfg(not(target_os = "windows"))]
    bail!(
        "Unknown encryption format (first bytes: {:?})",
        &encrypted[..encrypted.len().min(4)]
    );
}

/// v10: AES-128-CBC, PKCS7 padding
/// IV = 16 bytes of 0x20 (space)
/// Chrome prepends a 32-byte HMAC-SHA256 signature before the actual value,
/// so after decryption we strip the first 32 bytes.
fn decrypt_v10(data: &[u8], derived_key: &[u8]) -> Result<String> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;

    let iv = [0x20u8; 16];
    let key_16 = &derived_key[..16.min(derived_key.len())];

    let mut buf = data.to_vec();
    let decrypted = Aes128CbcDec::new_from_slices(key_16, &iv)
        .context("AES-128-CBC init failed")?
        .decrypt_padded_mut::<Pkcs7>(&mut buf)
        .map_err(|e| anyhow::anyhow!("AES-128-CBC decrypt failed: {}", e))?;

    // Strip 32-byte HMAC prefix if present
    let payload = if decrypted.len() > 32 {
        &decrypted[32..]
    } else {
        decrypted
    };

    if let Ok(decompressed) = try_lz4_decompress(payload) {
        return String::from_utf8(decompressed).context("UTF-8 decode after lz4 failed");
    }

    String::from_utf8(payload.to_vec()).context("UTF-8 decode failed")
}

/// v20: AES-256-GCM, nonce = first 12 bytes
pub(crate) fn decrypt_v20(data: &[u8], key: &[u8]) -> Result<String> {
    use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};

    if data.len() < 12 + 16 {
        bail!("v20 data too short: {} bytes", data.len());
    }

    let nonce_bytes = &data[..12];
    let ciphertext = &data[12..];

    let cipher = Aes256Gcm::new_from_slice(key).context("AES-256-GCM key init failed")?;
    let nonce = Nonce::from_slice(nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-256-GCM decrypt failed: {}", e))?;

    if let Ok(decompressed) = try_lz4_decompress(&plaintext) {
        return String::from_utf8(decompressed).context("UTF-8 decode after lz4 failed");
    }

    String::from_utf8(plaintext).context("UTF-8 decode failed")
}

fn try_lz4_decompress(data: &[u8]) -> Result<Vec<u8>> {
    if data.is_empty() {
        bail!("empty");
    }
    if std::str::from_utf8(data).is_ok()
        && data
            .iter()
            .all(|&b| b >= 0x20 || b == b'\n' || b == b'\r' || b == b'\t')
    {
        bail!("already valid text");
    }
    let decompressed = lz4_flex::block::decompress_size_prepended(data)
        .map_err(|e| anyhow::anyhow!("lz4 decompress failed: {}", e))?;
    Ok(decompressed)
}

// ── Key retrieval ──────────────────────────────────────────────────────────

/// Get the browser encryption key.
#[cfg(target_os = "macos")]
pub fn get_browser_key(browser_name: &str) -> Result<Vec<u8>> {
    let (service, account) = keychain_service_account(browser_name)?;
    let password = get_keychain_password_via_cli(&service, &account)?;
    derive_key_pbkdf2(password.as_bytes(), 16)
}

#[cfg(target_os = "windows")]
pub fn get_browser_key(browser_name: &str) -> Result<Vec<u8>> {
    get_windows_local_state_key(browser_name)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn get_browser_key(_browser_name: &str) -> Result<Vec<u8>> {
    let password = b"peanuts";
    derive_key_pbkdf2(password, 16)
}

#[cfg(target_os = "macos")]
fn keychain_service_account(browser_name: &str) -> Result<(String, String)> {
    match browser_name {
        "Chrome" => Ok(("Chrome Safe Storage".into(), "Chrome".into())),
        "Chromium" => Ok(("Chromium Safe Storage".into(), "Chromium".into())),
        "Brave" => Ok(("Brave Safe Storage".into(), "Brave".into())),
        "Edge" => Ok((
            "Microsoft Edge Safe Storage".into(),
            "Microsoft Edge".into(),
        )),
        _ => bail!("Unknown browser for Keychain lookup: {}", browser_name),
    }
}

/// Read Keychain password using /usr/bin/security CLI.
/// This avoids the system authorization popup that security-framework triggers.
#[cfg(target_os = "macos")]
fn get_keychain_password_via_cli(service: &str, account: &str) -> Result<String> {
    let output = std::process::Command::new("/usr/bin/security")
        .args(["find-generic-password", "-w", "-a", account, "-s", service])
        .output()
        .context("Failed to execute /usr/bin/security")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Keychain read failed for '{}' (exit {}): {}",
            service,
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }

    let password = String::from_utf8(output.stdout)
        .context("Keychain password is not valid UTF-8")?
        .trim_end_matches('\n')
        .to_string();

    Ok(password)
}

fn derive_key_pbkdf2(password: &[u8], key_len: usize) -> Result<Vec<u8>> {
    use pbkdf2::pbkdf2_hmac;
    use sha1::Sha1;

    let salt = b"saltysalt";
    let iterations = 1003;
    let mut key = vec![0u8; key_len];
    pbkdf2_hmac::<Sha1>(password, salt, iterations, &mut key);
    Ok(key)
}

// ── Windows DPAPI ──────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn get_windows_local_state_key(browser_name: &str) -> Result<Vec<u8>> {
    use std::path::PathBuf;

    let local_app_data = PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default());
    let local_state_path = match browser_name {
        "Chrome" => local_app_data.join("Google/Chrome/User Data/Local State"),
        "Chromium" => local_app_data.join("Chromium/User Data/Local State"),
        "Brave" => local_app_data.join("BraveSoftware/Brave-Browser/User Data/Local State"),
        "Edge" => local_app_data.join("Microsoft/Edge/User Data/Local State"),
        _ => bail!("Unknown browser for Local State: {}", browser_name),
    };

    get_local_state_key(&local_state_path)
}

/// Decrypt the AES key from any Chromium-based Local State file (Chrome, WebView2, etc.).
#[cfg(target_os = "windows")]
pub fn get_local_state_key(local_state_path: &std::path::Path) -> Result<Vec<u8>> {
    let content = std::fs::read_to_string(local_state_path)
        .with_context(|| format!("Failed to read {}", local_state_path.display()))?;

    let json: serde_json::Value = serde_json::from_str(&content)?;
    let encoded_key = json
        .pointer("/os_crypt/encrypted_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing os_crypt.encrypted_key in Local State"))?;

    let decoded = base64_decode(encoded_key)?;

    if decoded.len() < 5 || &decoded[..5] != b"DPAPI" {
        bail!("Local State key missing DPAPI prefix");
    }

    dpapi_decrypt_bytes(&decoded[5..])
}

#[cfg(target_os = "windows")]
fn dpapi_decrypt_bytes(data: &[u8]) -> Result<Vec<u8>> {
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
        bail!("DPAPI CryptUnprotectData failed");
    }

    if output.pbData.is_null() {
        bail!("DPAPI CryptUnprotectData returned null pointer");
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

    Ok(result)
}

#[cfg(target_os = "windows")]
fn decrypt_dpapi_to_string(data: &[u8]) -> Result<String> {
    let bytes = dpapi_decrypt_bytes(data)?;
    String::from_utf8(bytes).context("DPAPI decrypted data is not UTF-8")
}

#[cfg(target_os = "windows")]
fn base64_decode(input: &str) -> Result<Vec<u8>> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut lookup = [255u8; 256];
    for (i, &b) in table.iter().enumerate() {
        lookup[b as usize] = i as u8;
    }

    let input = input.trim();
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in input.as_bytes() {
        if b == b'=' {
            break;
        }
        let val = lookup[b as usize];
        if val == 255 {
            continue;
        }
        buf = (buf << 6) | val as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            result.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(result)
}
