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
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: './src/test/setup.ts',
  },
})
