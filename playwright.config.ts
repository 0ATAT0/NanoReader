import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30_000,
  fullyParallel: true,
  use: { baseURL: 'http://127.0.0.1:1420', viewport: { width: 1280, height: 900 }, trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { browserName: 'chromium', channel: 'msedge' } }],
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: !process.env.CI },
});
