import { describe, expect, it } from "vitest";
import { selectBilibiliMediaStreams } from "../../../services/downloaders/bilibili/bilibiliStreamDownloader";

describe("bilibiliStreamDownloader", () => {
  describe("selectBilibiliMediaStreams", () => {
    it("selects highest resolution DASH video and best audio when no height cap is set", () => {
      const selected = selectBilibiliMediaStreams(
        {
          dash: {
            video: [
              {
                id: 64,
                baseUrl: "https://upos.bilivideo.com/720p.m4s",
                backupUrl: ["https://mirror.bilivideo.com/720p.m4s"],
                bandwidth: 2000000,
                codecs: "avc1.64001f",
                width: 1280,
                height: 720,
              },
              {
                id: 80,
                baseUrl: "https://upos.bilivideo.com/1080p.m4s",
                backupUrl: ["https://mirror.bilivideo.com/1080p.m4s"],
                bandwidth: 4500000,
                codecs: "avc1.640028",
                width: 1920,
                height: 1080,
              },
            ],
            audio: [
              {
                id: 30216,
                baseUrl: "https://upos.bilivideo.com/audio64k.m4s",
                bandwidth: 64000,
              },
              {
                id: 30280,
                baseUrl: "https://upos.bilivideo.com/audio192k.m4s",
                bandwidth: 192000,
              },
            ],
          },
        },
        {}
      );

      expect(selected.mode).toBe("dash");
      expect(selected.videoStream?.height).toBe(1080);
      expect(selected.videoStream?.width).toBe(1920);
      expect(selected.videoStream?.baseUrl).toBe(
        "https://upos.bilivideo.com/1080p.m4s"
      );
      expect(selected.audioStream?.baseUrl).toBe(
        "https://upos.bilivideo.com/audio192k.m4s"
      );
    });

    it("respects preferredHeight cap when selecting DASH video stream", () => {
      const selected = selectBilibiliMediaStreams(
        {
          dash: {
            video: [
              {
                id: 64,
                baseUrl: "https://upos.bilivideo.com/720p.m4s",
                bandwidth: 2000000,
                codecs: "avc1.64001f",
                width: 1280,
                height: 720,
              },
              {
                id: 80,
                baseUrl: "https://upos.bilivideo.com/1080p.m4s",
                bandwidth: 4500000,
                codecs: "avc1.640028",
                width: 1920,
                height: 1080,
              },
            ],
            audio: [
              {
                id: 30280,
                baseUrl: "https://upos.bilivideo.com/audio192k.m4s",
                bandwidth: 192000,
              },
            ],
          },
        },
        { preferredHeight: 720 }
      );

      expect(selected.mode).toBe("dash");
      expect(selected.videoStream?.height).toBe(720);
      expect(selected.videoStream?.baseUrl).toBe(
        "https://upos.bilivideo.com/720p.m4s"
      );
    });

    it("falls back to durl progressive stream when DASH is absent", () => {
      const selected = selectBilibiliMediaStreams(
        {
          durl: [
            {
              order: 1,
              url: "https://upos.bilivideo.com/progressive.mp4",
              backup_url: ["https://mirror.bilivideo.com/progressive.mp4"],
              size: 123456,
            },
          ],
        },
        {}
      );

      expect(selected.mode).toBe("durl");
      expect(selected.durlSegments?.[0]?.url).toBe(
        "https://upos.bilivideo.com/progressive.mp4"
      );
      expect(selected.audioStream).toBeUndefined();
    });

    it("selects only audio streams when audioOnly is true", () => {
      const selected = selectBilibiliMediaStreams(
        {
          dash: {
            video: [
              {
                id: 80,
                baseUrl: "https://upos.bilivideo.com/1080p.m4s",
                bandwidth: 4500000,
                width: 1920,
                height: 1080,
              },
            ],
            audio: [
              {
                id: 30280,
                baseUrl: "https://upos.bilivideo.com/audio192k.m4s",
                bandwidth: 192000,
              },
            ],
          },
        },
        { audioOnly: true }
      );

      expect(selected.mode).toBe("dash");
      expect(selected.videoStream).toBeUndefined();
      expect(selected.audioStream?.baseUrl).toBe(
        "https://upos.bilivideo.com/audio192k.m4s"
      );
    });
  });
});
