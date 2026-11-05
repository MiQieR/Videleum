import { createTheme, ThemeProvider } from '@mui/material/styles';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import Footer from '../Footer';

describe('Footer', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it('renders version number', () => {
        const theme = createTheme();
        render(
            <ThemeProvider theme={theme}>
                <Footer />
            </ThemeProvider>
        );

        expect(screen.getByText(/^v\d+\.\d+\.\d+/)).toBeInTheDocument();
    });

    it('renders GitHub link', () => {
        const theme = createTheme();
        render(
            <ThemeProvider theme={theme}>
                <Footer />
            </ThemeProvider>
        );

        const link = screen.getByRole('link', { name: /Videleum/i });
        expect(link).toHaveAttribute('href', 'https://github.com/MiQieR/Videleum');
    });

    it('renders description text', () => {
        const theme = createTheme();
        render(
            <ThemeProvider theme={theme}>
                <Footer />
            </ThemeProvider>
        );

        expect(screen.getByText(/跨平台流媒体采集与本地归档中心/i)).toBeInTheDocument();
    });
});
