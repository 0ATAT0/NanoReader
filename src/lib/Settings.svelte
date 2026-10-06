<script lang="ts">
  import ReadingControls from './ReadingControls.svelte';
  import type { Settings, ViewsConfig } from './types';
  let { settings, config, syncing, onsettings, onconfig, onreload, onrefresh, ondisconnect, onclose }: {
    settings: Settings; config: ViewsConfig; onsettings: (settings: Settings) => void;
    syncing: boolean; onconfig: () => void; onreload: () => void; onrefresh: () => void; ondisconnect: () => void; onclose: () => void;
  } = $props();
</script>

<main class="settings-screen">
  <header class="settings-heading"><div><p class="eyebrow">NANOREADER</p><h1>Settings</h1></div><button onclick={onclose}>Back to library</button></header>
  <section class="settings-section"><h2>Appearance</h2><div class="settings-fields"><label class="setting-field">Theme<select value={settings.theme} onchange={(event) => onsettings({ ...settings, theme: event.currentTarget.value as Settings['theme'] })}><option value="black">All Black</option><option value="dark">Dark</option><option value="light">Light</option></select></label></div></section>
  <section class="settings-section"><h2>Reading</h2><ReadingControls {settings} {onsettings} /><p class="setting-help">Desktop reading positions are saved here; Reader's mobile progress is available as an approximate starting point.</p></section>
  <section class="settings-section"><h2>Library</h2><div class="settings-fields">
    <label class="setting-field">Layout<select value={settings.view} onchange={(event) => onsettings({ ...settings, view: event.currentTarget.value as Settings['view'] })}><option value="covers">Covers</option><option value="list">List</option></select></label>
    <label class="setting-field">Sort<select value={settings.sort} onchange={(event) => onsettings({ ...settings, sort: event.currentTarget.value as Settings['sort'] })}><option value="newest">Newest first</option><option value="oldest">Oldest first</option><option value="shortest">Shortest first</option></select></label>
  </div><button disabled={syncing} onclick={onrefresh}>{syncing ? 'Refreshing…' : 'Refresh entire library'}</button><p class="setting-help">A full refresh also removes items deleted from Reader.</p></section>
  <section class="settings-section"><h2>Your views</h2><p>Inbox, Later and Books work without configuration. To add a view, copy its name and query from Reader into the views file.</p><p class="setting-help">The views file is private to your Windows account and stays in NanoReader's local data folder.</p><p class="setting-help">Supported filters include location, category, tags and reading time, with AND, OR and parentheses; unsupported queries show an error.</p>{#if config.config_path}<details class="config-details"><summary>Show file location</summary><code class="config-path">{config.config_path}</code></details>{/if}{#if config.config_error}<p class="notice error" role="alert">{config.config_error}</p>{/if}<div class="settings-actions"><button onclick={onconfig}>Open views file</button><button onclick={onreload}>Reload views</button></div></section>
  <section class="settings-section"><h2>Readwise connection</h2><p>Disconnect clears this account’s local library and reading positions and removes the stored token.</p><button class="destructive" onclick={ondisconnect}>Disconnect Readwise</button></section>
</main>
