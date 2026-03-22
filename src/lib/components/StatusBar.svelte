<script lang="ts">
	import { projectStore } from '$lib/stores/projectStore';
	import { transportStore } from '$lib/stores/transportStore';
	import { activeTasks } from '$lib/stores/aiTaskStore';
	import { sidecarStore } from '$lib/stores/sidecarStore';

	$: runningCount = $sidecarStore.filter((s) => s.status === 'running').length;
</script>

<footer class="status-bar">
	<span class="status-item">
		{$projectStore.name}{$projectStore.dirty ? ' •' : ''}
	</span>
	<span class="status-sep">|</span>
	<span class="status-item">
		{$projectStore.bpm} BPM
	</span>
	<span class="status-sep">|</span>
	<span class="status-item">
		{$projectStore.tracks.length} tracks
	</span>
	<span class="status-sep">|</span>
	<span class="status-item">
		{runningCount}/4 sidecars active
	</span>
	{#if $activeTasks.length > 0}
		<span class="status-sep">|</span>
		<span class="status-item ai-running">
			⚙ {$activeTasks[0].label}
			{#if $activeTasks[0].progress > 0}
				({$activeTasks[0].progress}%)
			{/if}
		</span>
	{/if}
	<span class="status-spacer"></span>
	<span class="status-item muted">MusicStudio v0.1.0</span>
</footer>

<style>
	.status-bar {
		height: 22px;
		background: var(--bg-surface);
		border-top: 1px solid var(--border);
		display: flex;
		align-items: center;
		padding: 0 10px;
		gap: 0;
		flex-shrink: 0;
	}

	.status-item {
		font-size: 11px;
		color: var(--text-muted);
		padding: 0 8px;
		white-space: nowrap;
	}

	.status-item.ai-running {
		color: var(--accent-hover);
	}

	.status-item.muted {
		color: var(--text-muted);
		font-size: 10px;
	}

	.status-sep {
		color: var(--border);
		font-size: 11px;
	}

	.status-spacer {
		flex: 1;
	}
</style>
