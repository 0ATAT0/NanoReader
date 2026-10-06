<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import Library from './lib/Library.svelte';
  import Reader from './lib/Reader.svelte';
  import SettingsPanel from './lib/Settings.svelte';
  import { applyTheme } from './lib/theme';
  import { bootstrap, connect, disconnect, syncLibrary, readDocument, archiveDocument,
    createHighlight, savePosition, saveSettings, reloadViews, openConfig, openExternal, onSyncProgress } from './lib/bridge';
  import type { Bootstrap, Document, Highlight, Position, ReadingDocument, Settings, ViewsConfig } from './lib/types';

  let connected = $state(false);
  let ready = $state(false);
  let documents = $state<Document[]>([]);
  let settings = $state<Settings>({ theme: 'black', font_family: 'inter', font_size: 20, font_weight: 400, line_height: 1.8, paragraph_spacing: 1.5, text_brightness: 83, reading_width: 900, cover_size: 280, view: 'covers', sort: 'newest' });
  const theme = $derived(settings.theme);
  $effect(() => applyTheme(theme));
  let config = $state<ViewsConfig>({ views: [], config_path: '', config_error: null });
  let lastSynced = $state<string | null>(null);
  let reading = $state<ReadingDocument | null>(null);
  let readingTarget = $state<string | null>(null);
  let screen = $state<'library' | 'settings'>('library');
  let token = $state('');
  let authenticating = $state(false);
  let syncing = $state(false);
  let opening = $state(false);
  let mutating = $state(false);
  let libraryView = $state('home');
  let librarySearch = $state('');
  let libraryPage = $state(1);
  let status = $state('');
  let error = $state<string | null>(null);
  let failedDocumentUrl = $state<string | null>(null);
  let generation = $state(0);
  let readGeneration = 0;
  let libraryRevision = 0;
  let stopProgress: (() => void) | undefined;
  let stopClose: (() => void) | undefined;
  let flushReading: (() => Promise<void>) | null = null;
  let settingsTimer: ReturnType<typeof setTimeout> | undefined;
  let progressTimer: ReturnType<typeof setTimeout> | undefined;
  let settingsWrite = Promise.resolve();
  let mounted = true;

  function report(reason: unknown) { if (mounted) { error = String(reason); failedDocumentUrl = null; } }
  function readerUrl(id: string): string | null {
    try {
      const url = new URL(documents.find(document => document.id === id)?.url ?? '');
      return url.protocol === 'https:' && ['read.readwise.io', 'readwise.io'].includes(url.hostname)
        && !url.username && !url.password && !url.port ? url.href : null;
    } catch { return null; }
  }
  function applySnapshot(snapshot: Bootstrap) {
    connected = snapshot.connected; documents = snapshot.documents; settings = snapshot.settings;
    config = { views: snapshot.views, config_path: snapshot.config_path, config_error: snapshot.config_error };
    lastSynced = snapshot.last_synced;
  }
  async function sync(full = false) {
    if (!connected || syncing) return;
    const account = generation;
    const revision = libraryRevision;
    syncing = true; error = null; status = 'Syncing your library…';
    try {
      let snapshot = await syncLibrary(full);
      if (account !== generation || !mounted) return;
      let snapshotRevision = revision;
      while (snapshotRevision !== libraryRevision) {
        snapshotRevision = libraryRevision;
        snapshot = await bootstrap();
        if (account !== generation || !mounted) return;
      }
      if (account !== generation || !mounted) return;
      documents = snapshot.documents; lastSynced = snapshot.last_synced; status = 'Library up to date.';
    } catch (reason) { if (account === generation) report(reason); }
    finally { if (account === generation) syncing = false; }
  }
  async function refreshDuringSync(account: number) {
    const revision = libraryRevision;
    try {
      const snapshot = await bootstrap();
      if (account === generation && revision === libraryRevision && connected && mounted) { documents = snapshot.documents; lastSynced = snapshot.last_synced; }
    } catch (reason) { if (account === generation) report(reason); }
  }
  async function signIn(event: SubmitEvent) {
    event.preventDefault();
    if (!token.trim() || authenticating) return;
    const supplied = token.trim(); token = '';
    const account = ++generation;
    authenticating = true; error = null; failedDocumentUrl = null;
    try {
      const snapshot = await connect(supplied);
      if (account !== generation || !mounted) return;
      applySnapshot(snapshot); await sync();
    } catch (reason) { if (account === generation) report(reason); }
    finally { if (account === generation) authenticating = false; }
  }
  async function signOut() {
    ++generation; ++readGeneration;
    clearTimeout(settingsTimer); clearTimeout(progressTimer);
    connected = false; documents = []; reading = null; lastSynced = null;
    libraryView = 'home'; librarySearch = ''; libraryPage = 1; mutating = false;
    config = { views: [], config_path: '', config_error: null };
    token = ''; screen = 'library'; status = ''; error = null; failedDocumentUrl = null; syncing = false; opening = false; authenticating = false;
    try { await disconnect(); } catch (reason) { report(reason); }
  }
  async function open(id: string, chapter: number | null = null, target: string | null = null) {
    const account = generation; const request = ++readGeneration;
    opening = true; error = null; failedDocumentUrl = null; status = 'Opening article…';
    try {
      const result = await readDocument(id, chapter);
      if (account === generation && request === readGeneration && connected && mounted) { reading = result; readingTarget = target; }
    } catch (reason) {
      if (account === generation && request === readGeneration) {
        if (reading) throw reason;
        report(reason);
        failedDocumentUrl = readerUrl(id);
      }
    }
    finally { if (account === generation && request === readGeneration) opening = false; }
  }
  function back() { ++readGeneration; reading = null; failedDocumentUrl = null; window.scrollTo(0, 0); }
  async function archive(id: string, account: number) {
    if (account !== generation) throw new Error('This account was disconnected.');
    mutating = true; status = 'Archiving…';
    try { await archiveDocument(id); } finally { if (account === generation) mutating = false; }
    if (account !== generation) return;
    ++libraryRevision;
    documents = documents.map(doc => doc.id === id ? { ...doc, location: 'archive' } : doc);
    back(); status = 'Archived in Reader.';
  }
  async function highlight(id: string, text: string, offset: number, chapter: number, account: number): Promise<Highlight> {
    if (account !== generation) throw new Error('This account was disconnected.');
    mutating = true; status = 'Saving highlight…';
    let result: Highlight;
    try { result = await createHighlight(id, text, offset, chapter); }
    finally { if (account === generation) mutating = false; }
    if (account !== generation) throw new Error('This account was disconnected.');
    return result;
  }
  async function position(id: string, value: Position, account: number) {
    if (account !== generation || !connected) return;
    await savePosition(id, value);
  }
  function changeSettings(value: Settings) {
    settings = value; clearTimeout(settingsTimer);
    settingsTimer = setTimeout(() => { settingsTimer = undefined; queueSettings().catch(report); }, 250);
  }
  function queueSettings(): Promise<void> {
    const account = generation;
    const next = { ...settings };
    settingsWrite = settingsWrite.catch(() => undefined).then(async () => {
      if (account === generation && connected) await saveSettings(next);
    });
    return settingsWrite;
  }
  async function flushSettings() {
    if (settingsTimer) { clearTimeout(settingsTimer); settingsTimer = undefined; await queueSettings(); }
    else await settingsWrite;
  }
  async function reload() {
    const account = generation;
    try { const result = await reloadViews(); if (account === generation) { config = result; status = 'Views reloaded.'; } }
    catch (reason) { if (account === generation) report(reason); }
  }
  onMount(() => {
    const account = generation;
    bootstrap().then(snapshot => {
      if (account !== generation || !mounted) return;
      applySnapshot(snapshot); ready = true;
      if (connected) sync();
    }).catch(reason => { report(reason); ready = true; });
    onSyncProgress(progress => {
      if (!connected) return;
      status = progress.message;
      if (syncing && !progressTimer) {
        const current = generation;
        progressTimer = setTimeout(() => { progressTimer = undefined; refreshDuringSync(current); }, 1000);
      }
    }).then(unlisten => { if (mounted) stopProgress = unlisten; else unlisten(); }).catch(report);
    if (isTauri()) {
      getCurrentWindow().onCloseRequested(async event => {
        event.preventDefault();
        try { await flushReading?.(); await flushSettings(); await getCurrentWindow().destroy(); }
        catch (reason) { report(reason); }
      }).then(unlisten => { if (mounted) stopClose = unlisten; else unlisten(); }).catch(report);
    }
  });
  onDestroy(() => { mounted = false; ++generation; clearTimeout(settingsTimer); clearTimeout(progressTimer); stopProgress?.(); stopClose?.(); });
</script>

<svelte:head><title>NanoReader</title><meta name="color-scheme" content={theme === 'light' ? 'light' : 'dark'} /></svelte:head>
{#if error}<div class="app-notice notice error" role="alert"><span>{error}</span><div class="notice-actions">{#if failedDocumentUrl}<button onclick={() => { if (failedDocumentUrl) openExternal(failedDocumentUrl).catch(report); }}>Open in Reader</button>{/if}<button aria-label="Dismiss error" onclick={() => { error = null; failedDocumentUrl = null; }}>Dismiss</button></div></div>{/if}
{#if reading && connected}
  {@const account = generation}
  {@const id = reading.document.id}
  {#key `${id}:${reading.chapter}`}
    <Reader data={reading} target={readingTarget} requestStatus={opening || mutating || syncing ? status : ''} {settings} onback={back} onarchive={() => archive(id, account)} onchapter={(chapter, target = null) => open(id, chapter, target)} onhighlight={(text, offset, chapter) => highlight(id, text, offset, chapter, account)} onposition={(value) => position(id, value, account)} onsettings={changeSettings} onexternal={openExternal} onflush={(flush) => flushReading = flush} />
  {/key}
{:else}
  <header class="app-header"><a class="app-brand" href="#library" onclick={(event) => { event.preventDefault(); screen = 'library'; }}>NanoReader</a>
    {#if connected}<div class="app-actions"><button disabled={syncing} onclick={() => sync()}>{syncing ? 'Syncing…' : 'Sync'}</button><button aria-pressed={screen === 'settings'} onclick={() => screen = screen === 'settings' ? 'library' : 'settings'}>Settings</button></div>{/if}
  </header>
  {#if !ready}<main class="onboarding"><p class="eyebrow">NANOREADER</p><h1>Opening your library…</h1></main>
  {:else if !connected}<main class="onboarding"><p class="eyebrow">MAKE ROOM FOR READING</p><h1>Your library.<br />A quieter place to read.</h1><p>Articles and books saved in Readwise Reader, with just the things you need to read them.</p>
    <form onsubmit={signIn}><label for="access-token">Readwise access token</label><input id="access-token" type="password" autocomplete="off" spellcheck="false" bind:value={token} disabled={authenticating} required /><button class="primary" type="submit" disabled={authenticating || !token.trim()}>{authenticating ? 'Connecting…' : 'Connect Readwise'}</button></form>
    <button class="text-link" onclick={() => openExternal('https://readwise.io/access_token').catch(report)}>Get your token from Readwise ↗</button><p class="setting-help">Stored in Windows Credential Manager and used only to connect to Readwise.</p>
  </main>
  {:else if screen === 'settings'}<SettingsPanel {settings} {config} {syncing} onsettings={changeSettings} onconfig={() => openConfig().catch(report)} onreload={reload} onrefresh={() => sync(true)} ondisconnect={signOut} onclose={() => screen = 'library'} />
  {:else}<Library {documents} views={config.views} {settings} bind:selected={libraryView} bind:search={librarySearch} bind:page={libraryPage} busy={syncing} onopen={open} onsettings={changeSettings} onconfig={() => openConfig().catch(report)} />{/if}
  {#if connected}<footer class="app-footer"><span role="status" aria-label="Library status">{status || (lastSynced ? `Synced ${new Date(lastSynced).toLocaleString()}` : 'Ready to sync')}</span>{#if config.config_error}<button class="config-warning" onclick={() => screen = 'settings'}>Views file needs attention</button>{/if}</footer>{/if}
{/if}
