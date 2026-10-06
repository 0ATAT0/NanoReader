<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import ReadingControls from './ReadingControls.svelte';
  import { sanitizeContent, selectionDetails, capturePosition, restorePosition, applyHighlights, clearHighlights } from './content';
  import type { SelectedText } from './content';
  import type { Highlight, Position, ReadingDocument, Settings } from './types';
  let { data, target, requestStatus, settings, onback, onarchive, onchapter, onhighlight, onposition, onsettings, onexternal, onflush }: {
    data: ReadingDocument; settings: Settings; onback: () => void; onarchive: () => Promise<void>;
    requestStatus: string;
    target: string | null; onchapter: (chapter: number, target?: string | null) => Promise<void>; onhighlight: (text: string, offset: number, chapter: number) => Promise<Highlight>;
    onposition: (position: Position) => Promise<void>; onsettings: (settings: Settings) => void; onexternal: (url: string) => Promise<void>;
    onflush: (flush: (() => Promise<void>) | null) => void;
  } = $props();
  let article: HTMLElement;
  let textSettingsButton: HTMLButtonElement;
  let typographyPanel = $state<HTMLElement>();
  let selection = $state<SelectedText | null>(null);
  let addedHighlights = $state<Highlight[]>([]);
  const highlights = $derived([...data.highlights, ...addedHighlights]);
  let error = $state<string | null>(null);
  let status = $state('');
  let busy = $state(false);
  let typography = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let write = Promise.resolve();
  let disposed = false;
  const html = $derived(sanitizeContent(data.html, data.document.source_url, data.document.category === 'epub'));
  const percent = $derived(Math.round(Math.min(1, Math.max(0, data.document.reading_progress)) * 100));
  const fonts = {
    inter: "'Inter Variable', Inter, sans-serif",
    georgia: 'Georgia, serif',
    system: 'system-ui, sans-serif',
  };
  const readingFont = $derived(fonts[settings.font_family]);
  const readingInk = $derived(`rgb(${settings.text_brightness}% ${settings.text_brightness}% ${settings.text_brightness}%)`);
  const floatingLeft = $derived(selection ? Math.max(12, Math.min(window.innerWidth - 180, selection.rect.left)) : 0);
  const floatingTop = $derived(selection ? Math.max(76, Math.min(window.innerHeight - 80, selection.rect.bottom + 10)) : 0);

  function save(): Promise<void> {
    clearTimeout(timer);
    if (!article) return write;
    const position = capturePosition(article, data.chapter, data.document.reading_progress);
    write = write.catch(() => undefined).then(() => onposition(position));
    return write;
  }
  function schedulePosition() { clearTimeout(timer); timer = setTimeout(() => { save().catch(report); }, 850); selection = null; }
  function report(reason: unknown) { if (!disposed) error = String(reason); }
  function updateSelection() { if (article && !busy) selection = selectionDetails(article); }
  async function leave(action: () => void | Promise<void>) {
    if (busy) return;
    busy = true; error = null;
    try { await save(); await action(); } catch (reason) { report(reason); } finally { busy = false; }
  }
  async function highlight() {
    if (!selection || busy) return;
    const chosen = selection;
    busy = true; error = null; status = '';
    try {
      const result = await onhighlight(chosen.text, chosen.offset, data.chapter);
      if (disposed) return;
      addedHighlights = [...addedHighlights, result];
      applyHighlights(article, highlights, data.chapter);
      window.getSelection()?.removeAllRanges(); selection = null;
      status = 'Highlight saved to Reader.';
    } catch (reason) { report(reason); } finally { busy = false; }
  }
  function followLink(event: MouseEvent) {
    const link = (event.target as Element).closest('a');
    if (!link || !article.contains(link)) return;
    event.preventDefault();
    const href = link.getAttribute('href');
    if (!href) return;
    if (href.startsWith('#')) {
      if (href.startsWith('#quiet-reader-chapter=')) {
        const destination = new URLSearchParams(href.slice(1));
        const chapter = Number(destination.get('quiet-reader-chapter'));
        if (Number.isInteger(chapter) && chapter >= 0 && chapter < data.chapters.length) leave(() => onchapter(chapter, destination.get('fragment')));
        return;
      }
      try { article.querySelector(`[id="${CSS.escape(decodeURIComponent(href.slice(1)))}"]`)?.scrollIntoView(); } catch { /* An invalid fragment has no destination. */ }
    } else { onexternal(href).catch(report); }
  }
  function stopNativeLinkNavigation(event: Event) {
    if ((event.target as Element).closest('a')) event.preventDefault();
  }
  function approximateResume() {
    if (data.chapters.length) {
      const chapter = Math.round(data.document.reading_progress * (data.chapters.length - 1));
      if (chapter === data.chapter) { window.scrollTo(0, 0); schedulePosition(); return; }
      leave(() => onchapter(chapter, ''));
      return;
    }
    window.scrollTo({ top: data.document.reading_progress * Math.max(0, document.documentElement.scrollHeight - window.innerHeight), behavior: 'instant' });
    schedulePosition();
  }
  function chooseChapter(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const next = Number(select.value);
    select.value = String(data.chapter);
    leave(() => onchapter(next));
  }
  function closeTypography(returnFocus = true) {
    typography = false;
    if (returnFocus) textSettingsButton?.focus({ preventScroll: true });
  }
  function toggleTypography() {
    if (typography) { closeTypography(); return; }
    typography = true;
    tick().then(() => { if (typography && !disposed) typographyPanel?.querySelector('select')?.focus({ preventScroll: true }); });
  }
  function closeOutsideTypography(event: PointerEvent) {
    if (typography && event.target instanceof Node && !typographyPanel?.contains(event.target)
      && !textSettingsButton?.contains(event.target)) closeTypography(false);
  }
  onMount(() => {
    let cancelled = false;
    tick().then(() => {
      if (cancelled) return;
      applyHighlights(article, highlights, data.chapter);
      if (target) article.querySelector(`[id="${CSS.escape(target)}"]`)?.scrollIntoView();
      else if (data.position && data.position.chapter === data.chapter) restorePosition(article, data.position);
      else window.scrollTo(0, 0);
    });
    document.addEventListener('selectionchange', updateSelection);
    article.addEventListener('click', followLink);
    article.addEventListener('auxclick', followLink);
    article.addEventListener('contextmenu', stopNativeLinkNavigation);
    article.addEventListener('dragstart', stopNativeLinkNavigation);
    onflush(save);
    return () => {
      cancelled = true; document.removeEventListener('selectionchange', updateSelection);
      article.removeEventListener('click', followLink); article.removeEventListener('auxclick', followLink);
      article.removeEventListener('contextmenu', stopNativeLinkNavigation); article.removeEventListener('dragstart', stopNativeLinkNavigation);
    };
  });
  onDestroy(() => { disposed = true; clearTimeout(timer); onflush(null); clearHighlights(); });
</script>

<svelte:window onscroll={schedulePosition} onpointerdown={closeOutsideTypography} onkeydown={(event) => { if (event.key === 'Escape' && typography) closeTypography(); }} onbeforeunload={() => { save().catch(() => undefined); }} />
<div class="reader-screen" style:--reading-font-family={readingFont} style:--reading-font-size={`${settings.font_size}px`} style:--reading-font-weight={settings.font_weight} style:--reading-line-height={settings.line_height} style:--reading-paragraph-spacing={`${settings.paragraph_spacing}em`} style:--reading-ink={readingInk} style:--reading-width={`${settings.reading_width}px`}>
  <header class="reader-toolbar"><button disabled={busy} onclick={() => leave(onback)}>← Library</button><div class="reader-actions"><button bind:this={textSettingsButton} aria-expanded={typography} aria-controls="reading-typography" onclick={toggleTypography}>Text settings</button><button disabled={busy} onclick={() => onexternal(data.document.url).catch(report)}>Open in Reader</button><button disabled={busy} onclick={() => leave(onarchive)}>Archive</button></div></header>
  {#if typography}<section bind:this={typographyPanel} id="reading-typography" class="reading-controls" aria-label="Reading typography"><div class="reading-controls-heading"><strong>Reading typography</strong><button onclick={() => closeTypography()}>Close</button></div><ReadingControls {settings} {onsettings} /></section>{/if}
  <main class="reading-page">
    <header class="article-heading"><p class="eyebrow">{data.document.site_name ?? data.document.category}</p><h1>{data.document.title}</h1><p class="reading-byline">{data.document.author ?? ''}{#if data.document.published_date}<span>{new Date(data.document.published_date).toLocaleDateString(undefined, { day: 'numeric', month: 'long', year: 'numeric' })}</span>{/if}</p>
      {#if percent > 0}<div class="remote-progress"><span>Reader progress: {percent}%</span><button onclick={approximateResume}>Resume approximately at {percent}%</button></div>{/if}
      {#if data.chapters.length > 0}<label class="chapter-field">Chapter<select aria-label="Book chapter" value={data.chapter} disabled={busy} onchange={chooseChapter}>{#each data.chapters as chapter}<option value={chapter.index}>{chapter.title}</option>{/each}</select></label>{/if}
    </header>
    {#if error}<p class="notice error" role="alert">{error}</p>{/if}<p class="reader-status" role="status" aria-label="Reading status">{requestStatus || status}</p>
    <!-- Sanitization lives in content.ts; native article links are intercepted before navigation. -->
    <article class="article-body" bind:this={article}>{@html html}</article>
    {#if data.chapters.length > 0}<nav class="chapter-navigation" aria-label="Book chapters"><button disabled={busy || data.chapter <= 0} onclick={() => leave(() => onchapter(data.chapter - 1))}>Previous chapter</button><span>{data.chapter + 1} / {data.chapters.length}</span><button disabled={busy || data.chapter >= data.chapters.length - 1} onclick={() => leave(() => onchapter(data.chapter + 1))}>Next chapter</button></nav>{/if}
    <footer class="reading-footer"><span>End of {data.document.category === 'epub' ? 'chapter' : 'article'}</span><button disabled={busy} onclick={() => leave(onarchive)}>Archive</button><button disabled={busy} onclick={() => leave(onback)}>Back to library</button></footer>
  </main>
  {#if selection}<div class="highlight-popover" style:left={`${floatingLeft}px`} style:top={`${floatingTop}px`}><button disabled={busy} onmousedown={(event) => event.preventDefault()} onclick={highlight}>{busy ? 'Saving…' : 'Highlight in Reader'}</button>{#if selection.repeated}<p>This passage repeats; Reader matches highlights by text and may place it at another occurrence.</p>{/if}</div>{/if}
</div>
