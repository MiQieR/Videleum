import axios from "axios";
import { spawn, type ChildProcess } from "child_process";
import fs from "fs-extra";
import path from "path";
import { extractBilibiliVideoId } from "../../../utils/helpers";
import { logger } from "../../../utils/logger";
import * as storageService from "../../storageService";
import { buildBilibiliApiHeaders } from "./bilibiliHeaders";
import { buildSignedBilibiliUrl } from "./bilibiliWbi";

export interface BilibiliDashStream {
  id: number;
  baseUrl?: string;
  base_url?: string;
  backupUrl?: string[];
  backup_url?: string[];
  bandwidth?: number;
  mimeType?: string;
  mime_type?: string;
  codecs?: string;
  width?: number;
  height?: number;
  codecid?: number;
}

export interface BilibiliDurlSegment {
  order: number;
  length?: number;
  size?: number;
  url: string;
  backup_url?: string[];
}

export interface BilibiliPlayUrlData {
  quality?: number;
  timelength?: number;
  dash?: {
    duration?: number;
    video?: BilibiliDashStream[];
    audio?: BilibiliDashStream[] | null;
    dolby?: {
      audio?: BilibiliDashStream[] | null;
    } | null;
    flac?: {
      audio?: BilibiliDashStream | null;
    } | null;
  };
  durl?: BilibiliDurlSegment[];
}

export interface BilibiliViewMetadata {
  bvid: string;
  aid: number;
  cid: number;
  title: string;
  partTitle?: string;
  author: string;
  authorMid?: number | string;
  authorAvatarUrl?: string;
  description: string;
  thumbnailUrl: string | null;
  uploadDate: string;
  duration?: number;
  canonicalUrl: string;
}

export interface SelectedBilibiliStreams {
  mode: "dash" | "durl";
  videoStream?: BilibiliDashStream;
  audioStream?: BilibiliDashStream;
  durlSegments?: BilibiliDurlSegment[];
}

const FFMPEG_CANDIDATE_PATHS = [
  process.env.FFMPEG_PATH,
  "/opt/homebrew/bin/ffmpeg",
  "/usr/local/bin/ffmpeg",
  "/usr/bin/ffmpeg",
].filter((candidate): candidate is string => Boolean(candidate));

export function resolveFfmpegBinary(): string {
  for (const candidate of FFMPEG_CANDIDATE_PATHS) {
    try {
      if (fs.existsSync(candidate)) {
        return candidate;
      }
    } catch {
      // Ignore inaccessible candidate
    }
  }
  return "ffmpeg";
}

function normalizeBilibiliAssetUrl(rawUrl: unknown): string | null {
  if (typeof rawUrl !== "string" || !rawUrl.trim()) {
    return null;
  }
  const trimmed = rawUrl.trim();
  if (trimmed.startsWith("//")) {
    return `https:${trimmed}`;
  }
  if (trimmed.startsWith("http://")) {
    return `https://${trimmed.slice("http://".length)}`;
  }
  return trimmed;
}

function formatUnixDateYYYYMMDD(pubdate: unknown): string {
  const num = Number(pubdate);
  if (Number.isFinite(num) && num > 0) {
    return new Date(num * 1000).toISOString().slice(0, 10).replace(/-/g, "");
  }
  return new Date().toISOString().slice(0, 10).replace(/-/g, "");
}

function formatBytesHuman(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex++;
  }
  return `${value.toFixed(unitIndex === 0 ? 0 : 2)}${units[unitIndex]}`;
}

function extractPartNumberFromUrl(url: string): number {
  try {
    const parsed = new URL(url);
    const pParam = parsed.searchParams.get("p");
    if (pParam) {
      const parsedInt = Number.parseInt(pParam, 10);
      if (Number.isFinite(parsedInt) && parsedInt >= 1) {
        return parsedInt;
      }
    }
  } catch {
    const match = url.match(/[?&]p=(\d+)/i);
    if (match?.[1]) {
      const parsedInt = Number.parseInt(match[1], 10);
      if (Number.isFinite(parsedInt) && parsedInt >= 1) {
        return parsedInt;
      }
    }
  }
  return 1;
}

function isRiskControlPayload(payload: any): boolean {
  const code = payload?.code;
  return (
    code === -352 ||
    code === -412 ||
    Boolean(payload?.data?.v_voucher)
  );
}

async function getWithAnonymousRiskRetry<T = any>(
  url: string,
  axiosConfig: Record<string, unknown>
): Promise<{ data: T; responseUrl?: string }> {
  const headers = buildBilibiliApiHeaders(url);
  const hasCookie = Boolean(headers.Cookie);

  try {
    const response = await axios.get<T>(url, {
      ...axiosConfig,
      headers,
      timeout: 15000,
    });
    if (hasCookie && isRiskControlPayload(response.data)) {
      const anonymousHeaders = { ...headers };
      delete anonymousHeaders.Cookie;
      const retryResponse = await axios.get<T>(url, {
        ...axiosConfig,
        headers: anonymousHeaders,
        timeout: 15000,
      });
      return {
        data: retryResponse.data,
        responseUrl: (retryResponse.request as any)?.res?.responseUrl,
      };
    }
    return {
      data: response.data,
      responseUrl: (response.request as any)?.res?.responseUrl,
    };
  } catch (error: any) {
    if (hasCookie && error?.response?.status === 412) {
      const anonymousHeaders = { ...headers };
      delete anonymousHeaders.Cookie;
      const retryResponse = await axios.get<T>(url, {
        ...axiosConfig,
        headers: anonymousHeaders,
        timeout: 15000,
      });
      return {
        data: retryResponse.data,
        responseUrl: (retryResponse.request as any)?.res?.responseUrl,
      };
    }
    throw error;
  }
}

/**
 * Resolve Bilibili video URL (including b23.tv short links) and fetch metadata + CID
 * from `https://api.bilibili.com/x/web-interface/view`.
 */
export async function fetchBilibiliViewMetadata(
  inputUrl: string,
  axiosConfig: Record<string, unknown> = {}
): Promise<BilibiliViewMetadata> {
  let effectiveUrl = inputUrl;
  let videoId = extractBilibiliVideoId(effectiveUrl);

  if (!videoId) {
    const resolved = await axios.get(inputUrl, {
      ...axiosConfig,
      headers: buildBilibiliApiHeaders(inputUrl),
      maxRedirects: 5,
      timeout: 10000,
      validateStatus: (status) => status >= 200 && status < 400,
    });
    const redirectedUrl =
      (resolved.request as any)?.res?.responseUrl ||
      resolved.headers?.location ||
      inputUrl;
    if (typeof redirectedUrl === "string" && redirectedUrl) {
      effectiveUrl = redirectedUrl;
      videoId = extractBilibiliVideoId(effectiveUrl);
    }
  }

  if (!videoId) {
    throw new Error(`Unable to extract Bilibili BV/av ID from URL: ${inputUrl}`);
  }

  const partNumber = extractPartNumberFromUrl(inputUrl) || extractPartNumberFromUrl(effectiveUrl);
  const isBv = videoId.startsWith("BV");
  const viewApiUrl = isBv
    ? `https://api.bilibili.com/x/web-interface/view?bvid=${videoId}`
    : `https://api.bilibili.com/x/web-interface/view?aid=${videoId.replace(/^av/i, "")}`;

  const { data: payload } = await getWithAnonymousRiskRetry<any>(
    viewApiUrl,
    axiosConfig
  );

  if (!payload || payload.code !== 0 || !payload.data) {
    const code = payload?.code ?? "unknown";
    const msg = payload?.message || payload?.msg || "Failed to fetch Bilibili video view metadata";
    throw new Error(`Bilibili view API error (code ${code}): ${msg}`);
  }

  const view = payload.data;
  const bvid: string = view.bvid || (isBv ? videoId : "");
  const aid: number = Number(view.aid || (!isBv ? videoId.replace(/^av/i, "") : 0));
  const pages: Array<{ page: number; cid: number; part?: string; duration?: number }> =
    Array.isArray(view.pages) ? view.pages : [];
  const selectedPage =
    pages.find((p) => p.page === partNumber) ||
    pages[partNumber - 1] ||
    pages[0];
  const cid = Number(selectedPage?.cid || view.cid || 0);

  if (!cid) {
    throw new Error(`Bilibili video ${videoId} (part ${partNumber}) did not return a valid cid`);
  }

  const canonicalUrl = bvid
    ? `https://www.bilibili.com/video/${bvid}${partNumber > 1 ? `?p=${partNumber}` : ""}`
    : effectiveUrl;

  return {
    bvid,
    aid,
    cid,
    title: view.title || "Bilibili Video",
    partTitle: selectedPage?.part,
    author: view.owner?.name || "Bilibili User",
    authorMid: view.owner?.mid,
    authorAvatarUrl: normalizeBilibiliAssetUrl(view.owner?.face) ?? undefined,
    description: view.desc || "",
    thumbnailUrl: normalizeBilibiliAssetUrl(view.pic),
    uploadDate: formatUnixDateYYYYMMDD(view.pubdate),
    duration: Number(selectedPage?.duration || view.duration || 0) || undefined,
    canonicalUrl,
  };
}

/**
 * Fetch Bilibili DASH / durl playurl streams using WBI signing (`x/player/wbi/playurl`),
 * following `bili-sync` and `bilibili-video-downloader`.
 */
export async function fetchBilibiliPlayUrlData(
  meta: Pick<BilibiliViewMetadata, "bvid" | "aid" | "cid">,
  axiosConfig: Record<string, unknown> = {}
): Promise<BilibiliPlayUrlData> {
  const queryParams: Record<string, string | number> = {
    cid: meta.cid,
    qn: 127,
    fnval: 4048,
    fnver: 0,
    fourk: 1,
  };
  if (meta.bvid) {
    queryParams.bvid = meta.bvid;
  } else if (meta.aid) {
    queryParams.avid = meta.aid;
  }

  const wbiEndpoint = "https://api.bilibili.com/x/player/wbi/playurl";
  let requestUrl: string;
  try {
    requestUrl = await buildSignedBilibiliUrl(wbiEndpoint, queryParams, {
      ...axiosConfig,
      headers: buildBilibiliApiHeaders(
        "https://api.bilibili.com/x/web-interface/nav"
      ),
      timeout: 10000,
    });
  } catch (wbiError) {
    logger.warn(
      "WBI signing for Bilibili playurl failed, falling back to unsigned playurl:",
      wbiError
    );
    const search = new URLSearchParams(
      Object.entries(queryParams).map(([k, v]) => [k, String(v)])
    );
    requestUrl = `https://api.bilibili.com/x/player/playurl?${search.toString()}`;
  }

  const { data: payload } = await getWithAnonymousRiskRetry<any>(
    requestUrl,
    axiosConfig
  );

  if (!payload || payload.code !== 0 || !payload.data) {
    const code = payload?.code ?? "unknown";
    const msg = payload?.message || payload?.msg || "Failed to fetch Bilibili playurl";
    throw new Error(`Bilibili playurl API error (code ${code}): ${msg}`);
  }

  return payload.data as BilibiliPlayUrlData;
}

function getStreamCandidateUrls(
  stream: BilibiliDashStream | BilibiliDurlSegment
): string[] {
  const primary =
    (stream as BilibiliDashStream).baseUrl ||
    (stream as BilibiliDashStream).base_url ||
    (stream as BilibiliDurlSegment).url;
  const backups =
    (stream as BilibiliDashStream).backupUrl ||
    (stream as BilibiliDashStream).backup_url ||
    [];
  return [primary, ...(Array.isArray(backups) ? backups : [])].filter(
    (u): u is string => typeof u === "string" && u.trim().length > 0
  );
}

function resolveCodecPriority(
  codecid: number | undefined,
  preferredCodecHint?: string
): number {
  const hint = (preferredCodecHint || "").toLowerCase();
  if (hint.includes("hevc") || hint.includes("h265") || hint.includes("hev1")) {
    if (codecid === 12) return 3;
    if (codecid === 7) return 2;
    if (codecid === 13) return 1;
  } else if (hint.includes("av01") || hint.includes("av1")) {
    if (codecid === 13) return 3;
    if (codecid === 12) return 2;
    if (codecid === 7) return 1;
  }
  // Default: AVC (7) > HEVC (12) > AV1 (13) for broadest playback compatibility
  if (codecid === 7) return 3;
  if (codecid === 12) return 2;
  if (codecid === 13) return 1;
  return 0;
}

/**
 * Select the optimal video and audio stream from Bilibili PlayURL response.
 */
export function selectBilibiliMediaStreams(
  playData: BilibiliPlayUrlData,
  options: {
    preferredHeight?: number | null;
    retryFloorHeight?: number | null;
    preferredCodecHint?: string;
    audioOnly?: boolean;
  } = {}
): SelectedBilibiliStreams {
  if (playData.dash) {
    const allAudioCandidates: BilibiliDashStream[] = [
      ...(playData.dash.flac?.audio ? [playData.dash.flac.audio] : []),
      ...(Array.isArray(playData.dash.dolby?.audio)
        ? playData.dash.dolby!.audio!
        : []),
      ...(Array.isArray(playData.dash.audio) ? playData.dash.audio : []),
    ].filter((s) => getStreamCandidateUrls(s).length > 0);

    allAudioCandidates.sort(
      (a, b) => (b.bandwidth || 0) - (a.bandwidth || 0)
    );
    const bestAudio = allAudioCandidates[0];

    if (options.audioOnly && bestAudio) {
      return {
        mode: "dash",
        audioStream: bestAudio,
      };
    }

    const videoCandidates = (playData.dash.video || []).filter(
      (s) => getStreamCandidateUrls(s).length > 0
    );

    if (videoCandidates.length > 0) {
      let filtered = videoCandidates;

      if (
        typeof options.retryFloorHeight === "number" &&
        options.retryFloorHeight > 0
      ) {
        const aboveFloor = filtered.filter(
          (v) => (v.height || 0) >= options.retryFloorHeight!
        );
        if (aboveFloor.length > 0) {
          filtered = aboveFloor;
        }
      } else if (
        typeof options.preferredHeight === "number" &&
        options.preferredHeight > 0
      ) {
        const withinCap = filtered.filter(
          (v) => (v.height || 0) > 0 && (v.height || 0) <= options.preferredHeight!
        );
        if (withinCap.length > 0) {
          filtered = withinCap;
        }
      }

      filtered.sort((a, b) => {
        const qualityDiff = (b.id || 0) - (a.id || 0);
        if (qualityDiff !== 0) return qualityDiff;
        const heightDiff = (b.height || 0) - (a.height || 0);
        if (heightDiff !== 0) return heightDiff;
        const codecDiff =
          resolveCodecPriority(b.codecid, options.preferredCodecHint) -
          resolveCodecPriority(a.codecid, options.preferredCodecHint);
        if (codecDiff !== 0) return codecDiff;
        return (b.bandwidth || 0) - (a.bandwidth || 0);
      });

      return {
        mode: "dash",
        videoStream: filtered[0],
        audioStream: bestAudio,
      };
    }
  }

  if (Array.isArray(playData.durl) && playData.durl.length > 0) {
    return {
      mode: "durl",
      durlSegments: playData.durl,
    };
  }

  throw new Error("No downloadable DASH or durl media streams found in Bilibili playurl response");
}

/**
 * Stream-download a single media track with mirror failover and real-time progress updates.
 */
async function downloadStreamWithMirrorFailover(params: {
  candidateUrls: string[];
  destinationPath: string;
  axiosConfig: Record<string, unknown>;
  abortSignal: AbortSignal;
  checkCancelled: () => void;
  onProgressDelta: (deltaBytes: number, streamTotalBytes: number) => void;
}): Promise<void> {
  const {
    candidateUrls,
    destinationPath,
    axiosConfig,
    abortSignal,
    checkCancelled,
    onProgressDelta,
  } = params;

  let lastError: unknown = null;

  for (let i = 0; i < candidateUrls.length; i++) {
    const streamUrl = candidateUrls[i];
    checkCancelled();

    let downloadedForAttempt = 0;
    let reportedStreamTotal = 0;

    try {
      const headers = {
        ...buildBilibiliApiHeaders(streamUrl),
        Referer: "https://www.bilibili.com/",
        Origin: "https://www.bilibili.com",
      };

      const response = await axios.get(streamUrl, {
        ...axiosConfig,
        headers,
        responseType: "stream",
        timeout: 30000,
        maxRedirects: 5,
        signal: abortSignal,
      });

      const contentLengthHeader = response.headers?.["content-length"];
      const streamTotalBytes = contentLengthHeader
        ? Number.parseInt(String(contentLengthHeader), 10) || 0
        : 0;
      reportedStreamTotal = streamTotalBytes;
      if (streamTotalBytes > 0) {
        onProgressDelta(0, streamTotalBytes);
      }

      await new Promise<void>((resolve, reject) => {
        const fileStream = fs.createWriteStream(destinationPath);
        const readable = response.data;

        const cleanupStreams = (err?: unknown) => {
          try {
            readable.destroy?.();
          } catch {
            // ignore
          }
          try {
            fileStream.destroy();
          } catch {
            // ignore
          }
          if (err) {
            reject(err);
          }
        };

        readable.on("data", (chunk: Buffer) => {
          try {
            checkCancelled();
            downloadedForAttempt += chunk.length;
            onProgressDelta(chunk.length, 0);
          } catch (cancelErr) {
            cleanupStreams(cancelErr);
          }
        });

        readable.on("error", (err: unknown) => {
          cleanupStreams(err);
        });

        fileStream.on("error", (err: unknown) => {
          cleanupStreams(err);
        });

        fileStream.on("finish", () => {
          resolve();
        });

        readable.pipe(fileStream);
      });

      return;
    } catch (error) {
      // Roll back progress bytes counted during this failed mirror attempt
      if (downloadedForAttempt > 0 || reportedStreamTotal > 0) {
        onProgressDelta(-downloadedForAttempt, -reportedStreamTotal);
      }
      try {
        if (fs.existsSync(destinationPath)) {
          fs.unlinkSync(destinationPath);
        }
      } catch {
        // ignore cleanup error
      }

      checkCancelled();
      lastError = error;
      logger.warn(
        `Bilibili stream mirror ${i + 1}/${candidateUrls.length} failed, trying next mirror if available:`,
        error instanceof Error ? error.message : String(error)
      );
    }
  }

  throw lastError instanceof Error
    ? lastError
    : new Error(`All Bilibili stream mirrors failed for ${destinationPath}`);
}

/**
 * Remux video and audio streams (or convert single stream) into outputFilePath using FFmpeg.
 */
export async function muxBilibiliStreamsWithFfmpeg(params: {
  videoInputPath?: string;
  audioInputPath?: string;
  outputFilePath: string;
  mergeOutputFormat: string;
  onRegisterKill: (killFn: () => void) => void;
  checkCancelled: () => void;
}): Promise<void> {
  const {
    videoInputPath,
    audioInputPath,
    outputFilePath,
    mergeOutputFormat,
    onRegisterKill,
    checkCancelled,
  } = params;

  checkCancelled();

  const ffmpegBin = resolveFfmpegBinary();
  const args: string[] = ["-y", "-hide_banner", "-loglevel", "error"];

  if (videoInputPath) {
    args.push("-i", videoInputPath);
  }
  if (audioInputPath) {
    args.push("-i", audioInputPath);
  }

  args.push("-c", "copy");
  if (mergeOutputFormat.toLowerCase() === "mp4") {
    args.push("-movflags", "+faststart");
  }
  args.push(outputFilePath);

  await new Promise<void>((resolve, reject) => {
    let child: ChildProcess | null = null;
    let stderrText = "";

    try {
      child = spawn(ffmpegBin, args, {
        stdio: ["ignore", "pipe", "pipe"],
      });
    } catch (spawnErr) {
      reject(spawnErr);
      return;
    }

    onRegisterKill(() => {
      if (child && !child.killed) {
        child.kill("SIGTERM");
      }
    });

    child.stderr?.on("data", (chunk: Buffer) => {
      stderrText += chunk.toString();
    });

    child.on("error", (err) => {
      reject(
        new Error(
          `Failed to run ffmpeg (${ffmpegBin}): ${err.message}`
        )
      );
    });

    child.on("close", (code, signal) => {
      if (code === 0) {
        resolve();
      } else {
        reject(
          new Error(
            `ffmpeg muxing exited with ${signal ? `signal ${signal}` : `code ${code}`}${
              stderrText.trim() ? `: ${stderrText.trim()}` : ""
            }`
          )
        );
      }
    });
  });
}

/**
 * Execute the full native Bilibili download pipeline (View API -> WBI PlayURL -> HTTP Stream Download -> FFmpeg Mux).
 */
export async function downloadBilibiliVideoNative(params: {
  url: string;
  tempDir: string;
  mergeOutputFormat: string;
  axiosConfig: Record<string, unknown>;
  preferredHeight?: number | null;
  retryFloorHeight?: number | null;
  preferredCodecHint?: string;
  downloadId?: string;
  onStart?: (cancel: () => void) => void;
  checkCancelled: () => void;
}): Promise<{
  metadata: BilibiliViewMetadata;
  outputFilename: string;
}> {
  const {
    url,
    tempDir,
    mergeOutputFormat,
    axiosConfig,
    preferredHeight,
    retryFloorHeight,
    preferredCodecHint,
    downloadId,
    onStart,
    checkCancelled,
  } = params;

  const abortController = new AbortController();
  let killFfmpeg: (() => void) | null = null;

  if (onStart) {
    onStart(() => {
      abortController.abort();
      if (killFfmpeg) {
        killFfmpeg();
      }
    });
  }

  checkCancelled();

  // 1. Fetch video & part metadata via x/web-interface/view
  const metadata = await fetchBilibiliViewMetadata(url, axiosConfig);
  checkCancelled();

  if (downloadId) {
    storageService.updateActiveDownload(downloadId, {
      title: metadata.title,
      filename: metadata.title,
      progress: 0,
      speed: "0 B/s",
    });
  }

  // 2. Fetch DASH/durl streams via WBI x/player/wbi/playurl
  const playData = await fetchBilibiliPlayUrlData(metadata, axiosConfig);
  checkCancelled();

  const selected = selectBilibiliMediaStreams(playData, {
    preferredHeight,
    retryFloorHeight,
    preferredCodecHint,
  });

  // Shared progress state across video + audio tracks
  let totalDownloadedBytes = 0;
  let totalExpectedBytes = 0;
  let lastReportTime = Date.now();
  let lastReportedBytes = 0;

  const reportProgress = (force = false) => {
    if (!downloadId) return;
    const now = Date.now();
    const elapsedMs = now - lastReportTime;
    if (!force && elapsedMs < 200) {
      return;
    }

    const deltaBytes = Math.max(0, totalDownloadedBytes - lastReportedBytes);
    const speedBps = elapsedMs > 0 ? (deltaBytes * 1000) / elapsedMs : 0;
    lastReportTime = now;
    lastReportedBytes = totalDownloadedBytes;

    const progress =
      totalExpectedBytes > 0
        ? Math.min(
            99,
            Math.round((totalDownloadedBytes / totalExpectedBytes) * 1000) / 10
          )
        : 0;

    storageService.updateActiveDownload(downloadId, {
      title: metadata.title,
      filename: metadata.title,
      progress,
      speed: `${formatBytesHuman(speedBps)}/s`,
      downloadedSize: formatBytesHuman(totalDownloadedBytes),
      totalSize:
        totalExpectedBytes > 0 ? formatBytesHuman(totalExpectedBytes) : "?",
    });
  };

  const onProgressDelta = (deltaBytes: number, streamTotalDelta: number) => {
    totalDownloadedBytes = Math.max(0, totalDownloadedBytes + deltaBytes);
    totalExpectedBytes = Math.max(0, totalExpectedBytes + streamTotalDelta);
    reportProgress(false);
  };

  const ext = (mergeOutputFormat || "mp4").replace(/^\./, "");
  const outputFilename = `video.${ext}`;
  const outputFilePath = path.join(tempDir, outputFilename);

  if (selected.mode === "dash") {
    const tempVideoPath = selected.videoStream
      ? path.join(tempDir, "bilibili_video_track.m4s")
      : undefined;
    const tempAudioPath = selected.audioStream
      ? path.join(tempDir, "bilibili_audio_track.m4s")
      : undefined;

    if (selected.videoStream && tempVideoPath) {
      await downloadStreamWithMirrorFailover({
        candidateUrls: getStreamCandidateUrls(selected.videoStream),
        destinationPath: tempVideoPath,
        axiosConfig,
        abortSignal: abortController.signal,
        checkCancelled,
        onProgressDelta,
      });
    }

    if (selected.audioStream && tempAudioPath) {
      await downloadStreamWithMirrorFailover({
        candidateUrls: getStreamCandidateUrls(selected.audioStream),
        destinationPath: tempAudioPath,
        axiosConfig,
        abortSignal: abortController.signal,
        checkCancelled,
        onProgressDelta,
      });
    }

    reportProgress(true);
    checkCancelled();

    await muxBilibiliStreamsWithFfmpeg({
      videoInputPath: tempVideoPath,
      audioInputPath: tempAudioPath,
      outputFilePath,
      mergeOutputFormat: ext,
      onRegisterKill: (fn) => {
        killFfmpeg = fn;
      },
      checkCancelled,
    });

    // Clean up raw DASH track files
    for (const rawTrack of [tempVideoPath, tempAudioPath]) {
      if (rawTrack && fs.existsSync(rawTrack)) {
        try {
          fs.unlinkSync(rawTrack);
        } catch {
          // ignore
        }
      }
    }
  } else if (selected.mode === "durl" && selected.durlSegments?.length) {
    const firstSegment = selected.durlSegments[0];
    const tempSegmentPath = path.join(tempDir, "bilibili_durl_segment.mp4");
    await downloadStreamWithMirrorFailover({
      candidateUrls: getStreamCandidateUrls(firstSegment),
      destinationPath: tempSegmentPath,
      axiosConfig,
      abortSignal: abortController.signal,
      checkCancelled,
      onProgressDelta,
    });

    reportProgress(true);
    checkCancelled();

    if (ext === "mp4") {
      fs.moveSync(tempSegmentPath, outputFilePath, { overwrite: true });
    } else {
      await muxBilibiliStreamsWithFfmpeg({
        videoInputPath: tempSegmentPath,
        outputFilePath,
        mergeOutputFormat: ext,
        onRegisterKill: (fn) => {
          killFfmpeg = fn;
        },
        checkCancelled,
      });
      if (fs.existsSync(tempSegmentPath)) {
        fs.unlinkSync(tempSegmentPath);
      }
    }
  }

  if (downloadId) {
    storageService.updateActiveDownload(downloadId, {
      title: metadata.title,
      filename: metadata.title,
      progress: 100,
      speed: "0 B/s",
      downloadedSize: formatBytesHuman(totalDownloadedBytes),
      totalSize: formatBytesHuman(
        totalExpectedBytes || totalDownloadedBytes
      ),
    });
  }

  return {
    metadata,
    outputFilename,
  };
}
