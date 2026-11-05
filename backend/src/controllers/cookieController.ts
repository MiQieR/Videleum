import { Request, Response } from "express";
import { ValidationError } from "../errors/DownloadErrors";
import * as cookieService from "../services/cookieService";
import type { CookiePlatform } from "../services/cookieService";
import { successMessage } from "../utils/response";

const parseCookiePlatform = (req: Request): CookiePlatform | undefined => {
  const raw =
    (typeof req.query?.platform === "string" && req.query.platform) ||
    (typeof req.body?.platform === "string" && req.body.platform) ||
    "";
  const normalized = raw.trim().toLowerCase();
  if (normalized === "youtube" || normalized === "bilibili") {
    return normalized;
  }
  return undefined;
};

/**
 * Upload cookies file
 */
export const uploadCookies = async (
  req: Request,
  res: Response
): Promise<void> => {
  if (!req.file) {
    throw new ValidationError("No file uploaded", "file");
  }
  if (!req.file.buffer) {
    throw new ValidationError("Uploaded file buffer is missing", "file");
  }

  const platform = parseCookiePlatform(req);
  if (platform) {
    cookieService.uploadCookies(req.file.buffer, platform);
  } else {
    cookieService.uploadCookies(req.file.buffer);
  }
  res.json(successMessage("Cookies uploaded successfully"));
};

/**
 * Delete cookies file
 */
export const deleteCookies = async (
  req: Request,
  res: Response
): Promise<void> => {
  const platform = parseCookiePlatform(req);
  if (platform) {
    cookieService.deleteCookies(platform);
  } else {
    cookieService.deleteCookies();
  }
  res.json(successMessage("Cookies deleted successfully"));
};

/**
 * Check if cookies file exists
 */
export const checkCookies = async (
  req: Request,
  res: Response
): Promise<void> => {
  const rawPlatform =
    typeof req.query?.platform === "string"
      ? req.query.platform.trim().toLowerCase()
      : "";

  if (
    rawPlatform === "youtube" ||
    rawPlatform === "bilibili" ||
    rawPlatform === "all"
  ) {
    const result = cookieService.checkCookies(rawPlatform);
    res.json(result);
    return;
  }

  const result = cookieService.checkCookies();
  res.json(result);
};
