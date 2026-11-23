# EasyNote

EasyNote 是一款把**待办事项和便签放在同一桌面窗口**的轻量应用。它适合随手记录想法、安排有时间要求的事项，并在需要时通过系统托盘快速找回。应用数据保存在本机，无需账号。

> 当前发布的成品为 **Windows x64 版**。源码使用 Tauri 2 构建；其他平台尚未提供预编译安装包。

## 下载

| 版本 | 下载 | 用途 |
| --- | --- | --- |
| 安装版 | [EasyNote_1.0.0_x64_setup.exe](https://github.com/MonQian/EasyNote/releases/download/v1.0.0/EasyNote_1.0.0_x64_setup.exe) | 常规安装，包含卸载程序 |

完整版本信息及源码压缩包见 [v1.0.0 发布页](https://github.com/MonQian/EasyNote/releases/tag/v1.0.0)。

检查更新功能已加入主分支的 1.0.1 源码；上方 1.0.0 安装包暂不包含此功能。

## 主要功能

- **桌面便签**：在多个便签页之间切换，自动保存文字；可调整便签高度、窗口宽度和位置。
- **待办管理**：添加、编辑、完成或删除待办；设置备注、日期时间、时长、优先级及工作、学习、生活等类型。支持每天、每周、每月重复，并可指定结束日期。
- **提醒**：可选提前提醒和自定义提醒时间；通过托盘切换弹窗、声音或两者结合，并调整音量和提示音。
- **整理与导出**：按时间、优先级或类型排序；设置待办自动清理条件与保留时间；将历史待办导出为 Excel、Word 或 PDF。
- **外观与托盘**：雾白、夜岩、暖纸三种主题；调整透明度、窗口层级和待办/便签区域显示状态，并可设置开机自启。
- **检查更新**：从系统托盘打开独立的检查更新窗口，比较当前版本与 GitHub 最新发布版本；发现新版本时可打开下载页。

## 技术栈与数据

| 部分 | 技术 |
| --- | --- |
| 桌面应用与系统功能 | Tauri 2、Rust |
| 界面 | TypeScript、原生 HTML/CSS、Vite |
| 本地存储 | JSON（便签及设置）、SQLite（待办及历史记录） |

数据文件 `notes.json` 和 `todos.sqlite3` 位于系统分配的 Tauri 应用数据目录。升级前建议备份这两个文件。早期 Tkinter 原型的数据格式与当前版本不同，未提供自动迁移。

## 开发环境

- Node.js 与 npm
- Rust 工具链
- Windows 构建需要 Microsoft C++ Build Tools 和 WebView2。其他平台的依赖请参阅 [Tauri 2 环境要求](https://v2.tauri.app/start/prerequisites/)。

## 从源码启动与构建

```powershell
git clone https://github.com/MonQian/EasyNote.git
cd EasyNote
npm install
npm run tauri dev
```

构建桌面应用：

```powershell
npm run tauri build
```

仅检查并构建前端时运行 `npm run build`。桌面安装包由 Tauri 构建流程生成，具体输出位置以命令行结果为准。仓库中的 `release/` 用于本地发布文件，未纳入 Git；下载已发布的成品请使用上方链接。

## 项目结构

| 路径 | 内容 |
| --- | --- |
| `src/main.ts`、`src/style.css` | 主窗口交互与样式 |
| `src/add-todo.ts`、`add-todo.html` | 新增和编辑待办窗口 |
| `src/export.ts`、`export.html` | 历史待办导出窗口 |
| `src/update.ts`、`update.html` | 独立的检查更新窗口 |
| `src-tauri/src/lib.rs` | Tauri 命令、托盘、提醒及窗口管理 |
| `src-tauri/src/storage.rs`、`src-tauri/src/export.rs` | SQLite 存储与文档导出 |
| `src-tauri/tauri.conf.json`、`src-tauri/capabilities/` | 应用配置与窗口权限 |
| `src-tauri/icons/`、`src-tauri/nsis-hooks.nsh` | 图标与 Windows 安装脚本 |

## 当前限制

- 发布页目前只提供 Windows x64 成品。
- 检查更新需要访问 GitHub；发现新版本后打开发布页手动下载安装。
