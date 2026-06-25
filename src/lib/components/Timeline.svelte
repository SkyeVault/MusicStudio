<script lang="ts">
	import { onDestroy } from 'svelte';
	import WaveSurfer from 'wavesurfer.js';
	import { projectStore, addTrack, removeTrack, updateTrack, type Clip } from '$lib/stores/projectStore';
	import { transportStore } from '$lib/stores/transportStore';
	import { open } from '@tauri-apps/plugin-dialog';
	import { convertFileSrc, invoke } from '@tauri-apps/api/core';

	// Map from clip.id → WaveSurfer instance
	let waveInstances: Map<string, WaveSurfer> = new Map();

	$: tracks = $projectStore.tracks;

	// When transport stops, stop all wavesurfers
	$: if (!$transportStore.playing) {
		for (const ws of waveInstances.values()) ws.pause();
	}

	/** Svelte action: binds a waveform container and initialises WaveSurfer. */
	function waveformAction(node: HTMLElement, clipId: string) {
		initWaveSurfer(clipId, node);
		return {
			destroy() {
				const ws = waveInstances.get(clipId);
				if (ws) { ws.destroy(); waveInstances.delete(clipId); }
			}
		};
	}

	async function initWaveSurfer(clipId: string, container: HTMLElement) {
		const clip = findClip(clipId);
		if (!clip?.filePath) return;

		waveInstances.get(clipId)?.destroy();

		const ws = WaveSurfer.create({
			container,
			waveColor: '#7c5cbf',
			progressColor: '#b388ff',
			height: 60,
			barWidth: 2,
			barGap: 1,
			barRadius: 2,
			normalize: true,
			interact: true
		});

		await ws.load(convertFileSrc(clip.filePath));
		waveInstances.set(clipId, ws);
	}

	function findClip(clipId: string): Clip | undefined {
		for (const t of $projectStore.tracks) {
			const clip = t.clips.find((c) => c.id === clipId);
			if (clip) return clip;
		}
		return undefined;
	}

	async function addAudioTrack() {
		const selected = await open({
			multiple: false,
			filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'aiff', 'm4a'] }]
		});
		if (!selected || Array.isArray(selected)) return;

		addTrack('audio');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];

		projectStore.update((p) => ({
			...p,
			dirty: true,
			tracks: p.tracks.map((t) =>
				t.id === newTrack.id
					? {
							...t,
							name: selected.split('/').pop() ?? 'Audio',
							clips: [
								{
									id: crypto.randomUUID(),
									trackId: t.id,
									startBeat: 0,
									durationBeats: 64,
									filePath: selected,
									name: selected.split('/').pop() ?? 'Audio',
									color: t.color,
									type: 'audio' as const
								}
							]
					  }
					: t
			)
		}));
	}

	async function addVideoTrack() {
		const selected = await open({
			multiple: false,
			filters: [{ name: 'Video', extensions: ['mp4', 'mov', 'mkv', 'webm'] }]
		});
		if (!selected || Array.isArray(selected)) return;

		addTrack('video');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];
		const name = selected.split('/').pop() ?? 'Video';

		let duration = 0;
		let thumbnailPath: string | undefined;
		try {
			duration = await invoke<number>('probe_media_duration', { filePath: selected });
		} catch (e) {
			console.error('Failed to probe video duration:', e);
		}
		try {
			thumbnailPath = await invoke<string>('generate_video_thumbnail', { filePath: selected });
		} catch (e) {
			console.error('Failed to generate video thumbnail:', e);
		}

		const bpm = $projectStore.bpm;
		const durationBeats = duration > 0 ? duration * (bpm / 60) : 64;

		projectStore.update((p) => ({
			...p,
			dirty: true,
			tracks: p.tracks.map((t) =>
				t.id === newTrack.id
					? {
							...t,
							name,
							clips: [
								{
									id: crypto.randomUUID(),
									trackId: t.id,
									startBeat: 0,
									durationBeats,
									filePath: selected,
									name,
									color: t.color,
									type: 'video' as const,
									videoInPoint: 0,
									videoOutPoint: duration,
									sourceDurationSeconds: duration,
									thumbnailPath
								}
							]
					  }
					: t
			)
		}));
	}

	onDestroy(() => {
		for (const ws of waveInstances.values()) ws.destroy();
	});
</script>

<div class="timeline">
	<div class="track-list">
		{#each tracks as track (track.id)}
			<div class="track-row">
				<!-- Track header -->
				<div class="track-header" style="border-left: 3px solid {track.color}">
					<span class="track-name" title={track.name}>{track.name}</span>
					<div class="track-controls">
						<button
							class="track-btn"
							class:active={track.muted}
							title="Mute"
							on:click={() => updateTrack(track.id, { muted: !track.muted })}
						>M</button>
						<button
							class="track-btn"
							class:active={track.solo}
							title="Solo"
							on:click={() => updateTrack(track.id, { solo: !track.solo })}
						>S</button>
						<button
							class="track-btn danger"
							title="Remove track"
							on:click={() => removeTrack(track.id)}
						>✕</button>
					</div>
					<input
						class="track-volume"
						type="range"
						min="0"
						max="1"
						step="0.01"
						value={track.volume}
						title="Volume"
						on:input={(e) => updateTrack(track.id, { volume: Number((e.target as HTMLInputElement).value) })}
					/>
				</div>

				<!-- Clip area -->
				<div class="clip-area">
					{#each track.clips as clip (clip.id)}
						<div class="clip" style="border-color: {clip.color}">
							<span class="clip-name">{clip.name}</span>
							{#if clip.type === 'video'}
								<div class="video-thumb-container">
									{#if clip.thumbnailPath}
										<img class="video-thumb" src={convertFileSrc(clip.thumbnailPath)} alt={clip.name} />
									{:else}
										<div class="video-thumb-placeholder">🎬</div>
									{/if}
								</div>
							{:else}
								<div class="waveform-container" use:waveformAction={clip.id}></div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/each}
	</div>

	<div class="add-track-row">
		<button class="add-track-btn" on:click={addAudioTrack}>
			+ Add Audio Track
		</button>
		<button class="add-track-btn" on:click={addVideoTrack}>
			+ Add Video Track
		</button>
	</div>

	{#if tracks.length === 0}
		<div class="empty-state">
			<p>Click <strong>+ Add Audio Track</strong> or <strong>+ Add Video Track</strong> to load media and get started.</p>
		</div>
	{/if}
</div>

<style>
	.timeline {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow-y: auto;
		background: var(--bg-base);
		padding: 8px 0;
	}

	.track-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 0 8px;
	}

	.track-row {
		display: flex;
		height: 96px;
		background: var(--bg-surface);
		border-radius: 6px;
		overflow: hidden;
		border: 1px solid var(--border);
	}

	.track-header {
		width: 160px;
		flex-shrink: 0;
		background: var(--bg-elevated);
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.track-name {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.track-controls {
		display: flex;
		gap: 3px;
	}

	.track-btn {
		font-size: 10px;
		font-weight: 700;
		width: 20px;
		height: 20px;
		border-radius: 3px;
		color: var(--text-muted);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.track-btn:hover {
		background: var(--bg-highlight);
		color: var(--text-primary);
	}

	.track-btn.active {
		background: var(--accent);
		color: #fff;
		border-color: var(--accent);
	}

	.track-btn.danger:hover {
		background: var(--error);
		color: #fff;
		border-color: var(--error);
	}

	.track-volume {
		width: 100%;
		height: 3px;
		accent-color: var(--accent);
		cursor: pointer;
	}

	.clip-area {
		flex: 1;
		padding: 8px;
		overflow-x: auto;
		display: flex;
		gap: 4px;
	}

	.clip {
		min-width: 200px;
		flex: 1;
		border: 1px solid;
		border-radius: 4px;
		padding: 4px;
		background: rgba(255, 255, 255, 0.02);
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.clip-name {
		font-size: 10px;
		color: var(--text-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.waveform-container {
		flex: 1;
	}

	.video-thumb-container {
		flex: 1;
		display: flex;
		align-items: center;
		overflow: hidden;
		border-radius: 3px;
		background: rgba(0, 0, 0, 0.2);
	}

	.video-thumb {
		height: 100%;
		width: auto;
		object-fit: cover;
	}

	.video-thumb-placeholder {
		width: 100%;
		text-align: center;
		font-size: 18px;
		color: var(--text-muted);
	}

	.add-track-row {
		display: flex;
		gap: 8px;
		margin: 8px;
	}

	.add-track-btn {
		flex: 1;
		margin: 0;
		padding: 8px;
		border: 1px dashed var(--border);
		border-radius: 6px;
		color: var(--text-muted);
		font-size: 12px;
		transition: all 0.1s;
		text-align: center;
	}

	.add-track-btn:hover {
		border-color: var(--accent);
		color: var(--accent-hover);
		background: var(--accent-dim);
	}

	.empty-state {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
		font-size: 13px;
		text-align: center;
		padding: 40px;
	}

	.empty-state strong {
		color: var(--text-secondary);
	}
</style>
