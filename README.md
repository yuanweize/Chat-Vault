<div align="center">

<img src="src-tauri/icons/128x128.png" width="120" style="border-radius:22px; box-shadow: 0 8px 24px rgba(0,0,0,0.15); margin-bottom: 20px"/>

# Chat Vault

**Your Ultimate Google Gemini Local Backup & Management Vault**

Native Desktop App for macOS & Windows · Unattended Background Sync · Password Protection · Timeline & Multi-format Export

[**简体中文**](./README.zh-CN.md)

![GitHub Release](https://img.shields.io/github/v/release/yuanweize/Chat-Vault?color=0071e3&style=for-the-badge)
![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Windows-lightgrey?style=for-the-badge)
![License](https://img.shields.io/badge/License-AGPL%203.0-green?style=for-the-badge)

</div>

---

## 📸 Screenshots

> **✨ What's New in v3.0.0**: We have completely overhauled the UI with a new Flat Minimalist design, stripping away heavy glassmorphism for a clean, snappy aesthetic. The core parsing engine has also been refactored to perfectly handle massive Deep Research and Canvas generation payloads without UI freezing.

<div align="center">

| Light theme | Dark theme |
|:---:|:---:|
| <img src=".github/images/account-picker-light.png" width="420" alt="Account picker light theme" title="Light theme" /> | <img src=".github/images/account-picker-dark.png" width="420" alt="Account picker dark theme" title="Dark theme" /> |

| Chat view | Media messages |
|:---:|:---:|
| <img src=".github/images/chat-main.png" width="420" alt="Chat view" title="Chat view" /> | <img src=".github/images/chat-media.png" width="420" alt="Media messages" title="Media messages" /> |

| Conversation list | Export |
|:---:|:---:|
| <img src=".github/images/chat-conversation.png" width="420" alt="Conversation list" title="Conversation list" /> | <img src=".github/images/export-dialog.png" width="420" alt="Export dialog" title="Export" /> |

</div>

---

## ✨ Features (Chat-Vault Exclusive)

We took the brilliant foundation of `gemini-collector` and upgraded it into a full-fledged, unattended **Vault** for your AI data:

- 🛡️ **Vault Security**: Set a password lock with auto-lock timers (1m, 5m, 15m, etc.) to keep your local AI chats private.
- 🥷 **Unattended Background Sync**: Run silently in the macOS Menu Bar/Windows Tray without cluttering your Dock. Configure auto-sync intervals directly in the UI.
- 📦 **Advanced Multi-Format Export**:
  - Export beautiful native **PDFs** optimized for reading and printing.
  - Export rich **Markdown** files with local media embedding.
  - Export **ZIP archives** containing full conversation metadata and assets.
- 📅 **Voyager-Style Timeline**: Seamlessly browse thousands of conversations neatly grouped by month (e.g., "This Month", "June 2026", "Earlier").
- 🌐 **Modern i18n System**: Full dynamic support for English and Simplified Chinese interfaces.
- ⚡ **Worry-Free Sync Engine**: Exponential backoff and auto-retries for media downloads, guaranteeing zero data loss during network fluctuations.

### Core Capabilities
- **Zero-config sync**: Automatically detects Chrome sessions on macOS. Sign-in via WebView on Windows.
- **Full content archival**: Syncs text, user-uploaded attachments, AI-generated images, canvas files, and Deep Research reports.
- **Reading experience**: Native UI with automatic light/dark theme switching, LaTeX math rendering, and syntax-highlighted code.

---

## 🔒 Security & Privacy

**Everything runs locally. No data is ever uploaded.**

- **macOS**: reads local Chrome cookies for Gemini authorization — no manual login required.
- **Windows**: Google sign-in via built-in WebView2 browser, cookies stored locally only.
- All synced content stays on your machine — no third-party servers involved.
- No account registration or extra authorization needed.

---

## 🚀 Install

| Platform | Status | Instructions |
|:---|:---:|:---|
| macOS | ✅ | Download the latest `.dmg` from Releases, drag to Applications |
| Windows | ✅ | Download the latest installer from Releases and run it |

> **macOS "unverified developer" warning**: Go to **System Settings → Privacy & Security** and click "Open Anyway".
>
> **macOS "damaged" warning**: Run the following in Terminal, then reopen:
> ```bash
> xattr -cr /Applications/Chat-Vault.app
> ```

---

## 🙏 Acknowledgements

This project (`Chat-Vault`) is heavily inspired by and built upon the excellent work of [**Nagi-ovo/gemini-voyager**](https://github.com/Nagi-ovo/gemini-voyager) and [**FirenzeLor/gemini-collector**](https://github.com/FirenzeLor/gemini-collector). We extend our deepest gratitude to the original authors for their open-source contributions. 

---

## 📄 License

[GNU AGPL-3.0](./LICENSE) — free to use, modify, and share. If you run a modified version as a network service, or distribute it, you must release the complete corresponding source code under the same license.
