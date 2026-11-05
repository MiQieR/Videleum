import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PlatformFilterPills } from '../PlatformFilterPills';

vi.mock('../../contexts/LanguageContext', () => ({
  useLanguage: () => ({ t: (key: string) => (key === 'all' ? '全部' : key) }),
}));

describe('PlatformFilterPills', () => {
  it('renders all platform pill options', () => {
    const onSelect = vi.fn();
    render(
      <PlatformFilterPills
        selectedPlatform="all"
        onPlatformSelect={onSelect}
      />
    );

    expect(screen.getByTestId('platform-filter-pill-all')).toBeInTheDocument();
    expect(screen.getByTestId('platform-filter-pill-youtube')).toBeInTheDocument();
    expect(screen.getByTestId('platform-filter-pill-bilibili')).toBeInTheDocument();
    expect(screen.getByTestId('platform-filter-pill-twitch')).toBeInTheDocument();
  });

  it('triggers onPlatformSelect when a pill is clicked', () => {
    const onSelect = vi.fn();
    render(
      <PlatformFilterPills
        selectedPlatform="all"
        onPlatformSelect={onSelect}
      />
    );

    fireEvent.click(screen.getByTestId('platform-filter-pill-bilibili'));
    expect(onSelect).toHaveBeenCalledWith('bilibili');

    fireEvent.click(screen.getByTestId('platform-filter-pill-youtube'));
    expect(onSelect).toHaveBeenCalledWith('youtube');
  });

  it('does not trigger onPlatformSelect when disabled', () => {
    const onSelect = vi.fn();
    render(
      <PlatformFilterPills
        selectedPlatform="all"
        onPlatformSelect={onSelect}
        disabled={true}
      />
    );

    fireEvent.click(screen.getByTestId('platform-filter-pill-twitch'));
    expect(onSelect).not.toHaveBeenCalled();
  });
});
