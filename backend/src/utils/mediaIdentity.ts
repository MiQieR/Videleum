export type MediaPlatform =
  | "youtube"
  | "bilibili"
  | "twitch"
  | "missav"
  | "local"
  | "cloud"
  | "other"
  | "unknown";

const PLATFORM_ALIASES: Readonly<Record<string, MediaPlatform>> = {
  youtube: "youtube",
  "youtube-dl": "youtube",
  yt: "youtube",
  bilibili: "bilibili",
  bili: "bilibili",
  twitch: "twitch",
  missav: "missav",
  local: "local",
  cloud: "cloud",
  other: "other",
  unknown: "unknown",
};

export function normalizeMediaPlatform(value: unknown): MediaPlatform {
  if (typeof value !== "string") return "unknown";
  return PLATFORM_ALIASES[value.trim().toLowerCase()] ?? "unknown";
}

export interface MediaIdentity {
  platform: MediaPlatform;
  sourceVideoId: string | null;
  sourceUrl?: string | null;
  mediaType: "video" | "audio";
  partNumber?: number | null;
  localVideoId?: string | null;
}

export function normalizeMediaIdentity(input: {
  platform: unknown;
  sourceVideoId?: string | null;
  sourceUrl?: string | null;
  mediaType?: unknown;
  partNumber?: number | null;
  localVideoId?: string | null;
}): MediaIdentity {
  const identity: MediaIdentity = {
    platform: normalizeMediaPlatform(input.platform),
    sourceVideoId: input.sourceVideoId ?? null,
    sourceUrl: input.sourceUrl ?? null,
    mediaType: input.mediaType === "audio" ? "audio" : "video",
    partNumber: input.partNumber ?? null,
  };
  if (input.localVideoId !== undefined) {
    identity.localVideoId = input.localVideoId;
  }
  return identity;
}

export function getMediaIdentityKey(
  identity: Pick<MediaIdentity, "platform" | "sourceVideoId" | "mediaType">,
): string {
  return `${identity.platform}:${identity.sourceVideoId ?? ""}:${identity.mediaType}`;
}

export function isSameMediaIdentity(
  a: Pick<MediaIdentity, "platform" | "sourceVideoId" | "mediaType" | "partNumber">,
  b: Pick<MediaIdentity, "platform" | "sourceVideoId" | "mediaType" | "partNumber">,
): boolean {
  return (
    a.platform === b.platform &&
    (a.sourceVideoId ?? "") === (b.sourceVideoId ?? "") &&
    a.mediaType === b.mediaType &&
    (a.partNumber ?? null) === (b.partNumber ?? null)
  );
}
