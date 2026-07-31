# Project Structure

Chat Vault is a Tauri 2 desktop application with a React frontend and a Rust backend. The repository keeps generated application assets in source control only when they are required for packaging, such as desktop icons.

## Top-level paths

| Path | Purpose |
| --- | --- |
| `src/` | React frontend source. |
| `src/components/` | App-level UI components and modal dialogs. |
| `src/data/` | Shared frontend TypeScript models and settings defaults. |
| `src/utils/` | Frontend utility functions. |
| `src-tauri/` | Tauri application, Rust commands, packaging config, and icons. |
| `src-tauri/src/` | Rust backend modules for sync, storage, import/export, search, settings, cookies, and Gemini protocol handling. |
| `src-tauri/tests/` | Rust integration test targets. Network-dependent Gemini tests are ignored by default. |
| `src-tauri/icons/` | Generated application icon set used by Tauri bundling. |
| `public/` | Static assets copied into the frontend build. |
| `references/` | Upstream or historical reference material kept for implementation comparison, not compiled into the app. |
| `docs/` | Maintained project documentation. Local scratch docs belong in `docs/local/` and are ignored. |

## Frontend map

| Area | Main files |
| --- | --- |
| App shell and state | `src/App.tsx` |
| Account picker | `src/components/AccountPicker.tsx` |
| Conversation sidebar | `src/components/Sidebar.tsx` |
| Conversation reader | `src/components/ChatView.tsx`, `src/components/ResearchDetailModal.tsx` |
| Settings and lock screen | `src/components/SettingsModal.tsx`, `src/components/LockScreen.tsx` |
| Shared icons | `src/components/Icons.tsx` |
| Localization | `src/i18n.ts` |
| Theme and global UI | `src/theme.ts`, `src/index.css` |

## Backend map

| Area | Main files |
| --- | --- |
| Tauri command surface | `src-tauri/src/lib.rs` |
| Gemini API client | `src-tauri/src/gemini_api/`, `src-tauri/src/protocol.rs` |
| Cookie and account discovery | `src-tauri/src/cookies/`, `src-tauri/src/browser_info.rs` |
| Sync workers | `src-tauri/src/worker_host.rs`, `src-tauri/src/sync.rs`, `src-tauri/src/scheduler.rs` |
| Archive storage | `src-tauri/src/storage.rs`, `src-tauri/src/media.rs` |
| Search index | `src-tauri/src/search.rs` |
| Import/export | `src-tauri/src/import.rs`, `src-tauri/src/export.rs`, `src-tauri/src/export_md.rs` |
| Gemini Collector migration | `src-tauri/src/legacy_migration.rs` |
| Settings and password lock | `src-tauri/src/settings.rs` |

## Generated and local-only paths

These paths should not be committed:

- `dist/`
- `src-tauri/target/`
- `node_modules/`
- `release/`
- `.DS_Store`
- `docs/local/`

Run a clean production check before publishing:

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
git diff --check
```
