import { writable } from 'svelte/store';

export type PanelId =
	| 'timeline'
	| 'piano-roll'
	| 'transcribe'
	| 'stem-sep'
	| 'fx-rack'
	| 'voice'
	| 'song-factory'
	| 'backing'
	| 'master'
	| 'models';

export const activePanelStore = writable<PanelId>('timeline');

export function setActivePanel(panel: PanelId) {
	activePanelStore.set(panel);
}
