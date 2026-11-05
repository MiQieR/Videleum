import {
  BILIBILI_COOKIES_FILENAME,
  COOKIES_FILENAME,
  DATA_DIR,
  YOUTUBE_COOKIES_FILENAME,
} from "../config/paths";
import { FileError, ValidationError } from "../errors/DownloadErrors";
import {
  CookiePlatformHint,
  UnsupportedCookieFormatError,
  normalizeCookiesFileContent,
} from "../utils/cookieFileFormat";
import { logger } from "../utils/logger";
import {
  ensureDirSafeSync,
  moveSafeSync,
  pathExistsSafeSync,
  resolveSafeChildPath,
  unlinkSafeSync,
  writeFileSafeSync,
} from "../utils/security";

export type CookiePlatform = CookiePlatformHint;

const getCookieFilenameForPlatform = (platform?: CookiePlatform): string => {
  if (platform === "youtube") {
    return YOUTUBE_COOKIES_FILENAME;
  }
  if (platform === "bilibili") {
    return BILIBILI_COOKIES_FILENAME;
  }
  return COOKIES_FILENAME;
};

const getCookiesPathForPlatform = (platform?: CookiePlatform): string =>
  resolveSafeChildPath(DATA_DIR, getCookieFilenameForPlatform(platform));

/**
 * Check if cookies file exists (supports default/legacy, youtube, bilibili, or all)
 */
export const checkCookies = (
  platform?: CookiePlatform | "all"
): {
  exists: boolean;
  youtubeExists?: boolean;
  bilibiliExists?: boolean;
} => {
  if (!platform) {
    const legacyPath = getCookiesPathForPlatform();
    return { exists: pathExistsSafeSync(legacyPath, DATA_DIR) };
  }

  const legacyExists = pathExistsSafeSync(
    getCookiesPathForPlatform(),
    DATA_DIR
  );
  const youtubeSpecificExists = pathExistsSafeSync(
    getCookiesPathForPlatform("youtube"),
    DATA_DIR
  );
  const bilibiliSpecificExists = pathExistsSafeSync(
    getCookiesPathForPlatform("bilibili"),
    DATA_DIR
  );

  if (platform === "youtube") {
    return { exists: youtubeSpecificExists || legacyExists };
  }
  if (platform === "bilibili") {
    return { exists: bilibiliSpecificExists || legacyExists };
  }

  const youtubeExists = youtubeSpecificExists || legacyExists;
  const bilibiliExists = bilibiliSpecificExists || legacyExists;
  return {
    exists: youtubeExists || bilibiliExists,
    youtubeExists,
    bilibiliExists,
  };
};

const writeCookiesAtomically = (
  targetPath: string,
  normalizedContent: string
): void => {
  const tempPath = `${targetPath}.tmp`;
  try {
    ensureDirSafeSync(DATA_DIR, DATA_DIR);
    writeFileSafeSync(tempPath, DATA_DIR, normalizedContent, "utf8");
    moveSafeSync(tempPath, DATA_DIR, targetPath, DATA_DIR, {
      overwrite: true,
    });
  } catch (error) {
    if (pathExistsSafeSync(tempPath, DATA_DIR)) {
      unlinkSafeSync(tempPath, DATA_DIR);
    }
    throw error;
  }
};

/**
 * Upload cookies file from memory buffer
 */
export const uploadCookies = (
  fileBuffer: Buffer,
  platform?: CookiePlatform
): void => {
  try {
    const normalizedContent = normalizeCookiesFileContent(
      fileBuffer.toString("utf8"),
      platform
    );
    const targetPath = getCookiesPathForPlatform(platform);
    writeCookiesAtomically(targetPath, normalizedContent);

    // When uploading YouTube cookies, also mirror to legacy cookies.txt so
    // any yt-dlp invocation expecting cookies.txt picks it up automatically.
    if (platform === "youtube") {
      const legacyPath = getCookiesPathForPlatform();
      writeCookiesAtomically(legacyPath, normalizedContent);
    }

    logger.info(
      `Cookies (${platform ?? "default"}) uploaded and saved to ${targetPath}`
    );
  } catch (error: unknown) {
    if (error instanceof UnsupportedCookieFormatError) {
      throw new ValidationError(error.message, "file");
    }
    throw FileError.writeError(
      getCookiesPathForPlatform(platform),
      error instanceof Error ? error.message : String(error)
    );
  }
};

/**
 * Delete cookies file
 */
export const deleteCookies = (platform?: CookiePlatform): void => {
  try {
    if (!platform) {
      const legacyPath = getCookiesPathForPlatform();
      if (pathExistsSafeSync(legacyPath, DATA_DIR)) {
        unlinkSafeSync(legacyPath, DATA_DIR);
        return;
      }
      throw new FileError("Cookies file not found", legacyPath);
    }

    const platformPath = getCookiesPathForPlatform(platform);
    const legacyPath = getCookiesPathForPlatform();
    let deleted = false;

    if (pathExistsSafeSync(platformPath, DATA_DIR)) {
      unlinkSafeSync(platformPath, DATA_DIR);
      deleted = true;
    }

    if (platform === "youtube" && pathExistsSafeSync(legacyPath, DATA_DIR)) {
      unlinkSafeSync(legacyPath, DATA_DIR);
      deleted = true;
    } else if (
      platform === "bilibili" &&
      !deleted &&
      pathExistsSafeSync(legacyPath, DATA_DIR)
    ) {
      unlinkSafeSync(legacyPath, DATA_DIR);
      deleted = true;
    }

    if (!deleted) {
      throw new FileError("Cookies file not found", platformPath);
    }
  } catch (error: unknown) {
    if (error instanceof FileError) {
      throw error;
    }
    throw FileError.writeError(
      getCookiesPathForPlatform(platform),
      error instanceof Error ? error.message : String(error)
    );
  }
};
