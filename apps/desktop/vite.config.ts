import { readFile } from 'node:fs/promises';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
export default defineConfig({
  plugins: [react(), {
    name: 'disposable-fixture-preview',
    configureServer(server) {
      server.middlewares.use('/__rooster_fixture.json', async (_request, response) => {
        const path = process.env.ROOSTER_BROWSER_FIXTURE;
        if (!path) { response.statusCode = 404; response.end(); return; }
        try { response.setHeader('Content-Type', 'application/json'); response.end(await readFile(path)); }
        catch { response.statusCode = 404; response.end(); }
      });
    },
  }],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**'] } },
  test: { environment: 'jsdom', setupFiles: ['./src/test/setup.ts'], css: false },
});
