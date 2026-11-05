import type { TranslationKey } from './translations';

/**
 * Backend failure messages that have a translation. Each key must match the
 * backend text exactly - see YOUTUBE_COOKIES_EXPIRED_MESSAGE in
 * backend/src/utils/ytdlp/errorClassification.ts.
 */
const LOCALIZED_DOWNLOAD_ERRORS: Record<string, TranslationKey> = {
    'YouTube refused the download (HTTP 403). Your YouTube cookies may have expired; please re-upload them.':
        'youtubeCookiesMayHaveExpired',
    'YouTube requires authentication cookies to pass bot verification. Please upload your YouTube cookies in Settings -> Security & Access -> Cookie Settings.':
        'youtubeCookiesRequiredForBotCheck',
};

/** The translation key for a stored download error, if it has one. */
export function getDownloadErrorKey(error: string): TranslationKey | undefined {
    const exact = LOCALIZED_DOWNLOAD_ERRORS[error];
    if (exact) {
        return exact;
    }
    if (
        /requires authentication cookies to pass bot verification|sign in to confirm you[\u2019']re not a bot/i.test(
            error
        )
    ) {
        return 'youtubeCookiesRequiredForBotCheck';
    }
    return undefined;
}
