import { writable, derived } from 'svelte/store';

export type TaskStatus = 'pending' | 'running' | 'completed' | 'failed';
export type TaskType =
	| 'stem-separation'
	| 'voice-convert'
	| 'voice-train'
	| 'song-generate'
	| 'backing-generate'
	| 'transcribe'
	| 'master'
	| 'effects-render';

export interface AiTask {
	id: string;
	type: TaskType;
	label: string;
	status: TaskStatus;
	progress: number; // 0–100
	errorMessage?: string;
	resultPath?: string;
	createdAt: number;
	completedAt?: number;
}

export const aiTaskStore = writable<AiTask[]>([]);

export const activeTasks = derived(aiTaskStore, ($tasks) =>
	$tasks.filter((t) => t.status === 'pending' || t.status === 'running')
);

export const completedTasks = derived(aiTaskStore, ($tasks) =>
	$tasks.filter((t) => t.status === 'completed' || t.status === 'failed')
);

export function createTask(type: TaskType, label: string): string {
	const id = crypto.randomUUID();
	aiTaskStore.update((tasks) => [
		...tasks,
		{ id, type, label, status: 'pending', progress: 0, createdAt: Date.now() }
	]);
	return id;
}

export function updateTask(id: string, updates: Partial<AiTask>) {
	aiTaskStore.update((tasks) =>
		tasks.map((t) => (t.id === id ? { ...t, ...updates } : t))
	);
}

export function completeTask(id: string, resultPath?: string) {
	aiTaskStore.update((tasks) =>
		tasks.map((t) =>
			t.id === id
				? { ...t, status: 'completed', progress: 100, resultPath, completedAt: Date.now() }
				: t
		)
	);
}

export function failTask(id: string, errorMessage: string) {
	aiTaskStore.update((tasks) =>
		tasks.map((t) =>
			t.id === id
				? { ...t, status: 'failed', errorMessage, completedAt: Date.now() }
				: t
		)
	);
}

export function clearCompletedTasks() {
	aiTaskStore.update((tasks) => tasks.filter((t) => t.status === 'pending' || t.status === 'running'));
}
