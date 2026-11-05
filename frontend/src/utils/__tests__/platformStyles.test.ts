import { describe, expect, it } from 'vitest';
import {
  getPlatformBadgeStyle,
  getPlatformDisplayName,
  normalizePlatform,
  PLATFORM_VISUAL_MAP,
} from '../platformStyles';

describe('platformStyles', () => {
  it('normalizes platform aliases correctly', () => {
    expect(normalizePlatform('youtube')).toBe('youtube');
    expect(normalizePlatform('YT')).toBe('youtube');
    expect(normalizePlatform('bilibili')).toBe('bilibili');
    expect(normalizePlatform('Bili')).toBe('bilibili');
    expect(normalizePlatform('twitch')).toBe('twitch');
    expect(normalizePlatform('custom')).toBe('custom');
    expect(normalizePlatform(undefined)).toBe('unknown');
    expect(normalizePlatform(null)).toBe('unknown');
  });

  it('provides correct display names for platforms', () => {
    expect(getPlatformDisplayName('youtube')).toBe('YouTube');
    expect(getPlatformDisplayName('bilibili')).toBe('Bilibili');
    expect(getPlatformDisplayName('twitch')).toBe('Twitch');
    expect(getPlatformDisplayName('other')).toBe('Other');
    expect(getPlatformDisplayName(undefined)).toBe('Unknown');
  });

  it('provides distinct badge styles for major platforms', () => {
    const ytStyle = getPlatformBadgeStyle('youtube');
    const biliStyle = getPlatformBadgeStyle('bilibili');
    const twitchStyle = getPlatformBadgeStyle('twitch');
    const fallbackStyle = getPlatformBadgeStyle('unknown');

    expect(ytStyle.bgcolor).toBe(PLATFORM_VISUAL_MAP.youtube.badgeBgColor);
    expect(biliStyle.bgcolor).toBe(PLATFORM_VISUAL_MAP.bilibili.badgeBgColor);
    expect(twitchStyle.bgcolor).toBe(PLATFORM_VISUAL_MAP.twitch.badgeBgColor);
    expect(fallbackStyle.bgcolor).toContain('rgba(0, 0, 0');
  });
});
