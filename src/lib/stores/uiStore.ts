import { writable } from 'svelte/store';

export type PanelId =
	| 'timeline'
	| 'video-preview'
	| 'screen-record'
	| 'piano-roll'
	| 'transcribe'
	| 'stem-sep'
	| 'fx-rack'
	| 'captions'
	| 'voice'
	| 'song-factory'
	| 'backing'
	| 'master'
	| 'models'
	| 'diagnostics';

export const activePanelStore = writable<PanelId>('timeline');

export function setActivePanel(panel: PanelId) {
	activePanelStore.set(panel);
}
