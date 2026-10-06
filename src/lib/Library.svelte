<script lang="ts">
  import { builtInMatch, compileQuery, readingMinutes, selectDocuments } from './library';
  import { safeImageUrl } from './content';
  import type { CustomView, Document, Settings } from './types';

  let { documents, views, settings, busy, selected = $bindable('inbox'), search = $bindable(''), page = $bindable(1), onopen, onsettings, onconfig }: {
    documents: Document[]; views: CustomView[]; settings: Settings; busy: boolean;
    selected: string; search: string; page: number;
    onopen: (id: string) => void; onsettings: (value: Settings) => void; onconfig: () => void;
  } = $props();
  let failedImages = $state<Record<string, boolean>>({});
  const pageSize = 60;
  const tabs = $derived([{ key: 'inbox', name: 'Inbox' }, { key: 'later', name: 'Later' }, { key: 'books', name: 'Books' },
    ...views.map((view, index) => ({ key: `custom-${index}`, name: view.name }))]);
  const filtered = $derived.by(() => {
    try {
      const custom = views[Number(selected.replace('custom-', ''))];
      const predicate = selected.startsWith('custom-') && custom ? compileQuery(custom.query)
        : (doc: Document) => builtInMatch(doc, selected as 'inbox' | 'later' | 'books');
      return { documents: selectDocuments(documents, predicate, search, settings.sort), error: null };
    } catch (error) { return { documents: [], error: String(error) }; }
  });
  const pageCount = $derived(Math.max(1, Math.ceil(filtered.documents.length / pageSize)));
  const currentPage = $derived(Math.min(page, pageCount));
  const visible = $derived(filtered.documents.slice((currentPage - 1) * pageSize, currentPage * pageSize));
  const title = $derived(tabs.find(tab => tab.key === selected)?.name ?? 'Inbox');

  $effect(() => { if (!tabs.some(tab => tab.key === selected)) selected = 'inbox'; });
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

<main class="library-screen" style:--cover-size={`${settings.cover_size}px`}>
  <div class="library-heading"><div><p class="eyebrow">YOUR LIBRARY</p><h1>{title}</h1></div><span class="library-count">{filtered.documents.length} {filtered.documents.length === 1 ? 'item' : 'items'}</span></div>
  <nav class="view-tabs" aria-label="Library views">
    {#each tabs as tab (tab.key)}
      <button class:active={selected === tab.key} aria-current={selected === tab.key ? 'page' : undefined} onclick={() => { selected = tab.key; page = 1; }}>{tab.name}</button>
    {/each}
  </nav>
  <div class="library-controls">
    <label class="search-field"><span class="sr-only">Search title, author or source</span><input type="search" placeholder="Search your library" bind:value={search} oninput={() => page = 1} /></label>
    <label class="sort-field"><span class="sr-only">Sort articles</span><select value={settings.sort} onchange={(event) => { onsettings({ ...settings, sort: event.currentTarget.value as Settings['sort'] }); page = 1; }}><option value="newest">Newest first</option><option value="oldest">Oldest first</option><option value="shortest">Shortest first</option></select></label>
    <div class="layout-toggle" aria-label="Library layout"><button aria-pressed={settings.view === 'covers'} onclick={() => onsettings({ ...settings, view: 'covers' })}>Covers</button><button aria-pressed={settings.view === 'list'} onclick={() => onsettings({ ...settings, view: 'list' })}>List</button></div>
    {#if settings.view === 'covers'}<label class="cover-size-control"><span>Cover size</span><input aria-label="Cover size" type="range" min="180" max="480" step="20" value={settings.cover_size} oninput={(event) => onsettings({ ...settings, cover_size: Number(event.currentTarget.value) })} /><output>{settings.cover_size}px</output></label>{/if}
  </div>
  {#key `${selected}:${settings.view}:${currentPage}`}
  <div class="library-results">
  {#if filtered.error}
    <div class="notice error" role="alert"><p>This view could not be read: {filtered.error}</p><button onclick={onconfig}>Edit views file</button></div>
  {:else if visible.length === 0}
    <div class="empty-state"><h2>{search ? 'No matching reads' : 'Nothing here yet'}</h2><p>{search ? 'Try another title, author or source.' : busy ? 'Your library is syncing.' : 'Saved items in this view will appear after a sync.'}</p></div>
  {:else}
    <div class:cover-grid={settings.view === 'covers'} class:article-list={settings.view === 'list'}>
      {#each visible as doc (doc.id)}
        {@const image = safeImageUrl(doc.image_url)}
        {@const minutes = readingMinutes(doc)}
        <button class="article-card" class:book-card={doc.category === 'epub'} onclick={() => onopen(doc.id)}>
          <div class="article-cover" class:book-cover={doc.category === 'epub'}>
            {#if image && !failedImages[doc.id]}<img src={image} alt="" loading="lazy" decoding="async" referrerpolicy="no-referrer" onerror={() => failedImages[doc.id] = true} />
            {:else}<div class="cover-fallback"><span class="cover-category">{doc.category === 'epub' ? 'BOOK' : source(doc)}</span><span class="cover-title">{doc.title}</span><span class="cover-author">{doc.author ?? 'Saved in Reader'}</span></div>{/if}
          </div>
          <div class="article-details"><span class="article-source">{source(doc)}</span><h2>{doc.title}</h2>{#if doc.author}<p class="article-author">{doc.author}</p>{/if}
            <div class="article-meta">{#if minutes !== null}<span>{Math.ceil(minutes)} min</span>{/if}<span>{date(doc.saved_at)}</span>{#if doc.reading_progress > 0}<span>{Math.round(Math.min(1, doc.reading_progress) * 100)}% read</span>{/if}</div>
          </div>
        </button>
      {/each}
    </div>
    {#if pageCount > 1}<nav class="pagination" aria-label="Library pages"><button disabled={currentPage === 1} onclick={() => { page = currentPage - 1; window.scrollTo(0, 0); }}>Previous</button><span>Page {currentPage} of {pageCount}</span><button disabled={currentPage === pageCount} onclick={() => { page = currentPage + 1; window.scrollTo(0, 0); }}>Next</button></nav>{/if}
  {/if}
  </div>
  {/key}
</main>
