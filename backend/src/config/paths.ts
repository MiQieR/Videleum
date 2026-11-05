import fs from "fs-extra";
import os from "os";
import path from "path";

// Assuming the application is started from the 'backend' directory
export const ROOT_DIR: string = process.cwd();

const isDesktopPackagedRuntime = (): boolean =>
  process.env.VIDELEUM_DESKTOP === "1" ||
  ROOT_DIR.includes(".app/Contents/Resources");

const resolveDefaultDataDir = (): string => {
  if (process.env.VIDELEUM_BACKEND_DATA_DIR) {
    return path.resolve(process.env.VIDELEUM_BACKEND_DATA_DIR);
  }
  if (process.env.VIDELEUM_DATA_DIR) {
    return path.resolve(process.env.VIDELEUM_DATA_DIR);
  }
  if (isDesktopPackagedRuntime()) {
    if (process.platform === "darwin") {
      return path.join(
        os.homedir(),
        "Library",
        "Application Support",
        "Videleum",
        "data"
      );
    }
    if (process.platform === "win32") {
      const appData =
        process.env.APPDATA || path.join(os.homedir(), "AppData", "Roaming");
      return path.join(appData, "Videleum", "data");
    }
  }
  return path.join(ROOT_DIR, "data");
};

export const DATA_DIR: string = resolveDefaultDataDir();

export const STORAGE_CONFIG_PATH: string = path.join(
  DATA_DIR,
  "storage-config.json"
);

export const getDefaultUploadsDir = (): string => {
  if (process.env.VIDELEUM_UPLOADS_DIR) {
    return path.resolve(process.env.VIDELEUM_UPLOADS_DIR);
  }
  if (isDesktopPackagedRuntime()) {
    if (process.platform === "darwin") {
      return path.join(os.homedir(), "Movies", "Videleum");
    }
    if (process.platform === "win32") {
      return path.join(os.homedir(), "Videos", "Videleum");
    }
  }
  return path.join(ROOT_DIR, "uploads");
};

export const readCustomUploadsDirFromConfig = (): string | null => {
  if (process.env.NODE_ENV === "test" || process.env.VITEST === "true") {
    return null;
  }
  try {
    if (fs.existsSync(STORAGE_CONFIG_PATH)) {
      const raw = fs.readFileSync(STORAGE_CONFIG_PATH, "utf8");
      const parsed = JSON.parse(raw) as { customUploadsDir?: unknown };
      if (
        typeof parsed.customUploadsDir === "string" &&
        parsed.customUploadsDir.trim().length > 0
      ) {
        return path.resolve(parsed.customUploadsDir.trim());
      }
    }
  } catch {
    // Ignore malformed storage-config.json and fall back to default
  }
  return null;
};

const initialUploadsDir =
  readCustomUploadsDirFromConfig() || getDefaultUploadsDir();

export let UPLOADS_DIR: string = initialUploadsDir;
export let VIDEOS_DIR: string = path.join(UPLOADS_DIR, "videos");
export let IMAGES_DIR: string = path.join(UPLOADS_DIR, "images");
export let IMAGES_SMALL_DIR: string = path.join(UPLOADS_DIR, "images-small");
export let AVATARS_DIR: string = path.join(UPLOADS_DIR, "avatars");
export let SUBTITLES_DIR: string = path.join(UPLOADS_DIR, "subtitles");
export let CLOUD_THUMBNAIL_CACHE_DIR: string = path.join(
  UPLOADS_DIR,
  "cloud-thumbnail-cache"
);

/**
 * 更新媒体文件存储根目录（在运行时动态生效并持久化到 storage-config.json）
 */
export const setCustomUploadsDir = (
  customDir: string | null
): {
  uploadsDir: string;
  videosDir: string;
  imagesDir: string;
  subtitlesDir: string;
  customUploadsDir: string | null;
  defaultUploadsDir: string;
  isCustom: boolean;
} => {
  const defaultUploadsDir = getDefaultUploadsDir();
  const normalizedCustom =
    typeof customDir === "string" && customDir.trim().length > 0
      ? path.resolve(customDir.trim())
      : null;

  const effectiveUploadsDir = normalizedCustom || defaultUploadsDir;

  fs.ensureDirSync(DATA_DIR);
  fs.writeFileSync(
    STORAGE_CONFIG_PATH,
    JSON.stringify(
      { customUploadsDir: normalizedCustom ?? "" },
      null,
      2
    ),
    "utf8"
  );

  UPLOADS_DIR = effectiveUploadsDir;
  VIDEOS_DIR = path.join(UPLOADS_DIR, "videos");
  IMAGES_DIR = path.join(UPLOADS_DIR, "images");
  IMAGES_SMALL_DIR = path.join(UPLOADS_DIR, "images-small");
  AVATARS_DIR = path.join(UPLOADS_DIR, "avatars");
  SUBTITLES_DIR = path.join(UPLOADS_DIR, "subtitles");
  CLOUD_THUMBNAIL_CACHE_DIR = path.join(UPLOADS_DIR, "cloud-thumbnail-cache");

  fs.ensureDirSync(VIDEOS_DIR);
  fs.ensureDirSync(IMAGES_DIR);
  fs.ensureDirSync(IMAGES_SMALL_DIR);
  fs.ensureDirSync(AVATARS_DIR);
  fs.ensureDirSync(SUBTITLES_DIR);
  fs.ensureDirSync(CLOUD_THUMBNAIL_CACHE_DIR);

  return {
    uploadsDir: UPLOADS_DIR,
    videosDir: VIDEOS_DIR,
    imagesDir: IMAGES_DIR,
    subtitlesDir: SUBTITLES_DIR,
    customUploadsDir: normalizedCustom,
    defaultUploadsDir,
    isCustom: Boolean(normalizedCustom),
  };
};

export const getStoragePathInfo = (): {
  uploadsDir: string;
  videosDir: string;
  imagesDir: string;
  subtitlesDir: string;
  customUploadsDir: string | null;
  defaultUploadsDir: string;
  defaultVideosDir: string;
  isCustom: boolean;
  dataDir: string;
} => {
  const defaultUploadsDir = getDefaultUploadsDir();
  const customUploadsDir = readCustomUploadsDirFromConfig();
  return {
    uploadsDir: UPLOADS_DIR,
    videosDir: VIDEOS_DIR,
    imagesDir: IMAGES_DIR,
    subtitlesDir: SUBTITLES_DIR,
    customUploadsDir,
    defaultUploadsDir,
    defaultVideosDir: path.join(defaultUploadsDir, "videos"),
    isCustom: Boolean(customUploadsDir),
    dataDir: DATA_DIR,
  };
};

export const COOKIES_FILENAME = "cookies.txt";
export const YOUTUBE_COOKIES_FILENAME = "youtube_cookies.txt";
export const BILIBILI_COOKIES_FILENAME = "bilibili_cookies.txt";

export const VIDEOS_DATA_PATH: string = path.join(DATA_DIR, "videos.json");
export const STATUS_DATA_PATH: string = path.join(DATA_DIR, "status.json");
export const COLLECTIONS_DATA_PATH: string = path.join(
  DATA_DIR,
  "collections.json"
);
export const HOOKS_DIR: string = path.join(DATA_DIR, "hooks");
