import { writable, derived, get } from 'svelte/store';
import { Midi } from '@tonejs/midi';

// ---------------------------------------------------------------------------
// Data model
// ---------------------------------------------------------------------------

export interface MidiNote {
	id: string;
	pitch: number;        // 0–127 MIDI note number
	startTick: number;
	durationTicks: number;
	velocity: number;     // 0–127
	selected: boolean;
}

export interface MidiTrackData {
	id: string;
	name: string;
	color: string;
	instrument: number;   // GM program number 0–127
	notes: MidiNote[];
}

export interface MidiState {
	ppq: number;          // pulses per quarter note (ticks per beat)
	tracks: MidiTrackData[];
	activeTrackId: string | null;
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

const DEFAULT_PPQ = 480;

function defaultState(): MidiState {
	const trackId = crypto.randomUUID();
	return {
		ppq: DEFAULT_PPQ,
		activeTrackId: trackId,
		tracks: [{
			id: trackId,
			name: 'MIDI 1',
			color: '#7c5cbf',
			instrument: 0,
			notes: []
		}]
	};
}

export const midiStore = writable<MidiState>(defaultState());

export const activeTrack = derived(midiStore, ($m) =>
	$m.tracks.find((t) => t.id === $m.activeTrackId) ?? $m.tracks[0] ?? null
);

// ---------------------------------------------------------------------------
// Tick / time helpers
// ---------------------------------------------------------------------------

export function ticksToSeconds(ticks: number, bpm: number, ppq: number): number {
	return (ticks / ppq) * (60 / bpm);
}

export function secondsToTicks(seconds: number, bpm: number, ppq: number): number {
	return seconds * (bpm / 60) * ppq;
}

export function ticksToBeats(ticks: number, ppq: number): number {
	return ticks / ppq;
}

export function beatsToTicks(beats: number, ppq: number): number {
	return Math.round(beats * ppq);
}

// Snap a tick value to the nearest grid division (e.g. 1/16 note)
export function snapTick(tick: number, ppq: number, division: number): number {
	const gridTicks = ppq * (4 / division);
	return Math.round(tick / gridTicks) * gridTicks;
}

// MIDI note number → note name (e.g. 60 → "C4")
export function midiToNoteName(midi: number): string {
	const names = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
	return names[midi % 12] + (Math.floor(midi / 12) - 1);
}

// ---------------------------------------------------------------------------
// Note CRUD
// ---------------------------------------------------------------------------

export function addNote(trackId: string, note: Omit<MidiNote, 'id' | 'selected'>) {
	midiStore.update((m) => ({
		...m,
		tracks: m.tracks.map((t) =>
			t.id === trackId
				? { ...t, notes: [...t.notes, { ...note, id: crypto.randomUUID(), selected: false }] }
				: t
		)
	}));
}

export function updateNote(trackId: string, noteId: string, updates: Partial<MidiNote>) {
	midiStore.update((m) => ({
		...m,
		tracks: m.tracks.map((t) =>
			t.id === trackId
				? { ...t, notes: t.notes.map((n) => (n.id === noteId ? { ...n, ...updates } : n)) }
				: t
		)
	}));
}

export function deleteSelectedNotes(trackId: string) {
	midiStore.update((m) => ({
		...m,
		tracks: m.tracks.map((t) =>
			t.id === trackId ? { ...t, notes: t.notes.filter((n) => !n.selected) } : t
		)
	}));
}

export function selectAll(trackId: string) {
	midiStore.update((m) => ({
		...m,
		tracks: m.tracks.map((t) =>
			t.id === trackId ? { ...t, notes: t.notes.map((n) => ({ ...n, selected: true })) } : t
		)
	}));
}

export function clearSelection(trackId: string) {
	midiStore.update((m) => ({
		...m,
		tracks: m.tracks.map((t) =>
			t.id === trackId ? { ...t, notes: t.notes.map((n) => ({ ...n, selected: false })) } : t
		)
	}));
}

// ---------------------------------------------------------------------------
// Import from @tonejs/midi parsed object
// ---------------------------------------------------------------------------

export function loadFromToneJsMidi(midi: Midi) {
	const tracks: MidiTrackData[] = midi.tracks.map((t, i) => ({
		id: crypto.randomUUID(),
		name: t.name || `Track ${i + 1}`,
		color: ['#7c5cbf', '#4caf7d', '#e6a817', '#e05252', '#4a90d9'][i % 5],
		instrument: t.instrument.number,
		notes: t.notes.map((n) => ({
			id: crypto.randomUUID(),
			pitch: n.midi,
			startTick: Math.round(n.ticks),
			durationTicks: Math.round(n.durationTicks),
			velocity: Math.round(n.velocity * 127),
			selected: false
		}))
	}));

	midiStore.set({
		ppq: midi.header.ppq,
		activeTrackId: tracks[0]?.id ?? null,
		tracks
	});
}

// ---------------------------------------------------------------------------
// Export to MIDI bytes (via @tonejs/midi)
// ---------------------------------------------------------------------------

export function exportToMidiBytes(bpm: number): Uint8Array {
	const state = get(midiStore);
	const midi = new Midi();
	midi.header.setTempo(bpm);

	for (const trackData of state.tracks) {
		const track = midi.addTrack();
		track.name = trackData.name;
		for (const note of trackData.notes) {
			track.addNote({
				midi: note.pitch,
				ticks: note.startTick,
				durationTicks: note.durationTicks,
				velocity: note.velocity / 127
			});
		}
	}

	return midi.toArray();
}

// ---------------------------------------------------------------------------
// Parse a raw MIDI file Uint8Array
// ---------------------------------------------------------------------------

export function parseMidiBytes(bytes: Uint8Array): void {
	const midi = new Midi(bytes);
	loadFromToneJsMidi(midi);
}
