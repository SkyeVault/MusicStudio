import { writable, derived } from 'svelte/store';

export interface TransportState {
	playing: boolean;
	recording: boolean;
	positionBeats: number;
	positionSeconds: number;
	loopEnabled: boolean;
	loopStartBeat: number;
	loopEndBeat: number;
}

export const transportStore = writable<TransportState>({
	playing: false,
	recording: false,
	positionBeats: 0,
	positionSeconds: 0,
	loopEnabled: false,
	loopStartBeat: 0,
	loopEndBeat: 8
});

export const isPlaying = derived(transportStore, ($t) => $t.playing);
export const isRecording = derived(transportStore, ($t) => $t.recording);

export function setPlaying(playing: boolean) {
	transportStore.update((t) => ({ ...t, playing }));
}

export function setRecording(recording: boolean) {
	transportStore.update((t) => ({ ...t, recording }));
}

export function setPosition(positionSeconds: number, bpm: number) {
	const beatsPerSecond = bpm / 60;
	transportStore.update((t) => ({
		...t,
		positionSeconds,
		positionBeats: positionSeconds * beatsPerSecond
	}));
}

export function resetPosition() {
	transportStore.update((t) => ({ ...t, positionBeats: 0, positionSeconds: 0 }));
}
