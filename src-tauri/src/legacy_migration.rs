//! Non-destructive migration from the original Gemini Collector data directory.
//!
//! The archival formats are compatible, but runtime state is deliberately kept
//! separate: the two apps must not share settings, interrupted sync state, or
//! Tantivy indexes created by different Tantivy versions.

use serde::Serialize;
use serde_json::Value;
use std::path::{Component, Path, PathBuf};
use tauri::Manager;

use crate::search;
use crate::str_err::ToStringErr;

const LEGACY_IDENTIFIER: &str = "com.gemini-collector";
const STAGING_DIR: &str = ".legacy-migration-staging";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationInfo {
    pub available: bool,
    pub account_count: usize,
    pub conversation_count: usize,
    pub media_file_count: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyMigrationResult {
    pub account_count: usize,
    pub conversation_count: usize,
    pub media_file_count: usize,
    pub total_bytes: u64,
    pub rebuilt_search_accounts: usize,
}

#[derive(Debug, Clone)]
struct LegacyAccount {
    id: String,
    source_dir: PathBuf,
}

fn legacy_data_dir(current_data_dir: &Path) -> Option<PathBuf> {
    let parent = current_data_dir.parent()?;
    let legacy = parent.join(LEGACY_IDENTIFIER);
    (legacy != current_data_dir).then_some(legacy)
}

fn is_safe_relative_path(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}

fn is_safe_account_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn read_legacy_accounts(legacy_dir: &Path) -> Result<Vec<LegacyAccount>, String> {
    let registry_file = legacy_dir.join("accounts.json");
    let registry: Value = serde_json::from_str(
        &std::fs::read_to_string(&registry_file)
            .map_err(|error| format!("Could not read legacy accounts.json: {error}"))?,
    )
    .map_err(|error| format!("Invalid legacy accounts.json: {error}"))?;
    let entries = registry
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or_else(|| "Legacy accounts.json does not contain an accounts array".to_string())?;

    let mut accounts = Vec::new();
    for entry in entries {
        let Some(id) = entry.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !is_safe_account_id(id) {
            continue;
        }
        let relative = entry
            .get("dataDir")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("accounts/{id}"));
        if !is_safe_relative_path(&relative) {
            continue;
        }
        let source_dir = legacy_dir.join(relative);
        if source_dir.is_dir() {
            accounts.push(LegacyAccount {
                id: id.to_string(),
                source_dir,
            });
        }
    }
    Ok(accounts)
}

fn count_files(dir: &Path, extension: Option<&str>) -> Result<(usize, u64), String> {
    if !dir.is_dir() {
        return Ok((0, 0));
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in std::fs::read_dir(&current).str_err()? {
            let entry = entry.str_err()?;
            let file_type = entry.file_type().str_err()?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file() {
                let matches_extension = extension.is_none_or(|expected| {
                    entry.path().extension().and_then(|value| value.to_str()) == Some(expected)
                });
                if matches_extension {
                    count += 1;
                    bytes = bytes.saturating_add(entry.metadata().str_err()?.len());
                }
            }
        }
    }
    Ok((count, bytes))
}

fn current_archive_is_empty(current_data_dir: &Path) -> Result<bool, String> {
    let accounts_dir = current_data_dir.join("accounts");
    if !accounts_dir.is_dir() {
        return Ok(true);
    }
    for entry in std::fs::read_dir(accounts_dir).str_err()? {
        let entry = entry.str_err()?;
        if !entry.file_type().str_err()?.is_dir() {
            continue;
        }
        let account_dir = entry.path();
        if count_files(&account_dir.join("conversations"), Some("jsonl"))?.0 > 0
            || count_files(&account_dir.join("media"), None)?.0 > 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn inspect_legacy_data(
    current_data_dir: &Path,
    legacy_dir: &Path,
) -> Result<LegacyMigrationInfo, String> {
    if !current_archive_is_empty(current_data_dir)? || !legacy_dir.is_dir() {
        return Ok(LegacyMigrationInfo {
            available: false,
            account_count: 0,
            conversation_count: 0,
            media_file_count: 0,
            total_bytes: 0,
        });
    }

    let accounts = read_legacy_accounts(legacy_dir)?;
    let mut conversation_count = 0usize;
    let mut media_file_count = 0usize;
    let mut total_bytes = std::fs::metadata(legacy_dir.join("accounts.json"))
        .map(|meta| meta.len())
        .unwrap_or(0);
    for account in &accounts {
        let (conversations, conversation_bytes) =
            count_files(&account.source_dir.join("conversations"), Some("jsonl"))?;
        let (media, media_bytes) = count_files(&account.source_dir.join("media"), None)?;
        let (_, export_bytes) = count_files(&account.source_dir.join("exports"), None)?;
        conversation_count += conversations;
        media_file_count += media;
        total_bytes = total_bytes
            .saturating_add(conversation_bytes)
            .saturating_add(media_bytes)
            .saturating_add(export_bytes);
        for file in [
            "meta.json",
            "conversations.json",
            "media_manifest.json",
            "folders.json",
        ] {
            total_bytes = total_bytes.saturating_add(
                std::fs::metadata(account.source_dir.join(file))
                    .map(|meta| meta.len())
                    .unwrap_or(0),
            );
        }
    }

    Ok(LegacyMigrationInfo {
        available: !accounts.is_empty() && conversation_count > 0,
        account_count: accounts.len(),
        conversation_count,
        media_file_count,
        total_bytes,
    })
}

fn copy_file(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).str_err()?;
    }
    std::fs::copy(source, target).str_err()?;
    Ok(())
}

fn write_normalized_accounts_registry(
    legacy_dir: &Path,
    target_file: &Path,
    accounts: &[LegacyAccount],
) -> Result<(), String> {
    let registry_file = legacy_dir.join("accounts.json");
    let mut registry: Value = serde_json::from_str(
        &std::fs::read_to_string(&registry_file)
            .map_err(|error| format!("Could not read legacy accounts.json: {error}"))?,
    )
    .map_err(|error| format!("Invalid legacy accounts.json: {error}"))?;
    let valid_ids: std::collections::HashSet<&str> =
        accounts.iter().map(|account| account.id.as_str()).collect();

    if let Some(entries) = registry.get_mut("accounts").and_then(Value::as_array_mut) {
        entries.retain(|entry| {
            entry
                .get("id")
                .and_then(Value::as_str)
                .map(|id| valid_ids.contains(id))
                .unwrap_or(false)
        });
        for entry in entries {
            if let Some(id) = entry.get("id").and_then(Value::as_str).map(str::to_string) {
                entry["dataDir"] = Value::String(format!("accounts/{id}"));
            }
        }
    }

    std::fs::write(
        target_file,
        serde_json::to_string_pretty(&registry).str_err()?,
    )
    .str_err()?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(target).str_err()?;
    for entry in std::fs::read_dir(source).str_err()? {
        let entry = entry.str_err()?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry.file_type().str_err()?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            copy_file(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn unique_backup_path(current_data_dir: &Path, label: &str) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    current_data_dir.join(format!(".pre-legacy-migration-{label}-{timestamp}"))
}

fn copy_legacy_archive(
    current_data_dir: &Path,
    legacy_dir: &Path,
) -> Result<LegacyMigrationResult, String> {
    let info = inspect_legacy_data(current_data_dir, legacy_dir)?;
    if !info.available {
        return Err("No compatible legacy archive is available for migration".to_string());
    }
    let accounts = read_legacy_accounts(legacy_dir)?;
    std::fs::create_dir_all(current_data_dir).str_err()?;
    let staging_dir = current_data_dir.join(STAGING_DIR);
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir).str_err()?;
    }
    std::fs::create_dir_all(staging_dir.join("accounts")).str_err()?;

    let staged_registry = staging_dir.join("accounts.json");
    write_normalized_accounts_registry(legacy_dir, &staged_registry, &accounts)?;
    for account in &accounts {
        let target = staging_dir.join("accounts").join(&account.id);
        for file in [
            "meta.json",
            "conversations.json",
            "media_manifest.json",
            "folders.json",
        ] {
            let source = account.source_dir.join(file);
            if source.is_file() {
                copy_file(&source, &target.join(file))?;
            }
        }
        for directory in ["conversations", "media", "exports"] {
            copy_dir_recursive(&account.source_dir.join(directory), &target.join(directory))?;
        }
    }

    let target_accounts = current_data_dir.join("accounts");
    let target_registry = current_data_dir.join("accounts.json");
    let backup_accounts = unique_backup_path(current_data_dir, "accounts");
    let backup_registry = unique_backup_path(current_data_dir, "accounts.json");
    if target_accounts.exists() {
        std::fs::rename(&target_accounts, &backup_accounts).str_err()?;
    }
    if target_registry.exists() {
        if let Err(error) = std::fs::rename(&target_registry, &backup_registry) {
            if backup_accounts.exists() {
                let _ = std::fs::rename(&backup_accounts, &target_accounts);
            }
            return Err(error.to_string());
        }
    }

    if let Err(error) = std::fs::rename(staging_dir.join("accounts"), &target_accounts) {
        if backup_accounts.exists() {
            let _ = std::fs::rename(&backup_accounts, &target_accounts);
        }
        if backup_registry.exists() {
            let _ = std::fs::rename(&backup_registry, &target_registry);
        }
        return Err(error.to_string());
    }
    if let Err(error) = std::fs::rename(&staged_registry, &target_registry) {
        let _ = std::fs::remove_dir_all(&target_accounts);
        if backup_accounts.exists() {
            let _ = std::fs::rename(&backup_accounts, &target_accounts);
        }
        if backup_registry.exists() {
            let _ = std::fs::rename(&backup_registry, &target_registry);
        }
        return Err(error.to_string());
    }
    let _ = std::fs::remove_dir(&staging_dir);

    // The swap is complete. Old Chat Vault account stubs are no longer needed;
    // remove their temporary backups without turning a successful migration
    // into a retryable failure if the OS keeps a file handle open briefly.
    if backup_accounts.exists() {
        if let Err(error) = std::fs::remove_dir_all(&backup_accounts) {
            log::warn!("Could not remove legacy migration account backup: {error}");
        }
    }
    if backup_registry.exists() {
        if let Err(error) = std::fs::remove_file(&backup_registry) {
            log::warn!("Could not remove legacy migration registry backup: {error}");
        }
    }

    let marker = serde_json::json!({
        "source": LEGACY_IDENTIFIER,
        "migratedAt": chrono::Utc::now().to_rfc3339(),
        "conversationCount": info.conversation_count,
    });
    if let Err(error) = std::fs::write(
        current_data_dir.join("legacy_migration.json"),
        serde_json::to_string_pretty(&marker).str_err()?,
    ) {
        log::warn!("Could not write the legacy migration marker: {error}");
    }

    Ok(LegacyMigrationResult {
        account_count: info.account_count,
        conversation_count: info.conversation_count,
        media_file_count: info.media_file_count,
        total_bytes: info.total_bytes,
        rebuilt_search_accounts: 0,
    })
}

#[tauri::command]
pub async fn detect_legacy_data(app: tauri::AppHandle) -> Result<LegacyMigrationInfo, String> {
    let current_data_dir = app.path().app_data_dir().str_err()?;
    let Some(legacy_dir) = legacy_data_dir(&current_data_dir) else {
        return Ok(LegacyMigrationInfo {
            available: false,
            account_count: 0,
            conversation_count: 0,
            media_file_count: 0,
            total_bytes: 0,
        });
    };
    tauri::async_runtime::spawn_blocking(move || {
        inspect_legacy_data(&current_data_dir, &legacy_dir)
    })
    .await
    .str_err()?
}

#[tauri::command]
pub async fn migrate_legacy_data(app: tauri::AppHandle) -> Result<LegacyMigrationResult, String> {
    let current_data_dir = app.path().app_data_dir().str_err()?;
    let legacy_dir = legacy_data_dir(&current_data_dir)
        .ok_or_else(|| "Could not locate the Gemini Collector data directory".to_string())?;
    let index_root = current_data_dir.clone();
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        copy_legacy_archive(&current_data_dir, &legacy_dir)
    })
    .await
    .str_err()??;

    let mut rebuilt = 0usize;
    if let Ok(accounts) = read_legacy_accounts(&index_root) {
        for account in accounts {
            let conversations = account.source_dir.join("conversations");
            if let Ok(index) = search::open_or_create_index(&account.source_dir) {
                if search::index_all(&index, &account.source_dir, &conversations).is_ok() {
                    rebuilt += 1;
                }
            }
        }
    }
    result.rebuilt_search_accounts = rebuilt;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(label: &str) -> PathBuf {
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("chat-vault-{label}-{id}"))
    }

    fn create_legacy_fixture(root: &Path) {
        create_legacy_fixture_with_data_dir(root, "accounts/test_example_com");
    }

    fn create_legacy_fixture_with_data_dir(root: &Path, data_dir: &str) {
        let account = root.join(data_dir);
        std::fs::create_dir_all(account.join("conversations")).unwrap();
        std::fs::create_dir_all(account.join("media")).unwrap();
        std::fs::create_dir_all(account.join("search_index")).unwrap();
        std::fs::write(
            root.join("accounts.json"),
            format!(
                r#"{{"version":1,"accounts":[{{"id":"test_example_com","dataDir":"{data_dir}"}}]}}"#
            ),
        )
        .unwrap();
        std::fs::write(account.join("meta.json"), r#"{"version":1}"#).unwrap();
        std::fs::write(
            account.join("conversations.json"),
            r#"{"version":1,"items":[]}"#,
        )
        .unwrap();
        std::fs::write(
            account.join("conversations/example.jsonl"),
            "{\"type\":\"meta\",\"id\":\"example\"}\n",
        )
        .unwrap();
        std::fs::write(account.join("media/example.png"), b"image").unwrap();
        std::fs::write(account.join("sync_state.json"), r#"{"fullSync":{}}"#).unwrap();
        std::fs::write(account.join("search_mtimes.json"), r#"{}"#).unwrap();
        std::fs::write(account.join("search_index/meta.json"), r#"{}"#).unwrap();
    }

    #[test]
    fn detects_legacy_archive_when_current_app_only_has_account_stubs() {
        let root = test_root("detect");
        let current = root.join("com.chat-vault");
        let legacy = root.join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(current.join("accounts/test_example_com")).unwrap();
        create_legacy_fixture(&legacy);

        let info = inspect_legacy_data(&current, &legacy).unwrap();
        assert!(info.available);
        assert_eq!(info.account_count, 1);
        assert_eq!(info.conversation_count, 1);
        assert_eq!(info.media_file_count, 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migration_normalizes_legacy_registry_data_dirs() {
        let root = test_root("registry");
        let current = root.join("com.chat-vault");
        let legacy = root.join(LEGACY_IDENTIFIER);
        create_legacy_fixture_with_data_dir(&legacy, "profiles/test_example_com");

        copy_legacy_archive(&current, &legacy).unwrap();
        let registry: Value =
            serde_json::from_str(&std::fs::read_to_string(current.join("accounts.json")).unwrap())
                .unwrap();
        let data_dir = registry["accounts"][0]["dataDir"].as_str().unwrap();
        assert_eq!(data_dir, "accounts/test_example_com");
        assert!(current
            .join("accounts/test_example_com/conversations/example.jsonl")
            .is_file());
        assert!(current.join(data_dir).join("meta.json").is_file());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn migration_copies_archives_but_keeps_runtime_state_isolated() {
        let root = test_root("copy");
        let current = root.join("com.chat-vault");
        let legacy = root.join(LEGACY_IDENTIFIER);
        std::fs::create_dir_all(current.join("accounts/test_example_com")).unwrap();
        create_legacy_fixture(&legacy);

        let result = copy_legacy_archive(&current, &legacy).unwrap();
        let migrated = current.join("accounts/test_example_com");
        assert_eq!(result.conversation_count, 1);
        assert!(migrated.join("conversations/example.jsonl").is_file());
        assert!(migrated.join("media/example.png").is_file());
        assert!(!migrated.join("sync_state.json").exists());
        assert!(!migrated.join("search_mtimes.json").exists());
        assert!(!migrated.join("search_index").exists());
        assert!(current.join("legacy_migration.json").is_file());
        assert!(std::fs::read_dir(&current).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".pre-legacy-migration-")
        }));
        assert!(legacy
            .join("accounts/test_example_com/conversations/example.jsonl")
            .is_file());

        let _ = std::fs::remove_dir_all(root);
    }
}
