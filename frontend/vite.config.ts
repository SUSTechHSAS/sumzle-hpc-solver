/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
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
    // vitest so `npm test` (jsdom) doesn't try to load them. Preserve the
    // default vitest exclude patterns so node_modules / dist / etc. are
    // still skipped.
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/cypress/**',
      '**/.{idea,git,cache,output,temp}/**',
      '**/{karma,rollup,webpack,vite,vitest}.config.*',
      'layout-tests/**',
    ],
  },
})
