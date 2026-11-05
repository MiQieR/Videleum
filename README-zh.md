<p align="center">
<img width="256" height="256" alt="logo" src="https://github.com/user-attachments/assets/cf900a36-8ed0-41f0-97b1-d330977eaa15" />
</p>

# Videleum

面向 **macOS** 与 **Windows** 的跨平台原生桌面媒体采集、归档与流媒体播放中心，支持 **YouTube**、**Bilibili**、**Twitch**、**MissAV** 及 [yt-dlp 站点](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md)。

**Videleum** 采用 Rust **GPUI + GPUI Kit** 构建自带原生界面的桌面客户端，内置原生 Bilibili WBI 签名与 DASH 流下载引擎（彻底弃用 `yt-dlp` 的 Bilibili 下载方案）、YouTube/Bilibili 独立 Cookie 管理、可自定义且默认直观可见的下载存储目录、统一的跨平台媒体身份模型（`(sourceVideoId, platform, mediaType)`）、个人收藏与私有播放列表同步、Kodi/Jellyfin/Emby/Plex 标准 NFO 元数据导出与媒体库自动刷新通知，以及深度对标 [bili-sync](https://github.com/amtoaer/bili-sync) 与 [Jellyfin](https://jellyfin.org/) 的现代化流媒体中心体验。

[![GitHub License](https://img.shields.io/github/license/MiQieR/Videleum)](LICENSE)
![桌面端安装包构建状态](https://img.shields.io/github/actions/workflow/status/MiQieR/Videleum/release-packages.yml?label=Release%20Packages)

[English](README.md) | [更新日志](CHANGELOG.md)

---

## Videleum 核心特性

### 1. 原生 macOS & Windows 桌面应用（基于 GPUI & GPUI Kit）
- **Rust GPUI + GPUI Kit (`0.9.0`) 原生桌面 UI**：在 `desktop/` 中提供现代化原生桌面窗口应用，包含四大核心工作区（**媒体概览与海报墙**、**下载队列与快速投递**、**收藏夹与频道订阅**、**存储目录与平台 Cookie 设置**），无需依赖浏览器即可直接管理与使用。
- **专为 macOS 与 Windows 打造**：打包完整的 `AppIcon.icns`（macOS）与 `app.ico`（Windows）高清应用图标，支持系统托盘/状态栏驻留。
- **聚焦桌面端体验**：专攻 **macOS (`arm64` / `x64`)** 与 **Windows (`x64`)** 原生桌面软件体验。

### 2. 原生 Bilibili WBI + DASH 下载引擎（彻底弃用 `yt-dlp` Bilibili 方案）
- **Bilibili 官方 Web API + WBI 签名直连**：参考 `amtoaer/bili-sync` 与 `lanyeeee/bilibili-video-downloader`，通过 `x/web-interface/view` 获取分 P 与完整视频元数据，使用 WBI 混淆密钥签名请求 `x/player/wbi/playurl`（`fnval=4048`）解析最高画质 DASH 音视频流与备用 `durl` 流。
- **实时速度/进度回调与 FFmpeg 混流**：直连 HTTP 流式下载（自动携带 `Referer: https://www.bilibili.com/` 与 `SESSDATA` Cookie），实时上报下载百分比、速率（`B/s`）、已下载大小与总大小，下载完成后通过 `ffmpeg -c copy` 无损合并并写入标准媒体库。

### 3. YouTube 与 Bilibili 分离式 Cookie 管理与智能提醒
- **分平台独立 Cookie 卡片**：在 **设置 → 安全与访问 → Cookie 设置** 中拆分为独立的 **YouTube Cookies**（`youtube_cookies.txt`）与 **Bilibili Cookies**（`bilibili_cookies.txt`）上传、状态检测与删除面板。
- **YouTube 机器人验证友好提示**：未配置 YouTube Cookie 下载触发机器人验证（`Sign in to confirm you’re not a bot`）时，自动识别并展示友好提示，引导用户前往设置上传 YouTube Cookie。

### 4. 默认可见且支持自定义的视频下载存储目录
- **桌面端标准用户目录默认值**：macOS 默认存储在 `~/Movies/Videleum`，Windows 默认存储在 `%USERPROFILE%\Videos\Videleum`，打开访达或资源管理器即可直接找到下载好的视频与封面文件。
- **设置页可视化修改与一键打开目录**：在 **设置 → 基本设置 → 媒体下载存储目录** 及桌面端设置页中实时显示当前下载路径，支持随时修改并立即生效，同时提供「在文件管理器中打开」快捷按钮。

### 5. 跨平台统一媒体身份模型与队列自愈
- **规范化三元组身份**：以 `(sourceVideoId, platform, mediaType)` 作为核心唯一键，严格区分规范平台（`youtube`、`bilibili`、`twitch`、`missav`、`local`、`cloud`、`other`）与底层下载器实现。
- **音视频独立身份与 Bilibili 多分集持久化**：纯音频（`audio`）与视频（`video`）独立归档互不覆盖；完整保留 Bilibili `partNumber` 与 `totalParts`。

### 6. 流媒体中心级视觉与媒体服务器联动
- **标准大小写与品牌色角标**：海报墙卡片右上角显示 **`YouTube`**（红底白字 `#FF0000`）、**`Bilibili`**（Bilibili Logo 蓝底白字 `#00AEEC`）、**`Twitch`**（Twitch Logo 紫底白字 `#9146FF`）。
- **纯净海报墙浏览体验**：底部版权页脚仅在「设置」页面显示，主页海报墙与详情页保持沉浸式视觉体验。
- **Jellyfin / Emby / Plex NFO 导出与自动刷新**：支持生成 `<tvshow>`、`<episodedetails>` 及 `<movie>` NFO 文件，并在下载完成后触发自动刷新通知。

---

## 基础功能一览

- **音视频下载**：支持通过 URL 下载 YouTube、Bilibili、Twitch、MissAV 及 yt-dlp 支持站点的视频或纯音频。
- **本地上传**：直接上传本地视频文件并自动生成封面缩略图。
- **TMDB 元数据刮削**：根据文件名自动匹配 TMDB 电影与剧集元数据。
- **并发队列与批量下载**：支持多任务并发下载与实时进度追踪。
- **云存储集成**：支持下载完成后自动上传至 OpenList / Alist。
- **媒体完整性检查**（管理员）：自动扫描缺失、截断或损坏的媒体文件并支持一键重下。
- **字幕下载与实时语音翻译**：自动下载默认字幕，支持可选的 Google Gemini 实时语音翻译与双语字幕。
- **频道与列表订阅**：定时轮询 YouTube、Bilibili、Twitch 频道或播放列表并自动下载新内容。
- **私密 RSS 订阅源**：为外部阅读器生成带多维过滤条件的私密 RSS 链接。
- **多级安全登录与访客账户**：支持密码登录、Passkey 通行密钥（WebAuthn）、管理员 3×3 手势登录以及只读具名访客账户。
- **多语言国际化**：内置中、英、日、韩、西、法、德、俄、葡、阿 10 种界面语言。
- **Chrome 浏览器扩展**：支持在网页端一键发送视频到 Videleum 下载队列。

---

## 快速开始

### 方式一：原生桌面端安装包（macOS / Windows）

前往 Releases 下载对应系统的安装包：

- **macOS (`arm64` / `x64`, ad-hoc 签名)**：打开 `Videleum-v<version>-macos-<arch>.dmg`，将 `Videleum.app` 拖入 `Applications`；首次运行若遇系统提示可执行 `xattr -cr /Applications/Videleum.app`。
- **Windows (`x64`)**：运行 `Videleum-Setup-v<version>-windows-x64.exe` 安装向导，或解压 `Videleum-v<version>-windows-x64.zip` 后双击 `Videleum.exe`。

### 方式二：源码运行

**环境要求：** Node.js **22.22+、24.x 或 26.x**、npm v9+，以及 Rust 工具链（用于构建 `desktop/` 原生客户端）。

```bash
git clone https://github.com/MiQieR/Videleum.git
cd Videleum
npm run install:all
npm run dev

# 编译并启动 GPUI 原生桌面客户端：
cd desktop
cargo run --release
```

---

## 免责声明

- **使用目的与限制**：本软件（及相关代码、文档）仅供个人学习、研究及技术交流使用。严禁将本软件用于任何形式的商业用途，或利用本软件进行违反法律法规的活动。
- **责任界定**：因用户非法或不当使用本软件而产生的任何法律责任、纠纷或损失，均由用户自行承担，开发者不承担任何直接、间接或连带责任。

## 致谢与参考项目

Videleum 经历了全面的底层与架构重构，演进为面向 macOS 与 Windows 的原生 GPU 加速桌面端应用程序。在此对以下优秀的开源项目及其设计理念致以诚挚的谢意：

- **[MyTube](https://github.com/franklioxygen/MyTube)**：在流媒体归档、视频抓取流转及 Web 管理界面的功能目标上提供了重要的概念参考。
- **[bili-sync](https://github.com/amtoaer/bili-sync)**：为本项目原生桌面端架构、侧边导航与仪表盘界面风格，以及 Bilibili WBI DASH 直链流式下载提供了灵感来源。
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**：为多平台音视频提取提供了强大的底层解析支持。
- **[FFmpeg](https://ffmpeg.org/)**：为音视频无损混流、格式封装及媒体处理提供核心能力。
- **[GPUI & Zed](https://github.com/zed-industries/zed)**：提供了高性能 GPU 加速的原生桌面 UI 渲染底座。
- **[gpui-component](https://github.com/longbridge/gpui-component)**：为 Rust 桌面端界面提供了精美的组件库支持。

## 许可证

MIT License — 详见 [LICENSE](LICENSE)。
