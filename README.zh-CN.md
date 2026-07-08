<div align="center">

<img src="src-tauri/icons/128x128.png" width="120" style="border-radius:22px; box-shadow: 0 8px 24px rgba(0,0,0,0.15); margin-bottom: 20px"/>

# Chat Vault

**你的 Google Gemini 本地备份与管理终极金库**

macOS & Windows 原生桌面端 · 无感后台同步 · 密码锁保护 · 时间轴导航 · 多格式高级导出

[**English**](./README.md)

![GitHub Release](https://img.shields.io/github/v/release/yuanweize/Chat-Vault?color=0071e3&style=for-the-badge)
![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Windows-lightgrey?style=for-the-badge)
![License](https://img.shields.io/badge/License-AGPL%203.0-green?style=for-the-badge)

</div>

---

## 📸 界面预览

> **✨ v3.0.0 重大更新**: 我们对 UI 进行了全面扁平化 (Flat Design) 重构，去除了所有厚重的毛玻璃效果，界面更加干净清爽。同时，底层架构也经过了深度优化重构，现在即使面对成百上千张的深度研究报告 (Deep Research) 和 Canvas 生成内容，UI 也绝不会卡顿假死。

<div align="center">

| 浅色模式 | 深色模式 |
|:---:|:---:|
| <img src=".github/images/account-picker-light.png" width="420" alt="多账号选择浅色" title="浅色模式" /> | <img src=".github/images/account-picker-dark.png" width="420" alt="多账号选择深色" title="深色模式" /> |

| 聊天视图 | 媒体文件 |
|:---:|:---:|
| <img src=".github/images/chat-main.png" width="420" alt="聊天视图" title="聊天视图" /> | <img src=".github/images/chat-media.png" width="420" alt="媒体文件" title="媒体文件" /> |

| 会话列表 | 导出对话 |
|:---:|:---:|
| <img src=".github/images/chat-conversation.png" width="420" alt="会话列表" title="会话列表" /> | <img src=".github/images/export-dialog.png" width="420" alt="导出对话" title="导出对话" /> |

</div>

---

## ✨ 核心特性 (Chat-Vault 独占升级)

我们在原版 `gemini-collector` 的优秀基础上进行了深度重构，将其打造成了一个真正可“无人值守”的数据金库：

- 🛡️ **金库级安全加密**：支持设置启动密码锁，并带有自动锁屏机制（1分钟、5分钟等），守护你的本地 AI 对话隐私。
- 🥷 **无感后台静默运行**：完美融入 macOS 菜单栏或 Windows 托盘，告别繁杂的 Dock 栏图标。支持在全新设置界面自定义自动同步间隔。
- 📦 **高级多格式导出**：
  - 导出排版绝美、适合阅读与打印的 **原生 PDF**。
  - 导出带有本地媒体路径的纯粹 **Markdown**。
  - 导出打包好一切附件与数据的单会话 **ZIP 压缩包**。
- 📅 **Voyager 风格时间轴**：侧边栏重构，成百上千的对话现在会自动按照“本月”、“上个月”、“2026年6月”等维度优雅分组。
- 🌐 **现代化多语言架构**：全方位支持简体中文与英文无缝切换。
- ⚡ **无忧同步引擎**：底层网络波动自动容错，媒体文件下载失败最高触发 3 次指数退避重试，最大可能保证你的数据完整性，不留遗漏。

### 基础能力
- **零配置同步**：macOS 自动读取 Chrome 的 Gemini 登录态，点击即同步；Windows 首次内置浏览器登录即可。
- **全量归档**：不仅同步文本，更将用户上传的文件、AI 生成的图片、音乐、视频甚至 Deep Research 深度研究报告等全数存至本地。
- **卓越阅读体验**：原生 UI、自动深浅色切换、全量 Markdown 支持（代码高亮、LaTeX 数学公式）。

---

## 🔒 隐私声明

**完全在本地运行，你的数据永远不会上传到任何服务器。**

- **macOS**：只在本地读取 Chrome 的 cookie，无需你输入账号密码。
- **Windows**：通过内置的 WebView2 进行 Google 登录，cookie 仅保留在你的本地硬盘中。
- 所有同步下来的聊天记录只存在你的电脑上。

---

## 🚀 下载与安装

| 平台 | 状态 | 步骤 |
|:---|:---:|:---|
| macOS | ✅ | 在 Releases 下载最新的 `.dmg`，拖入应用程序即可 |
| Windows | ✅ | 在 Releases 下载最新的安装程序并运行 |

> **macOS“无法打开”提示**：进入**系统设置 → 隐私与安全性**，向下滚动并点击“仍要打开”。
>
> **macOS“已损坏”提示**：在终端执行以下命令，然后重新打开：
> ```bash
> xattr -cr /Applications/Chat-Vault.app
> ```

---

## 🙏 致谢

本项目的诞生离不开开源社区的伟大贡献，我们基于以下卓越项目进行了深度二次开发：
- [**FirenzeLor/gemini-collector**](https://github.com/FirenzeLor/gemini-collector)：提供了坚实的同步引擎与基础 UI 框架。
- [**Nagi-ovo/gemini-voyager**](https://github.com/Nagi-ovo/gemini-voyager)：启发了我们的 Timeline 时间轴设计与交互思路。

向原作者致以最诚挚的感谢！

---

## 📄 许可协议

[GNU AGPL-3.0](./LICENSE) — 自由使用、修改与分享。如果你修改了此项目并作为网络服务运行或分发，必须开源相同的源代码。
