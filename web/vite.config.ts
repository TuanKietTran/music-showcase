import { defineConfig } from 'vite';
// Dev proxy to the Rust server (see docs/api.md). No React plugin needed:
// Vite transforms TSX with its default automatic JSX runtime.
export default defineConfig({ server: { port: 5173, proxy: { '/api': 'http://localhost:8080' } } });
