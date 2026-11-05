import os from "os";
import path from "path";
import { beforeEach, describe, expect, it, vi } from "vitest";

// Mock path and security
vi.mock("../../config/paths", async (importOriginal) => {
  const actual = await importOriginal<Record<string, any>>();
  const os = require("os");
  const path = require("path");
  const tempDir = path.join(os.tmpdir(), "platform-archive-integration-test");
  return {
    ...actual,
    VIDEOS_DIR: path.join(tempDir, "videos"),
    IMAGES_DIR: path.join(tempDir, "images"),
    IMAGES_SMALL_DIR: path.join(tempDir, "images_small"),
    SUBTITLES_DIR: path.join(tempDir, "subtitles"),
    AVATARS_DIR: path.join(tempDir, "avatars"),
    DATA_DIR: path.join(tempDir, "data"),
  };
});

vi.mock("../../utils/security", () => ({
  pathExistsSafeSync: vi.fn().mockReturnValue(true),
  pathExistsTrustedSync: vi.fn().mockReturnValue(true),
  resolveSafeChildPath: vi.fn((base: string, child: string) => `${base}/${child}`),
  ensureDirSafeSync: vi.fn(),
  writeFileSafeSync: vi.fn(),
  renameSafeSync: vi.fn(),
  copyFileSafeSync: vi.fn(),
  sanitizeFilename: vi.fn((name: string) => name),
}));

import { buildEpisodeNfo } from "../../services/mediaServerExport/nfoBuilders";
import {
  notifyConfiguredMediaServers,
} from "../../services/mediaServerExport/mediaServerClients";
import {
  buildContextFromBilibiliMetadata,
  buildContextFromYtDlpInfo,
} from "../../services/filenameTemplate/contextBuilder";
import { renderFilenameTemplate } from "../../services/filenameTemplate/renderer";
import { normalizeMediaIdentity } from "../../utils/mediaIdentity";
import {
  extractYouTubePlaylistId,
  getYouTubeSpecialPlaylistTitle,
  isYouTubeSpecialPlaylistId,
} from "../../services/subscription/playlistResolution";
import type { Video } from "../../services/storageService";

describe("Platform Media Archive End-to-End Integration (Phase 3)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("handles YouTube private Liked playlist detection and metadata export", async () => {
    // 1. 播放列表解析与 Cookies 需求识别
    const playlistId = extractYouTubePlaylistId("https://www.youtube.com/playlist?list=LL");
    expect(playlistId).toBe("LL");
    expect(isYouTubeSpecialPlaylistId(playlistId!)).toBe(true);
    expect(getYouTubeSpecialPlaylistTitle(playlistId!)).toBe("Liked Videos");

    // 2. 身份规范化
    const identity = normalizeMediaIdentity({
      platform: "youtube",
      sourceVideoId: "dQw4w9WgXcQ",
      mediaType: "video",
    });
    expect(identity.platform).toBe("youtube");
    expect(identity.sourceVideoId).toBe("dQw4w9WgXcQ");

    // 3. NFO 元数据导出校验
    const video: Partial<Video> = {
      id: "yt-video-1",
      title: "Rick Astley - Never Gonna Give You Up",
      author: "RickAstleyVEVO",
      source: "youtube",
      platform: "youtube",
      sourceVideoId: "dQw4w9WgXcQ",
      sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      date: "20091025",
      createdAt: "2026-05-01T00:00:00.000Z",
    };

    const nfo = buildEpisodeNfo({ video: video as Video, showTitle: "RickAstleyVEVO" });
    expect(nfo).toContain('<uniqueid type="youtube">dQw4w9WgXcQ</uniqueid>');
    expect(nfo).toContain('<platform>youtube</platform>');
    expect(nfo).toContain('<originalurl>https://www.youtube.com/watch?v=dQw4w9WgXcQ</originalurl>');
  });

  it("handles Bilibili multi-part video template rendering and NFO export", () => {
    // 1. 上下文构建
    const context = buildContextFromBilibiliMetadata(
      "https://www.bilibili.com/video/BV1xx411c7mD?p=3",
      {
        bvid: "BV1xx411c7mD",
        title: "全流程实战课程",
        owner: { name: "技术导师" },
        pubdate: 1714464000,
      },
      {
        partNumber: 3,
        sourceCollectionName: "全流程实战系列",
      }
    );

    expect(context.platform).toBe("bilibili");
    expect(context.partNumber).toBe(3);

    // 2. 高级模板渲染
    const template = "{{ platform }}/{{ source_collection_name }}/{{ title }}_P{{ part_number }}.{{ ext }}";
    const rendered = renderFilenameTemplate({
      template,
      context,
      mode: "video",
      extension: "mp4",
    });

    expect(rendered.relativePath).toBe("bilibili/全流程实战系列/全流程实战课程_P03.mp4");

    // 3. NFO 规范导出
    const video: Partial<Video> = {
      id: "bili-video-3",
      title: "全流程实战课程 P3",
      author: "技术导师",
      source: "bilibili",
      platform: "bilibili",
      sourceVideoId: "BV1xx411c7mD",
      partNumber: 3,
      sourceUrl: "https://www.bilibili.com/video/BV1xx411c7mD?p=3",
      createdAt: "2026-05-01T00:00:00.000Z",
    };

    const nfo = buildEpisodeNfo({
      video: video as Video,
      showTitle: "全流程实战系列",
      episodeNumber: 3,
    });
    expect(nfo).toContain('<uniqueid type="bilibili">BV1xx411c7mD</uniqueid>');
    expect(nfo).toContain('<platform>bilibili</platform>');
    expect(nfo).toContain('<episode>3</episode>');
  });

  it("handles Twitch audio-only vs video independent identities and refresh notification", async () => {
    // 1. Twitch 视频任务
    const videoIdentity = normalizeMediaIdentity({
      platform: "twitch",
      sourceVideoId: "vod123456",
      mediaType: "video",
    });

    // 2. Twitch 音频任务
    const audioIdentity = normalizeMediaIdentity({
      platform: "twitch",
      sourceVideoId: "vod123456",
      mediaType: "audio",
    });

    // 验证不同 mediaType 具备独立身份
    expect(videoIdentity.mediaType).toBe("video");
    expect(audioIdentity.mediaType).toBe("audio");
    expect(videoIdentity.sourceVideoId).toBe(audioIdentity.sourceVideoId);
    expect(videoIdentity.mediaType).not.toBe(audioIdentity.mediaType);

    // 3. 模板归档路径区分
    const audioContext = buildContextFromYtDlpInfo(
      "https://www.twitch.tv/videos/vod123456",
      {
        id: "vod123456",
        title: "Live Stream Audio",
        uploader: "TwitchGamer",
        vcodec: "none",
      },
      {
        platform: "twitch",
        mediaType: "audio",
      }
    );

    const template = "{{ platform }}/{{ media_type }}/{{ uploader }} - {{ title }}.{{ ext }}";
    const rendered = renderFilenameTemplate({
      template,
      context: audioContext,
      mode: "video",
      extension: "m4a",
    });

    expect(rendered.relativePath).toBe("twitch/audio/TwitchGamer - Live Stream Audio.m4a");

    // 4. 模拟媒体服务器通知
    const originalFetch = global.fetch;
    const mockFetch = vi.fn().mockResolvedValue({ ok: true, status: 200 });
    global.fetch = mockFetch as any;

    try {
      const results = await notifyConfiguredMediaServers({
        immediate: true,
        settings: {
          jellyfinEnabled: true,
          jellyfinServerUrl: "http://localhost:8096",
          jellyfinApiKey: "key-123",
          mediaServerAutoRefreshOnDownload: true,
        } as any,
      });

      expect(results).toHaveLength(1);
      expect(results[0].server).toBe("jellyfin");
      expect(results[0].success).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith(
        "http://localhost:8096/Library/Refresh",
        expect.objectContaining({
          method: "POST",
          headers: expect.objectContaining({ "X-Emby-Token": "key-123" }),
        })
      );
    } finally {
      global.fetch = originalFetch;
    }
  });
});
