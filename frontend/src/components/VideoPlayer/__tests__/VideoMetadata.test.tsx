import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import VideoMetadata from '../VideoInfo/VideoMetadata';

vi.mock('../../../contexts/LanguageContext', () => ({
  useLanguage: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../contexts/SnackbarContext', () => ({
  useSnackbar: () => ({ showSnackbar: vi.fn() }),
}));

vi.mock('../../../hooks/useCloudStorageUrl', () => ({
  useCloudStorageUrl: () => null,
}));

describe('VideoMetadata', () => {
  const baseVideo: any = {
    id: 'test-1',
    title: 'Test Video',
    videoPath: '/media/video.mp4',
    sourceUrl: 'https://www.youtube.com/watch?v=123',
    source: 'youtube',
    platform: 'youtube',
  };

  it('renders normalized platform display name', () => {
    render(
      <VideoMetadata
        video={baseVideo}
        videoCollections={[]}
        onCollectionClick={vi.fn()}
        videoResolution="1080p"
      />
    );

    const platformEl = screen.getByTestId('video-platform-metadata');
    expect(platformEl).toHaveTextContent('YouTube');
  });

  it('renders Bilibili when platform is bilibili', () => {
    render(
      <VideoMetadata
        video={{ ...baseVideo, source: 'bilibili', platform: 'bilibili' }}
        videoCollections={[]}
        onCollectionClick={vi.fn()}
        videoResolution="1080p"
      />
    );

    const platformEl = screen.getByTestId('video-platform-metadata');
    expect(platformEl).toHaveTextContent('Bilibili');
  });
});
