import { describe, expect, it, vi } from "vitest";

// Mock paths before importing modules that use them
vi.mock("../../../config/paths", () => ({
  VIDEOS_DIR: "/mock/videos",
  IMAGES_DIR: "/mock/images",
  SUBTITLES_DIR: "/mock/subtitles",
  AVATARS_DIR: "/mock/avatars",
  DATA_DIR: "/mock/data",
}));

vi.mock("../../../utils/security", () => ({
  pathExistsSafeSync: vi.fn().mockReturnValue(false),
  resolveSafeChildPath: vi.fn((base: string, child: string) => `${base}/${child}`),
  ensureDirSafeSync: vi.fn(),
}));

import { renderFilenameTemplate } from "../../../services/filenameTemplate/renderer";
import {
  buildContextFromBilibiliMetadata,
  buildContextFromVideoRecord,
  buildContextFromYtDlpInfo,
} from "../../../services/filenameTemplate/contextBuilder";
import { validateTemplate } from "../../../services/filenameTemplate/validators";

describe("Filename Template Extended Placeholders (Phase 2)", () => {
  it("validates templates containing new placeholders without errors", () => {
    const template = "{{ platform }}/{{ uploader }}/{{ title }}_P{{ part_number }}_{{ media_type }}.{{ ext }}";
    const res = validateTemplate(template);
    expect(res.valid).toBe(true);
    expect(res.errors).toHaveLength(0);
  });

  it("renders platform, part_number, part, and media_type for Bilibili multipart video", () => {
    const context = buildContextFromBilibiliMetadata(
      "https://www.bilibili.com/video/BV1xx411c7mD?p=2",
      {
        bvid: "BV1xx411c7mD",
        title: "Part 2 Title",
        partNumber: 2,
        owner: { name: "UP 主" },
        pubdate: 1714464000,
      },
      {
        partNumber: 2,
      }
    );

    expect(context.platform).toBe("bilibili");
    expect(context.partNumber).toBe(2);
    expect(context.mediaType).toBe("video");

    const template = "{{ platform }}/{{ uploader }}/{{ title }}_P{{ part_number }}_part{{ part }}_{{ media_type }}.{{ ext }}";
    const rendered = renderFilenameTemplate({
      template,
      context,
      mode: "video",
      extension: "mp4",
    });

    expect(rendered.relativePath).toBe("bilibili/UP 主/Part 2 Title_P02_part2_video.mp4");
  });

  it("renders platform and audio media_type for YouTube audio download", () => {
    const context = buildContextFromYtDlpInfo(
      "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      {
        id: "dQw4w9WgXcQ",
        title: "Never Gonna Give You Up",
        uploader: "Rick Astley",
        upload_date: "20091025",
        vcodec: "none", // audio-only stream
      },
      {
        mediaType: "audio",
        platform: "youtube",
      }
    );

    expect(context.platform).toBe("youtube");
    expect(context.mediaType).toBe("audio");
    expect(context.partNumber).toBeNull();

    const template = "{{ platform }}/{{ media_type }}/{{ artist_name }} - {{ title }}.{{ ext }}";
    const rendered = renderFilenameTemplate({
      template,
      context,
      mode: "video",
      extension: "m4a",
    });

    expect(rendered.relativePath).toBe("youtube/audio/Rick Astley - Never Gonna Give You Up.m4a");
  });

  it("renders context correctly from an existing Video DB record", () => {
    const video = {
      id: "local-rec-1",
      title: "Twitch VOD Highlight",
      sourceUrl: "https://www.twitch.tv/videos/123456789",
      sourceVideoId: "123456789",
      source: "twitch",
      platform: "twitch",
      author: "StreamerName",
      createdAt: "2026-05-01T10:00:00.000Z",
      mediaType: "video" as const,
      partNumber: null,
    };

    const context = buildContextFromVideoRecord(video as any);
    expect(context.platform).toBe("twitch");
    expect(context.mediaType).toBe("video");
    expect(context.partNumber).toBeNull();

    const template = "{{ platform }}/{{ channel }}/{{ title }}.{{ ext }}";
    const rendered = renderFilenameTemplate({
      template,
      context,
      mode: "video",
      extension: "mp4",
    });

    expect(rendered.relativePath).toBe("twitch/StreamerName/Twitch VOD Highlight.mp4");
  });
});
