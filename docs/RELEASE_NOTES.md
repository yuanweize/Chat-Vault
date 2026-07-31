# Release Notes

## 3.1.0

- Redesigned the application icon and regenerated platform icon assets.
- Reworked the main UI, modal dialogs, settings, lock screen, export flow, sync overlay, and shared SVG icon system.
- Completed Simplified Chinese and English localization coverage and removed component-level Chinese fallback strings.
- Added non-destructive Gemini Collector migration detection and user-confirmed copy migration.
- Normalized migrated account registry paths so legacy custom `dataDir` layouts load correctly after migration.
- Kept Chat Vault and Gemini Collector data directories isolated to avoid writable-directory conflicts and incompatible Tantivy indexes.
- Added one-time stale WebView asset cleanup for upgraded installations.
- Upgraded local access password verification to salted PBKDF2-SHA256 while preserving legacy verifier migration.
- Improved export/import handling, media inclusion, filename safety, and account clearing safeguards.
- Added uniform path-component validation for account, conversation, and media identifiers across the WebView and Rust command boundary.
- Made ZIP restore streaming and bounded (entry count, per-file size, and total extracted size) to prevent memory and disk exhaustion.
- Sanitized rendered Gemini HTML and search snippets before they reach the DOM.
- Restricted the asset protocol to Chat Vault's app-data directory and removed inline-script permission from the production CSP.
- Updated dependency versions and security posture, including Tantivy 0.26.x; production and development npm audits now report zero known vulnerabilities.
- Added maintained project structure documentation.

## Verification

The 3.1.0 working tree was validated with:

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
npm audit
cargo audit --file src-tauri/Cargo.lock --no-fetch
git diff --check
```

Network-dependent Gemini API integration tests remain ignored by default.
