export type UiTheme = 'moonlight' | 'deck' | 'highContrast';

export const themeOptions: { value: UiTheme; label: string }[] = [
  { value: 'moonlight', label: 'Moonlight' },
  { value: 'deck', label: 'Steam Deck' },
  { value: 'highContrast', label: 'High contrast' },
];

export const defaultTheme: UiTheme = 'moonlight';
export const themeStorageKey = 'moonlight-tauri-ui-theme';

export function readStoredTheme(): UiTheme {
  const storedTheme = window.localStorage.getItem(themeStorageKey);
  return themeOptions.some((theme) => theme.value === storedTheme) ? storedTheme as UiTheme : defaultTheme;
}

export function applyStoredTheme(theme: UiTheme) {
  document.documentElement.dataset.theme = theme;
  window.localStorage.setItem(themeStorageKey, theme);
}
