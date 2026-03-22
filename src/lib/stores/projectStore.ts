import { writable, derived } from 'svelte/store';

export interface Clip {
	id: string;
	trackId: string;
	startBeat: number;
	durationBeats: number;
	filePath: string | null;
	name: string;
	color: string;
	type: 'audio' | 'midi';
}

export interface Track {
	id: string;
	name: string;
	color: string;
	type: 'audio' | 'midi' | 'instrument';
	muted: boolean;
	solo: boolean;
	volume: number; // 0–1
	pan: number;    // -1 to 1
	clips: Clip[];
}

export interface ProjectState {
	name: string;
	filePath: string | null;
	bpm: number;
	timeSignatureNumerator: number;
	timeSignatureDenominator: number;
	tracks: Track[];
	dirty: boolean;
}

const defaultProject = (): ProjectState => ({
	name: 'Untitled Project',
	filePath: null,
	bpm: 120,
	timeSignatureNumerator: 4,
	timeSignatureDenominator: 4,
	tracks: [],
	dirty: false
});

export const projectStore = writable<ProjectState>(defaultProject());

export const trackCount = derived(projectStore, ($p) => $p.tracks.length);

export function addTrack(type: Track['type'] = 'audio') {
	const colors = ['#7c5cbf', '#4caf7d', '#e6a817', '#e05252', '#4a90d9', '#e86db7'];
	projectStore.update((p) => {
		const id = crypto.randomUUID();
		const color = colors[p.tracks.length % colors.length];
		return {
			...p,
			dirty: true,
			tracks: [...p.tracks, {
				id,
				name: `Track ${p.tracks.length + 1}`,
				color,
				type,
				muted: false,
				solo: false,
				volume: 0.8,
				pan: 0,
				clips: []
			}]
		};
	});
}

export function removeTrack(trackId: string) {
	projectStore.update((p) => ({
		...p,
		dirty: true,
		tracks: p.tracks.filter((t) => t.id !== trackId)
	}));
}

export function updateTrack(trackId: string, updates: Partial<Track>) {
	projectStore.update((p) => ({
		...p,
		dirty: true,
		tracks: p.tracks.map((t) => (t.id === trackId ? { ...t, ...updates } : t))
	}));
}

export function addClipToTrack(trackId: string, clip: Omit<Clip, 'id' | 'trackId'>) {
	projectStore.update((p) => ({
		...p,
		dirty: true,
		tracks: p.tracks.map((t) =>
			t.id === trackId
				? { ...t, clips: [...t.clips, { ...clip, id: crypto.randomUUID(), trackId }] }
				: t
		)
	}));
}

export function resetProject() {
	projectStore.set(defaultProject());
}
