import { Box, Chip } from '@mui/material';
import React from 'react';
import { useLanguage } from '../contexts/LanguageContext';
import { PLATFORM_VISUAL_MAP } from '../utils/platformStyles';

export interface PlatformFilterPillsProps {
  selectedPlatform: string | null;
  onPlatformSelect: (platform: string) => void;
  disabled?: boolean;
}

const SUPPORTED_PLATFORMS = [
  { id: 'all', labelKey: 'all' as const },
  { id: 'youtube', label: 'YouTube' },
  { id: 'bilibili', label: 'Bilibili' },
  { id: 'twitch', label: 'Twitch' },
];

export const PlatformFilterPills: React.FC<PlatformFilterPillsProps> = ({
  selectedPlatform,
  onPlatformSelect,
  disabled = false,
}) => {
  const { t } = useLanguage();
  const current = (selectedPlatform || 'all').toLowerCase();

  return (
    <Box
      data-testid="platform-filter-pills"
      sx={{
        display: 'flex',
        alignItems: 'center',
        gap: 1,
        overflowX: 'auto',
        py: 0.5,
        scrollbarWidth: 'none',
        '&::-webkit-scrollbar': { display: 'none' },
      }}
    >
      {SUPPORTED_PLATFORMS.map((item) => {
        const isSelected = current === item.id;
        const visual = PLATFORM_VISUAL_MAP[item.id];
        const label = item.labelKey ? t(item.labelKey) : item.label!;

        return (
          <Chip
            key={item.id}
            data-testid={`platform-filter-pill-${item.id}`}
            label={label}
            size="small"
            clickable={!disabled}
            disabled={disabled}
            onClick={() => {
              if (!disabled) {
                onPlatformSelect(item.id);
              }
            }}
            sx={{
              fontWeight: isSelected ? 600 : 500,
              fontSize: '0.8rem',
              height: 28,
              borderRadius: '14px',
              transition: 'all 0.2s cubic-bezier(0.4, 0, 0.2, 1)',
              ...(isSelected
                ? {
                    bgcolor: visual ? visual.badgeBgColor : 'primary.main',
                    color: '#ffffff',
                    boxShadow: visual
                      ? `0 2px 8px ${visual.primaryColor}55`
                      : '0 2px 8px rgba(0, 0, 0, 0.25)',
                    '&:hover': {
                      bgcolor: visual ? visual.badgeBgColor : 'primary.dark',
                    },
                  }
                : {
                    bgcolor: 'action.hover',
                    color: 'text.secondary',
                    border: '1px solid transparent',
                    '&:hover': {
                      bgcolor: 'action.selected',
                      color: 'text.primary',
                      borderColor: visual ? visual.pillBorderColor : 'divider',
                    },
                  }),
            }}
          />
        );
      })}
    </Box>
  );
};
