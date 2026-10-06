import palettes from './themes.json';
import type { Settings } from './types';

export function applyTheme(theme: Settings['theme']): void {
  const root = document.documentElement;
  for (const [name, colour] of Object.entries(palettes[theme])) root.style.setProperty(`--${name}`, colour);
  root.style.colorScheme = theme === 'light' ? 'light' : 'dark';
  root.dataset.theme = theme;
}
