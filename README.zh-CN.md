<div align="center">
  <img src="app-icon.png" width="112" alt="Chat Vault 图标" />

# Chat Vault

**Google Gemini 对话的私密本地归档与阅读器**

macOS · Windows · 多账号 · 后台同步 · 全文搜索 · 富媒体 · 可迁移导出

[English](./README.md)

[![Release](https://img.shields.io/github/v/release/yuanweize/Chat-Vault?style=flat-square&color=5558d9)](https://github.com/yuanweize/Chat-Vault/releases)
![macOS](https://img.shields.io/badge/macOS-支持-171a24?style=flat-square)
![Windows](https://img.shields.io/badge/Windows-支持-171a24?style=flat-square)
[![AGPL-3.0](https://img.shields.io/badge/许可-AGPL--3.0-16815d?style=flat-square)](./LICENSE)
</div>

当前版本：**3.1.0** · [项目结构](./docs/PROJECT_STRUCTURE.md) · [发布说明](./docs/RELEASE_NOTES.md)

Chat Vault 将你的 Gemini 历史记录同步为设备上的本地归档。即使某段对话从远端列表消失，本地副本仍可继续搜索、阅读和导出。它专注于可靠归档和数据管理，不是 Gemini 聊天客户端的替代品。

## 核心能力

- **可靠的本地同步**：支持列表同步、单对话同步、增量同步和完整账号同步，提供任务续传、取消、重试与失败媒体恢复。
- **完整的阅读体验**：支持 Markdown、表格、代码高亮、LaTeX、上传附件、生成图片、音视频、Canvas 文档和 Deep Research 计划/报告。
- **面向大型归档**：对话与消息虚拟列表、时间轴分组、本地全文搜索、文件夹、状态筛选和多种排序方式。
- **多账号工作流**：账号发现、快速切换、独立归档和逐账号同步状态。
- **实用的导入导出**：单对话 Markdown/ZIP、打印为 PDF、完整账号 ZIP、Kelivo 兼容格式（含分包）和 ZIP 恢复导入。
- **本地访问保护**：可选 PBKDF2-SHA256 密码验证与按闲置时间自动锁定。
- **一致的双语界面**：完整简体中文/英文资源，支持跟随系统、浅色和深色主题。

## 平台差异

| 平台 | 登录方式 | 说明 |
| --- | --- | --- |
| macOS | 读取本机 Chrome/Chromium 系浏览器中的 Gemini 登录状态 | 系统可能要求授权钥匙串与完全磁盘访问。 |
| Windows | 在应用内 WebView2 窗口登录 Google | 登录 Cookie 只保留在本机。 |

当前没有正式支持 Linux 发行包。

## 隐私与安全

- 对话归档、搜索索引、设置和导出文件均保存在本地。
- Chat Vault 会直接连接 Google Gemini 来同步用户选择的账号；项目不使用 Chat Vault 云服务或统计分析后端。
- 可选密码是**应用界面访问锁**，不是磁盘加密。能够访问你的操作系统账号或文件系统的软件仍可读取对话和媒体文件。如需静态加密，请启用 FileVault 或 BitLocker。
- 密码验证信息使用随机盐与 PBKDF2-SHA256。旧版 SHA-256 验证值会在成功解锁后自动迁移。
- Gemini HTML 与搜索高亮在进入 DOM 前会经过安全清洗；账号、对话和媒体路径会在前后端边界同时校验。
- ZIP 恢复采用流式写盘，并限制条目数量和解压大小，避免无上限占用内存或磁盘。
- 导出的 ZIP、Markdown、JSON 与媒体文件都可能包含私人对话，请谨慎存储和分享。

## 安装

从 [GitHub Releases](https://github.com/yuanweize/Chat-Vault/releases) 下载最新版本：

- **macOS**：打开 `.dmg`，将 Chat Vault 拖入“应用程序”。
- **Windows**：运行对应安装程序。

如果 macOS 阻止未签名版本，请进入**系统设置 → 隐私与安全性**并选择“仍要打开”。若系统提示安装包“已损坏”，请先确认文件确实来自本仓库，再执行：

```bash
xattr -cr "/Applications/Chat Vault.app"
```

### 从 Gemini Collector 无损迁移

Chat Vault 使用独立的应用标识和数据目录，不会覆盖或修改原 Gemini Collector。首次启动且 Chat Vault 尚无真实归档时，应用会自动检测相邻的 `com.gemini-collector` 数据目录并在迁移前征求确认。

对话 JSONL、账号元数据、媒体、文件夹和生成的导出产物会复制到 Chat Vault；密码设置、未完成的同步状态和旧搜索索引不会复制。两者的核心归档格式兼容，但不应直接共享同一个可写目录：并发同步可能产生冲突，且 Chat Vault 使用的新版 Tantivy 搜索索引与旧索引不兼容。迁移后 Chat Vault 会重建自己的搜索索引，原始数据保持不变。

## 基本使用

1. 选择或登录 Gemini 账号。
2. 使用“列表同步”刷新对话元数据，或使用“完整同步”下载所有内容与媒体。
3. 按时间轴浏览、搜索本地索引，或使用文件夹整理对话。
4. 在对话顶部导出 Markdown/ZIP、打印为 PDF，或打开 Gemini 原始页面。
5. 通过账号导出弹窗创建完整备份或 Kelivo 兼容归档。

同步和导出任务可以安全停止。中断或失败后可以重新执行，不会丢弃已经写入的本地归档。

## 开发

项目源码分布与本地专用目录见[项目结构文档](./docs/PROJECT_STRUCTURE.md)。

### 环境要求

- Node.js 20.19+（或 22.12+）
- npm
- 当前稳定版 Rust 工具链
- 对应平台的 [Tauri 2 系统依赖](https://v2.tauri.app/start/prerequisites/)
- Windows 需要 WebView2

### 本地运行

```bash
npm install
npm run tauri dev
```

在 macOS/Linux Shell 中也可以使用 `./dev.sh` 与 `./stop.sh` 快速重启或停止开发实例。

### 质量检查

```bash
npm run check       # TypeScript 类型检查
npm run build       # 前端生产构建
npm test            # Rust 单元测试与集成测试目标
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
```

依赖真实 Gemini 网络与账号的 API 集成测试默认忽略；常规单元测试不需要账号。

### 生产构建

```bash
npm run tauri build
```

macOS 可用 `./build_dmg.sh` 构建应用并将 DMG 写入 `release/`。正式发布所需的开发者签名与公证仍由分发者负责。

## 架构概览

| 模块 | 主要路径 | 职责 |
| --- | --- | --- |
| React 界面 | `src/App.tsx`、`src/components/` | 账号、导航、阅读器、设置、导出弹窗和本地化 |
| UI 基础 | `src/theme.ts`、`src/index.css`、`src/i18n.ts` | 主题令牌、通用交互样式、中英文资源 |
| Tauri 命令 | `src-tauri/src/lib.rs` | 桌面生命周期与前端命令入口 |
| 同步任务 | `worker_host.rs`、`sync.rs`、`scheduler.rs` | 任务队列、后台调度、增量/完整同步和取消 |
| Gemini 协议 | `gemini_api/`、`protocol.rs` | 鉴权请求、响应解析和媒体下载 |
| 本地数据 | `storage.rs`、`search.rs`、`settings.rs` | JSONL 归档、媒体、Tantivy 索引、文件夹和偏好设置 |
| 导入导出 | `import.rs`、`legacy_migration.rs`、`export.rs` | 账号备份恢复、旧版迁移、Markdown 与 Kelivo 转换 |

每个账号的本地归档通常包含元数据、对话 JSONL、自动生成的 Markdown、媒体资源和可重建的搜索索引。同步或导入任务运行时请不要手动编辑这些文件。

## 常见问题

- **macOS 找不到账号**：确认 Chrome 中可正常访问 Gemini，再点击“重新检测账号”。如果诊断提示权限问题，请授予完全磁盘访问，并允许读取 Chrome Safe Storage 钥匙串项目。
- **Windows 找不到账号**：使用应用内 Google 登录，并等待 Gemini 页面加载完成。
- **媒体缺失**：重新同步对应对话。失败记录会保留，并在后续同步中重试。
- **搜索结果没有更新**：完成一次同步；本地索引会在对话写入后更新，也可以从归档重新构建。
- **升级后仍显示旧图标或旧界面**：完全退出所有旧 Chat Vault 进程，再用新版覆盖“应用程序”中的旧安装包。v3 首次启动会清理一次 WebView 静态缓存；前端脚本、样式和品牌图片也使用版本化文件名，后续升级不应混用旧资源。

## 致谢

Chat Vault 基于 [FirenzeLor/gemini-collector](https://github.com/FirenzeLor/gemini-collector) 与 [Nagi-ovo/gemini-voyager](https://github.com/Nagi-ovo/gemini-voyager) 的开源成果和设计思路构建。感谢原作者与所有贡献者。

## 许可

本仓库采用 [GNU AGPL-3.0-only](./LICENSE)。请阅读[商业许可说明](./COMMERCIAL-LICENSE.md)了解上游权利边界；该说明本身不授予替代许可。
