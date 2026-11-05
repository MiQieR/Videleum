<p align="center">
<img width="256" height="256" alt="logo" src="https://github.com/user-attachments/assets/cf900a36-8ed0-41f0-97b1-d330977eaa15" />
</p>

# Videleum

Native **macOS** & **Windows** desktop media collector, archiver, and streaming player for **YouTube**, **Bilibili**, **Twitch**, **MissAV**, and [yt-dlp sites](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md).

**Videleum** features a native **GPUI & GPUI Kit** desktop client for macOS and Windows, a direct Bilibili WBI/DASH stream engine (replacing `yt-dlp` for Bilibili), platform-separated Cookie management, a unified cross-platform media identity model (`(sourceVideoId, platform, mediaType)`), personal favorites & private playlist synchronization, Kodi/Jellyfin/Emby/Plex NFO metadata export & automatic library refresh notifications, multi-platform filename templates, and a streaming-center visual experience inspired by [bili-sync](https://github.com/amtoaer/bili-sync) and [Jellyfin](https://jellyfin.org/).

[![GitHub License](https://img.shields.io/github/license/MiQieR/Videleum)](LICENSE)
![Release Packages Workflow Status](https://img.shields.io/github/actions/workflow/status/MiQieR/Videleum/release-packages.yml?label=Release%20Packages)

[中文文档](README-zh.md) | [Changelog](CHANGELOG.md)

---

## What's New in Videleum

### 1. Native macOS & Windows Desktop Application (GPUI & GPUI Kit)
- **Built with Rust GPUI + GPUI Kit (`0.9.0`)**: Dedicated native desktop GUI window (`desktop/`) modelled after `bili-sync`, featuring a 4-section navigation sidebar (**Dashboard / Poster Wall**, **Download Queue & Quick Add**, **Subscriptions**, **Storage & Cookie Settings**), real-time download progress cards, and direct folder/storage controls.
- **Native macOS & Windows Experience**: Complete `NSApplication` / Win32 event loop with system tray and menu bar support, packaged with official icons (`AppIcon.icns` and `app.ico`).
- **Focused Platform Support**: Exclusively targets **macOS (`arm64` & `x64`)** and **Windows (`x64`)** native desktop packages.

### 2. Native Bilibili WBI + DASH Stream Engine
- **Direct Bilibili API + WBI Signing**: Fetches metadata via `x/web-interface/view`, signs DASH/durl play URLs with Bilibili's WBI mixin-key algorithm (`x/player/wbi/playurl`, `fnval=4048`), and downloads video/audio streams directly over HTTP with proper referer and cookies.
- **Real-Time Progress & FFmpeg Muxing**: Reports live percentage, transfer speed, downloaded bytes, and total size during stream download, then remuxes video and audio streams cleanly via `ffmpeg -c copy`.

### 3. Platform-Separated Cookies & Friendly Auth Prompts
- **Independent Cookie Storage**: Separates **YouTube Cookies** (`youtube_cookies.txt`) and **Bilibili Cookies** (`bilibili_cookies.txt`) in **Settings → Security & Access → Cookie Settings** with independent upload, status inspection, and deletion controls.
- **Smart Bot-Check Guidance**: Automatically detects YouTube bot-verification errors (`Sign in to confirm you’re not a bot`) when downloading without cookies and displays a clear reminder guiding users to upload YouTube cookies.

### 4. User-Accessible & Configurable Download Storage Directory
- **Visible Default Paths**: Stores downloaded media in user-friendly system directories by default (`~/Movies/Videleum` on macOS, `%USERPROFILE%\Videos\Videleum` on Windows).
- **Dynamic Runtime Reconfiguration**: View the current path, switch to any custom directory without restarting, and open the media folder directly in Finder / Explorer from **Settings → Basic Settings** or the GPUI desktop window.

### 5. Unified Multi-Platform Media Identity & Queue Self-Healing
- **Canonical Identity Key**: Every media item is uniquely tracked by `(sourceVideoId, platform, mediaType)`, strictly separating the canonical platform (`youtube`, `bilibili`, `twitch`, `missav`, `local`, `cloud`, `other`) from the underlying downloader engine.
- **Independent Video & Audio Identities**: Downloading audio-only (`audio`) and full video (`video`) from the same source URL creates independent library records without overwriting or false deduplication.
- **Bilibili Multi-Part Persistence**: Preserves `partNumber` and `totalParts` across active downloads, retries, and download history.

### 6. Personal Favorites, Media Server Integration & Streaming Visuals
- **Personal Favorites & Private Playlists**: Supports YouTube private playlists (`Liked Videos` `list=LL`, `Watch Later` `list=WL`, `Liked Music` `list=LM`) and Bilibili favorite folders (`favlist?fid=` / `/ml{fid}`) and Watch Later (`watchlater`).
- **Jellyfin / Emby / Plex NFO & Auto-Refresh**: Exports `<tvshow>`, `<episodedetails>`, and `<movie>` NFO files with `<uniqueid type="youtube|bilibili|twitch">`, and triggers automatic library scan notifications on configured media servers.
- **Streaming-Center Visuals**: Proper cased brand badges (**`YouTube`** Red `#FF0000`, **`Bilibili`** Logo Blue `#00AEEC`, **`Twitch`** Logo Purple `#9146FF`), bottom playback progress bars, top platform filter pills, and clean poster walls.

---

## Core Features

- **Video & Audio Downloading**: Download YouTube, Bilibili, Twitch, MissAV and other supported videos or audio streams with a simple URL input.
- **Video Upload**: Upload local video files directly to your library with automatic thumbnail generation.
- **TMDB Metadata Scraping**: Automatically scrape movie and TV show metadata from TMDB based on filename.
- **Parallel & Batch Downloads**: Queue multiple downloads and track their progress simultaneously.
- **Cloud Storage Integration**: Automatically upload videos and thumbnails to cloud storage (OpenList/Alist) after download.
- **Media Integrity Check** (admin): Detect missing, truncated, or unreadable files under **Settings → Library & Storage**.
- **Auto Subtitles & Live Audio Translation**: Automatically download YouTube / Bilibili subtitles, with optional real-time Gemini Live Audio Translation.
- **Channel & Playlist Subscriptions**: Automatically poll and download new uploads from YouTube, Bilibili, and Twitch channels or playlists.
- **Private RSS Feeds**: Create private RSS links for external feed readers with per-link filters.
- **Login Protection & Visitor Access**: Password login, Passkeys (WebAuthn), admin 3×3 Gesture Login, and read-only named visitor accounts.
- **Internationalization**: 10 languages (English, Chinese, Spanish, French, German, Japanese, Korean, Arabic, Portuguese, Russian).
- **Browser Extension**: Chrome extension for one-click video downloading directly from your browser.

---

## Quick Start

### Option 1: Standalone Desktop Installers (macOS / Windows)

Download the latest release package for your operating system from GitHub Releases:

- **macOS (`arm64` / `x64`, ad-hoc signed)**: Open `Videleum-v<version>-macos-<arch>.dmg`, drag `Videleum.app` to `Applications`, and run `xattr -cr /Applications/Videleum.app` on first launch if Gatekeeper prompts.
- **Windows (`x64`)**: Run `Videleum-Setup-v<version>-windows-x64.exe` or extract `Videleum-v<version>-windows-x64.zip` and launch `Videleum.exe`.

### Option 2: From Source

**Requirements:** Node.js **22.22+, 24.x, or 26.x**, npm v9+, and Rust toolchain (for `desktop/`).

```bash
git clone https://github.com/MiQieR/Videleum.git
cd Videleum
npm run install:all
npm run dev

# Build / launch the GPUI native desktop client:
cd desktop
cargo run --release
```

---

## Disclaimer

- **Purpose and Restrictions**: This software (including code and documentation) is intended solely for personal learning, research, and technical exchange. It is strictly prohibited to use this software for any commercial purposes or for any illegal activities that violate local laws and regulations.
- **Liability**: Any legal liabilities, disputes, or damages arising from the illegal or improper use of this software (including but not limited to copyright infringement) shall be borne solely by the user. The developers assume no direct, indirect, or joint liability.

## Acknowledgements & References

Videleum has undergone an extensive architectural and infrastructural redesign into a native GPU-accelerated desktop application for macOS and Windows. We gratefully acknowledge and reference the following outstanding open-source projects for their inspiration and technological contributions:

- **[MyTube](https://github.com/franklioxygen/MyTube)**: Referenced for high-level functional concepts of media archiving, video ingestion workflows, and web management interfaces.
- **[bili-sync](https://github.com/amtoaer/bili-sync)**: Inspired the native desktop application architecture, dashboard & sidebar design, and Bilibili direct WBI/DASH stream download design.
- **[yt-dlp](https://github.com/yt-dlp/yt-dlp)**: Provides universal media extraction capabilities for thousands of video platforms.
- **[FFmpeg](https://ffmpeg.org/)**: Powers high-performance lossless stream muxing, remuxing, and metadata processing.
- **[GPUI & Zed](https://github.com/zed-industries/zed)**: Provides the high-performance GPU-accelerated desktop UI framework for Rust.
- **[gpui-component](https://github.com/longbridge/gpui-component)**: Component kit powering the desktop GUI elements.

## License

MIT License — see [LICENSE](LICENSE) for details.
