import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  timeout: 45_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  use: { browserName: 'chromium', headless: true, actionTimeout: 10_000, trace: 'retain-on-failure', screenshot: 'only-on-failure' },
  reporter: 'line',
});
