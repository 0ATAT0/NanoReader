import { test, expect, type Page } from '@playwright/test';
import type { Bootstrap, Document } from '../src/lib/types';
import examples from '../views.example.json' with { type: 'json' };

function document(id: string, values: Partial<Document> = {}): Document {
  return { id, url: `https://read.readwise.io/read/${id}`, source_url: 'https://example.com/article', title: id,
    author: 'An author', site_name: 'The Review', category: 'article', location: 'new', image_url: null,
    word_count: 1400, reading_time: '7 mins', reading_progress: 0, saved_at: '2026-01-01T00:00:00Z',
    published_date: '2026-01-01', updated_at: '2026-01-01T00:00:00Z', summary: null, tags: {}, ...values };
}

const documents = [document('streets', { title: 'Streets made for walking', image_url: 'http://127.0.0.1:1420/test-cover.svg' }),
  document('ten', { title: 'The ten-minute read', reading_time: '10 mins' }),
  document('long', { title: 'The patient city', location: 'later', reading_time: '22 mins' }),
  document('book', { title: 'A Small Book of Observation', category: 'epub', reading_time: '240 mins', reading_progress: 0.8 }),
  document('tagged', { title: 'A tagged essay', tags: { keep: 'Keep' } })];

const snapshot: Bootstrap = { connected: true, documents, settings: { font_size: 20, line_height: 1.8, reading_width: 900,
  font_family: 'inter', font_weight: 400, paragraph_spacing: 1.5, text_brightness: 83, cover_size: 280, view: 'covers', sort: 'newest' },
  views: examples.views, config_path: 'C:\\LocalAppData\\io.quietreader.desktop\\views.json', config_error: null, last_synced: '2026-01-01T00:00:00Z' };

const html = `<p id="opening">A street is best understood at walking pace. The ordinary details become visible when we slow down.</p>
  <p>Repeated passage. The middle of the article. Repeated passage.</p>
  <p><a href="https://example.com/original">Original reference</a> <a href="#ending">Jump to end</a></p>
  ${Array.from({ length: 45 }, (_, i) => `<p>Paragraph ${i + 1}. A reader should be able to return to the same place after changing views. Quiet interfaces leave space for the words and let the article set its own pace.</p>`).join('')}
  <p id="ending">The final paragraph.</p>`;

async function installBridge(page: Page, options: { count?: number; connected?: boolean; hostile?: boolean; readDelay?: number; readFailure?: boolean; archiveFailure?: boolean } = {}) {
  await page.route('**/test-cover.svg', route => route.fulfill({ contentType: 'image/svg+xml', body: '<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="750"><rect width="1200" height="750" fill="#313d43"/><path d="M0 590L500 220L900 470L1200 150V750H0Z" fill="#b1b8b7"/><path d="M0 690L700 330L1200 600V750H0Z" fill="#667477"/></svg>' }));
  const initial = structuredClone(snapshot);
  if (options.connected === false) { initial.connected = false; initial.documents = []; }
  if (options.count) initial.documents = Array.from({ length: options.count }, (_, i) => document(`item-${i}`, { title: `Article ${String(i).padStart(4, '0')}` }));
  await page.addInitScript(({ initial, html, options }) => {
    const win = window as any;
    win.isTauri = true;
    const callbacks = new Map<number, (payload: unknown) => void>();
    const listeners = new Map<string, number>();
    let nextCallback = 1;
    let library = structuredClone(initial);
    const positions: Record<string, unknown> = {};
    const highlights: Record<string, any[]> = {};
    win.testCalls = [];
    win.highlightFailure = false;
    win.chapterFailure = false;
    win.testEmit = (event: string, payload: unknown) => callbacks.get(listeners.get(event)!)?.({ event, id: 1, payload });
    win.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    win.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
      transformCallback: (callback: (payload: unknown) => void) => { const id = nextCallback++; callbacks.set(id, callback); return id; },
      invoke: async (command: string, args: any = {}) => {
        win.testCalls.push({ command, args });
        switch (command) {
          case 'plugin:event|listen': listeners.set(args.event, args.handler); return 1;
          case 'plugin:event|unlisten': return;
          case 'plugin:window|destroy': win.windowDestroyed = true; return;
          case 'bootstrap': return structuredClone(library);
          case 'connect': library = structuredClone(initial); library.connected = true; return structuredClone(library);
          case 'disconnect': library.connected = false; library.documents = []; return;
          case 'sync_library': return { documents: structuredClone(library.documents), last_synced: new Date().toISOString() };
          case 'save_settings': library.settings = structuredClone(args.settings); return;
          case 'reload_views': return { views: library.views, config_path: library.config_path, config_error: null };
          case 'open_config': case 'open_external': return;
          case 'read_document': {
            if (options.readDelay) await new Promise(resolve => setTimeout(resolve, options.readDelay));
            if (options.readFailure) throw 'Reader has no readable HTML for this item yet.';
            if (win.chapterFailure && args.chapter === 1) throw 'The requested chapter could not be read.';
            const doc = library.documents.find((document: any) => document.id === args.id);
            const chapter = args.chapter ?? (positions[args.id] as any)?.chapter ?? 0;
            const body = doc?.category === 'epub' ? `<h2>Chapter ${chapter + 1}</h2><p>Readable book chapter ${chapter + 1}.</p><a href="#quiet-reader-chapter=1&fragment=note">A book reference</a><p id="note">A chapter note.</p>` : html;
            return { document: doc, html: body + (options.hostile ? '<script>window.articleExecuted=true</script><style>body{background:red}</style><iframe src="https://evil.invalid"></iframe><form><input autofocus></form><img src="javascript:alert(1)" onerror="window.articleExecuted=true"><a href="file:///C:/Windows/notepad.exe">Unsafe link</a>' : ''),
              highlights: highlights[args.id] ?? [], position: positions[args.id] ?? null, chapter,
              chapters: doc?.category === 'epub' ? [{ index: 0, title: 'First chapter' }, { index: 1, title: 'Second chapter' }] : [] };
          }
          case 'archive_document':
            if (options.archiveFailure) throw 'Readwise is temporarily unavailable.';
            library.documents = library.documents.map((document: any) => document.id === args.id ? { ...document, location: 'archive' } : document); return;
          case 'create_highlight': {
            if (win.highlightFailure) throw 'Reader could not match this passage.';
            const value = { id: 'created-highlight', text: args.text, offset: args.offset, chapter: args.chapter };
            highlights[args.id] = [...(highlights[args.id] ?? []), value]; return value;
          }
          case 'save_position': positions[args.id] = structuredClone(args.position); return;
          default: throw `Unexpected native command: ${command}`;
        }
      },
    };
  }, { initial, html, options });
}

test('zero-config browsing, copied views, metadata covers, search and list layout', async ({ page }) => {
  await installBridge(page); await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Inbox', exact: true })).toBeVisible();
  await expect(page.locator('.article-card')).toHaveCount(4);
  await expect(page.locator('.article-cover img')).toHaveJSProperty('naturalWidth', 1200);
  await page.getByRole('button', { name: 'Quick Reads', exact: true }).click();
  await expect(page.locator('.article-card')).toHaveCount(2);
  await expect(page.getByRole('heading', { name: 'The ten-minute read' })).toBeVisible();
  await page.getByRole('button', { name: 'Long Reads', exact: true }).click();
  await expect(page.locator('.article-card')).toHaveCount(1);
  await expect(page.getByRole('heading', { name: 'The patient city' })).toBeVisible();
  await page.getByRole('button', { name: 'Books', exact: true }).click();
  await expect(page.getByRole('heading', { name: 'A Small Book of Observation' })).toBeVisible();
  await page.getByRole('button', { name: 'Inbox', exact: true }).click();
  await page.getByRole('searchbox').fill('walking');
  await expect(page.locator('.article-card')).toHaveCount(1);
  await page.getByRole('button', { name: 'List', exact: true }).click();
  await expect(page.locator('.article-list')).toBeVisible();
});

test('large library renders one page and can reach subsequent pages', async ({ page }) => {
  await installBridge(page, { count: 1000 }); await page.goto('/');
  await expect(page.locator('.article-card')).toHaveCount(60);
  await expect(page.getByText('1000 items')).toBeVisible();
  await page.getByRole('button', { name: 'Next', exact: true }).click();
  await expect(page.getByText('Page 2 of 17')).toBeVisible();
  await expect(page.locator('.article-card')).toHaveCount(60);
  await page.locator('.article-card').first().click();
  await page.getByRole('button', { name: '← Library' }).click();
  await expect(page.getByText('Page 2 of 17')).toBeVisible();
});

test('cover size changes the rendered grid and reading controls share saved preferences', async ({ page }) => {
  await installBridge(page); await page.goto('/');
  await page.getByRole('slider', { name: 'Cover size' }).fill('180');
  const small = await page.locator('.article-cover').first().boundingBox();
  await page.getByRole('slider', { name: 'Cover size' }).fill('480');
  await expect.poll(async () => (await page.locator('.article-cover').first().boundingBox())!.width).toBeGreaterThan(small!.width * 1.5);
  await expect(page.locator('.article-card')).toHaveCount(4);
  await page.getByRole('button', { name: 'List', exact: true }).click();
  await expect(page.getByRole('slider', { name: 'Cover size' })).toHaveCount(0);
  await page.getByRole('button', { name: 'Covers', exact: true }).click();
  await expect(page.getByRole('slider', { name: 'Cover size' })).toHaveValue('480');
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.getByRole('button', { name: 'Text settings' }).click();
  await page.getByRole('combobox', { name: 'Typeface' }).selectOption('georgia');
  await page.getByRole('slider', { name: 'Text weight' }).fill('700');
  await page.getByRole('slider', { name: 'Paragraph spacing' }).fill('0.9');
  await page.getByRole('slider', { name: 'Text brightness' }).fill('100');
  await expect(page.locator('.article-body')).toHaveCSS('font-weight', '700');
  await expect(page.locator('.article-body')).toHaveCSS('color', 'rgb(255, 255, 255)');
  await expect(page.locator('.article-body p').first()).toHaveCSS('margin-bottom', '18px');
  expect(await page.locator('.article-body').evaluate(element => getComputedStyle(element).fontFamily)).toContain('Georgia');
  await page.getByRole('button', { name: '← Library' }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await expect(page.getByRole('combobox', { name: 'Typeface' })).toHaveValue('georgia');
  await expect(page.getByRole('slider', { name: 'Text brightness' })).toHaveValue('100');
  await page.evaluate(() => (window as any).testEmit('tauri://close-requested', null));
  await expect.poll(() => page.evaluate(() => (window as any).windowDestroyed)).toBe(true);
  const saved = await page.evaluate(() => (window as any).testCalls.filter((call: any) => call.command === 'save_settings').at(-1).args.settings);
  expect(saved).toMatchObject({ cover_size: 480, font_family: 'georgia', font_weight: 700, paragraph_spacing: 0.9, text_brightness: 100 });
});

test('reduced motion disables screen and interaction animations', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await installBridge(page); await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Inbox', exact: true })).toBeVisible();
  const settingsButton = page.getByRole('button', { name: 'Settings', exact: true });
  const beforePress = (await settingsButton.boundingBox())!;
  await page.mouse.move(beforePress.x + beforePress.width / 2, beforePress.y + beforePress.height / 2);
  await page.mouse.down();
  expect((await settingsButton.boundingBox())!.y).toBe(beforePress.y);
  await page.mouse.up();
  await expect(page.getByRole('heading', { name: 'Settings', exact: true })).toBeVisible();
  expect(await page.evaluate(() => document.getAnimations().filter(animation => animation.playState === 'running').length)).toBe(0);
  await page.getByRole('button', { name: 'Back to library' }).click();
  await page.locator('.article-card').first().hover();
  expect(await page.evaluate(() => document.getAnimations().filter(animation => animation.playState === 'running').length)).toBe(0);
  await page.locator('.article-card').first().click();
  await page.getByRole('button', { name: 'Text settings' }).click();
  await expect(page.getByRole('combobox', { name: 'Typeface' })).toBeVisible();
  expect(await page.evaluate(() => document.getAnimations().filter(animation => animation.playState === 'running').length)).toBe(0);
});

test('request waits remain visible outside a library sync', async ({ page }) => {
  await installBridge(page, { readDelay: 1500 }); await page.goto('/');
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.evaluate(() => (window as any).testEmit('sync-progress', { message: 'Reader request limit reached; retrying in 17 seconds…', completed: 0 }));
  await expect(page.getByRole('status', { name: 'Library status' })).toContainText('retrying in 17 seconds');
});

test('unavailable article retains the library and offers its Reader link', async ({ page }) => {
  await installBridge(page, { readFailure: true }); await page.goto('/');
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await expect(page.getByRole('alert')).toContainText('no readable HTML');
  await expect(page.locator('.article-card')).toHaveCount(4);
  await page.getByRole('button', { name: 'Open in Reader', exact: true }).click();
  expect(await page.evaluate(() => (window as any).testCalls.filter((call: any) => call.command === 'open_external').at(-1).args.url)).toBe('https://read.readwise.io/read/streets');
  await page.getByRole('button', { name: 'Dismiss error' }).click();
  await expect(page.getByRole('button', { name: 'Open in Reader', exact: true })).toHaveCount(0);
});

test('VC-QR-4: hostile content is inert and article links open through the native boundary', async ({ page }) => {
  await installBridge(page, { hostile: true }); await page.goto('/');
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await expect(page.locator('.article-body')).toBeVisible();
  expect(await page.evaluate(() => (window as any).articleExecuted)).toBeUndefined();
  await expect(page.locator('.article-body script,.article-body iframe,.article-body style,.article-body form')).toHaveCount(0);
  await expect(page.getByRole('link', { name: 'Unsafe link' })).toHaveCount(0);
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe('rgb(0, 0, 0)');
  await page.getByRole('link', { name: 'Original reference' }).click();
  expect(await page.evaluate(() => (window as any).testCalls.filter((call: any) => call.command === 'open_external').at(-1).args.url)).toBe('https://example.com/original');
  await page.getByRole('button', { name: 'Open in Reader', exact: true }).click();
  expect(await page.evaluate(() => (window as any).testCalls.filter((call: any) => call.command === 'open_external').at(-1).args.url)).toBe('https://read.readwise.io/read/streets');
  expect(page.url()).toBe('http://127.0.0.1:1420/');
});

test('VC-QR-3: repeated selection keeps the local offset and a failed highlight is not presented as saved', async ({ page }) => {
  await installBridge(page); await page.goto('/'); await page.getByRole('button', { name: /Streets made for walking/ }).click();
  const choose = () => page.locator('.article-body p').nth(1).evaluate(element => {
    const node = element.firstChild!; const text = node.textContent!; const start = text.lastIndexOf('Repeated passage.');
    const range = document.createRange(); range.setStart(node, start); range.setEnd(node, start + 'Repeated passage.'.length);
    const selection = window.getSelection()!; selection.removeAllRanges(); selection.addRange(range); document.dispatchEvent(new Event('selectionchange'));
  });
  await choose(); await expect(page.getByText(/This passage repeats/)).toBeVisible();
  await page.evaluate(() => (window as any).highlightFailure = true);
  await page.getByRole('button', { name: 'Highlight in Reader' }).click();
  await expect(page.getByRole('alert')).toContainText('Reader could not match');
  await expect(page.getByText('Highlight saved to Reader.')).toHaveCount(0);
  expect(await page.evaluate(() => (CSS as any).highlights.get('saved-highlights').size)).toBe(0);
  await page.evaluate(() => (window as any).highlightFailure = false); await choose();
  await page.getByRole('button', { name: 'Highlight in Reader' }).click();
  await expect(page.getByText('Highlight saved to Reader.')).toBeVisible();
  expect(await page.evaluate(() => { const ranges = [...(CSS as any).highlights.get('saved-highlights')]; return ranges[0].toString(); })).toBe('Repeated passage.');
});

test('VC-QR-2: archive succeeds visibly and failures keep the article available', async ({ page }) => {
  await installBridge(page, { archiveFailure: true }); await page.goto('/'); await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.getByRole('button', { name: 'Archive', exact: true }).first().click();
  await expect(page.getByRole('alert')).toContainText('temporarily unavailable');
  await expect(page.locator('.article-body')).toBeVisible();
  await page.getByRole('button', { name: '← Library' }).click();
  await expect(page.getByRole('heading', { name: 'Streets made for walking' })).toBeVisible();
  await page.evaluate(() => { (window as any).__TAURI_INTERNALS__.invoke = new Proxy((window as any).__TAURI_INTERNALS__.invoke, { apply: (fn, self, args) => args[0] === 'archive_document' ? Promise.resolve() : Reflect.apply(fn, self, args) }); });
  await page.getByRole('button', { name: /Streets made for walking/ }).click(); await page.getByRole('button', { name: 'Archive', exact: true }).first().click();
  await expect(page.locator('.article-body')).toHaveCount(0);
  await expect(page.getByRole('heading', { name: 'Streets made for walking' })).toHaveCount(0);
});

test('VC-QR-7: local reading anchor restores and native close flushes pending typography and position', async ({ page }) => {
  await installBridge(page); await page.goto('/'); await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.evaluate(() => window.scrollTo(0, 1800));
  await page.getByRole('button', { name: '← Library' }).click();
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(1500);
  await page.getByRole('button', { name: 'Text settings' }).click();
  await page.getByRole('slider', { name: 'Text size' }).fill('24');
  await page.evaluate(() => (window as any).testEmit('tauri://close-requested', null));
  await expect.poll(() => page.evaluate(() => (window as any).windowDestroyed)).toBe(true);
  const calls = await page.evaluate(() => (window as any).testCalls);
  const destroy = calls.findIndex((call: any) => call.command === 'plugin:window|destroy');
  const settings = calls.findLastIndex((call: any) => call.command === 'save_settings');
  const position = calls.findLastIndex((call: any) => call.command === 'save_position');
  expect(settings).toBeLessThan(destroy); expect(position).toBeLessThan(destroy);
  expect(calls[settings].args.settings.font_size).toBe(24);
});

test('book chapters render, failed chapter changes retain correct selection, and references navigate', async ({ page }) => {
  await installBridge(page); await page.goto('/'); await page.getByRole('button', { name: 'Books', exact: true }).click();
  await page.getByRole('button', { name: /A Small Book of Observation/ }).click();
  await expect(page.getByText('Readable book chapter 1.')).toBeVisible();
  await page.evaluate(() => (window as any).chapterFailure = true);
  await page.getByRole('button', { name: 'Next chapter' }).click();
  await expect(page.getByRole('alert')).toContainText('could not be read');
  await expect(page.getByRole('combobox', { name: 'Book chapter' })).toHaveValue('0');
  await page.evaluate(() => (window as any).chapterFailure = false);
  await page.getByRole('link', { name: 'A book reference' }).click();
  await expect(page.getByText('Readable book chapter 2.')).toBeVisible();
  await expect(page.getByRole('combobox', { name: 'Book chapter' })).toHaveValue('1');
  await page.getByRole('button', { name: 'Previous chapter' }).click();
  await page.getByRole('button', { name: 'Resume approximately at 80%' }).click();
  await expect(page.getByRole('combobox', { name: 'Book chapter' })).toHaveValue('1');
});

test('VC-QR-6: a pending old-account read cannot appear after disconnect', async ({ page }) => {
  await installBridge(page, { readDelay: 1000 }); await page.goto('/');
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.getByRole('button', { name: 'Disconnect Readwise' }).click();
  await expect(page.getByLabel('Readwise access token')).toBeVisible();
  await page.waitForTimeout(1100);
  await expect(page.locator('.article-body')).toHaveCount(0);
  await expect(page.locator('.article-card')).toHaveCount(0);
  expect(await page.evaluate(() => Object.values(localStorage).join(''))).not.toContain('token');
});

test('visual acceptance: black library and reading screens have no horizontal overflow', async ({ page }, testInfo) => {
  await installBridge(page); await page.goto('/'); await page.getByRole('button', { name: 'Quick Reads', exact: true }).click();
  await page.evaluate(() => document.fonts.ready);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('library.png'), fullPage: false, animations: 'disabled' });
  await page.getByRole('button', { name: /Streets made for walking/ }).click();
  await page.screenshot({ path: testInfo.outputPath('reader.png'), fullPage: false, animations: 'disabled' });
  await page.getByRole('button', { name: 'Text settings' }).click();
  await page.screenshot({ path: testInfo.outputPath('reading-controls.png'), fullPage: false, animations: 'disabled' });
  await page.keyboard.press('Escape');
  await expect(page.getByRole('button', { name: 'Text settings' })).toBeFocused();
  await page.getByRole('button', { name: '← Library' }).click();
  await page.getByRole('button', { name: 'Settings', exact: true }).click();
  await page.screenshot({ path: testInfo.outputPath('settings.png'), fullPage: false, animations: 'disabled' });
  await page.setViewportSize({ width: 720, height: 900 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});
