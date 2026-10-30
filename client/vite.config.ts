import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { VitePWA } from 'vite-plugin-pwa';
import { fileURLToPath } from 'node:url';

// Game server the dev server proxies to: ARENA_API=http://localhost:8090 for the Rust server
const server = process.env.ARENA_API ?? process.env.ARENA_SERVER ?? 'http://localhost:8080';

export default defineConfig({
  resolve: {
    alias: {
      // protocol.ts is the single source of truth for the client/server types
      '@protocol': fileURLToPath(new URL('../protocol/protocol.ts', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    host: true,
    proxy: {
      '/api': { target: server, changeOrigin: true },
      '/assets': { target: server, changeOrigin: true },
    },
    fs: { allow: ['..'] },
  },
  preview: {
    port: 4173,
    proxy: {
      '/api': { target: server, changeOrigin: true },
      '/assets': { target: server, changeOrigin: true },
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    target: 'es2022',
  },
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['apple-touch-icon.png', 'fonts/*', 'img/**/*'],
      manifest: {
        name: 'Altered Arena',
        short_name: 'Altered',
        description: 'Play the Altered TCG in your browser',
        theme_color: '#0f1020',
        background_color: '#0f1020',
        display: 'standalone',
        orientation: 'landscape',
        start_url: '/',
        icons: [
          { src: 'icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: 'icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: 'icon-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
        ],
      },
      workbox: {
        globPatterns: ['**/*.{js,css,html,woff,woff2,png,jpg,svg}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/api\//, /^\/assets\//],
        runtimeCaching: [
          {
            urlPattern: ({ url }) => url.pathname.startsWith('/assets/cards/'),
            handler: 'CacheFirst',
            options: {
              cacheName: 'card-art',
              expiration: { maxEntries: 2500, maxAgeSeconds: 60 * 60 * 24 * 60 },
              cacheableResponse: { statuses: [200] },
            },
          },
          {
            urlPattern: ({ url }) => url.pathname === '/api/v1/cards' || url.pathname === '/api/v1/decks',
            handler: 'StaleWhileRevalidate',
            options: { cacheName: 'static-data', cacheableResponse: { statuses: [200] } },
          },
        ],
      },
    }),
  ],
});
