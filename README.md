# Videleum

Videleum is a native desktop client for building and browsing a personal video
archive. It downloads videos from YouTube, Bilibili and Twitch, stores the media
and its metadata in a portable library folder, and presents the result as a
poster wall rather than a file browser.

Built with Rust and [GPUI Kit](https://github.com/longbridge/gpui-component).
There is no browser UI, local server or Docker dependency.

## Features

- Add any YouTube, Bilibili or Twitch video URL.
- Follow YouTube channels, playlists and favorites lists, Bilibili creators,
  and Twitch channels. New uploads can be downloaded automatically.
- Downloads include the video, cover, creator avatar, NFO metadata, subtitles
  and Bilibili danmaku converted to ASS.
- Poster-wall library with search, platform and collection filters, sorting,
  favorites and watched state.
- Detail page with playback, description, tags, captured comments, file list
  and technical metadata.
- One portable catalogue per library: `<library>/.videleum/videleum.db`.
- Configurable library folder, quality, naming template, download concurrency,
  metadata capture and subscription checks.

## Requirements

- Rust 1.85 or newer (edition 2024).
- macOS or Windows.
- `yt-dlp` for YouTube and Twitch downloads. Videleum can install it into its
  app-data tools folder from Settings.
- `ffmpeg` for merging Bilibili DASH streams and remuxing. Point Settings at an
  existing executable if it is not on `PATH`.

Bilibili downloads use Bilibili's native APIs and do not go through `yt-dlp`.

## Build and run

```sh
cargo run -p videleum
```

Release build:

```sh
cargo build --release -p videleum
```

Tests:

```sh
cargo test -p videleum-core -p videleum-fetcher
```

## Library layout

Videleum writes files next to each other inside the configured library folder:

```
<library>/
  .videleum/videleum.db
  <creator>/<video title> [<source id>]/
    <video>.mp4
    poster.jpg
    avatar.jpg
    tvshow.nfo
    subtitles/
    danmaku.ass
```

The naming template in Settings controls the folder and file names. The
catalogue lives inside the library, so the whole folder can be moved or copied
between machines.

Settings are stored separately at the platform app-data location
(`Videleum/settings.json`), because they describe this installation rather than
the archive.

## Cookies

Private, age-restricted or member-only videos need a Netscape cookies file for
the matching platform. Choose cookie files per platform in Settings. They stay
on the local machine and are passed only to the matching downloader.

For development, test cookie files can live under `dev/cookies/`, which is
git-ignored. Never commit cookie files.

## Environment overrides

Useful for portable setups and testing:

- `VIDELEUM_LIBRARY`: library folder.
- `VIDELEUM_YTDLP`: path to `yt-dlp`.
- `VIDELEUM_FFMPEG`: path to `ffmpeg`.
- `VIDELEUM_YT_COOKIES`: YouTube cookies file.
- `VIDELEUM_BILI_COOKIES`: Bilibili cookies file.
- `VIDELEUM_TWITCH_COOKIES`: Twitch cookies file.

## Workspace layout

- `crates/core`: platform parsing, settings, catalogue, archive layout, NFO and
  danmaku generation.
- `crates/fetcher`: YouTube/Twitch via `yt-dlp`, native Bilibili API, HTTP
  helpers, subscription feeds and the download pipeline.
- `crates/app`: GPUI desktop application, theme, shared components and views.

## Packaging notes

The binary embeds the Lucide icon set and Videleum brand artwork, so the
executable runs without external UI assets. Ship `yt-dlp` and `ffmpeg` either
as system dependencies or point users at them from Settings. Cookies are always
user-provided and must remain outside the application bundle.
