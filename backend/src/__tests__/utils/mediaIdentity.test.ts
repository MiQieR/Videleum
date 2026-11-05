import { describe, expect, it } from "vitest";
import {
  getMediaIdentityKey,
  isSameMediaIdentity,
  normalizeMediaIdentity,
  normalizeMediaPlatform,
} from "../../utils/mediaIdentity";

describe("mediaIdentity", () => {
  it("normalizes aliases and rejects unknown values", () => {
    expect(normalizeMediaPlatform(" YouTube ")).toBe("youtube");
    expect(normalizeMediaPlatform("youtube-dl")).toBe("youtube");
    expect(normalizeMediaPlatform("yt")).toBe("youtube");
    expect(normalizeMediaPlatform("Bilibili")).toBe("bilibili");
    expect(normalizeMediaPlatform("bili")).toBe("bilibili");
    expect(normalizeMediaPlatform("twitch")).toBe("twitch");
    expect(normalizeMediaPlatform("missav")).toBe("missav");
    expect(normalizeMediaPlatform("local")).toBe("local");
    expect(normalizeMediaPlatform("cloud")).toBe("cloud");
    expect(normalizeMediaPlatform("other")).toBe("other");
    expect(normalizeMediaPlatform("unknown")).toBe("unknown");
    expect(normalizeMediaPlatform("https://youtube.evil.test")).toBe("unknown");
    expect(normalizeMediaPlatform(null)).toBe("unknown");
    expect(normalizeMediaPlatform(undefined)).toBe("unknown");
    expect(normalizeMediaPlatform(123)).toBe("unknown");
  });

  it("preserves other platform when normalizing identity", () => {
    const identity = normalizeMediaIdentity({
      platform: "other",
      sourceVideoId: "https://example.com/custom/video",
      sourceUrl: "https://example.com/custom/video",
      mediaType: "video",
    });

    expect(identity.platform).toBe("other");
    expect(identity.sourceVideoId).toBe("https://example.com/custom/video");
    expect(getMediaIdentityKey(identity)).toBe("other:https://example.com/custom/video:video");
  });

  it("keeps video and audio identities distinct", () => {
    const identity = normalizeMediaIdentity({
      platform: "Bilibili",
      sourceVideoId: "BV1xx",
      sourceUrl: "https://b23.tv/BV1xx",
      mediaType: "audio",
      partNumber: 2,
    });

    expect(identity).toEqual({
      platform: "bilibili",
      sourceVideoId: "BV1xx",
      sourceUrl: "https://b23.tv/BV1xx",
      mediaType: "audio",
      partNumber: 2,
    });
    expect(getMediaIdentityKey(identity)).toBe("bilibili:BV1xx:audio");
    expect(getMediaIdentityKey({ ...identity, mediaType: "video" })).not.toBe(
      getMediaIdentityKey(identity),
    );
  });

  it("handles localVideoId and checks identity equality", () => {
    const idA = normalizeMediaIdentity({
      platform: "youtube",
      sourceVideoId: "dQw4w9WgXcQ",
      sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      mediaType: "video",
      localVideoId: "video-uuid-1",
    });

    expect(idA.localVideoId).toBe("video-uuid-1");

    const idB = normalizeMediaIdentity({
      platform: "youtube",
      sourceVideoId: "dQw4w9WgXcQ",
      sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      mediaType: "video",
    });

    expect(isSameMediaIdentity(idA, idB)).toBe(true);

    const idC = normalizeMediaIdentity({
      platform: "youtube",
      sourceVideoId: "dQw4w9WgXcQ",
      sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
      mediaType: "audio",
    });

    expect(isSameMediaIdentity(idA, idC)).toBe(false);
  });
});
