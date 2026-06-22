import { defineConfig, devices } from '@playwright/test';

/**
 * Playwright config for layout-shift tests.
 *
 * These tests run a real browser engine (Chromium) so they can catch
 * layout shifts that jsdom (used by vitest) cannot — e.g. an inline error
 * message pushing sibling controls because it changes the parent's width
 * or height (issue #33).
 *
 * The tests assume a production build has been produced (`npm run build`)
 * and start `vite preview` themselves via the `webServer` config below.
 */
export default defineConfig({
  testDir: './layout-tests',
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://localhost:4317',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: 'npm run build && npm run preview -- --port 4317 --strictPort',
    url: 'http://localhost:4317',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
