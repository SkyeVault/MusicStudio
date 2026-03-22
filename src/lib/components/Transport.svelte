<script lang="ts">
	import { projectStore } from '$lib/stores/projectStore';
	import { transportStore, setPlaying, resetPosition } from '$lib/stores/transportStore';
	import { audioEngine } from '$lib/audio/engine';

	function togglePlay() {
		if ($transportStore.playing) {
			audioEngine.pause();
			setPlaying(false);
		} else {
			audioEngine.play();
			setPlaying(true);
		}
	}

	function stop() {
		audioEngine.stop();
		setPlaying(false);
		resetPosition();
	}

	function onBpmInput(e: Event) {
		const val = Number((e.target as HTMLInputElement).value);
		if (val >= 20 && val <= 300) {
			projectStore.update((p) => ({ ...p, bpm: val }));
		}
	}

	// Format seconds as M:SS
	function formatTime(secs: number): string {
		const m = Math.floor(secs / 60);
		const s = Math.floor(secs % 60).toString().padStart(2, '0');
		return `${m}:${s}`;
	}
</script>

<div class="transport">
	<!-- Rewind -->
	<button class="ctrl-btn" title="Rewind to start" on:click={stop}>⏮</button>

	<!-- Play/Pause -->
	<button class="ctrl-btn play" title={$transportStore.playing ? 'Pause' : 'Play'} on:click={togglePlay}>
		{$transportStore.playing ? '⏸' : '▶'}
	</button>

	<!-- Stop -->
	<button class="ctrl-btn" title="Stop" on:click={stop}>⏹</button>

	<!-- Position display -->
	<span class="position" title="Playhead position">
		{formatTime($transportStore.positionSeconds)}
	</span>

	<!-- BPM -->
	<label class="bpm-label">
		BPM
		<input
			class="bpm-input"
			type="number"
			min="20"
			max="300"
			value={$projectStore.bpm}
			on:change={onBpmInput}
		/>
	</label>

	<!-- Time signature -->
	<span class="time-sig">
		{$projectStore.timeSignatureNumerator}/{$projectStore.timeSignatureDenominator}
	</span>
</div>

<style>
	.transport {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.ctrl-btn {
		width: 28px;
		height: 28px;
		border-radius: 4px;
		font-size: 12px;
		color: var(--text-secondary);
		display: flex;
		align-items: center;
		justify-content: center;
		transition: background 0.1s, color 0.1s;
	}

	.ctrl-btn:hover {
		background: var(--bg-highlight);
		color: var(--text-primary);
	}

	.ctrl-btn.play {
		background: var(--accent-dim);
		color: var(--accent-hover);
		width: 32px;
		height: 32px;
		font-size: 14px;
		border-radius: 50%;
	}

	.ctrl-btn.play:hover {
		background: var(--accent);
		color: #fff;
	}

	.position {
		font-family: var(--font-mono);
		font-size: 13px;
		color: var(--text-primary);
		min-width: 48px;
		text-align: center;
		padding: 0 6px;
	}

	.bpm-label {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-secondary);
	}

	.bpm-input {
		width: 52px;
		text-align: center;
		font-family: var(--font-mono);
		font-size: 13px;
		padding: 2px 4px;
	}

	.time-sig {
		font-size: 12px;
		color: var(--text-muted);
		padding: 0 4px;
	}
</style>
