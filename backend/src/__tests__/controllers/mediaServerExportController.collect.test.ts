import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  refreshMediaServersHandler,
  testEmbyConnectionHandler,
  testJellyfinConnectionHandler,
  testPlexConnectionHandler,
} from "../../controllers/mediaServerExportController";

vi.mock("../../services/storageService", () => ({
  getSettings: vi.fn(() => ({
    jellyfinServerUrl: "http://localhost:8096",
    jellyfinApiKey: "saved-jf-key",
    plexServerUrl: "http://localhost:32400",
    plexToken: "saved-plex-token",
  })),
  getDownloadStatus: vi.fn(() => ({
    activeDownloads: [],
    queuedDownloads: [],
  })),
}));

vi.mock("../../services/mediaServerExport/mediaServerClients", () => ({
  testJellyfinConnection: vi.fn((url, key) => {
    if (key === "bad-key") {
      return Promise.resolve({ success: false, error: "Authentication failed" });
    }
    return Promise.resolve({
      success: true,
      serverName: "Jellyfin Server",
      version: "10.9.0",
    });
  }),
  testPlexConnection: vi.fn((url, token) => {
    if (token === "bad-token") {
      return Promise.resolve({ success: false, error: "Authentication failed" });
    }
    return Promise.resolve({
      success: true,
      serverName: "Plex Server",
      version: "1.40.0",
    });
  }),
  notifyConfiguredMediaServers: vi.fn(() =>
    Promise.resolve([
      { server: "jellyfin" as const, success: true },
      { server: "plex" as const, success: true },
    ])
  ),
}));

describe("mediaServerExportController media server client handlers", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  function createMockRes() {
    const res: any = {};
    res.status = vi.fn().mockReturnValue(res);
    res.json = vi.fn().mockReturnValue(res);
    return res;
  }

  describe("testJellyfinConnectionHandler", () => {
    it("returns 200 and details on valid connection", async () => {
      const req: any = {
        body: { serverUrl: "http://192.168.1.10:8096", apiKey: "valid-key" },
      };
      const res = createMockRes();

      await testJellyfinConnectionHandler(req, res);

      expect(res.json).toHaveBeenCalledWith(
        expect.objectContaining({
          success: true,
          serverName: "Jellyfin Server",
        })
      );
    });

    it("returns 400 when connection fails", async () => {
      const req: any = {
        body: { serverUrl: "http://192.168.1.10:8096", apiKey: "bad-key" },
      };
      const res = createMockRes();

      await testJellyfinConnectionHandler(req, res);

      expect(res.status).toHaveBeenCalledWith(400);
      expect(res.json).toHaveBeenCalledWith(
        expect.objectContaining({
          success: false,
          error: "Authentication failed",
        })
      );
    });

    it("falls back to stored settings when body fields are missing", async () => {
      const req: any = { body: {} };
      const res = createMockRes();

      await testJellyfinConnectionHandler(req, res);

      expect(res.json).toHaveBeenCalledWith(
        expect.objectContaining({
          success: true,
        })
      );
    });
  });

  describe("testPlexConnectionHandler", () => {
    it("returns 200 on valid Plex token", async () => {
      const req: any = {
        body: { serverUrl: "http://192.168.1.10:32400", token: "valid-token" },
      };
      const res = createMockRes();

      await testPlexConnectionHandler(req, res);

      expect(res.json).toHaveBeenCalledWith(
        expect.objectContaining({
          success: true,
          serverName: "Plex Server",
        })
      );
    });
  });

  describe("refreshMediaServersHandler", () => {
    it("returns 200 and refresh results for all configured servers", async () => {
      const req: any = {};
      const res = createMockRes();

      await refreshMediaServersHandler(req, res);

      expect(res.json).toHaveBeenCalledWith({
        success: true,
        results: [
          { server: "jellyfin", success: true },
          { server: "plex", success: true },
        ],
      });
    });
  });
});
