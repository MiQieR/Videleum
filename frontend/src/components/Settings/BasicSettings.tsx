import { FolderOpen, Save } from '@mui/icons-material';
import {
    Alert,
    Box,
    Button,
    FormControl,
    FormControlLabel,
    InputLabel,
    MenuItem,
    Paper,
    Select,
    Switch,
    TextField,
    Typography,
} from '@mui/material';
import React, { useEffect, useState } from 'react';
import { useAuth } from '../../contexts/AuthContext';
import { useLanguage } from '../../contexts/LanguageContext';
import { ThemePreference } from '../../contexts/ThemeContext';
import { api } from '../../utils/apiClient';
import { SETTINGS_CONTROL_MAX_WIDTH } from './settingsLayout';

interface BasicSettingsProps {
    language: string;
    theme?: ThemePreference;
    showThemeButton?: boolean;
    showAudioDownloadButton?: boolean;
    websiteName?: string;
    onChange: (field: string, value: string | number | boolean) => void;
}

interface StoragePathInfo {
    uploadsDir: string;
    videosDir: string;
    imagesDir: string;
    subtitlesDir: string;
    customUploadsDir: string | null;
    defaultUploadsDir: string;
}

const BasicSettings: React.FC<BasicSettingsProps> = ({
    language,
    theme,
    showThemeButton = true,
    showAudioDownloadButton = true,
    websiteName,
    onChange,
}) => {
    const { t } = useLanguage();
    const { userRole } = useAuth();
    const isVisitor = userRole === 'visitor';
    const isZh = language === 'zh';

    const [storageInfo, setStorageInfo] = useState<StoragePathInfo | null>(null);
    const [customPathInput, setCustomPathInput] = useState<string>('');
    const [storageFeedback, setStorageFeedback] = useState<{
        type: 'success' | 'error';
        message: string;
    } | null>(null);
    const [savingPath, setSavingPath] = useState(false);

    useEffect(() => {
        if (isVisitor) return;
        let cancelled = false;
        const loadStoragePath = async () => {
            try {
                const res = await api.get('/settings/storage-path');
                if (!cancelled && res?.data) {
                    setStorageInfo(res.data);
                    setCustomPathInput(res.data.customUploadsDir || res.data.uploadsDir || '');
                }
            } catch {
                // Ignore in test environments or when offline
            }
        };
        void loadStoragePath();
        return () => {
            cancelled = true;
        };
    }, [isVisitor]);

    const handleSaveStoragePath = async () => {
        setSavingPath(true);
        setStorageFeedback(null);
        try {
            const res = await api.post('/settings/storage-path', {
                path: customPathInput.trim(),
            });
            if (res?.data) {
                setStorageInfo(res.data);
                setCustomPathInput(res.data.customUploadsDir || res.data.uploadsDir || '');
                onChange('downloadStoragePath', res.data.customUploadsDir || '');
                setStorageFeedback({
                    type: 'success',
                    message: isZh
                        ? `存储目录已更新为：${res.data.videosDir}`
                        : `Download storage directory updated: ${res.data.videosDir}`,
                });
            }
        } catch (err: any) {
            setStorageFeedback({
                type: 'error',
                message:
                    err?.response?.data?.error ||
                    err?.message ||
                    (isZh ? '更新存储目录失败' : 'Failed to update storage directory'),
            });
        } finally {
            setSavingPath(false);
        }
    };

    const handleOpenStorageFolder = async () => {
        setStorageFeedback(null);
        try {
            await api.post('/settings/open-storage-path', { target: 'videos' });
        } catch (err: any) {
            setStorageFeedback({
                type: 'error',
                message:
                    err?.response?.data?.error ||
                    err?.message ||
                    (isZh ? '打开存储目录失败' : 'Failed to open storage folder'),
            });
        }
    };

    return (
        <Box>
            <Box sx={{ maxWidth: SETTINGS_CONTROL_MAX_WIDTH, display: 'flex', flexDirection: 'column', gap: 3 }}>
                <FormControl fullWidth>
                    <InputLabel id="language-select-label">{t('language')}</InputLabel>
                    <Select
                        labelId="language-select-label"
                        id="language-select"
                        value={language || 'en'}
                        label={t('language')}
                        onChange={(e) => onChange('language', e.target.value)}
                    >
                        <MenuItem value="en">English</MenuItem>
                        <MenuItem value="zh">中文 (Chinese)</MenuItem>
                        <MenuItem value="es">Español (Spanish)</MenuItem>
                        <MenuItem value="de">Deutsch (German)</MenuItem>
                        <MenuItem value="ja">日本語 (Japanese)</MenuItem>
                        <MenuItem value="fr">Français (French)</MenuItem>
                        <MenuItem value="ko">한국어 (Korean)</MenuItem>
                        <MenuItem value="ar">العربية (Arabic)</MenuItem>
                        <MenuItem value="pt">Português (Portuguese)</MenuItem>
                        <MenuItem value="ru">Русский (Russian)</MenuItem>
                    </Select>
                </FormControl>

                <FormControl fullWidth>
                    <InputLabel id="theme-select-label">{t('theme')}</InputLabel>
                    <Select
                        labelId="theme-select-label"
                        id="theme-select"
                        value={theme || 'system'}
                        label={t('theme')}
                        onChange={(e) => onChange('theme', e.target.value)}
                    >
                        <MenuItem value="light">{t('themeLight')}</MenuItem>
                        <MenuItem value="dark">{t('themeDark')}</MenuItem>
                        <MenuItem value="system">{t('themeSystem')}</MenuItem>
                    </Select>
                </FormControl>

                {!isVisitor && (
                    <FormControlLabel
                        control={
                            <Switch
                                checked={showThemeButton !== false}
                                onChange={(e) => onChange('showThemeButton', e.target.checked)}
                                color="primary"
                            />
                        }
                        label={t('showThemeButtonInHeader')}
                    />
                )}

                {!isVisitor && (
                    <FormControlLabel
                        control={
                            <Switch
                                checked={showAudioDownloadButton !== false}
                                onChange={(e) => onChange('showAudioDownloadButton', e.target.checked)}
                                color="primary"
                            />
                        }
                        label={t('showAudioDownloadButton')}
                    />
                )}

                {!isVisitor && (
                    <TextField
                        fullWidth
                        label={t('websiteName')}
                        value={websiteName || ''}
                        onChange={(e) => onChange('websiteName', e.target.value)}
                        placeholder="Videleum"
                        helperText={t('websiteNameHelper', {
                            current: (websiteName || '').length,
                            max: 15,
                            default: 'Videleum'
                        })}
                        slotProps={{ htmlInput: { maxLength: 15 } }}
                    />
                )}

                {!isVisitor && (
                    <Paper
                        variant="outlined"
                        sx={{
                            p: 2.5,
                            borderRadius: 2,
                            borderColor: 'divider',
                            display: 'flex',
                            flexDirection: 'column',
                            gap: 1.5,
                        }}
                    >
                        <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
                            {isZh ? '媒体下载存储目录' : 'Media Download Storage Directory'}
                        </Typography>
                        <Typography variant="body2" color="text.secondary">
                            {isZh
                                ? `当前视频文件保存在：${storageInfo?.videosDir || '~/Movies/Videleum/videos'}`
                                : `Videos are currently stored in: ${storageInfo?.videosDir || '~/Movies/Videleum/videos'}`}
                        </Typography>
                        <TextField
                            fullWidth
                            size="small"
                            label={isZh ? '存储根目录路径' : 'Storage Root Directory'}
                            value={customPathInput}
                            onChange={(e) => setCustomPathInput(e.target.value)}
                            placeholder={storageInfo?.defaultUploadsDir || '~/Movies/Videleum'}
                            helperText={
                                isZh
                                    ? '留空并保存将恢复默认目录（macOS: ~/Movies/Videleum，Windows: %USERPROFILE%\\Videos\\Videleum）'
                                    : 'Leave empty and apply to restore default directory (macOS: ~/Movies/Videleum, Windows: %USERPROFILE%\\Videos\\Videleum)'
                            }
                        />
                        <Box sx={{ display: 'flex', gap: 1.5, flexWrap: 'wrap' }}>
                            <Button
                                variant="contained"
                                size="small"
                                startIcon={<Save />}
                                onClick={handleSaveStoragePath}
                                disabled={savingPath}
                            >
                                {isZh ? '应用存储目录' : 'Apply Directory'}
                            </Button>
                            <Button
                                variant="outlined"
                                size="small"
                                startIcon={<FolderOpen />}
                                onClick={handleOpenStorageFolder}
                            >
                                {isZh ? '在 Finder / 资源管理器中打开' : 'Open in File Manager'}
                            </Button>
                        </Box>
                        {storageFeedback && (
                            <Alert severity={storageFeedback.type} sx={{ mt: 0.5 }}>
                                {storageFeedback.message}
                            </Alert>
                        )}
                    </Paper>
                )}
            </Box>
        </Box>
    );
};

export default BasicSettings;
