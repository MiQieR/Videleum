import { describe, expect, it, vi } from "vitest";
import {
  buildDetachedTask,
  buildRetryHistoryItem,
  BILIBILI_RETRY_RESTORE_FAILED_MESSAGE,
  resolveRetryPolicy,
} from "../../../services/downloadManager/retryScheduler";
import type { DownloadTask } from "../../../services/downloadManager/types";
import type { DownloadHistoryItem } from "../../../services/storageService";
import { logger } from "../../../utils/logger";

// createDownloadTask is imported by retryScheduler from downloadService; mock it
// so buildDetachedTask doesn't require a real downloader at import time.
vi.mock("../../../services/downloadService", () => ({
  createDownloadTask: vi.fn().mockReturnValue(() => Promise.resolve()),
}));

const baseTask = (overrides: Partial<DownloadTask> = {}): DownloadTask => ({
  downloadFn: vi.fn(),
  id: "task-1",
  title: "Sample title",
  resolve: vi.fn(),
  reject: vi.fn(),
  sourceUrl: "https://youtube.com/watch?v=abc",
  type: "youtube",
  ...overrides,
});

describe("downloadManager retryScheduler", () => {
  it("exports the bilibili restore-failed message verbatim", () => {
    expect(BILIBILI_RETRY_RESTORE_FAILED_MESSAGE).toBe(
      "Bilibili retry could not be restored after restart. Please download again."
    );
  });

  describe("buildDetachedTask", () => {
    it("returns a task that logs on resolve/reject (no caller to notify)", () => {
      const logSpy = vi.spyOn(logger, "info").mockImplementation(() => {});
      const errSpy = vi.spyOn(logger, "error").mockImplementation(() => {});

      const task = buildDetachedTask("id", "title", "https://x", "youtube");
      expect(task).not.toBeNull();
      task!.resolve("value");
      task!.reject(new Error("err"));
      expect(logSpy).toHaveBeenCalled();
      expect(errSpy).toHaveBeenCalled();

      logSpy.mockRestore();
      errSpy.mockRestore();
    });

    it("returns null for an unrestorable bilibili retry with unparsable metadata", () => {
      // canRestoreDetachedTask returns false when type=bilibili and rawMetadata
      // is present but unparseable. Provide an obviously-invalid raw value.
      const task = buildDetachedTask(
        "id",
        "title",
        "https://bilibili.com",
        "bilibili",
        undefined,
        "not-valid-json"
      );
      expect(task).toBeNull();
    });

    it("restores canonical media identity without changing downloader type", () => {
      const task = buildDetachedTask(
        "id",
        "title",
        "https://www.twitch.tv/videos/123",
        "youtube",
        undefined,
        null,
        {
          platform: "Twitch",
          sourceVideoId: "123",
          mediaType: "video",
        },
      );

      expect(task?.type).toBe("youtube");
      expect(task?.identity).toEqual({
        platform: "twitch",
        sourceVideoId: "123",
        sourceUrl: "https://www.twitch.tv/videos/123",
        mediaType: "video",
        partNumber: null,
      });
    });
  });

  describe("resolveRetryPolicy", () => {
    it("returns null when the retry budget is exhausted", () => {
      const task = baseTask();
      const existing: DownloadHistoryItem = {
        id: "task-1",
        title: "t",
        finishedAt: 1,
        status: "pending_retry",
        retryCount: 3,
        retryLimit: 3,
        retryIntervalMinutes: 5,
      };
      expect(resolveRetryPolicy(task, 3, 5, existing)).toBeNull();
    });

    it("falls back to normalized setting defaults when history is missing", () => {
      const task = baseTask();
      const policy = resolveRetryPolicy(task, 2, 10);
      expect(policy).toEqual({
        retryLimit: 2,
        retryIntervalMinutes: 10,
        retryCount: 0,
      });
    });

    it("prefers history's limit/interval/count over settings", () => {
      const task = baseTask();
      const existing: DownloadHistoryItem = {
        id: "task-1",
        title: "t",
        finishedAt: 1,
        status: "pending_retry",
        retryCount: 1,
        retryLimit: 5,
        retryIntervalMinutes: 15,
      };
      // Settings say 2/10, but history already pinned 5/15.
      expect(resolveRetryPolicy(task, 2, 10, existing)).toEqual({
        retryLimit: 5,
        retryIntervalMinutes: 15,
        retryCount: 1,
      });
    });
  });

  describe("buildRetryHistoryItem", () => {
    it("builds a pending_retry item with incremented count and propagated metadata", () => {
      const task = baseTask({
        statistics: {
          actorRole: "admin",
          surface: "background",
          sourceKind: "manual",
          relatedEventId: null,
          enqueuedEventId: null,
        },
      });
      const before = Date.now();
      const item = buildRetryHistoryItem({
        task,
        error: new Error("download blew up"),
        retryLimit: 3,
        retryIntervalMinutes: 5,
        retryCount: 0,
      });
      const after = Date.now();

      expect(item.id).toBe("task-1");
      expect(item.status).toBe("pending_retry");
      expect(item.error).toBe("download blew up");
      expect(item.sourceUrl).toBe("https://youtube.com/watch?v=abc");
      expect(item.platform).toBe("youtube");
      expect(item.sourceKind).toBe("manual");
      expect(item.downloadType).toBe("youtube");
      expect(item.retryCount).toBe(1);
      expect(item.retryLimit).toBe(3);
      expect(item.retryIntervalMinutes).toBe(5);
      expect(item.nextRetryAt).toBeGreaterThanOrEqual(before + 5 * 60 * 1000);
      expect(item.nextRetryAt).toBeLessThanOrEqual(after + 5 * 60 * 1000);
    });

    it("falls back to existing history's sourceKind when task has no statistics", () => {
      const task = baseTask(); // no statistics
      const item = buildRetryHistoryItem({
        task,
        error: "err",
        retryLimit: 2,
        retryIntervalMinutes: 5,
        retryCount: 1,
        existingHistory: {
          id: "task-1",
          title: "t",
          finishedAt: 1,
          status: "pending_retry",
          sourceKind: "subscription",
        } as DownloadHistoryItem,
      });
      expect(item.sourceKind).toBe("subscription");
      expect(item.retryCount).toBe(2);
    });

    it("preserves Twitch platform and audio-only mediaType when building retry history item", () => {
      const twitchAudioTask = baseTask({
        id: "twitch-audio-1",
        title: "Twitch stream audio",
        sourceUrl: "https://www.twitch.tv/videos/987654321",
        type: "youtube",
        identity: {
          platform: "twitch",
          sourceVideoId: "987654321",
          sourceUrl: "https://www.twitch.tv/videos/987654321",
          mediaType: "audio",
        },
      });

      const item = buildRetryHistoryItem({
        task: twitchAudioTask,
        error: new Error("network timeout"),
        retryLimit: 3,
        retryIntervalMinutes: 5,
        retryCount: 0,
      });

      expect(item.platform).toBe("twitch");
      expect(item.downloadType).toBe("youtube");
      expect(item.mediaType).toBe("audio");
    });
  });

  describe("buildDetachedTask identity restoration", () => {
    it("restores YouTube video task with derived sourceVideoId", () => {
      const task = buildDetachedTask(
        "yt-1",
        "YouTube Video",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "youtube",
      );
      expect(task).not.toBeNull();
      expect(task?.type).toBe("youtube");
      expect(task?.identity).toEqual({
        platform: "youtube",
        sourceVideoId: "dQw4w9WgXcQ",
        sourceUrl: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        mediaType: "video",
        partNumber: null,
      });
    });

    it("restores Bilibili multipart task with partNumber", () => {
      const task = buildDetachedTask(
        "bili-1",
        "Bilibili P2",
        "https://www.bilibili.com/video/BV1xx411c7mD?p=2",
        "bilibili",
        undefined,
        null,
        {
          platform: "bilibili",
          sourceVideoId: "BV1xx411c7mD",
          mediaType: "video",
          partNumber: 2,
        },
      );
      expect(task).not.toBeNull();
      expect(task?.type).toBe("bilibili");
      expect(task?.identity).toEqual({
        platform: "bilibili",
        sourceVideoId: "BV1xx411c7mD",
        sourceUrl: "https://www.bilibili.com/video/BV1xx411c7mD?p=2",
        mediaType: "video",
        partNumber: 2,
      });
    });

    it("restores audio-only task preserving mediaType: audio", () => {
      const task = buildDetachedTask(
        "audio-1",
        "Audio Only",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "youtube",
        undefined,
        null,
        {
          platform: "youtube",
          sourceVideoId: "dQw4w9WgXcQ",
          mediaType: "audio",
        },
      );
      expect(task).not.toBeNull();
      expect(task?.identity?.mediaType).toBe("audio");
    });
  });
});
