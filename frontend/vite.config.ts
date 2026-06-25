import { defineConfig } from 'vite'
import { configDefaults } from 'vitest/config'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    // Tauri's Android system WebView can be very old. Huawei WebView 11 on
    // Android 10 (SDK 29) can't parse the logical-assignment operators
    // (`??=`/`||=`/`&&=`) esbuild emits at Vite's modern default target, so the
    // app loaded to a blank screen with "Uncaught SyntaxError: Unexpected
    // token '='". When building for Tauri, lower the esbuild target to es2015
    // so that modern syntax is transpiled down; the plain web build keeps
    // Vite's modern default (`undefined` → Vite's built-in default target).
    // Tauri sets TAURI_ENV_PLATFORM while running the beforeBuildCommand.
    target: process.env.TAURI_ENV_PLATFORM ? 'es2015' : undefined,
  },
  server: {
    proxy: {
      '/api': 'http://localhost:3000',
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
    // Playwright layout tests live in ./layout-tests and run via
    // `npm run test:layout` (a real Chromium engine). Exclude them from
    // vitest so `npm test` (jsdom) doesn't try to load them. Extend
    // configDefaults.exclude rather than duplicating it, so any future
    // vitest default excludes are inherited automatically.
    exclude: [...configDefaults.exclude, 'layout-tests/**'],
  },
})
