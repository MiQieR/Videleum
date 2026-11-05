import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  notifyConfiguredMediaServers,
  refreshJellyfinLibrary,
  refreshPlexLibrary,
  sanitizeMediaServerUrl,
  testJellyfinConnection,
  testPlexConnection,
} from "../../../services/mediaServerExport/mediaServerClients";
import type { Settings } from "../../../types/settings";

describe("mediaServerClients", () => {
  const originalFetch = global.fetch;

  beforeEach(() => {
    vi.restoreAllMocks();
  });

  afterEach(() => {
    global.fetch = originalFetch;
  });

  describe("sanitizeMediaServerUrl", () => {
    it("normalizes trailing slashes and validates http/https", () => {
      expect(sanitizeMediaServerUrl("http://192.168.1.100:8096/")).toBe(
        "http://192.168.1.100:8096"
      );
      expect(sanitizeMediaServerUrl("https://media.lan:32400///")).toBe(
        "https://media.lan:32400"
      );
      expect(sanitizeMediaServerUrl("http://localhost:8096/jellyfin/")).toBe(
        "http://localhost:8096/jellyfin"
      );
    });

    it("rejects invalid or non-http protocols", () => {
      expect(() => sanitizeMediaServerUrl("")).toThrow("Server URL must not be empty");
      expect(() => sanitizeMediaServerUrl("ftp://media.server/")).toThrow(
        "Only http and https are allowed"
      );
      expect(() => sanitizeMediaServerUrl("not-a-url")).toThrow(
        "Invalid server URL format"
      );
    });

    it("rejects embedded credentials and link-local / cloud metadata addresses", () => {
      expect(() =>
        sanitizeMediaServerUrl("http://admin:secret@192.168.1.100:8096")
      ).toThrow("embedded credentials");
      expect(() =>
        sanitizeMediaServerUrl("http://169.254.169.254/latest/meta-data")
      ).toThrow("restricted link-local or metadata address");
      expect(() =>
        sanitizeMediaServerUrl("http://[fe80::1]:8096")
      ).toThrow("restricted link-local or metadata address");
    });
  });

  describe("testJellyfinConnection", () => {
    it("returns success and server details on 200 response", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({
          ServerName: "My Home Jellyfin",
          Version: "10.9.1",
        }),
      });
      global.fetch = mockFetch as any;

      const res = await testJellyfinConnection("http://localhost:8096", "valid-key");
      expect(res.success).toBe(true);
      expect(res.serverName).toBe("My Home Jellyfin");
      expect(res.version).toBe("10.9.1");
      expect(mockFetch).toHaveBeenCalledWith(
        "http://localhost:8096/System/Info",
        expect.objectContaining({
          headers: expect.objectContaining({ "X-Emby-Token": "valid-key" }),
        })
      );
    });

    it("returns error on 401 unauthorized", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
      }) as any;

      const res = await testJellyfinConnection("http://localhost:8096", "bad-key");
      expect(res.success).toBe(false);
      expect(res.error).toContain("Authentication failed");
    });

    it("handles network failure gracefully", async () => {
      global.fetch = vi.fn().mockRejectedValue(new Error("Connection refused")) as any;

      const res = await testJellyfinConnection("http://localhost:8096", "any-key");
      expect(res.success).toBe(false);
      expect(res.error).toBe("Connection refused");
    });
  });

  describe("refreshJellyfinLibrary", () => {
    it("returns success on 204 No Content response", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 204,
      }) as any;

      const res = await refreshJellyfinLibrary("http://localhost:8096", "valid-key");
      expect(res.success).toBe(true);
    });

    it("returns failure when server returns 500", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 500,
      }) as any;

      const res = await refreshJellyfinLibrary("http://localhost:8096", "valid-key");
      expect(res.success).toBe(false);
      expect(res.error).toContain("HTTP 500");
    });
  });

  describe("testPlexConnection", () => {
    it("returns success on valid token and 200 response", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({
          MediaContainer: {
            friendlyName: "Living Room Plex",
            version: "1.40.0",
          },
        }),
      }) as any;

      const res = await testPlexConnection("http://localhost:32400", "plex-token-123");
      expect(res.success).toBe(true);
      expect(res.serverName).toBe("Living Room Plex");
      expect(res.version).toBe("1.40.0");
    });

    it("returns authentication error on 401 response", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: false,
        status: 401,
      }) as any;

      const res = await testPlexConnection("http://localhost:32400", "wrong-token");
      expect(res.success).toBe(false);
      expect(res.error).toContain("Authentication failed");
    });
  });

  describe("refreshPlexLibrary", () => {
    it("returns success on 200 response", async () => {
      global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
      }) as any;

      const res = await refreshPlexLibrary("http://localhost:32400", "plex-token-123");
      expect(res.success).toBe(true);
    });
  });

  describe("notifyConfiguredMediaServers", () => {
    it("triggers refresh for only enabled media servers on immediate call", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 204,
      });
      global.fetch = mockFetch as any;

      const mockSettings: Partial<Settings> = {
        jellyfinEnabled: true,
        jellyfinServerUrl: "http://192.168.1.100:8096",
        jellyfinApiKey: "jf-key",
        embyEnabled: false,
        plexEnabled: true,
        plexServerUrl: "http://192.168.1.100:32400",
        plexToken: "plex-token",
        mediaServerAutoRefreshOnDownload: true,
      };

      const results = await notifyConfiguredMediaServers({
        immediate: true,
        settings: mockSettings as Settings,
      });

      expect(results).toHaveLength(2);
      expect(results.some((r) => r.server === "jellyfin" && r.success)).toBe(true);
      expect(results.some((r) => r.server === "plex" && r.success)).toBe(true);
      expect(mockFetch).toHaveBeenCalledTimes(2);
    });

    it("returns empty array when no server is enabled", async () => {
      const mockFetch = vi.fn();
      global.fetch = mockFetch as any;

      const mockSettings: Partial<Settings> = {
        jellyfinEnabled: false,
        plexEnabled: false,
        embyEnabled: false,
      };

      const results = await notifyConfiguredMediaServers({
        immediate: true,
        settings: mockSettings as Settings,
      });

      expect(results).toEqual([]);
      expect(mockFetch).not.toHaveBeenCalled();
    });

    it("resolves all concurrent debounced callers when the debounce timer fires", async () => {
      const mockFetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 204,
      });
      global.fetch = mockFetch as any;

      const mockSettings: Partial<Settings> = {
        jellyfinEnabled: true,
        jellyfinServerUrl: "http://192.168.1.100:8096",
        jellyfinApiKey: "jf-key",
        mediaServerAutoRefreshOnDownload: true,
      };

      const p1 = notifyConfiguredMediaServers({
        immediate: false,
        debounceWaitMs: 20,
        settings: mockSettings as Settings,
      });
      const p2 = notifyConfiguredMediaServers({
        immediate: false,
        debounceWaitMs: 20,
        settings: mockSettings as Settings,
      });

      const [r1, r2] = await Promise.all([p1, p2]);
      expect(r1).toHaveLength(1);
      expect(r2).toHaveLength(1);
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });
  });
});
