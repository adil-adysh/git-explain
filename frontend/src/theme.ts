import type { SessionSettings, Theme, FontScale } from './types';

export const defaultSettings: SessionSettings = {
  theme: 'system',
  fontScale: 'normal',
  codeWrap: true,
  plainSource: false,
  hideExplanations: false,
};

let activeSettings: SessionSettings = { ...defaultSettings };
let mediaQueryList: MediaQueryList | null = null;

function onSystemThemePreferenceChange(event: MediaQueryListEvent) {
  if (activeSettings.theme === 'system') {
    applyResolvedTheme(event.matches ? 'dark' : 'light');
  }
}

function applyResolvedTheme(resolved: 'light' | 'dark') {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.appliedTheme = resolved;
  }
}

export function applyTheme(theme: Theme) {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.theme = theme;

  if (typeof window !== 'undefined') {
    if (!mediaQueryList) {
      mediaQueryList = window.matchMedia('(prefers-color-scheme: dark)');
      mediaQueryList.addEventListener('change', onSystemThemePreferenceChange);
    }
    if (theme === 'system') {
      applyResolvedTheme(mediaQueryList.matches ? 'dark' : 'light');
    } else {
      applyResolvedTheme(theme);
    }
  }
}

export function applyFontScale(scale: FontScale) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.fontScale = scale;
  }
}

export function applyCodeWrap(wrap: boolean) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.codeWrap = wrap ? 'wrap' : 'nowrap';
  }
}

export function applyPlainSource(plain: boolean) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.plainSource = plain ? 'true' : 'false';
  }
}

export function applySettings(settings: SessionSettings) {
  activeSettings = { ...settings };
  applyTheme(settings.theme);
  applyFontScale(settings.fontScale);
  applyCodeWrap(settings.codeWrap);
  applyPlainSource(settings.plainSource);
}
