import {
    Alert,
    Box,
    Button,
    CircularProgress,
    Dialog,
    DialogActions,
    DialogContent,
    DialogContentText,
    DialogTitle,
    Divider,
    FormControl,
    FormControlLabel,
    LinearProgress,
    MenuItem,
    Select,
    Switch,
    TextField,
    Tooltip,
    Typography,
} from '@mui/material';
import React, { useState } from 'react';
import { useLanguage } from '../../contexts/LanguageContext';
import { useSettingsJobPolling } from '../../hooks/useSettingsJobPolling';
import { Settings } from '../../types';
import { api } from '../../utils/apiClient';
import {
    MEDIA_SERVER_EXPORT_OPTIONS,
    MediaServerExportJob,
    getMediaServerExportErrorMessage,
    mediaServerExportJobUrl,
} from './filenameTemplateShared';
import { SETTINGS_CONTROL_MAX_WIDTH } from './settingsLayout';

interface MediaServerExportSettingsProps {
    settings: Settings;
    onChange: (field: keyof Settings, value: any) => void;
    // True when the current filename template produces a Season/Episode-style
    // layout, which media servers expect for NFO export.
    recommendedTvLayout: boolean;
}


/**
 * Media-server export mode selector plus the rebuild/cleanup job runner.
 * Owns the export job state and its polling; the parent only supplies the
 * saved settings and whether the active template yields a TV-style layout.
 */
const MediaServerExportSettings: React.FC<MediaServerExportSettingsProps> = ({
    settings,
    onChange,
    recommendedTvLayout,
}) => {
    const { t } = useLanguage();
    const [exportConfirmOpen, setExportConfirmOpen] = useState(false);
    const [exportJob, setExportJob] = useState<MediaServerExportJob | null>(null);
    const [exportError, setExportError] = useState<string | null>(null);
    const [startingExport, setStartingExport] = useState(false);
    const [testingServer, setTestingServer] = useState<'jellyfin' | 'emby' | 'plex' | null>(null);
    const [testResult, setTestResult] = useState<{
        server: 'jellyfin' | 'emby' | 'plex';
        success: boolean;
        message: string;
    } | null>(null);
    const [refreshing, setRefreshing] = useState(false);
    const [refreshMessage, setRefreshMessage] = useState<string | null>(null);

    const handleTestConnection = async (type: 'jellyfin' | 'emby' | 'plex') => {
        setTestingServer(type);
        setTestResult(null);
        try {
            let res;
            if (type === 'jellyfin') {
                res = await api.post('/settings/media-server/jellyfin/test', {
                    serverUrl: settings.jellyfinServerUrl,
                    apiKey: settings.jellyfinApiKey,
                });
            } else if (type === 'emby') {
                res = await api.post('/settings/media-server/emby/test', {
                    serverUrl: settings.embyServerUrl,
                    apiKey: settings.embyApiKey,
                });
            } else {
                res = await api.post('/settings/media-server/plex/test', {
                    serverUrl: settings.plexServerUrl,
                    token: settings.plexToken,
                });
            }
            const info = res.data as { serverName?: string; version?: string };
            setTestResult({
                server: type,
                success: true,
                message: `${t('connectionSuccess')}: ${info.serverName || ''} (${info.version || ''})`,
            });
        } catch (err: any) {
            const msg = err.response?.data?.error || err.message || t('connectionFailed');
            setTestResult({
                server: type,
                success: false,
                message: `${t('connectionFailed')}: ${msg}`,
            });
        } finally {
            setTestingServer(null);
        }
    };

    const handleManualRefresh = async () => {
        setRefreshing(true);
        setRefreshMessage(null);
        try {
            await api.post('/settings/media-server/refresh');
            setRefreshMessage(t('refreshTriggered'));
        } catch (err: any) {
            setRefreshMessage(err.response?.data?.error || err.message || 'Refresh failed');
        } finally {
            setRefreshing(false);
        }
    };

    useSettingsJobPolling(exportJob, mediaServerExportJobUrl, setExportJob);

    const handleStartMediaServerExportRebuild = async () => {
        setStartingExport(true);
        setExportError(null);
        const mode = settings.mediaServerExportMode || 'off';
        try {
            const res = await api.post<{
                jobId: string;
                status: string;
                total: number;
                processed: number;
                succeeded: number;
                skipped: number;
                failed: number;
                action: 'rebuild' | 'cleanup';
                mode: 'off' | 'nfo' | 'nfo_and_source_json';
            }>(
                '/settings/media-server-export/rebuild',
                { mediaServerExportMode: mode }
            );
            const jobData = res.data;
            setExportJob({
                id: jobData.jobId,
                status: jobData.status as any,
                lockedAt: Date.now(),
                mode: jobData.mode,
                action: jobData.action,
                total: jobData.total,
                processed: jobData.processed,
                succeeded: jobData.succeeded,
                skipped: jobData.skipped,
                failed: jobData.failed,
                items: [],
            });
            setExportConfirmOpen(false);
        } catch (e: unknown) {
            setExportError(getMediaServerExportErrorMessage(e, mode, t));
        } finally {
            setStartingExport(false);
        }
    };

    const isExportRunning = exportJob?.status === 'running';
    const isExportComplete = exportJob?.status === 'completed';
    const exportMode = settings.mediaServerExportMode || 'off';
    const exportAction = exportMode === 'off' ? 'cleanup' : 'rebuild';
    const activeExportAction = exportJob?.action || exportAction;

    return (
        <Box sx={{ mt: 3 }}>
            <Typography variant="h6" gutterBottom>
                {t('mediaServerExportMode')}
            </Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
                {exportMode === 'off'
                    ? t('mediaServerExportModeOffDescription')
                    : t('mediaServerExportModeDescription')}
            </Typography>

            <FormControl fullWidth sx={{ maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                <Select
                    value={settings.mediaServerExportMode || 'off'}
                    onChange={(e) => onChange('mediaServerExportMode', e.target.value)}
                >
                    {MEDIA_SERVER_EXPORT_OPTIONS.map((opt) => (
                        <MenuItem key={opt.value} value={opt.value}>
                            {t(opt.labelKey)}
                        </MenuItem>
                    ))}
                </Select>
            </FormControl>

            {(settings.mediaServerExportMode || 'off') !== 'off' && !recommendedTvLayout && (
                <Alert severity="warning" sx={{ mt: 2 }}>
                    {t('mediaServerExportRecommendedLayoutWarning')}
                </Alert>
            )}

            {exportMode === 'off' && (
                <Alert severity="info" sx={{ mt: 2 }}>
                    {t('mediaServerExportCleanupHint')}
                </Alert>
            )}

            {exportError && (
                <Alert severity="error" sx={{ mt: 2 }}>
                    {exportError}
                </Alert>
            )}

            {isExportRunning && exportJob && (
                <Box sx={{ mt: 2 }}>
                    <Typography variant="body2" sx={{ mb: 0.75 }}>
                        {t(activeExportAction === 'cleanup'
                            ? 'mediaServerExportCleanupRunning'
                            : 'mediaServerExportRebuildRunning')} {exportJob.processed}/{exportJob.total}
                        {exportJob.currentTitle && ` – ${exportJob.currentTitle}`}
                    </Typography>
                    <LinearProgress
                        variant="determinate"
                        value={exportJob.total > 0 ? (exportJob.processed / exportJob.total) * 100 : 0}
                    />
                </Box>
            )}

            {isExportComplete && exportJob && (
                <Alert severity="success" sx={{ mt: 2 }}>
                    {t(activeExportAction === 'cleanup'
                        ? 'mediaServerExportCleanupComplete'
                        : 'mediaServerExportRebuildComplete')} –{' '}
                    {t(activeExportAction === 'cleanup'
                        ? 'mediaServerExportCleanupSummary'
                        : 'mediaServerExportRebuildSummary')
                        .replace('{succeeded}', String(exportJob.succeeded))
                        .replace('{skipped}', String(exportJob.skipped))
                        .replace('{failed}', String(exportJob.failed))}
                    {exportJob.sweptFiles && exportJob.sweptFiles > 0
                        ? ` ${t('mediaServerExportSweptSummary').replace('{count}', String(exportJob.sweptFiles))}`
                        : ''}
                </Alert>
            )}

            <Tooltip
                title={
                    isExportRunning
                            ? t('mediaServerExportRebuildDisabledRunning')
                            : ''
                }
                disableHoverListener={!isExportRunning}
            >
                <span>
                    <Button
                        variant="outlined"
                        onClick={() => setExportConfirmOpen(true)}
                        disabled={isExportRunning}
                        sx={{ mt: 2 }}
                    >
                        {t(exportAction === 'cleanup'
                            ? 'mediaServerExportCleanup'
                            : 'mediaServerExportRebuild')}
                    </Button>
                </span>
            </Tooltip>

            <Divider sx={{ my: 4 }} />

            <Typography variant="h6" gutterBottom>
                {t('mediaServerIntegrationTitle')}
            </Typography>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
                {t('mediaServerIntegrationDescription')}
            </Typography>

            <Box sx={{ mb: 2 }}>
                <FormControlLabel
                    control={
                        <Switch
                            checked={settings.mediaServerAutoRefreshOnDownload !== false}
                            onChange={(e) => onChange('mediaServerAutoRefreshOnDownload', e.target.checked)}
                        />
                    }
                    label={t('mediaServerAutoRefreshOnDownload')}
                />
            </Box>

            {testResult && (
                <Alert severity={testResult.success ? 'success' : 'error'} sx={{ mb: 2, maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                    {testResult.message}
                </Alert>
            )}

            {refreshMessage && (
                <Alert severity="info" sx={{ mb: 2, maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                    {refreshMessage}
                </Alert>
            )}

            {/* Jellyfin */}
            <Box sx={{ p: 2, mb: 2, border: 1, borderColor: 'divider', borderRadius: 1, maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                <FormControlLabel
                    control={
                        <Switch
                            checked={!!settings.jellyfinEnabled}
                            onChange={(e) => onChange('jellyfinEnabled', e.target.checked)}
                        />
                    }
                    label="Jellyfin"
                />
                {settings.jellyfinEnabled && (
                    <Box sx={{ mt: 1.5, display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                        <TextField
                            size="small"
                            fullWidth
                            label={t('jellyfinServerUrl')}
                            placeholder="http://localhost:8096"
                            value={settings.jellyfinServerUrl || ''}
                            onChange={(e) => onChange('jellyfinServerUrl', e.target.value)}
                        />
                        <TextField
                            size="small"
                            fullWidth
                            type="password"
                            label={t('jellyfinApiKey')}
                            value={settings.jellyfinApiKey || ''}
                            onChange={(e) => onChange('jellyfinApiKey', e.target.value)}
                        />
                        <Button
                            variant="outlined"
                            size="small"
                            sx={{ alignSelf: 'flex-start' }}
                            disabled={testingServer === 'jellyfin'}
                            onClick={() => { void handleTestConnection('jellyfin'); }}
                        >
                            {testingServer === 'jellyfin' ? <CircularProgress size={18} sx={{ mr: 1 }} /> : null}
                            {t('testConnection')}
                        </Button>
                    </Box>
                )}
            </Box>

            {/* Emby */}
            <Box sx={{ p: 2, mb: 2, border: 1, borderColor: 'divider', borderRadius: 1, maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                <FormControlLabel
                    control={
                        <Switch
                            checked={!!settings.embyEnabled}
                            onChange={(e) => onChange('embyEnabled', e.target.checked)}
                        />
                    }
                    label="Emby"
                />
                {settings.embyEnabled && (
                    <Box sx={{ mt: 1.5, display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                        <TextField
                            size="small"
                            fullWidth
                            label={t('embyServerUrl')}
                            placeholder="http://localhost:8096"
                            value={settings.embyServerUrl || ''}
                            onChange={(e) => onChange('embyServerUrl', e.target.value)}
                        />
                        <TextField
                            size="small"
                            fullWidth
                            type="password"
                            label={t('embyApiKey')}
                            value={settings.embyApiKey || ''}
                            onChange={(e) => onChange('embyApiKey', e.target.value)}
                        />
                        <Button
                            variant="outlined"
                            size="small"
                            sx={{ alignSelf: 'flex-start' }}
                            disabled={testingServer === 'emby'}
                            onClick={() => { void handleTestConnection('emby'); }}
                        >
                            {testingServer === 'emby' ? <CircularProgress size={18} sx={{ mr: 1 }} /> : null}
                            {t('testConnection')}
                        </Button>
                    </Box>
                )}
            </Box>

            {/* Plex */}
            <Box sx={{ p: 2, mb: 2, border: 1, borderColor: 'divider', borderRadius: 1, maxWidth: SETTINGS_CONTROL_MAX_WIDTH }}>
                <FormControlLabel
                    control={
                        <Switch
                            checked={!!settings.plexEnabled}
                            onChange={(e) => onChange('plexEnabled', e.target.checked)}
                        />
                    }
                    label="Plex"
                />
                {settings.plexEnabled && (
                    <Box sx={{ mt: 1.5, display: 'flex', flexDirection: 'column', gap: 1.5 }}>
                        <TextField
                            size="small"
                            fullWidth
                            label={t('plexServerUrl')}
                            placeholder="http://localhost:32400"
                            value={settings.plexServerUrl || ''}
                            onChange={(e) => onChange('plexServerUrl', e.target.value)}
                        />
                        <TextField
                            size="small"
                            fullWidth
                            type="password"
                            label={t('plexToken')}
                            value={settings.plexToken || ''}
                            onChange={(e) => onChange('plexToken', e.target.value)}
                        />
                        <Button
                            variant="outlined"
                            size="small"
                            sx={{ alignSelf: 'flex-start' }}
                            disabled={testingServer === 'plex'}
                            onClick={() => { void handleTestConnection('plex'); }}
                        >
                            {testingServer === 'plex' ? <CircularProgress size={18} sx={{ mr: 1 }} /> : null}
                            {t('testConnection')}
                        </Button>
                    </Box>
                )}
            </Box>

            <Box sx={{ mt: 2, mb: 1 }}>
                <Button
                    variant="contained"
                    color="secondary"
                    disabled={refreshing}
                    onClick={() => { void handleManualRefresh(); }}
                >
                    {refreshing ? <CircularProgress size={18} sx={{ mr: 1 }} color="inherit" /> : null}
                    {refreshing ? t('refreshingMediaServers') : t('refreshMediaServers')}
                </Button>
            </Box>

            <Dialog
                open={exportConfirmOpen}
                onClose={() => {
                    if (!startingExport) {
                        setExportConfirmOpen(false);
                    }
                }}
                disableEscapeKeyDown={startingExport}
            >
                <DialogTitle>{t(exportAction === 'cleanup'
                    ? 'mediaServerExportCleanupConfirmTitle'
                    : 'mediaServerExportRebuildConfirmTitle')}</DialogTitle>
                <DialogContent>
                    <DialogContentText>
                        {t(exportAction === 'cleanup'
                            ? 'mediaServerExportCleanupConfirmBody'
                            : 'mediaServerExportRebuildConfirmBody')}
                    </DialogContentText>
                </DialogContent>
                <DialogActions>
                    <Button onClick={() => setExportConfirmOpen(false)} disabled={startingExport}>{t('cancel')}</Button>
                    <Button
                        onClick={() => { void handleStartMediaServerExportRebuild(); }}
                        variant="contained"
                        loading={startingExport}
                        loadingPosition="start"
                    >
                        {t(exportAction === 'cleanup'
                            ? 'mediaServerExportCleanup'
                            : 'mediaServerExportRebuild')}
                    </Button>
                </DialogActions>
            </Dialog>
        </Box>
    );
};

export default MediaServerExportSettings;
