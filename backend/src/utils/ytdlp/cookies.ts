import {
  BILIBILI_COOKIES_FILENAME,
  COOKIES_FILENAME,
  DATA_DIR,
  YOUTUBE_COOKIES_FILENAME,
} from "../../config/paths";
import {
  readFileSafeSync,
  resolveSafeChildPath,
  statSafeSync,
  writeFileSafeSync,
} from "../security";
import {
  UnsupportedCookieFormatError,
  isValidNetscapeCookiesFile,
  normalizeCookiesFileContent,
} from "../cookieFileFormat";
import { logger } from "../logger";

type CookiesFileSignature = {
  mtimeMs: number;
  size: number;
};
type CookiesFileCache = CookiesFileSignature & {
  path: string | null;
};

const cookiesFileCacheByPath = new Map<string, CookiesFileCache>();

function resolveCookiesPath(filename: string = COOKIES_FILENAME): string {
  return resolveSafeChildPath(DATA_DIR, filename);
}

function isMissingFileError(error: unknown): boolean {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    (error as NodeJS.ErrnoException).code === "ENOENT"
  );
}

function updateCookiesFileCacheFromDisk(
  cookiesPath: string,
  pathValue: string | null
): void {
  try {
    const stats = statSafeSync(cookiesPath, DATA_DIR);
    cookiesFileCacheByPath.set(cookiesPath, {
      mtimeMs: stats.mtimeMs,
      size: stats.size,
      path: pathValue,
    });
  } catch {
    cookiesFileCacheByPath.delete(cookiesPath);
  }
}

/**
 * Return the cookies file path after ensuring the file is usable by yt-dlp.
 * Existing Cookie header files are converted in place for backward compatibility.
 */
export function ensureCookiesFileIsNormalized(
  filename: string = COOKIES_FILENAME
): string | null {
  const cookiesPath = resolveCookiesPath(filename);
  let signature: CookiesFileSignature | null = null;
  try {
    const stats = statSafeSync(cookiesPath, DATA_DIR);
    signature = { mtimeMs: stats.mtimeMs, size: stats.size };
    const cached = cookiesFileCacheByPath.get(cookiesPath);
    if (
      cached &&
      cached.mtimeMs === signature.mtimeMs &&
      cached.size === signature.size
    ) {
      return cached.path;
    }

    const content = readFileSafeSync(cookiesPath, DATA_DIR, "utf8");
    if (isValidNetscapeCookiesFile(content)) {
      cookiesFileCacheByPath.set(cookiesPath, {
        ...signature,
        path: cookiesPath,
      });
      return cookiesPath;
    }

    const platformHint =
      filename === BILIBILI_COOKIES_FILENAME
        ? "bilibili"
        : filename === YOUTUBE_COOKIES_FILENAME
          ? "youtube"
          : undefined;
    const normalizedContent = normalizeCookiesFileContent(content, platformHint);
    writeFileSafeSync(cookiesPath, DATA_DIR, normalizedContent, "utf8");
    updateCookiesFileCacheFromDisk(cookiesPath, cookiesPath);
    logger.warn(
      `[yt-dlp] Converted ${filename} from Cookie header format to Netscape format.`
    );
    return cookiesPath;
  } catch (error) {
    if (error instanceof UnsupportedCookieFormatError) {
      if (signature) {
        // path=null means this exact file signature is known to be unparseable.
        cookiesFileCacheByPath.set(cookiesPath, { ...signature, path: null });
      }
      logger.warn(
        `[yt-dlp] Ignoring invalid ${filename}: ${error.message}`
      );
      return null;
    }

    if (isMissingFileError(error)) {
      cookiesFileCacheByPath.delete(cookiesPath);
      return null;
    }

    cookiesFileCacheByPath.delete(cookiesPath);
    logger.warn(
      `[yt-dlp] Unable to read ${filename}; continuing without cookies.`,
      error
    );
    return null;
  }
}

export function resetCookiesFileCache(): void {
  cookiesFileCacheByPath.clear();
}

/** Match a Netscape domain without accepting sibling or suffix-lookalike hosts. */
function cookieLineMatchesHost(
  rawDomain: string,
  includeSubdomains: boolean,
  host: string
): boolean {
  const domain = rawDomain.replace(/^\./, "").toLowerCase();
  if (!domain) {
    return false;
  }
  // A leading dot in the file is the classic "and its subdomains" marker; the
  // include-subdomains column carries the same meaning for exports that omit it.
  if (rawDomain.startsWith(".") || includeSubdomains) {
    return host === domain || host.endsWith(`.${domain}`);
  }
  return host === domain;
}

/**
 * RFC 6265 path-match: the cookie's path is the request path, or a prefix of it
 * that ends at a path segment boundary.
 */
function cookiePathMatches(cookiePath: string, requestPath: string): boolean {
  if (!cookiePath.startsWith("/")) {
    return false;
  }
  if (cookiePath === "/" || cookiePath === requestPath) {
    return true;
  }
  if (!requestPath.startsWith(cookiePath)) {
    return false;
  }
  return cookiePath.endsWith("/") || requestPath[cookiePath.length] === "/";
}

/** Only zero denotes a session cookie; negative timestamps are already expired. */
function isExpiredCookie(expires: string): boolean {
  const expiresAt = Number(expires);
  return (
    !Number.isFinite(expiresAt) ||
    (expiresAt !== 0 && expiresAt * 1000 <= Date.now())
  );
}

function extractMatchingCookiesFromFile(
  cookiesPath: string,
  normalizedHost: string,
  requestPath: string,
  isSecure: boolean
): string | null {
  try {
    const content = readFileSafeSync(cookiesPath, DATA_DIR, "utf8");
    const cookies = new Map<
      string,
      {
        rawDomain: string;
        includeSubdomains: boolean;
        path: string;
        secure: boolean;
        expires: string;
        pair: string;
      }
    >();

    for (const line of String(content).split(/\r?\n/)) {
      const trimmed = line.trim();
      if (
        trimmed === "" ||
        (trimmed.startsWith("#") && !trimmed.startsWith("#HttpOnly_"))
      ) {
        continue;
      }

      // Do not trim the data row: an empty value is a trailing tab.
      const parts = line
        .replace(/^\uFEFF/, "")
        .replace(/^#HttpOnly_/, "")
        .split("\t");
      if (parts.length !== 7) {
        continue;
      }

      const [
        rawDomain,
        includeSubdomains,
        cookiePath,
        secure,
        expires,
        name,
        value,
      ] = parts;
      if (
        !/^[!#$%&'*+\-.^_`|~\da-z]+$/i.test(name) ||
        /[\x00-\x20;\x7f-\uffff]/.test(value)
      ) {
        continue;
      }
      const domain = rawDomain.replace(/^\./, "").toLowerCase();
      cookies.set(JSON.stringify([domain, cookiePath, name]), {
        rawDomain,
        includeSubdomains: includeSubdomains.toUpperCase() === "TRUE",
        path: cookiePath,
        secure: secure.toUpperCase() === "TRUE",
        expires,
        pair: `${name}=${value}`,
      });
    }

    const pairs = [...cookies.values()]
      .filter(
        (cookie) =>
          cookieLineMatchesHost(
            cookie.rawDomain,
            cookie.includeSubdomains,
            normalizedHost
          ) &&
          cookiePathMatches(cookie.path, requestPath) &&
          (!cookie.secure || isSecure) &&
          !isExpiredCookie(cookie.expires)
      )
      .sort((left, right) => right.path.length - left.path.length)
      .map((cookie) => cookie.pair);

    return pairs.length > 0 ? pairs.join("; ") : null;
  } catch (error) {
    logger.warn(
      `Unable to read cookies file for ${normalizedHost}; continuing without cookies.`,
      error
    );
    return null;
  }
}

/**
 * Build a `Cookie` request header for `requestUrl` from the stored platform cookies
 * (preferring bilibili_cookies.txt for Bilibili and youtube_cookies.txt for YouTube,
 * with fallback to legacy cookies.txt).
 */
export function getCookieHeaderForUrl(requestUrl: string): string | null {
  let normalizedHost: string;
  let requestPath: string;
  let isSecure: boolean;
  try {
    const parsed = new URL(requestUrl);
    if (parsed.protocol !== "https:" && parsed.protocol !== "http:") {
      return null;
    }
    normalizedHost = parsed.hostname.toLowerCase();
    requestPath = parsed.pathname || "/";
    isSecure = parsed.protocol === "https:";
  } catch {
    return null;
  }
  if (!normalizedHost) {
    return null;
  }

  const isBilibiliHost =
    normalizedHost === "bilibili.com" ||
    normalizedHost.endsWith(".bilibili.com");
  const isYouTubeHost =
    normalizedHost === "youtube.com" ||
    normalizedHost.endsWith(".youtube.com") ||
    normalizedHost === "youtu.be";

  const candidateFilenames = isBilibiliHost
    ? [BILIBILI_COOKIES_FILENAME, COOKIES_FILENAME]
    : isYouTubeHost
      ? [YOUTUBE_COOKIES_FILENAME, COOKIES_FILENAME]
      : [COOKIES_FILENAME];

  const checkedPaths = new Set<string>();
  for (const filename of candidateFilenames) {
    const targetResolvedPath = resolveCookiesPath(filename);
    if (checkedPaths.has(targetResolvedPath)) {
      continue;
    }
    checkedPaths.add(targetResolvedPath);

    const normalizedCookiesPath = ensureCookiesFileIsNormalized(filename);
    if (!normalizedCookiesPath) {
      continue;
    }

    const header = extractMatchingCookiesFromFile(
      normalizedCookiesPath,
      normalizedHost,
      requestPath,
      isSecure
    );
    if (header) {
      return header;
    }
  }

  return null;
}
