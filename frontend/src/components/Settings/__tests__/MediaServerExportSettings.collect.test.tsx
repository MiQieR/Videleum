import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import MediaServerExportSettings from '../MediaServerExportSettings';
import { api } from '../../../utils/apiClient';

vi.mock('../../../contexts/LanguageContext', () => ({
    useLanguage: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../utils/apiClient', () => ({
    api: {
        post: vi.fn(),
        get: vi.fn(),
    },
}));

describe('MediaServerExportSettings', () => {
    const onChange = vi.fn();
    const defaultSettings: any = {
        mediaServerExportMode: 'nfo',
        mediaServerAutoRefreshOnDownload: true,
        jellyfinEnabled: false,
        jellyfinServerUrl: '',
        jellyfinApiKey: '',
        embyEnabled: false,
        plexEnabled: false,
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders media server export mode and toggles', () => {
        render(
            <MediaServerExportSettings
                settings={defaultSettings}
                onChange={onChange}
                recommendedTvLayout={true}
            />
        );

        expect(screen.getByText('mediaServerExportMode')).toBeInTheDocument();
        expect(screen.getByText('mediaServerIntegrationTitle')).toBeInTheDocument();
        expect(screen.getByRole('switch', { name: 'Jellyfin' })).not.toBeChecked();
        expect(screen.getByRole('switch', { name: 'Emby' })).not.toBeChecked();
        expect(screen.getByRole('switch', { name: 'Plex' })).not.toBeChecked();
    });

    it('displays server url and api key inputs when Jellyfin is enabled', () => {
        render(
            <MediaServerExportSettings
                settings={{
                    ...defaultSettings,
                    jellyfinEnabled: true,
                    jellyfinServerUrl: 'http://localhost:8096',
                    jellyfinApiKey: 'test-key',
                }}
                onChange={onChange}
                recommendedTvLayout={true}
            />
        );

        expect(screen.getByRole('switch', { name: 'Jellyfin' })).toBeChecked();
        expect(screen.getByLabelText('jellyfinServerUrl')).toHaveValue('http://localhost:8096');
        expect(screen.getByLabelText('jellyfinApiKey')).toHaveValue('test-key');
    });

    it('triggers test connection when button is clicked', async () => {
        const user = userEvent.setup();
        vi.mocked(api.post).mockResolvedValueOnce({
            data: { success: true, serverName: 'Test Jellyfin', version: '10.9' },
        } as any);

        render(
            <MediaServerExportSettings
                settings={{
                    ...defaultSettings,
                    jellyfinEnabled: true,
                    jellyfinServerUrl: 'http://localhost:8096',
                    jellyfinApiKey: 'test-key',
                }}
                onChange={onChange}
                recommendedTvLayout={true}
            />
        );

        const testBtn = screen.getByRole('button', { name: 'testConnection' });
        await user.click(testBtn);

        expect(api.post).toHaveBeenCalledWith('/settings/media-server/jellyfin/test', {
            serverUrl: 'http://localhost:8096',
            apiKey: 'test-key',
        });
        expect(await screen.findByText(/connectionSuccess/)).toBeInTheDocument();
    });
});
