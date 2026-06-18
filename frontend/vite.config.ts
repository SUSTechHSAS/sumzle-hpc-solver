/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  // Tauri dev server expects this fixed port.
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      '/api': 'http://localhost:3000',
    },
  },
  // Tauri builds the frontend with `vite build`. Make sure the output is
  // written to a path Tauri can pick up (configured in tauri.conf.json).
  build: {
    target: 'es2021',
    outDir: 'dist',
    emptyOutDir: true,
    // Use relative paths so assets resolve correctly under tauri://localhost
    // or http://tauri.localhost/ on Android WebView.
    assetsDir: 'assets',
    // Tauri serves files from the root, so relative paths work better.
  },
  // Ensure Tauri's asset protocol can load JS/CSS without CORS issues.
  // The `crossorigin` attribute on <script> tags can block loading under
  // custom schemes; setting base to './' makes Vite emit relative paths.
  base: './',
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})
