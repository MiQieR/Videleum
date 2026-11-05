import { CheckCircle, CloudUpload, Delete, ErrorOutline } from '@mui/icons-material';
import { Alert, Box, Button, Chip, CircularProgress, Paper, Typography } from '@mui/material';
import { useMutation, useQuery } from '@tanstack/react-query';
import React, { useState } from 'react';
import { useLanguage } from '../../contexts/LanguageContext';
import { api, getApiErrorMessage } from '../../utils/apiClient';
import { runMutationAsync } from '../../utils/mutationUtils';
import ConfirmationModal from '../ConfirmationModal';

interface CookieSettingsProps {
    onSuccess: (message: string) => void;
    onError: (message: string) => void;
}

type CookiePlatformType = 'youtube' | 'bilibili';

const CookieSettings: React.FC<CookieSettingsProps> = ({ onSuccess, onError }) => {
    const { t, language } = useLanguage();
    const isZh = language === 'zh';
    const [uploadingPlatform, setUploadingPlatform] = useState<CookiePlatformType | null>(null);
    const [deleteTargetPlatform, setDeleteTargetPlatform] = useState<CookiePlatformType | null>(null);

    const { data: cookieStatus, refetch: refetchCookieStatus, isLoading } = useQuery({
        queryKey: ['cookieStatus'],
        queryFn: async () => {
            const response = await api.get('/settings/check-cookies?platform=all');
            return response.data;
        }
    });

    const handleSuccess = (msg: string) => {
        onSuccess(msg);
        refetchCookieStatus();
    };

    const handleFileUpload = (platform: CookiePlatformType) => async (e: React.ChangeEvent<HTMLInputElement>) => {
        const file = e.target.files?.[0];
        if (!file) return;

        if (!file.name.endsWith('.txt')) {
            onError(t('onlyTxtFilesAllowed') || 'Only .txt files are allowed');
            return;
        }

        const formData = new FormData();
        formData.append('file', file);
        formData.append('platform', platform);

        setUploadingPlatform(platform);
        try {
            await api.post(`/settings/upload-cookies?platform=${platform}`, formData, {
                headers: {
                    'Content-Type': 'multipart/form-data'
                }
            });
            handleSuccess(t('cookiesUploadedSuccess') || 'Cookies uploaded successfully');
        } catch (error) {
            console.error(`Error uploading ${platform} cookies:`, error);
            onError(await getApiErrorMessage(error, t) || t('cookiesUploadFailed') || 'Failed to upload cookies');
        } finally {
            setUploadingPlatform(null);
            e.target.value = '';
        }
    };

    const deleteMutation = useMutation({
        mutationFn: async (platform?: CookiePlatformType) => {
            const target = platform || deleteTargetPlatform || 'youtube';
            const response = await api.post(`/settings/delete-cookies?platform=${target}`, {
                platform: target,
            });
            return response.data;
        },
        onSuccess: () => {
            onSuccess(t('cookiesDeletedSuccess') || 'Cookies deleted successfully');
            refetchCookieStatus();
        },
        onError: async (error: any) => {
            onError(await getApiErrorMessage(error, t) || t('cookiesDeleteFailed') || 'Failed to delete cookies');
        }
    });

    const handleDelete = (platform: CookiePlatformType) => {
        setDeleteTargetPlatform(platform);
    };

    const confirmDelete = async () => {
        const target = deleteTargetPlatform || 'youtube';
        setDeleteTargetPlatform(null);
        await runMutationAsync(deleteMutation, target);
    };

    const youtubeExists = Boolean(cookieStatus?.youtubeExists ?? cookieStatus?.exists);
    const bilibiliExists = Boolean(cookieStatus?.bilibiliExists);

    return (
        <Box>
            <Typography variant="h6" sx={{ mb: 1.5 }}>
                {t('cookieSettings') || 'Cookie Settings'}
            </Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 2.5 }}>
                {t('cookieUploadDescription') || 'Upload YouTube cookies and Bilibili cookies separately.'}
            </Typography>

            <Box sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: '1fr 1fr' }, gap: 2.5 }}>
                {/* YouTube Cookies Card */}
                <Paper
                    variant="outlined"
                    sx={{
                        p: 2.5,
                        borderRadius: 2,
                        borderColor: 'divider',
                        display: 'flex',
                        flexDirection: 'column',
                        justifyContent: 'space-between',
                    }}
                >
                    <Box>
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 1 }}>
                            <Chip
                                label="YouTube"
                                size="small"
                                sx={{
                                    bgcolor: '#FF0000',
                                    color: '#FFFFFF',
                                    fontWeight: 700,
                                    fontSize: '0.75rem',
                                    height: 22,
                                }}
                            />
                            <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
                                {isZh ? 'YouTube Cookie (youtube_cookies.txt)' : 'YouTube Cookies (youtube_cookies.txt)'}
                            </Typography>
                        </Box>
                        <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
                            {isZh
                                ? '用于通过 YouTube 机器人验证（Sign in to confirm you’re not a bot）及年龄限制视频下载。'
                                : 'Required to pass YouTube bot verification and download age-restricted or member-only videos.'}
                        </Typography>
                    </Box>

                    <Box>
                        <Box sx={{ display: 'flex', gap: 1.5, alignItems: 'center', flexWrap: 'wrap' }}>
                            <Button
                                variant="outlined"
                                component="label"
                                startIcon={<CloudUpload />}
                                loading={uploadingPlatform === 'youtube'}
                                loadingPosition="start"
                            >
                                {t('uploadCookies') || 'Upload Cookies'}
                                <input
                                    type="file"
                                    hidden
                                    accept=".txt"
                                    data-testid="upload-youtube-cookies-input"
                                    onChange={handleFileUpload('youtube')}
                                />
                            </Button>

                            {youtubeExists && (
                                <Button
                                    variant="outlined"
                                    color="error"
                                    startIcon={<Delete />}
                                    onClick={() => handleDelete('youtube')}
                                    loading={deleteMutation.isPending && deleteTargetPlatform === 'youtube'}
                                    loadingPosition="start"
                                >
                                    {t('deleteCookies') || 'Delete Cookies'}
                                </Button>
                            )}
                        </Box>

                        {isLoading ? (
                            <Box sx={{ mt: 2 }}>
                                <CircularProgress size={22} />
                            </Box>
                        ) : youtubeExists ? (
                            <Alert icon={<CheckCircle fontSize="inherit" />} severity="success" sx={{ mt: 2 }}>
                                {t('cookiesFound') || 'cookies.txt found'}
                            </Alert>
                        ) : (
                            <Alert icon={<ErrorOutline fontSize="inherit" />} severity="warning" sx={{ mt: 2 }}>
                                {t('cookiesNotFound') || 'cookies.txt not found'}
                            </Alert>
                        )}
                    </Box>
                </Paper>

                {/* Bilibili Cookies Card */}
                <Paper
                    variant="outlined"
                    sx={{
                        p: 2.5,
                        borderRadius: 2,
                        borderColor: 'divider',
                        display: 'flex',
                        flexDirection: 'column',
                        justifyContent: 'space-between',
                    }}
                >
                    <Box>
                        <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: 1 }}>
                            <Chip
                                label="Bilibili"
                                size="small"
                                sx={{
                                    bgcolor: '#00AEEC',
                                    color: '#FFFFFF',
                                    fontWeight: 700,
                                    fontSize: '0.75rem',
                                    height: 22,
                                }}
                            />
                            <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
                                {isZh ? 'Bilibili Cookie (bilibili_cookies.txt)' : 'Bilibili Cookies (bilibili_cookies.txt)'}
                            </Typography>
                        </Box>
                        <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
                            {isZh
                                ? '用于获取 Bilibili 1080P+/4K 高码率视频流、AI 字幕下载以及同步个人收藏夹/稍后再看。'
                                : 'Used for Bilibili 1080P+/4K high-bitrate streams, subtitle downloads, and favorite/watch-later sync.'}
                        </Typography>
                    </Box>

                    <Box>
                        <Box sx={{ display: 'flex', gap: 1.5, alignItems: 'center', flexWrap: 'wrap' }}>
                            <Button
                                variant="outlined"
                                component="label"
                                startIcon={<CloudUpload />}
                                loading={uploadingPlatform === 'bilibili'}
                                loadingPosition="start"
                            >
                                {isZh ? '上传 Bilibili Cookie' : 'Upload Bilibili Cookies'}
                                <input
                                    type="file"
                                    hidden
                                    accept=".txt"
                                    data-testid="upload-bilibili-cookies-input"
                                    onChange={handleFileUpload('bilibili')}
                                />
                            </Button>

                            {bilibiliExists && (
                                <Button
                                    variant="outlined"
                                    color="error"
                                    startIcon={<Delete />}
                                    onClick={() => handleDelete('bilibili')}
                                    loading={deleteMutation.isPending && deleteTargetPlatform === 'bilibili'}
                                    loadingPosition="start"
                                >
                                    {isZh ? '删除 Bilibili Cookie' : 'Delete Bilibili Cookies'}
                                </Button>
                            )}
                        </Box>

                        {isLoading ? (
                            <Box sx={{ mt: 2 }}>
                                <CircularProgress size={22} />
                            </Box>
                        ) : bilibiliExists ? (
                            <Alert icon={<CheckCircle fontSize="inherit" />} severity="success" sx={{ mt: 2 }}>
                                {isZh ? '已配置 bilibili_cookies.txt' : 'bilibili_cookies.txt configured'}
                            </Alert>
                        ) : (
                            <Alert icon={<ErrorOutline fontSize="inherit" />} severity="info" sx={{ mt: 2 }}>
                                {isZh ? '未上传 bilibili_cookies.txt（默认使用免登录接口下载）' : 'bilibili_cookies.txt not uploaded (using guest API)'}
                            </Alert>
                        )}
                    </Box>
                </Paper>
            </Box>

            <ConfirmationModal
                isOpen={deleteTargetPlatform !== null}
                onClose={() => setDeleteTargetPlatform(null)}
                onConfirm={confirmDelete}
                title={t('deleteCookies') || 'Delete Cookies'}
                message={t('confirmDeleteCookies') || 'Are you sure you want to delete the cookies file? This may affect downloading capabilities.'}
                confirmText={t('delete') || 'Delete'}
                cancelText={t('cancel') || 'Cancel'}
                isDanger={true}
            />
        </Box>
    );
};

export default CookieSettings;
