import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  base: '/ui-assets/',
  build: {
    outDir: 'dist',
    assetsDir: '',
    rollupOptions: {
      input: 'index.html',
      output: {
        entryFileNames: 'workspace.js',
        chunkFileNames: 'workspace.js',
        assetFileNames: (assetInfo) => {
          if (assetInfo.name && assetInfo.name.endsWith('.css')) {
            return 'workspace.css';
          }
          return '[name][extname]';
        },
      },
    },
  },
});
