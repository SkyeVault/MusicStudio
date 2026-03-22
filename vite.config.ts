import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [sveltekit()],
	clearScreen: false,
	optimizeDeps: {
		include: ['@tonejs/midi']
	},
	server: {
		host: host || false,
		port: 1420,
		strictPort: true,
		hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
		watch: {
			// Ignore Rust build artifacts and Python sidecar venvs (inotify limit)
			ignored: ['**/src-tauri/**', '**/sidecars/**']
		}
	},
	build: {
		// Tauri requires ES2021
		target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
		minify: !process.env.TAURI_ENV_DEBUG ? 'esbuild' : false,
		sourcemap: !!process.env.TAURI_ENV_DEBUG
	}
});
