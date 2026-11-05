# Videleum 开发规范与架构说明

## 1. 项目定位与架构

本仓库（**Videleum**，当前版本 `0.1.0`）是面向 **macOS (`arm64` / `x64`) 与 Windows (`x64`)** 的**原生桌面端多平台视频采集、归档与媒体管理中心**。
本项目已历经全面架构与底层重构，仅在功能目标上有所参考，代码、版权及架构完全独立。

> **核心架构原则**：
> 1. **放弃 Linux 与 Docker / Docker Compose**，专注 **macOS 与 Windows 原生桌面应用程序**。
> 2. **采用 Rust GPUI 与 GPUI Kit 构建原生桌面客户端**（`desktop/`），结合内置 Node/TypeScript 极速后端服务底座，支持 macOS 顶部状态栏与 Windows 系统托盘后台驻留。
> 3. **品牌与版本规范**：项目全局统一命名为 **Videleum**，当前版本号为 **`0.1.0`**。严禁在界面或源码中引入遗留名称或第三方版权声明。

---

## 2. 核心架构与功能特性

### 2.1 桌面端原生 UI 架构（GPUI + GPUI Kit）与平台支持
- **Rust GPUI 客户端 (`desktop/`)**：
  - 基于 Rust `gpui` + `gpui-component` 构建 macOS 与 Windows 原生 GPU 加速桌面窗口；
  - 客户端内置管理后端服务进程生命周期，支持窗口关闭后驻留于 **macOS 顶部状态栏 (`NSStatusItem`)** 或 **Windows 系统托盘 (`NotifyIcon`)**；
  - 拥有专属应用图标配置：macOS `AppIcon.icns` 与 Windows `app.ico`。

### 2.2 视频存储路径外置与自定义
- 默认视频下载目录位于系统用户媒体目录：
  - **macOS**：`~/Movies/Videleum`
  - **Windows**：`%USERPROFILE%\Videos\Videleum`
- 应用配置与数据库存储于系统标准数据目录：
  - **macOS**：`~/Library/Application Support/Videleum`
  - **Windows**：`%APPDATA%\Videleum`
- 数据库主文件命名为 `videleum.db`；
- 设置页面支持直观查看、修改下载保存目录以及一键在访达/资源管理器中打开。

### 2.3 平台原生下载与流式拉取
- **Bilibili 原生下载引擎**（`bilibiliStreamDownloader.ts`）：
  - 使用 Bilibili Web API 与 WBI 签名接口获取高清 DASH 音视频流；
  - HTTP 流式并发拉取，精准计算下载速度、进度与总大小；
  - 使用 `FFmpeg` (`-c copy -movflags +faststart`) 无损混流音视频。
- **Cookie 独立隔离管理**：
  - 独立管理 YouTube Cookies (`youtube_cookies.txt`) 与 Bilibili Cookies (`bilibili_cookies.txt`)；
  - 缺失 YouTube Cookie 触发机器人检测时提供友好的前台上传指引。

---

## 3. 核心数据契约

规范身份的基本键：

```text
(sourceVideoId, platform, mediaType)
```

- `platform` 规范值：`youtube`、`bilibili`、`twitch`、`missav`、`local`、`cloud`、`other`、`unknown`。
- `mediaType` 只能是 `video` 或 `audio`。
- `partNumber` 随分集任务保留。
- `sourceVideoId` 为来源 URL 解析结果。

---

## 4. 质量与测试验证

```bash
# Rust GPUI 桌面端编译与测试
cd desktop
cargo check
cargo test

# 后端测试
cd ../backend
npm test

# 前端测试与代码规范检查
cd ../frontend
npm test

# 全局检查
cd ..
npm run typecheck
npm run lint
```

---

## 5. 工程约束

- 严禁硬编码敏感凭据与令牌。
- 遵循不可变更新，避免原地修改已有对象。
- 数据库查询必须使用参数化 API，杜绝 SQL 注入。
- 新增代码注释与文档必须遵循中文规范。
