import { defineConfig } from 'vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { fileURLToPath } from 'node:url';

const host = process.env.TAURI_DEV_HOST;

// Web build ("vite build --mode web"): replace the Tauri IPC/plugin packages
// with browser shims so the same Svelte components talk to the Rust web server.
/** @param {string} file */
const webShim = (file) => fileURLToPath(new URL(`./src/lib/web/${file}`, import.meta.url));
const webAliases = [
  { find: '@tauri-apps/api/core', replacement: webShim('tauri-core.ts') },
  { find: '@tauri-apps/api/event', replacement: webShim('tauri-event.ts') },
  { find: '@tauri-apps/api/webviewWindow', replacement: webShim('tauri-webviewWindow.ts') },
  { find: '@tauri-apps/plugin-log', replacement: webShim('tauri-log.ts') },
  { find: '@tauri-apps/plugin-dialog', replacement: webShim('tauri-dialog.ts') },
  { find: '@tauri-apps/plugin-opener', replacement: webShim('tauri-opener.ts') },
  { find: '@tauri-apps/plugin-notification', replacement: webShim('tauri-notification.ts') },
];

// https://vite.dev/config/
export default defineConfig(async ({ mode }) => ({
  resolve: mode === 'web' ? { alias: webAliases } : undefined,
  plugins: [
    paraglideVitePlugin({
      project: './project.inlang',
      outdir: './src/paraglide',
      strategy: ['globalVariable', 'baseLocale'],
      emitTsDeclarations: true,
    }),
    sveltekit(),
  ],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
}));
