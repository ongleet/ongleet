import { defineConfig } from 'vite';

export default defineConfig({
  // Diff workers load CodeMirror language parsers on demand.
  worker: { format: 'es' },
});
