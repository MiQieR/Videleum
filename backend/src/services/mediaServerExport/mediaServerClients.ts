import { logger } from "../../utils/logger";
import { getSettings } from "../storageService/settings";
import type { Settings } from "../../types/settings";

export interface MediaServerTestResult {
  success: boolean;
  serverName?: string;
  version?: string;
  error?: string;
}

export interface MediaServerRefreshResult {
  server: "jellyfin" | "emby" | "plex";
  success: boolean;
  error?: string;
}

/**
 * 判断主机名是否指向受限的云元数据或链路本地地址
 */
function isRestrictedMediaServerHost(hostname: string): boolean {
  const normalized = hostname.replace(/^\[|\]$/g, "").trim().toLowerCase();
  if (!normalized) {
    return true;
  }
  if (
    normalized === "0.0.0.0" ||
    normalized === "::" ||
    normalized === "::0" ||
    normalized === "metadata.google.internal" ||
    normalized === "instance-data"
  ) {
    return true;
  }
  // 拦截 IPv4 链路本地 / 云厂商元数据网段 169.254.0.0/16 与 IPv6 链路本地 fe80::/10
  if (
    normalized.startsWith("169.254.") ||
    normalized.startsWith("::ffff:169.254.") ||
    normalized.startsWith("::ffff:a9fe:") ||
    normalized.startsWith("fe80:")
  ) {
    return true;
  }
  return false;
}

/**
 * 规范化并校验媒体服务器基础 URL
 */
export function sanitizeMediaServerUrl(rawUrl: string): string {
  const trimmed = (rawUrl || "").trim();
  if (!trimmed) {
    throw new Error("Server URL must not be empty");
  }

  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error("Invalid server URL format");
  }

  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error(
      `Invalid protocol '${parsed.protocol}'. Only http and https are allowed`
    );
  }

  if (parsed.username || parsed.password) {
    throw new Error("Server URL must not contain embedded credentials");
  }

  if (isRestrictedMediaServerHost(parsed.hostname)) {
    throw new Error(
      "Server URL points to a restricted link-local or metadata address"
    );
  }

  // 去除尾部斜杠，保留协议、主机与基础路径
  return `${parsed.protocol}//${parsed.host}${parsed.pathname.replace(/\/+$/, "")}`;
}

/**
 * 脱敏 URL 和令牌用于安全日志打印
 */
function maskSecret(secret?: string): string {
  if (!secret) return "";
  if (secret.length <= 6) return "***";
  return `${secret.slice(0, 2)}***${secret.slice(-2)}`;
}

/**
 * 测试 Jellyfin / Emby 服务器连接
 */
export async function testJellyfinConnection(
  serverUrl: string,
  apiKey: string,
  timeoutMs = 5000
): Promise<MediaServerTestResult> {
  try {
    const baseUrl = sanitizeMediaServerUrl(serverUrl);
    const token = (apiKey || "").trim();
    if (!token) {
      return { success: false, error: "API key is required" };
    }

    const endpoint = `${baseUrl}/System/Info`;
    const response = await fetch(endpoint, {
      method: "GET",
      headers: {
        "X-Emby-Token": token,
        Accept: "application/json",
      },
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!response.ok) {
      if (response.status === 401 || response.status === 403) {
        return {
          success: false,
          error: "Authentication failed. Please verify your API key.",
        };
      }
      return {
        success: false,
        error: `Server responded with status HTTP ${response.status}`,
      };
    }

    const data = (await response.json()) as Record<string, any>;
    const serverName = data.ServerName || data.Name || "Jellyfin/Emby Server";
    const version = data.Version || "Unknown";

    return { success: true, serverName, version };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { success: false, error: message };
  }
}

/**
 * 触发 Jellyfin / Emby 媒体库刷新扫描
 */
export async function refreshJellyfinLibrary(
  serverUrl: string,
  apiKey: string,
  timeoutMs = 5000
): Promise<{ success: boolean; error?: string }> {
  try {
    const baseUrl = sanitizeMediaServerUrl(serverUrl);
    const token = (apiKey || "").trim();
    if (!token) {
      return { success: false, error: "API key is required" };
    }

    const endpoint = `${baseUrl}/Library/Refresh`;
    const response = await fetch(endpoint, {
      method: "POST",
      headers: {
        "X-Emby-Token": token,
      },
      signal: AbortSignal.timeout(timeoutMs),
    });

    // 200..299 状态码均表示已接受扫描请求（通常是 204 No Content）
    if (response.ok) {
      return { success: true };
    }

    return {
      success: false,
      error: `Failed to trigger library refresh: HTTP ${response.status}`,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { success: false, error: message };
  }
}

/**
 * 测试 Plex 媒体服务器连接
 */
export async function testPlexConnection(
  serverUrl: string,
  token: string,
  timeoutMs = 5000
): Promise<MediaServerTestResult> {
  try {
    const baseUrl = sanitizeMediaServerUrl(serverUrl);
    const plexToken = (token || "").trim();
    if (!plexToken) {
      return { success: false, error: "Plex token is required" };
    }

    const endpoint = `${baseUrl}/identity`;
    const response = await fetch(endpoint, {
      method: "GET",
      headers: {
        "X-Plex-Token": plexToken,
        Accept: "application/json",
      },
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (!response.ok) {
      if (response.status === 401 || response.status === 403) {
        return {
          success: false,
          error: "Authentication failed. Please verify your Plex token.",
        };
      }
      return {
        success: false,
        error: `Server responded with status HTTP ${response.status}`,
      };
    }

    let serverName = "Plex Media Server";
    let version = "Unknown";
    try {
      const data = (await response.json()) as Record<string, any>;
      if (data.MediaContainer) {
        serverName = data.MediaContainer.friendlyName || serverName;
        version = data.MediaContainer.version || version;
      }
    } catch {
      // 某些版本若返回 XML 则保持默认值
    }

    return { success: true, serverName, version };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { success: false, error: message };
  }
}

/**
 * 触发 Plex 媒体库所有节刷新
 */
export async function refreshPlexLibrary(
  serverUrl: string,
  token: string,
  timeoutMs = 5000
): Promise<{ success: boolean; error?: string }> {
  try {
    const baseUrl = sanitizeMediaServerUrl(serverUrl);
    const plexToken = (token || "").trim();
    if (!plexToken) {
      return { success: false, error: "Plex token is required" };
    }

    const endpoint = `${baseUrl}/library/sections/all/refresh`;
    const response = await fetch(endpoint, {
      method: "GET",
      headers: {
        "X-Plex-Token": plexToken,
      },
      signal: AbortSignal.timeout(timeoutMs),
    });

    if (response.ok) {
      return { success: true };
    }

    return {
      success: false,
      error: `Failed to trigger Plex library refresh: HTTP ${response.status}`,
    };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { success: false, error: message };
  }
}

// 防抖计时器引用与待决议 Promise 队列
let autoRefreshDebounceTimer: NodeJS.Timeout | null = null;
let pendingDebounceResolvers: Array<(results: MediaServerRefreshResult[]) => void> = [];
const DEFAULT_DEBOUNCE_WAIT_MS = 3000;

function flushPendingDebounceResolvers(results: MediaServerRefreshResult[]): void {
  if (pendingDebounceResolvers.length === 0) {
    return;
  }
  const resolvers = pendingDebounceResolvers;
  pendingDebounceResolvers = [];
  for (const resolve of resolvers) {
    resolve(results);
  }
}

/**
 * 执行所有已配置媒体服务器的刷新通知
 */
async function executeMediaServersNotification(
  settingsInput?: Settings
): Promise<MediaServerRefreshResult[]> {
  const settings = settingsInput || (getSettings() as Settings);
  const tasks: Array<{
    server: MediaServerRefreshResult["server"];
    promise: Promise<MediaServerRefreshResult>;
  }> = [];

  // Jellyfin
  if (
    settings.jellyfinEnabled &&
    settings.jellyfinServerUrl &&
    settings.jellyfinApiKey
  ) {
    const url = settings.jellyfinServerUrl;
    const key = settings.jellyfinApiKey;
    tasks.push({
      server: "jellyfin",
      promise: refreshJellyfinLibrary(url, key).then((res) => {
        if (!res.success) {
          logger.warn(
            `[MediaServer] Jellyfin refresh notification failed for ${url} (key: ${maskSecret(key)}): ${res.error}`
          );
        } else {
          logger.info(`[MediaServer] Jellyfin library refresh triggered successfully.`);
        }
        return { server: "jellyfin" as const, success: res.success, error: res.error };
      }),
    });
  }

  // Emby
  if (
    settings.embyEnabled &&
    settings.embyServerUrl &&
    settings.embyApiKey
  ) {
    const url = settings.embyServerUrl;
    const key = settings.embyApiKey;
    tasks.push({
      server: "emby",
      promise: refreshJellyfinLibrary(url, key).then((res) => {
        if (!res.success) {
          logger.warn(
            `[MediaServer] Emby refresh notification failed for ${url} (key: ${maskSecret(key)}): ${res.error}`
          );
        } else {
          logger.info(`[MediaServer] Emby library refresh triggered successfully.`);
        }
        return { server: "emby" as const, success: res.success, error: res.error };
      }),
    });
  }

  // Plex
  if (
    settings.plexEnabled &&
    settings.plexServerUrl &&
    settings.plexToken
  ) {
    const url = settings.plexServerUrl;
    const token = settings.plexToken;
    tasks.push({
      server: "plex",
      promise: refreshPlexLibrary(url, token).then((res) => {
        if (!res.success) {
          logger.warn(
            `[MediaServer] Plex refresh notification failed for ${url} (token: ${maskSecret(token)}): ${res.error}`
          );
        } else {
          logger.info(`[MediaServer] Plex library refresh triggered successfully.`);
        }
        return { server: "plex" as const, success: res.success, error: res.error };
      }),
    });
  }

  if (tasks.length === 0) {
    return [];
  }

  const results = await Promise.allSettled(tasks.map((task) => task.promise));
  return results.map((item, idx) => {
    if (item.status === "fulfilled") {
      return item.value;
    }
    return {
      server: tasks[idx]?.server ?? "jellyfin",
      success: false,
      error: item.reason instanceof Error ? item.reason.message : String(item.reason),
    };
  });
}

/**
 * 统一通知已启用的媒体服务器刷新媒体库
 * - 当 immediate 为 true 时立即执行并返回结果（用于 API 手动调用或批处理完成）；
 * - 默认情况下采用防抖策略合并短时间内多次并发下载完成通知。
 */
export async function notifyConfiguredMediaServers(options?: {
  immediate?: boolean;
  settings?: Settings;
  debounceWaitMs?: number;
}): Promise<MediaServerRefreshResult[]> {
  const settings = options?.settings || (getSettings() as Settings);

  // 若关闭自动刷新则直接跳过
  if (settings.mediaServerAutoRefreshOnDownload === false && !options?.immediate) {
    return [];
  }

  if (options?.immediate) {
    if (autoRefreshDebounceTimer) {
      clearTimeout(autoRefreshDebounceTimer);
      autoRefreshDebounceTimer = null;
    }
    const results = await executeMediaServersNotification(settings);
    flushPendingDebounceResolvers(results);
    return results;
  }

  // 防抖延迟执行，合并同一窗口内所有调用方的 Promise
  return new Promise((resolve) => {
    pendingDebounceResolvers.push(resolve);
    if (autoRefreshDebounceTimer) {
      clearTimeout(autoRefreshDebounceTimer);
    }
    const wait = options?.debounceWaitMs ?? DEFAULT_DEBOUNCE_WAIT_MS;
    autoRefreshDebounceTimer = setTimeout(async () => {
      autoRefreshDebounceTimer = null;
      try {
        const results = await executeMediaServersNotification(settings);
        flushPendingDebounceResolvers(results);
      } catch (err) {
        logger.error(`[MediaServer] Error executing debounced notification:`, err);
        flushPendingDebounceResolvers([]);
      }
    }, wait);
  });
}
