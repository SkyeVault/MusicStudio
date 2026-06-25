<script lang="ts">
	import { open, save } from '@tauri-apps/plugin-dialog';
	import { readFile, writeTextFile } from '@tauri-apps/plugin-fs';
	import { invoke } from '@tauri-apps/api/core';
	import { projectStore, addTrack, addClipToTrack } from '$lib/stores/projectStore';
	import { createTask, updateTask, completeTask, failTask } from '$lib/stores/aiTaskStore';
	import { sidecarStore, startSidecar } from '$lib/stores/sidecarStore';

	const SIDECAR_URL = 'http://127.0.0.1:8005';

	interface CaptionSegment { start: number; end: number; text: string; }
	interface SilenceRange { start: number; end: number; }

	let selectedFile: string | null = null;
	let selectedFileName = '';

	let captionStatus: 'idle' | 'running' | 'completed' | 'failed' = 'idle';
	let captionProgress = 0;
	let segments: CaptionSegment[] = [];
	let captionError = '';

	let silenceStatus: 'idle' | 'running' | 'completed' | 'failed' = 'idle';
	let silenceProgress = 0;
	let silences: SilenceRange[] = [];
	let silenceError = '';
	let sourceDuration = 0;

	$: sidecarStatus = $sidecarStore.find((s) => s.id === 'video-ai')?.status ?? 'stopped';
	$: sidecarReady = sidecarStatus === 'running';

	async function ensureSidecar() {
		if (!sidecarReady) {
			await startSidecar('video-ai');
			await new Promise((r) => setTimeout(r, 3000));
		}
	}

	async function pickFile() {
		const result = await open({
			multiple: false,
			filters: [{ name: 'Media', extensions: ['mp4', 'mov', 'mkv', 'webm', 'wav', 'mp3', 'm4a'] }]
		});
		if (result && !Array.isArray(result)) {
			selectedFile = result;
			selectedFileName = result.split('/').pop() ?? result;
			segments = [];
			silences = [];
			captionStatus = 'idle';
			silenceStatus = 'idle';
			try {
				sourceDuration = await invoke<number>('probe_media_duration', { filePath: result });
			} catch {
				sourceDuration = 0;
			}
		}
	}

	async function uploadFile(): Promise<Blob> {
		const bytes = await readFile(selectedFile!);
		return new Blob([bytes]);
	}

	async function generateCaptions() {
		if (!selectedFile) return;
		await ensureSidecar();

		captionStatus = 'running';
		captionProgress = 0;
		captionError = '';
		const taskId = createTask('transcribe', `Captioning: ${selectedFileName}`);

		try {
			const formData = new FormData();
			formData.append('media', await uploadFile(), selectedFileName);

			const response = await window.fetch(`${SIDECAR_URL}/captions`, { method: 'POST', body: formData });
			if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`);
			const { job_id } = await response.json();

			const poll = setInterval(async () => {
				try {
					const res = await window.fetch(`${SIDECAR_URL}/captions/${job_id}`);
					const data = await res.json();
					captionProgress = data.progress ?? 0;
					updateTask(taskId, { progress: captionProgress });

					if (data.status === 'completed') {
						clearInterval(poll);
						captionStatus = 'completed';
						segments = data.segments ?? [];
						completeTask(taskId);
					} else if (data.status === 'failed') {
						clearInterval(poll);
						captionStatus = 'failed';
						captionError = data.error ?? 'Unknown error';
						failTask(taskId, captionError);
					}
				} catch {
					// transient — keep polling
				}
			}, 1500);
		} catch (e) {
			captionStatus = 'failed';
			captionError = String(e);
			failTask(taskId, captionError);
		}
	}

	async function detectSilence() {
		if (!selectedFile) return;
		await ensureSidecar();

		silenceStatus = 'running';
		silenceProgress = 0;
		silenceError = '';
		const taskId = createTask('transcribe', `Detecting silence: ${selectedFileName}`);

		try {
			const formData = new FormData();
			formData.append('media', await uploadFile(), selectedFileName);

			const response = await window.fetch(`${SIDECAR_URL}/silence-detect`, { method: 'POST', body: formData });
			if (!response.ok) throw new Error(`HTTP ${response.status}: ${await response.text()}`);
			const { job_id } = await response.json();

			const poll = setInterval(async () => {
				try {
					const res = await window.fetch(`${SIDECAR_URL}/silence-detect/${job_id}`);
					const data = await res.json();
					silenceProgress = data.progress ?? 0;
					updateTask(taskId, { progress: silenceProgress });

					if (data.status === 'completed') {
						clearInterval(poll);
						silenceStatus = 'completed';
						silences = data.silences ?? [];
						completeTask(taskId);
					} else if (data.status === 'failed') {
						clearInterval(poll);
						silenceStatus = 'failed';
						silenceError = data.error ?? 'Unknown error';
						failTask(taskId, silenceError);
					}
				} catch {
					// transient — keep polling
				}
			}, 1500);
		} catch (e) {
			silenceStatus = 'failed';
			silenceError = String(e);
			failTask(taskId, silenceError);
		}
	}

	/** Build the non-silent segments (the parts worth keeping) from the detected silent ranges. */
	function keepSegments(): { start: number; end: number }[] {
		if (sourceDuration <= 0) return [];
		const sorted = [...silences].sort((a, b) => a.start - b.start);
		const keep: { start: number; end: number }[] = [];
		let cursor = 0;
		for (const s of sorted) {
			if (s.start > cursor) keep.push({ start: cursor, end: s.start });
			cursor = Math.max(cursor, s.end);
		}
		if (cursor < sourceDuration) keep.push({ start: cursor, end: sourceDuration });
		return keep.filter((k) => k.end - k.start > 0.05);
	}

	function addToTimelineSkippingSilence() {
		if (!selectedFile) return;
		const segs = keepSegments();
		if (segs.length === 0) return;

		const isVideo = /\.(mp4|mov|mkv|webm)$/i.test(selectedFile);
		addTrack(isVideo ? 'video' : 'audio');
		const tracks = $projectStore.tracks;
		const newTrack = tracks[tracks.length - 1];
		const bpm = $projectStore.bpm;

		let startBeat = 0;
		for (const seg of segs) {
			const durationBeats = (seg.end - seg.start) * (bpm / 60);
			addClipToTrack(newTrack.id, {
				startBeat,
				durationBeats,
				filePath: selectedFile,
				name: selectedFileName,
				color: newTrack.color,
				type: isVideo ? 'video' : 'audio',
				videoInPoint: isVideo ? seg.start : undefined,
				videoOutPoint: isVideo ? seg.end : undefined,
				sourceDurationSeconds: isVideo ? sourceDuration : undefined
			});
			startBeat += durationBeats;
		}
	}

	function formatTime(seconds: number): string {
		const m = Math.floor(seconds / 60);
		const s = (seconds % 60).toFixed(2);
		return `${m}:${s.padStart(5, '0')}`;
	}

	function toSrtTimestamp(seconds: number): string {
		const ms = Math.round(seconds * 1000);
		const h = Math.floor(ms / 3600000);
		const m = Math.floor((ms % 3600000) / 60000);
		const s = Math.floor((ms % 60000) / 1000);
		const msRem = ms % 1000;
		return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')},${String(msRem).padStart(3, '0')}`;
	}

	async function exportSrt() {
		const path = await save({ filters: [{ name: 'SubRip', extensions: ['srt'] }], defaultPath: `${selectedFileName}.srt` });
		if (!path) return;
		const lines = segments
			.map((seg, i) => `${i + 1}\n${toSrtTimestamp(seg.start)} --> ${toSrtTimestamp(seg.end)}\n${seg.text}\n`)
			.join('\n');
		await writeTextFile(path, lines);
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>📝 Captions &amp; Silence Removal</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		<div class="section">
			<div class="section-label">Media File</div>
			<div class="file-row">
				<div class="drop-zone" class:has-file={!!selectedFile} role="button" tabindex="0"
					on:click={pickFile} on:keydown={(e) => e.key === 'Enter' && pickFile()}>
					{#if selectedFile}
						<span class="file-name">🎬 {selectedFileName}</span>
					{:else}
						<span>Click to pick a video or audio file</span>
					{/if}
				</div>
			</div>
		</div>

		<div class="actions-row">
			<button class="primary-btn" disabled={!selectedFile || captionStatus === 'running'} on:click={generateCaptions}>
				{captionStatus === 'running' ? 'Transcribing…' : 'Generate Captions'}
			</button>
			<button class="primary-btn" disabled={!selectedFile || silenceStatus === 'running'} on:click={detectSilence}>
				{silenceStatus === 'running' ? 'Analyzing…' : 'Detect Silence'}
			</button>
		</div>

		{#if captionStatus === 'running'}
			<div class="progress-bar"><div class="progress-fill" style="width: {captionProgress}%"></div></div>
		{/if}
		{#if captionStatus === 'failed'}
			<div class="error-box">{captionError}</div>
		{/if}
		{#if captionStatus === 'completed'}
			<div class="results-section">
				<div class="results-header">
					<span class="section-label">Captions ({segments.length})</span>
					<button class="add-all-btn" on:click={exportSrt}>Export .srt</button>
				</div>
				<div class="segment-list">
					{#each segments as seg}
						<div class="segment-row">
							<span class="segment-time">{formatTime(seg.start)} – {formatTime(seg.end)}</span>
							<span class="segment-text">{seg.text}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if silenceStatus === 'running'}
			<div class="progress-bar"><div class="progress-fill" style="width: {silenceProgress}%"></div></div>
		{/if}
		{#if silenceStatus === 'failed'}
			<div class="error-box">{silenceError}</div>
		{/if}
		{#if silenceStatus === 'completed'}
			<div class="results-section">
				<div class="results-header">
					<span class="section-label">Silent ranges ({silences.length})</span>
					<button class="add-all-btn" on:click={addToTimelineSkippingSilence}>+ Add to Timeline (skip silence)</button>
				</div>
				<div class="segment-list">
					{#each silences as s}
						<div class="segment-row">
							<span class="segment-time">{formatTime(s.start)} – {formatTime(s.end)}</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		{#if !sidecarReady}
			<div class="info-box">
				The Video AI sidecar is not running. It will start automatically when you click an action above.
				faster-whisper and pydub must be installed in <code>sidecars/video-ai/venv/</code>.
			</div>
		{/if}
	</div>
</div>

<style>
	.panel { flex: 1; display: flex; flex-direction: column; overflow: hidden; background: var(--bg-base); }

	.panel-header {
		display: flex; align-items: center; gap: 10px;
		padding: 14px 20px 10px; border-bottom: 1px solid var(--border);
		background: var(--bg-surface); flex-shrink: 0;
	}
	.panel-header h2 { font-size: 15px; font-weight: 600; color: var(--text-primary); }

	.badge { font-size: 10px; padding: 2px 8px; border-radius: 10px; border: 1px solid; }
	.badge.ready { color: var(--success); border-color: var(--success); background: rgba(76,175,125,0.1); }
	.badge.offline { color: var(--text-muted); border-color: var(--border); }

	.panel-body {
		flex: 1; overflow-y: auto; padding: 20px;
		display: flex; flex-direction: column; gap: 16px; max-width: 720px;
	}

	.section-label {
		font-size: 11px; font-weight: 600; color: var(--text-secondary);
		text-transform: uppercase; letter-spacing: 0.5px; display: block; margin-bottom: 6px;
	}

	.file-row { display: flex; gap: 8px; align-items: center; }

	.drop-zone {
		flex: 1; border: 1px dashed var(--border); border-radius: 8px;
		padding: 14px 18px; color: var(--text-muted); font-size: 13px;
		cursor: pointer; transition: all 0.15s; text-align: center;
	}
	.drop-zone:hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
	.drop-zone.has-file { border-color: var(--accent); color: var(--text-primary); background: var(--bg-elevated); }
	.file-name { font-weight: 500; }

	.actions-row { display: flex; gap: 10px; }

	.primary-btn {
		padding: 10px 20px; background: var(--accent); color: #fff; border-radius: 8px;
		font-weight: 600; font-size: 13px; transition: background 0.15s;
	}
	.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
	.primary-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.progress-bar { height: 6px; background: var(--bg-elevated); border-radius: 3px; overflow: hidden; }
	.progress-fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.4s ease; }

	.error-box {
		background: rgba(224, 82, 82, 0.1); border: 1px solid var(--error);
		border-radius: 8px; padding: 12px 16px; font-size: 12px; color: var(--error);
	}

	.results-section { display: flex; flex-direction: column; gap: 10px; }
	.results-header { display: flex; align-items: center; justify-content: space-between; }

	.add-all-btn {
		font-size: 12px; padding: 4px 12px; border: 1px solid var(--accent);
		border-radius: 6px; color: var(--accent-hover); transition: background 0.1s;
	}
	.add-all-btn:hover { background: var(--accent-dim); }

	.segment-list { display: flex; flex-direction: column; gap: 6px; max-height: 320px; overflow-y: auto; }

	.segment-row {
		display: flex; gap: 12px; background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: 6px; padding: 8px 12px; font-size: 12px;
	}
	.segment-time { font-family: var(--font-mono); color: var(--text-muted); white-space: nowrap; }
	.segment-text { color: var(--text-primary); }

	.info-box {
		background: var(--bg-elevated); border: 1px solid var(--border); border-radius: 8px;
		padding: 12px 16px; font-size: 12px; color: var(--text-secondary); line-height: 1.6;
	}
	.info-box code { background: var(--bg-highlight); padding: 1px 5px; border-radius: 3px; font-family: var(--font-mono); font-size: 11px; }
</style>
