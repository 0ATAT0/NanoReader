import type { Settings } from './types';

const fonts = {
  inter: "'Inter Variable', Inter, sans-serif",
  georgia: 'Georgia, serif',
  system: 'system-ui, sans-serif',
};

// The article and settings preview share the same rendering rules.
export function readingStyle(settings: Settings): string {
  return `--reading-font-family:${fonts[settings.font_family]};
    --reading-font-size:${settings.font_size}px;
    --reading-font-weight:${settings.font_weight};
    --reading-line-height:${settings.line_height};
    --reading-paragraph-spacing:${settings.paragraph_spacing}em;
    --reading-ink:rgb(${settings.text_brightness}% ${settings.text_brightness}% ${settings.text_brightness}%);
    --reading-width:${settings.reading_width}px;`;
}
