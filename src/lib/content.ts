import DOMPurify from 'dompurify';
import type { Highlight, Position } from './types';

const tags = ['article', 'section', 'div', 'p', 'br', 'hr', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6',
  'a', 'img', 'figure', 'figcaption', 'blockquote', 'pre', 'code', 'em', 'strong', 'b', 'i',
  'u', 's', 'del', 'sup', 'sub', 'small', 'mark', 'span', 'ul', 'ol', 'li', 'dl', 'dt', 'dd',
  'table', 'thead', 'tbody', 'tfoot', 'tr', 'th', 'td', 'caption', 'colgroup', 'col'];

function contentUrl(value: string, source: string | null, image: boolean, epub: boolean): string | null {
  if (!value.trim()) return null;
  if (!image && value.startsWith('#')) return value;
  if (image && epub && /^data:image\/(?:png|jpe?g|gif|webp|avif);base64,[a-z0-9+/=\s]+$/i.test(value)) return value;
  try {
    const url = new URL(value, source ?? undefined);
    return ['http:', 'https:'].includes(url.protocol) ? url.href : null;
  } catch { return null; }
}

export function safeImageUrl(value: string | null): string | null {
  return value ? contentUrl(value, null, true, false) : null;
}

export function sanitizeContent(html: string, sourceUrl: string | null, epub = false): string {
  const fragment = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: tags,
    ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'id', 'colspan', 'rowspan', 'scope', 'start', 'reversed'],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    RETURN_DOM_FRAGMENT: true,
  });
  for (const link of fragment.querySelectorAll('a')) {
    const href = contentUrl(link.getAttribute('href') ?? '', sourceUrl, false, epub);
    if (href) link.setAttribute('href', href); else link.removeAttribute('href');
  }
  for (const img of fragment.querySelectorAll('img')) {
    const src = contentUrl(img.getAttribute('src') ?? '', sourceUrl, true, epub);
    if (!src) { img.remove(); continue; }
    img.setAttribute('src', src);
    img.setAttribute('loading', 'lazy');
    img.setAttribute('decoding', 'async');
    img.setAttribute('referrerpolicy', 'no-referrer');
  }
  const host = document.createElement('div');
  host.append(fragment);
  return host.innerHTML;
}

function textNodes(root: HTMLElement): Text[] {
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const nodes: Text[] = [];
  while (walker.nextNode()) nodes.push(walker.currentNode as Text);
  return nodes;
}

export function textRange(root: HTMLElement, text: string, preferredOffset: number | null = null): Range | null {
  if (!text) return null;
  const nodes = textNodes(root);
  const all = nodes.map(node => node.data).join('');
  const offset = preferredOffset !== null && all.slice(preferredOffset, preferredOffset + text.length) === text
    ? preferredOffset : all.indexOf(text);
  if (offset < 0) return null;
  const range = document.createRange();
  let cursor = 0;
  let started = false;
  for (const node of nodes) {
    const end = cursor + node.length;
    if (!started && offset < end) { range.setStart(node, offset - cursor); started = true; }
    if (started && offset + text.length <= end) {
      range.setEnd(node, offset + text.length - cursor);
      return range;
    }
    cursor = end;
  }
  return null;
}

export interface SelectedText { text: string; offset: number; repeated: boolean; rect: DOMRect }

export function selectionDetails(root: HTMLElement): SelectedText | null {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed || selection.rangeCount !== 1) return null;
  const range = selection.getRangeAt(0);
  if (!root.contains(range.startContainer) || !root.contains(range.endContainer)) return null;
  const raw = range.toString();
  const text = raw.trim();
  if (!text) return null;
  const prefix = document.createRange();
  prefix.selectNodeContents(root);
  prefix.setEnd(range.startContainer, range.startOffset);
  const offset = prefix.toString().length + raw.indexOf(text);
  const content = root.textContent ?? '';
  const first = content.indexOf(text);
  return { text, offset, repeated: content.indexOf(text, first + text.length) !== -1, rect: range.getBoundingClientRect() };
}

export function capturePosition(root: HTMLElement, chapter: number, readerProgress: number): Position {
  const nodes = textNodes(root);
  let offset = 0;
  let anchor = '';
  const top = 88;
  for (const node of nodes) {
    if (!node.data.trim()) { offset += node.length; continue; }
    const range = document.createRange();
    range.selectNodeContents(node);
    if (range.getBoundingClientRect().bottom >= top) {
      let index = 0;
      // Find the first character on screen, rather than the start of a long paragraph.
      if (range.getBoundingClientRect().top < top) {
        let low = 0; let high = node.length - 1;
        while (low < high) {
          const middle = Math.floor((low + high) / 2);
          range.setStart(node, middle); range.setEnd(node, middle + 1);
          if (range.getBoundingClientRect().bottom < top) low = middle + 1; else high = middle;
        }
        index = low;
      }
      offset += index;
      anchor = (root.textContent ?? '').slice(offset, offset + 100);
      break;
    }
    offset += node.length;
  }
  const height = Math.max(1, document.documentElement.scrollHeight - window.innerHeight);
  return { chapter, anchor, offset, progress: Math.min(1, Math.max(0, window.scrollY / height)), reader_progress: readerProgress };
}

export function restorePosition(root: HTMLElement, position: Position): void {
  const range = textRange(root, position.anchor, position.offset);
  if (range) window.scrollBy({ top: range.getBoundingClientRect().top - 88, behavior: 'instant' });
  else window.scrollTo({ top: position.progress * Math.max(0, document.documentElement.scrollHeight - window.innerHeight), behavior: 'instant' });
}

type HighlightWindow = Window & typeof globalThis & {
  Highlight?: new (...ranges: Range[]) => unknown;
  CSS: typeof CSS & { highlights?: Map<string, unknown> };
};

export function applyHighlights(root: HTMLElement, highlights: Highlight[], chapter: number): boolean {
  const browser = window as HighlightWindow;
  if (!browser.Highlight || !browser.CSS.highlights) return false;
  const ranges = highlights.filter(item => item.chapter === null || item.chapter === chapter)
    .map(item => textRange(root, item.text, item.offset)).filter((range): range is Range => range !== null);
  browser.CSS.highlights.set('saved-highlights', new browser.Highlight(...ranges));
  return true;
}

export function clearHighlights(): void {
  (window as HighlightWindow).CSS.highlights?.delete('saved-highlights');
}
