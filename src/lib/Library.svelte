<script lang="ts">
  import { builtInMatch, compileQuery, readingMinutes, selectDocuments } from './library';
  import { safeImageUrl } from './content';
  import { resizeGrid } from './grid-motion';
  import type { CustomView, Document, Settings } from './types';

  let { documents, views, settings, busy, selected = $bindable('home'), search = $bindable(''), page = $bindable(1), onopen, onsettings, onconfig }: {
    documents: Document[]; views: CustomView[]; settings: Settings; busy: boolean;
    selected: string; search: string; page: number;
    onopen: (id: string) => void; onsettings: (value: Settings) => void; onconfig: () => void;
  } = $props();
  let failedImages = $state<Record<string, boolean>>({});
  const pageSize = 60;
  const tabs = $derived([{ key: 'home', name: 'Home' }, { key: 'inbox', name: 'Inbox' }, { key: 'later', name: 'Later' }, { key: 'books', name: 'Books' },
    ...views.map((view, index) => ({ key: `custom-${index}`, name: view.name }))]);
  const compiledViews = $derived(views.map(view => {
    try { return { ...view, predicate: compileQuery(view.query), error: null }; }
    catch (error) { return { ...view, predicate: null, error: String(error) }; }
  }));
  const ordered = $derived(selectDocuments(documents, () => true, search, settings.sort));
  const filtered = $derived.by(() => {
    if (selected === 'home') return { documents: [], error: null };
    const custom = compiledViews[Number(selected.replace('custom-', ''))];
    if (selected.startsWith('custom-') && custom) {
      return { documents: custom.predicate ? ordered.filter(custom.predicate) : [], error: custom.error };
    }
    return { documents: ordered.filter(doc => builtInMatch(doc, selected)), error: null };
  });
  const homeSections = $derived.by(() => {
    if (selected !== 'home') return [];
    let remaining = pageSize;
    return compiledViews.map((view, index) => {
      const matches = view.predicate ? ordered.filter(view.predicate) : [];
      const preview = matches.slice(0, Math.min(6, remaining));
      remaining -= preview.length;
      return { key: `custom-${index}`, name: view.name, error: view.error, matches, preview };
    });
  });
  const homeCount = $derived(new Set(homeSections.flatMap(section => section.matches.map(doc => doc.id))).size);
  const count = $derived(selected === 'home' ? homeCount : filtered.documents.length);
  const pageCount = $derived(Math.max(1, Math.ceil(filtered.documents.length / pageSize)));
  const currentPage = $derived(Math.min(page, pageCount));
  const visible = $derived(filtered.documents.slice((currentPage - 1) * pageSize, currentPage * pageSize));
  const title = $derived(tabs.find(tab => tab.key === selected)?.name ?? 'Home');

  $effect(() => { if (!tabs.some(tab => tab.key === selected)) selected = 'home'; });
  function date(value: string | null): string {
    if (!value) return '';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? '' : parsed.toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
  }
  function source(doc: Document): string {
    if (doc.site_name) return doc.site_name;
    try { return new URL(doc.source_url ?? doc.url).hostname.replace(/^www\./, ''); } catch { return doc.category; }
  }
</script>

{#snippet articleGrid(items: Document[])}
  <div class:cover-grid={settings.view === 'covers'} class:article-list={settings.view === 'list'}>
    {#each items as doc (doc.id)}
      {@const image = safeImageUrl(doc.image_url)}
      {@const minutes = readingMinutes(doc)}
      <button class="article-card" class:book-card={doc.category === 'epub'} onclick={() => onopen(doc.id)}>
        <div class="article-cover" class:book-cover={doc.category === 'epub'}>
          {#if image && !failedImages[doc.id]}<img src={image} alt="" loading="lazy" decoding="async" referrerpolicy="no-referrer" onerror={() => failedImages[doc.id] = true} />
          {:else}<div class="cover-fallback"><span class="cover-category">{doc.category === 'epub' ? 'BOOK' : source(doc)}</span><span class="cover-title">{doc.title}</span><span class="cover-author">{doc.author ?? 'Saved in Reader'}</span></div>{/if}
        </div>
        <div class="article-details"><span class="article-source">{source(doc)}</span>{#if selected === 'home'}<h3>{doc.title}</h3>{:else}<h2>{doc.title}</h2>{/if}{#if doc.author}<p class="article-author">{doc.author}</p>{/if}
          <div class="article-meta">{#if minutes !== null}<span>{Math.ceil(minutes)} min</span>{/if}<span>{date(doc.saved_at)}</span>{#if doc.reading_progress > 0}<span>{Math.round(Math.min(1, doc.reading_progress) * 100)}% read</span>{/if}</div>
        </div>
      </button>
    {/each}
  </div>
{/snippet}

<main class="library-screen" style:--cover-size={`${settings.cover_size}px`}>
  <div class="library-heading"><div><p class="eyebrow">YOUR LIBRARY</p><h1>{title}</h1></div><span class="library-count">{count} {count === 1 ? 'item' : 'items'}</span></div>
  <nav class="view-tabs" aria-label="Library views">
    {#each tabs as tab (tab.key)}
      <button class:active={selected === tab.key} aria-current={selected === tab.key ? 'page' : undefined} onclick={() => { selected = tab.key; page = 1; }}>{tab.name}</button>
    {/each}
  </nav>
  <div class="library-controls">
    <label class="search-field"><span class="sr-only">Search title, author or source</span><input type="search" placeholder="Search your library" bind:value={search} oninput={() => page = 1} /></label>
    <label class="sort-field"><span class="sr-only">Sort articles</span><select value={settings.sort} onchange={(event) => { onsettings({ ...settings, sort: event.currentTarget.value as Settings['sort'] }); page = 1; }}><option value="newest">Newest first</option><option value="oldest">Oldest first</option><option value="shortest">Shortest first</option></select></label>
    <div class="layout-toggle" aria-label="Library layout"><button aria-pressed={settings.view === 'covers'} onclick={() => onsettings({ ...settings, view: 'covers' })}>Covers</button><button aria-pressed={settings.view === 'list'} onclick={() => onsettings({ ...settings, view: 'list' })}>List</button></div>
    {#if settings.view === 'covers'}<label class="cover-size-control"><span>Cover size</span><span class="cover-size-slider"><span class="cover-slider-track" aria-hidden="true"></span><input aria-label="Cover size" type="range" min="180" max="480" step="20" value={settings.cover_size} oninput={(event) => onsettings({ ...settings, cover_size: Number(event.currentTarget.value) })} /></span><output>{settings.cover_size}px</output></label>{/if}
  </div>
  {#key `${selected}:${settings.view}:${currentPage}`}
  <div class="library-results" use:resizeGrid={settings.view === 'covers'}>
  {#if selected === 'home'}
    {#if homeSections.length === 0}
      <div class="empty-state"><h2>Make Home yours</h2><p>Add the names and queries of your Reader views to your views file, or start reading in Inbox, Later or Books.</p><button onclick={onconfig}>Open views file</button></div>
    {:else}
      <div class="home-sections">
        {#each homeSections as section (section.key)}
          <section class="home-section" aria-label={section.name}>
            <header class="home-section-heading"><div><h2 id={`home-${section.key}`}>{section.name}</h2><span class="library-count">{section.matches.length} {section.matches.length === 1 ? 'item' : 'items'}</span></div><button aria-label={`View all ${section.name}`} onclick={() => { selected = section.key; page = 1; window.scrollTo(0, 0); }}>View all</button></header>
            {#if section.error}<div class="notice error" role="alert"><p>This view could not be read: {section.error}</p><button onclick={onconfig}>Edit views file</button></div>
            {:else if section.preview.length}{@render articleGrid(section.preview)}
            {:else}<p class="home-section-note">{section.matches.length ? 'Open this view to browse all its items.' : search ? 'No matching reads in this view.' : busy ? 'Your library is syncing.' : 'Nothing in this view yet.'}</p>{/if}
          </section>
        {/each}
      </div>
    {/if}
  {:else if filtered.error}
    <div class="notice error" role="alert"><p>This view could not be read: {filtered.error}</p><button onclick={onconfig}>Edit views file</button></div>
  {:else if visible.length === 0}
    <div class="empty-state"><h2>{search ? 'No matching reads' : 'Nothing here yet'}</h2><p>{search ? 'Try another title, author or source.' : busy ? 'Your library is syncing.' : 'Saved items in this view will appear after a sync.'}</p></div>
  {:else}
    {@render articleGrid(visible)}
    {#if pageCount > 1}<nav class="pagination" aria-label="Library pages"><button disabled={currentPage === 1} onclick={() => { page = currentPage - 1; window.scrollTo(0, 0); }}>Previous</button><span>Page {currentPage} of {pageCount}</span><button disabled={currentPage === pageCount} onclick={() => { page = currentPage + 1; window.scrollTo(0, 0); }}>Next</button></nav>{/if}
  {/if}
  </div>
  {/key}
</main>

<style>
  .cover-size-slider { position: relative; width: 100px; height: 24px; }
  .cover-size-control .cover-size-slider input { appearance: none; -webkit-appearance: none; position: relative; width: 100%; height: 24px; margin: 0; padding: 0; border: 0; background: transparent; }
  .cover-size-slider input::-webkit-slider-runnable-track { height: 2px; background: transparent; }
  .cover-size-slider input::-webkit-slider-thumb { appearance: none; -webkit-appearance: none; width: 12px; height: 12px; margin-top: -5px; border: 0; border-radius: 50%; background: var(--heading); }
  .cover-size-slider input::-moz-range-track { height: 2px; background: transparent; }
  .cover-size-slider input::-moz-range-thumb { width: 12px; height: 12px; border: 0; border-radius: 50%; background: var(--heading); }
  .cover-slider-track { position: absolute; top: calc(50% - 1px); left: 0; right: 0; height: 2px; background: var(--rule); pointer-events: none; }
  .cover-size-control output, .cover-slider-track { opacity: 0; transition: opacity var(--motion-enter) var(--motion-ease); }
  .cover-size-control:hover output, .cover-size-control:focus-within output, .cover-size-control:hover .cover-slider-track, .cover-size-control:focus-within .cover-slider-track { opacity: 1; }
  @media (prefers-reduced-motion: reduce) { .cover-size-control output, .cover-slider-track { transition: none; } }
</style>
