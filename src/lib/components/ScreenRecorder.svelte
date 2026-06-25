<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { addTrack, addClipToTrack, projectStore } from '$lib/stores/projectStore';

	interface ToolStatus { name: string; available: boolean; detail: string; }

	let recording = false;
	let captureAudio = true;
	let fps = 30;
	let pendingPath: string | null = null;
	let error: string | null = null;
	let lastImportedName: string | null = null;
	let missingTools: string[] = [];
	let checkingTools = true;

	onMount(async () => {
		try {
			const tools = await invoke<ToolStatus[]>('check_video_tools');
			missingTools = tools.filter((t) => !t.available && (t.name === 'ffmpeg' || t.name === 'xrandr')).map((t) => t.name);
		} catch {
			// If the check itself fails, fall through and let Start surface the real error.
		} finally {
			checkingTools = false;
		}
	});

	function recordingsDir(): string {
		// Falls back to /tmp if the platform-specific home dir lookup fails;
		// the file still gets imported into the timeline regardless of location.
		return '/tmp';
	}

	async function startRecording() {
		error = null;
		const filename = `screen-recording-${Date.now()}.mp4`;
		pendingPath = `${recordingsDir()}/${filename}`;
		try {
			await invoke('start_screen_recording', {
				outputPath: pendingPath,
				fps,
				captureAudio
			});
			recording = true;
		} catch (e) {
			error = String(e);
			pendingPath = null;
		}
	}

	async function stopRecording() {
		try {
			const outputPath = await invoke<string>('stop_screen_recording');
			recording = false;
			await importRecording(outputPath);
		} catch (e) {
			error = String(e);
			recording = false;
		}
	}

	async function importRecording(filePath: string) {
		let duration = 0;
		let thumbnailPath: string | undefined;
		try {
			duration = await invoke<number>('probe_media_duration', { filePath });
		} catch (e) {
			console.error('Failed to probe recorded video duration:', e);
		}
		try {
			thumbnailPath = await invoke<string>('generate_video_thumbnail', { filePath });
		} catch (e) {
			console.error('Failed to generate thumbnail for recording:', e);
		}

		const name = filePath.split('/').pop() ?? 'Screen Recording';
		const bpm = $projectStore.bpm;
		const durationBeats = duration > 0 ? duration * (bpm / 60) : 64;

		addTrack('video');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];

		addClipToTrack(newTrack.id, {
			startBeat: 0,
			durationBeats,
			filePath,
			name,
			color: newTrack.color,
			type: 'video',
			videoInPoint: 0,
			videoOutPoint: duration,
			sourceDurationSeconds: duration,
			thumbnailPath
		});

		lastImportedName = name;
	}
</script>

<div class="screen-recorder">
	<h2>Screen Recorder</h2>
	<p class="hint">Captures the full screen via ffmpeg. The finished recording is added to the timeline as a new video track.</p>

	<div class="controls">
		<label>
			FPS
			<input type="number" min="10" max="60" bind:value={fps} disabled={recording} />
		</label>
		<label class="checkbox">
			<input type="checkbox" bind:checked={captureAudio} disabled={recording} />
			Capture default audio input
		</label>
	</div>

	{#if missingTools.length > 0}
		<p class="status error">
			Missing required tool(s): <code>{missingTools.join(', ')}</code>.
			Install them (e.g. <code>sudo apt install ffmpeg x11-xserver-utils</code>) and reopen this panel.
			See the Diagnose panel for details.
		</p>
	{/if}

	{#if !recording}
		<button class="record-btn" on:click={startRecording} disabled={checkingTools || missingTools.length > 0}>● Start Recording</button>
	{:else}
		<button class="record-btn stop" on:click={stopRecording}>■ Stop Recording</button>
	{/if}

	{#if recording}
		<p class="status recording">Recording in progress…</p>
	{/if}

	{#if lastImportedName && !recording}
		<p class="status success">Imported "{lastImportedName}" into the timeline.</p>
	{/if}

	{#if error}
		<p class="status error">{error}</p>
	{/if}
</div>

<style>
	.screen-recorder {
		flex: 1;
		padding: 24px;
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-width: 480px;
	}

	h2 {
		font-size: 16px;
		color: var(--text-primary);
	}

	.hint {
		font-size: 12px;
		color: var(--text-muted);
	}

	.controls {
		display: flex;
		flex-direction: column;
		gap: 8px;
		font-size: 12px;
		color: var(--text-secondary);
	}

	.controls label {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.controls input[type='number'] {
		width: 60px;
	}

	.checkbox {
		gap: 6px;
	}

	.record-btn {
		align-self: flex-start;
		padding: 8px 16px;
		border-radius: 6px;
		background: var(--accent);
		color: #fff;
		font-size: 13px;
		font-weight: 600;
	}

	.record-btn.stop {
		background: var(--error);
	}

	.record-btn:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.status code {
		background: var(--bg-highlight);
		padding: 1px 5px;
		border-radius: 3px;
		font-family: var(--font-mono);
		font-size: 11px;
	}

	.status {
		font-size: 12px;
	}

	.status.recording {
		color: var(--warning);
	}

	.status.success {
		color: var(--success);
	}

	.status.error {
		color: var(--error);
	}
</style>
