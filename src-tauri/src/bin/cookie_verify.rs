//! Cookie verification binary: reads Chrome cookies, runs Keychain diagnostics,
//! and optionally tests ListAccounts API.
use anyhow::Result;
use chat_vault_lib::cookies;
use std::collections::BTreeMap;

fn main() -> Result<()> {
    // ── Phase 0: Cookie file discovery ──
    eprintln!("[cookie-verify] Discovering browser cookie files...");
    let (entries, permission_issues) = cookies::discover_chrome_cookie_files();

    if !permission_issues.is_empty() {
        eprintln!("[cookie-verify] Permission issues:");
        for issue in &permission_issues {
            eprintln!("  {}", issue);
        }
    }
    eprintln!("[cookie-verify] Found {} cookie file(s)", entries.len());

    // ── Phase 0: Keychain diagnostics ──
    eprintln!("\n[cookie-verify] Running Keychain diagnostics...");
    let diag_report = cookies::run_full_diagnostics();
    eprintln!(
        "[cookie-verify] Diagnostics summary: {}",
        diag_report.summary
    );
    for kd in &diag_report.keychain_diagnostics {
        if kd.accessible {
            eprintln!(
                "  [OK] {} — Keychain \"{}\" readable",
                kd.browser, kd.service
            );
        } else {
            eprintln!("  [FAIL] {} — {}", kd.browser, kd.detail);
            if !kd.suggestion.is_empty() {
                for line in kd.suggestion.lines() {
                    eprintln!("         {}", line);
                }
            }
        }
    }

    // ── Phase 0: Per-browser cookie read ──
    let mut all_browsers: Vec<serde_json::Value> = Vec::new();
    for entry in &entries {
        let label = format!("{}/{}", entry.browser_name, entry.profile_name);
        eprintln!("\n[cookie-verify] Reading: {}", label);

        match cookies::read_chrome_cookies(
            std::path::Path::new(&entry.cookie_file),
            &entry.browser_name,
        ) {
            Ok(rows) => {
                let items: Vec<cookies::domain::CookieItem> = rows
                    .iter()
                    .map(|r| cookies::domain::CookieItem {
                        name: r.name.clone(),
                        value: r.value.clone(),
                        domain: r.domain.clone(),
                    })
                    .collect();
                let selected = cookies::select_preferred_google_cookies(&items);
                let sorted: BTreeMap<_, _> = selected.into_iter().collect();
                eprintln!(
                    "[cookie-verify]   {} cookies selected (from {} raw rows)",
                    sorted.len(),
                    rows.len()
                );
                all_browsers.push(serde_json::json!({
                    "browser": entry.browser_name,
                    "profile": entry.profile_name,
                    "cookie_file": entry.cookie_file,
                    "raw_count": rows.len(),
                    "selected_count": sorted.len(),
                    "selected": sorted,
                }));
            }
            Err(e) => {
                eprintln!("[cookie-verify]   FAILED: {}", e);
                all_browsers.push(serde_json::json!({
                    "browser": entry.browser_name,
                    "profile": entry.profile_name,
                    "cookie_file": entry.cookie_file,
                    "error": e.to_string(),
                }));
            }
        }
    }

    // ── High-level get_cookies_from_local_browser ──
    eprintln!("\n[cookie-verify] Running get_cookies_from_local_browser()...");
    let final_cookies = cookies::get_cookies_from_local_browser()?;
    let final_sorted: BTreeMap<_, _> = final_cookies.clone().into_iter().collect();
    let key_cookies = ["__Secure-1PSID", "__Secure-1PSIDTS"];
    let has_key_cookies = key_cookies.iter().any(|k| final_cookies.contains_key(*k));

    eprintln!(
        "[cookie-verify] Final: {} cookies, key_cookies_present={}",
        final_sorted.len(),
        has_key_cookies
    );

    // ── Phase 3: ListAccounts test (only if cookies available) ──
    let list_accounts_result = if has_key_cookies {
        eprintln!("\n[cookie-verify] Testing ListAccounts API...");
        let rt = tokio::runtime::Runtime::new()?;
        match rt.block_on(cookies::discover_email_authuser_mapping(&final_cookies)) {
            Ok(mappings) => {
                eprintln!(
                    "[cookie-verify] ListAccounts: found {} account(s)",
                    mappings.len()
                );
                for m in &mappings {
                    eprintln!(
                        "  email={}, authuser={}, redirect={}",
                        m.email,
                        m.authuser.as_deref().unwrap_or("?"),
                        m.redirect_url.as_deref().unwrap_or("-"),
                    );
                }
                Some(serde_json::json!({
                    "ok": true,
                    "accounts": mappings,
                }))
            }
            Err(e) => {
                eprintln!("[cookie-verify] ListAccounts FAILED: {}", e);
                Some(serde_json::json!({
                    "ok": false,
                    "error": e,
                }))
            }
        }
    } else {
        eprintln!("\n[cookie-verify] Skipping ListAccounts (no key cookies)");
        None
    };

    // ── JSON output ──
    let output = serde_json::json!({
        "browsers": all_browsers,
        "diagnostics": diag_report,
        "final_selected": final_sorted,
        "final_count": final_sorted.len(),
        "key_cookies_present": has_key_cookies,
        "list_accounts": list_accounts_result,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
