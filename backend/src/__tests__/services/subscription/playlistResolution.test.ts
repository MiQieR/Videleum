import { describe, expect, it } from "vitest";
import {
    detectPlaylistPlatform,
    deriveChannelName,
    extractYouTubePlaylistId,
    getYouTubeSpecialPlaylistTitle,
    isYouTubeSpecialPlaylistId,
    parseBilibiliSpecialPlaylist,
    sanitizePlaylistTitle,
    toPlaylistsTabUrl,
} from "../../../services/subscription/playlistResolution";

// These helpers reach into storageService / ytDlpUtils only via the heavier
// functions (resolve*/extract*). The pure helpers tested here don't need mocks.

describe("subscription playlistResolution (pure helpers)", () => {
  describe("extractYouTubePlaylistId", () => {
    it("extracts the list= param when present", () => {
      expect(
        extractYouTubePlaylistId("https://youtube.com/playlist?list=PLabc123")
      ).toBe("PLabc123");
      expect(
        extractYouTubePlaylistId("https://youtu.be/x?v=x&list=PL_def-456")
      ).toBe("PL_def-456");
    });

    it("returns null when no list= param is present", () => {
      expect(extractYouTubePlaylistId("https://youtube.com/watch?v=abc")).toBeNull();
    });
  });

  describe("detectPlaylistPlatform", () => {
    it("detects Bilibili URLs", () => {
      expect(detectPlaylistPlatform("https://www.bilibili.com/video/BV1xx")).toBe("Bilibili");
    });

    it("defaults to YouTube for non-Bilibili URLs", () => {
      expect(detectPlaylistPlatform("https://youtube.com/playlist?list=x")).toBe("YouTube");
    });
  });

  describe("toPlaylistsTabUrl", () => {
    it("appends /playlists when missing", () => {
      expect(toPlaylistsTabUrl("https://youtube.com/@chan")).toBe(
        "https://youtube.com/@chan/playlists"
      );
    });

    it("preserves a trailing slash when appending", () => {
      expect(toPlaylistsTabUrl("https://youtube.com/@chan/")).toBe(
        "https://youtube.com/@chan/playlists"
      );
    });

    it("leaves a URL that already targets playlists unchanged", () => {
      expect(toPlaylistsTabUrl("https://youtube.com/@chan/playlists")).toBe(
        "https://youtube.com/@chan/playlists"
      );
    });
  });

  describe("sanitizePlaylistTitle", () => {
    it("replaces filesystem-unsafe characters with dashes", () => {
      expect(sanitizePlaylistTitle('Bad:Name?')).toBe("Bad-Name-");
      expect(sanitizePlaylistTitle('a/b\\c:d*e?f"g<h>i|j')).toBe("a-b-c-d-e-f-g-h-i-j");
    });

    it("falls back to Untitled Playlist for empty input", () => {
      expect(sanitizePlaylistTitle("")).toBe("Untitled Playlist");
      expect(sanitizePlaylistTitle(undefined as unknown as string)).toBe("Untitled Playlist");
    });

    it("trims surrounding whitespace", () => {
      expect(sanitizePlaylistTitle("  Hello  ")).toBe("Hello");
    });
  });

  describe("deriveChannelName", () => {
    it("prefers uploader", () => {
      expect(deriveChannelName({ uploader: "Up", channel: "Ch" }, "u")).toBe("Up");
    });

    it("falls back to channel when uploader is absent", () => {
      expect(deriveChannelName({ channel: "Ch" }, "u")).toBe("Ch");
    });

    it("falls back to the first entry's uploader/channel", () => {
      expect(
        deriveChannelName(
          { channel_id: "x", entries: [{ uploader: "EntryUp" }] },
          "u"
        )
      ).toBe("EntryUp");
    });

    it("falls back to the URL handle when nothing else is present", () => {
      expect(
        deriveChannelName({}, "https://youtube.com/@Handle/videos")
      ).toBe("@Handle");
    });

    it("returns Unknown when nothing can be derived", () => {
      expect(deriveChannelName({}, "https://youtube.com/watch?v=x")).toBe("Unknown");
    });
  });

  describe("YouTube and Bilibili special playlists", () => {
    it("recognizes YouTube special playlist IDs (LL, WL)", () => {
      expect(isYouTubeSpecialPlaylistId("LL")).toBe(true);
      expect(isYouTubeSpecialPlaylistId("WL")).toBe(true);
      expect(isYouTubeSpecialPlaylistId("LM")).toBe(true);
      expect(isYouTubeSpecialPlaylistId("PL_regular_123")).toBe(false);
      expect(isYouTubeSpecialPlaylistId(null)).toBe(false);

      expect(getYouTubeSpecialPlaylistTitle("LL")).toBe("Liked Videos");
      expect(getYouTubeSpecialPlaylistTitle("WL")).toBe("Watch Later");
    });

    it("parses Bilibili favorite lists and watchlater URLs", () => {
      const fav = parseBilibiliSpecialPlaylist(
        "https://space.bilibili.com/12345/favlist?fid=888999"
      );
      expect(fav).toEqual({
        type: "favlist",
        fid: "888999",
        mid: "12345",
        title: "Bilibili 收藏夹 888999",
      });

      const medialist = parseBilibiliSpecialPlaylist(
        "https://www.bilibili.com/medialist/play/ml777888"
      );
      expect(medialist).toEqual({
        type: "favlist",
        fid: "777888",
        mid: undefined,
        title: "Bilibili 收藏夹 777888",
      });

      const watchlater = parseBilibiliSpecialPlaylist(
        "https://www.bilibili.com/watchlater/#/list"
      );
      expect(watchlater).toEqual({
        type: "watchlater",
        title: "稍后再看",
      });

      expect(parseBilibiliSpecialPlaylist("https://youtube.com/watch?v=x")).toBeNull();
    });
  });
});
