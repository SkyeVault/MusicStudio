import { writable, derived } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export type SidecarId = 'voice' | 'audio-fx' | 'song-gen' | 'stem-sep';
export type SidecarStatus = 'stopped' | 'starting' | 'running' | 'degraded' | 'error';

export interface SidecarState {
	id: SidecarId;
	label: string;
	port: number;
	status: SidecarStatus;
	lastPing?: number;
}

const SIDECARS: SidecarState[] = [
	{ id: 'audio-fx', label: 'Audio FX', port: 8002, status: 'stopped' },
	{ id: 'voice', label: 'Voice', port: 8001, status: 'stopped' },
	{ id: 'song-gen', label: 'Song Gen', port: 8003, status: 'stopped' },
	{ id: 'stem-sep', label: 'Stem Sep', port: 8004, status: 'stopped' }
];

export const sidecarStore = writable<SidecarState[]>(SIDECARS);

export const allSidecarsHealthy = derived(sidecarStore, ($sidecars) =>
	$sidecars.every((s) => s.status === 'running')
);

export function updateSidecarStatus(id: SidecarId, status: SidecarStatus) {
	sidecarStore.update((sidecars) =>
		sidecars.map((s) => (s.id === id ? { ...s, status, lastPing: Date.now() } : s))
	);
}

/** Ask the Rust backend to start a sidecar and watch its health. */
export async function startSidecar(id: SidecarId) {
	updateSidecarStatus(id, 'starting');
	try {
		await invoke('start_sidecar', { sidecarId: id });
	} catch (e) {
		updateSidecarStatus(id, 'error');
		console.error(`Failed to start sidecar ${id}:`, e);
	}
}

export async function stopSidecar(id: SidecarId) {
	try {
		await invoke('stop_sidecar', { sidecarId: id });
		updateSidecarStatus(id, 'stopped');
	} catch (e) {
		console.error(`Failed to stop sidecar ${id}:`, e);
	}
}

/** Poll sidecar health via Rust backend every 5 seconds. */
export function startHealthPolling() {
	const poll = async () => {
		const statuses = (await invoke('get_sidecar_statuses').catch(() => ({}))) as Record<string, string>;
		sidecarStore.update((sidecars) =>
			sidecars.map((s) => ({
				...s,
				status: (statuses[s.id] as SidecarStatus) ?? s.status
			}))
		);
	};

	poll();
	return setInterval(poll, 5000);
}
