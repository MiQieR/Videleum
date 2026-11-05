export interface PlatformVisualConfig {
  displayName: string;
  primaryColor: string;
  badgeBgColor: string;
  badgeTextColor: string;
  pillBorderColor?: string;
}

export const PLATFORM_VISUAL_MAP: Record<string, PlatformVisualConfig> = {
  youtube: {
    displayName: 'YouTube',
    primaryColor: '#FF0000',
    badgeBgColor: 'rgba(255, 0, 0, 0.92)',
    badgeTextColor: '#FFFFFF',
    pillBorderColor: '#FF0000',
  },
  bilibili: {
    displayName: 'Bilibili',
    primaryColor: '#00AEEC',
    badgeBgColor: 'rgba(0, 174, 236, 0.92)',
    badgeTextColor: '#FFFFFF',
    pillBorderColor: '#00AEEC',
  },
  twitch: {
    displayName: 'Twitch',
    primaryColor: '#9146FF',
    badgeBgColor: 'rgba(145, 70, 255, 0.92)',
    badgeTextColor: '#FFFFFF',
    pillBorderColor: '#9146FF',
  },
};

export function normalizePlatform(platform?: string | null): string {
  if (!platform) return 'unknown';
  const lower = platform.trim().toLowerCase();
  if (lower === 'yt' || lower === 'youtube') return 'youtube';
  if (lower === 'bili' || lower === 'bilibili') return 'bilibili';
  if (lower === 'twitch') return 'twitch';
  return lower;
}

export function getPlatformDisplayName(platform?: string | null): string {
  const norm = normalizePlatform(platform);
  const cfg = PLATFORM_VISUAL_MAP[norm];
  if (cfg) return cfg.displayName;
  if (!platform || norm === 'unknown') return 'Unknown';
  return platform.charAt(0).toUpperCase() + platform.slice(1);
}

export function getPlatformBadgeStyle(platform?: string | null): {
  bgcolor: string;
  color: string;
} {
  const norm = normalizePlatform(platform);
  const cfg = PLATFORM_VISUAL_MAP[norm];
  if (cfg) {
    return {
      bgcolor: cfg.badgeBgColor,
      color: cfg.badgeTextColor,
    };
  }
  return {
    bgcolor: 'rgba(0, 0, 0, 0.65)',
    color: '#FFFFFF',
  };
}
