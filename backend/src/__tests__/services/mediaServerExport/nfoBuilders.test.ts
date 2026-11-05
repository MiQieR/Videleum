import { describe, expect, it } from "vitest";
import {
  buildEpisodeNfo,
  buildMovieNfo,
  buildShowNfo,
  normalizeShowIdentifier,
  normalizeVideoDateToDay,
} from "../../../services/mediaServerExport/nfoBuilders";
import type { Video } from "../../../services/storageService";

describe("nfoBuilders", () => {
  const baseVideo: Video = {
    id: "v-12345",
    title: "Awesome Video",
    description: "Detailed video description",
    author: "Content Creator",
    date: "20261104",
    duration: "300",
    addedAt: "2026-11-04T12:00:00.000Z",
    createdAt: "2026-11-04T12:00:00.000Z",
    tags: ["tech", "guide"],
    sourceVideoId: "dQw4w9WgXcQ",
    sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    source: "YouTube",
    mediaType: "video",
  };

  describe("buildEpisodeNfo", () => {
    it("generates episodic NFO containing standard and platform uniqueid", () => {
      const xml = buildEpisodeNfo({
        video: baseVideo,
        showTitle: "Tech Channel",
        seasonNumber: 2026,
        episodeNumber: 1104,
        thumbFilename: "thumb.jpg",
      });

      expect(xml).toContain("<episodedetails>");
      expect(xml).toContain("<title>Awesome Video</title>");
      expect(xml).toContain("<showtitle>Tech Channel</showtitle>");
      expect(xml).toContain("<plot>Detailed video description</plot>");
      expect(xml).toContain("<season>2026</season>");
      expect(xml).toContain("<episode>1104</episode>");
      expect(xml).toContain("<aired>2026-11-04</aired>");
      expect(xml).toContain("<runtime>5</runtime>");
      expect(xml).toContain("<studio>Content Creator</studio>");
      expect(xml).toContain("<tag>tech</tag>");
      expect(xml).toContain("<tag>guide</tag>");
      expect(xml).toContain('<uniqueid type="videleum" default="true">videleum:video:v-12345</uniqueid>');
      expect(xml).toContain('<uniqueid type="youtube">dQw4w9WgXcQ</uniqueid>');
      expect(xml).toContain("<originalurl>https://www.youtube.com/watch?v=dQw4w9WgXcQ</originalurl>");
      expect(xml).toContain("<platform>youtube</platform>");
      expect(xml).toContain("<thumb>thumb.jpg</thumb>");
      expect(xml).toContain("</episodedetails>");
    });

    it("generates Bilibili platform uniqueid when video is from Bilibili", () => {
      const biliVideo: Video = {
        ...baseVideo,
        id: "v-bili",
        source: "Bilibili",
        sourceVideoId: "BV1xx411c7mD",
        sourceUrl: "https://www.bilibili.com/video/BV1xx411c7mD",
      };

      const xml = buildEpisodeNfo({
        video: biliVideo,
        showTitle: "UP 主系列",
      });

      expect(xml).toContain('<uniqueid type="bilibili">BV1xx411c7mD</uniqueid>');
      expect(xml).toContain("<platform>bilibili</platform>");
    });
  });

  describe("buildMovieNfo", () => {
    it("generates movie NFO for standalone media library items", () => {
      const xml = buildMovieNfo({
        video: baseVideo,
        thumbFilename: "poster.jpg",
      });

      expect(xml).toContain("<movie>");
      expect(xml).toContain("<title>Awesome Video</title>");
      expect(xml).toContain("<plot>Detailed video description</plot>");
      expect(xml).toContain("<premiered>2026-11-04</premiered>");
      expect(xml).toContain("<runtime>5</runtime>");
      expect(xml).toContain('<uniqueid type="videleum" default="true">videleum:video:v-12345</uniqueid>');
      expect(xml).toContain('<uniqueid type="youtube">dQw4w9WgXcQ</uniqueid>');
      expect(xml).toContain("<thumb>poster.jpg</thumb>");
      expect(xml).toContain("</movie>");
    });
  });

  describe("buildShowNfo", () => {
    it("generates tvshow NFO with sanitized show identifier", () => {
      const xml = buildShowNfo({
        showTitle: "Tech Channel!",
        plot: "A channel about technology",
        premiered: "2026-01-01",
        studio: "Creator Studio",
      });

      expect(xml).toContain("<tvshow>");
      expect(xml).toContain("<title>Tech Channel!</title>");
      expect(xml).toContain("<plot>A channel about technology</plot>");
      expect(xml).toContain("<premiered>2026-01-01</premiered>");
      expect(xml).toContain("<studio>Creator Studio</studio>");
      expect(xml).toContain("<uniqueid type=\"videleum\" default=\"true\">videleum:show:tech-channel</uniqueid>");
      expect(xml).toContain("</tvshow>");
    });
  });

  describe("utilities", () => {
    it("normalizes show identifier safely", () => {
      expect(normalizeShowIdentifier("  Linus Tech Tips / Clips! ")).toBe("linus-tech-tips-clips");
      expect(normalizeShowIdentifier("!!!")).toBe("show");
    });

    it("normalizes dates to standard YYYY-MM-DD", () => {
      expect(normalizeVideoDateToDay("20261104")).toBe("2026-11-04");
      expect(normalizeVideoDateToDay("2026-11-04")).toBe("2026-11-04");
      expect(normalizeVideoDateToDay("2026-11-04T12:00:00.000Z")).toBe("2026-11-04");
      expect(normalizeVideoDateToDay("invalid-date")).toBeUndefined();
    });
  });
});
