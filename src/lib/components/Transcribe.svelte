<script lang="ts">
	import { open } from '@tauri-apps/plugin-dialog';
	import { readFile } from '@tauri-apps/plugin-fs';
	import { parseMidiBytes } from '$lib/stores/midiStore';
	import { setActivePanel } from '$lib/stores/uiStore';
	import { sidecarStore } from '$lib/stores/sidecarStore';
	import { createTask, completeTask, failTask, updateTask } from '$lib/stores/aiTaskStore';

	const SIDECAR_URL = 'http://127.0.0.1:8002';

	type Mode = 'basic' | 'multitrack';
	let mode: Mode = 'basic';
	let selectedFile: string | null = null;
	let selectedFileName = '';
	let status: 'idle' | 'running' | 'done' | 'error' = 'idle';
	let errorMsg = '';
	let noteCount = 0;

	$: sidecarReady = ($sidecarStore.find((s) => s.id === 'audio-fx')?.status ?? 'stopped') === 'running';

	async function pickFile() {
		const result = await open({
			multiple: false,
			filters: [{ name: 'Audio', extensions: ['wav', 'mp3', 'ogg', 'flac', 'aiff', 'm4a'] }]
		});
		if (result && !Array.isArray(result)) {
			selectedFile = result;
			selectedFileName = result.split('/').pop() ?? result;
			status = 'idle';
		}
	}

	async function transcribe() {
		if (!selectedFile) return;
		status = 'running';
		errorMsg = '';
		const taskId = createTask('transcribe', `Transcribing: ${selectedFileName}`);

		try {
			const fileBytes = await readFile(selectedFile);
			const formData = new FormData();
			formData.append('audio', new Blob([fileBytes]), selectedFileName);

			updateTask(taskId, { progress: 20 });

			const endpoint = mode === 'basic' ? '/transcribe/basic' : '/transcribe/multitrack';
			const res = await window.fetch(`${SIDECAR_URL}${endpoint}`, {
				method: 'POST',
				body: formData
			});

			if (!res.ok) {
				const text = await res.text();
				throw new Error(`Sidecar returned ${res.status}: ${text}`);
			}

			updateTask(taskId, { progress: 80 });

			// Response is a MIDI file
			const blob = await res.blob();
			const arrayBuf = await blob.arrayBuffer();
			const midiBytes = new Uint8Array(arrayBuf);

			parseMidiBytes(midiBytes);

			// Count notes across all tracks
			const { Midi } = await import('@tonejs/midi');
			const midi = new Midi(midiBytes);
			noteCount = midi.tracks.reduce((sum, t) => sum + t.notes.length, 0);

			completeTask(taskId);
			status = 'done';

			// Auto-navigate to piano roll
			setTimeout(() => setActivePanel('piano-roll'), 800);

		} catch (e: unknown) {
			errorMsg = String(e);
			failTask(taskId, errorMsg);
			status = 'error';
		}
	}
</script>

<div class="panel">
	<div class="panel-header">
		<h2>🎼 Transcribe Audio → MIDI</h2>
		<span class="badge" class:ready={sidecarReady} class:offline={!sidecarReady}>
			{sidecarReady ? 'Ready' : 'Offline'}
		</span>
	</div>

	<div class="panel-body">
		<!-- Mode -->
		<div class="section">
			<div class="section-label">Transcription engine</div>
			<div class="mode-row">
				<label class="mode-card" class:selected={mode === 'basic'}>
					<input type="radio" bind:group={mode} value="basic" />
					<div class="mode-title">Basic Pitch <span class="mode-badge">Recommended</span></div>
					<div class="mode-desc">Spotify's polyphonic transcription. Works on any instrument. Fast, accurate. Returns a single MIDI track.</div>
				</label>
				<label class="mode-card" class:selected={mode === 'multitrack'}>
					<input type="radio" bind:group={mode} value="multitrack" />
					<div class="mode-title">MT3 Multitrack</div>
					<div class="mode-desc">Google Magenta's multi-instrument transcription. Separates drums, bass, piano, etc. into separate MIDI tracks. Slower.</div>
				</label>
			</div>
		</div>

		<!-- File -->
		<div class="section">
			<div class="section-label">Audio file</div>
			<div
				class="drop-zone"
				class:has-file={!!selectedFile}
				role="button"
				tabindex="0"
				on:click={pickFile}
				on:keydown={(e) => e.key === 'Enter' && pickFile()}
			>
				{#if selectedFile}
					<span>🎵 {selectedFileName}</span>
				{:else}
					<span>Click to pick an audio file to transcribe</span>
				{/if}
			</div>
		</div>

		<!-- Go -->
		<button
			class="primary-btn"
			disabled={!selectedFile || status === 'running' || !sidecarReady}
			on:click={transcribe}
		>
			{status === 'running' ? '⏳ Transcribing…' : '🎼 Transcribe to MIDI'}
		</button>

		<!-- Result -->
		{#if status === 'done'}
			<div class="result-box success">
				<span class="result-icon">✓</span>
				<div>
					<strong>Transcription complete</strong><br />
					{noteCount} notes detected. Opening Piano Roll…
				</div>
			</div>
		{/if}

		{#if status === 'error'}
			<div class="result-box error">
				<span class="result-icon">✕</span>
				<div>
					<strong>Transcription failed</strong><br />
					{errorMsg}
				</div>
			</div>
		{/if}

		<!-- Info -->
		<div class="info-section">
			<div class="section-label">How it works</div>
			<ol class="info-list">
				<li>Pick an audio file (vocals, guitar, piano, any instrument)</li>
				<li>Basic Pitch detects every pitch event and its timing</li>
				<li>The MIDI notes are loaded into the Piano Roll for editing</li>
				<li>Export the result as a MIDI file or use it in your project</li>
			</ol>
		</div>

		{#if !sidecarReady}
			<div class="info-box">
				Audio FX sidecar is not running. Ensure <code>basic-pitch</code> is installed:<br />
				<code>source sidecars/audio-fx/venv/bin/activate && pip install basic-pitch</code>
			</div>
		{/if}
	</div>
</div>

<style>
	.panel {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--bg-base);
	}

	.panel-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 14px 20px 10px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-surface);
		flex-shrink: 0;
	}

	.panel-header h2 { font-size: 15px; font-weight: 600; }

	.badge {
		font-size: 10px;
		padding: 2px 8px;
		border-radius: 10px;
		border: 1px solid;
	}
	.badge.ready  { color: var(--success); border-color: var(--success); background: rgba(76,175,125,0.1); }
	.badge.offline { color: var(--text-muted); border-color: var(--border); }

	.panel-body {
		flex: 1;
		overflow-y: auto;
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 600px;
	}

	.section { display: flex; flex-direction: column; gap: 8px; }

	.section-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: 0.5px;
	}

	.mode-row { display: flex; gap: 12px; }

	.mode-card {
		flex: 1;
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 14px;
		cursor: pointer;
		transition: all 0.15s;
		background: var(--bg-surface);
	}

	.mode-card:hover { border-color: var(--accent); }
	.mode-card.selected { border-color: var(--accent); background: var(--accent-dim); }
	.mode-card input { display: none; }

	.mode-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 4px;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.mode-badge {
		font-size: 9px;
		padding: 1px 6px;
		background: var(--accent);
		color: #fff;
		border-radius: 8px;
		font-weight: 600;
	}

	.mode-desc { font-size: 11px; color: var(--text-muted); line-height: 1.5; }

	.drop-zone {
		border: 1px dashed var(--border);
		border-radius: 8px;
		padding: 16px;
		color: var(--text-muted);
		font-size: 13px;
		cursor: pointer;
		transition: all 0.15s;
		text-align: center;
	}

	.drop-zone:hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
	.drop-zone.has-file { border-color: var(--accent); color: var(--text-primary); background: var(--bg-elevated); }

	.primary-btn {
		padding: 11px 28px;
		background: var(--accent);
		color: #fff;
		border-radius: 8px;
		font-weight: 600;
		font-size: 14px;
		transition: background 0.15s;
		align-self: flex-start;
	}

	.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
	.primary-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.result-box {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		border-radius: 10px;
		padding: 14px 18px;
		border: 1px solid;
		font-size: 13px;
		line-height: 1.5;
	}

	.result-box.success { background: rgba(76,175,125,0.08); border-color: var(--success); color: var(--success); }
	.result-box.error   { background: rgba(224,82,82,0.08);  border-color: var(--error);   color: var(--error); }
	.result-icon { font-size: 18px; flex-shrink: 0; }

	.info-section { display: flex; flex-direction: column; gap: 8px; }

	.info-list {
		padding-left: 18px;
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.8;
	}

	.info-box {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 12px 16px;
		font-size: 12px;
		color: var(--text-secondary);
		line-height: 1.7;
	}

	.info-box code {
		display: block;
		margin-top: 4px;
		background: var(--bg-base);
		padding: 5px 10px;
		border-radius: 4px;
		font-family: var(--font-mono);
		font-size: 10px;
	}
</style>
